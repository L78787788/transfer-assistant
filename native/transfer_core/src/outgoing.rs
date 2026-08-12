use std::{
    collections::HashSet,
    fs::{self, File},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Notify,
};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use crate::{
    chunk::{ChunkPlan, ChunkSpec, DEFAULT_CHUNK_SIZE, MAX_DATA_CHANNELS, ResumeMap},
    core::{CoreInner, PeerSummary, SourceHandle, TransferOffer},
    lan::LanError,
    manifest::{EntryKind, ManifestEntry, TransferManifest},
    model::{TransferDirection, TransferState},
    path_safety::{
        is_link_or_reparse, modified_unix_ms, safe_manifest_name, source_revision,
        unique_manifest_root,
    },
    persistence::{RepositoryError, StoredTransferItem, TransferRepository},
    protocol::{read_envelope, wire, write_envelope},
    storage::{read_chunk, read_sequential_chunk},
    transfer::write_header,
    wire::{
        connection_open, device_kind, envelope, expect_decision, expect_hello, expect_pairing,
        expect_result, expect_resume, hello_envelope, offer_envelope, pairing_code,
        pairing_confirmation, tls_peer_fingerprint, validate_hello,
    },
};

const MANIFEST_PAGE_ENTRIES: usize = 1_000;

/// 单个地址连接超时：多候选地址逐个尝试时避免被不可达地址拖死。
const ADDRESS_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 解析对端广播的地址列表（逗号分隔，来自 mDNS 多接口广播）。
fn parse_peer_addresses(address: &str) -> Result<Vec<SocketAddr>, LanError> {
    let parsed = address
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<SocketAddr>()
                .map_err(|_| LanError::InvalidPeerAddress(address.to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.is_empty() {
        return Err(LanError::InvalidPeerAddress(address.to_owned()));
    }
    Ok(parsed)
}

/// 依次尝试候选地址，返回首个连通的 TCP 连接及其实际地址。
async fn connect_any(addresses: &[SocketAddr]) -> Result<(TcpStream, SocketAddr), LanError> {
    let mut last_error = None;
    for &address in addresses {
        match tokio::time::timeout(ADDRESS_CONNECT_TIMEOUT, TcpStream::connect(address)).await {
            Ok(Ok(socket)) => return Ok((socket, address)),
            Ok(Err(error)) => last_error = Some(error),
            Err(_) => {
                last_error = Some(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("connect to {address} timed out"),
                ));
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no candidate address")
        })
        .into())
}

pub(crate) async fn write_manifest<S>(
    stream: &mut S,
    transfer_id: Uuid,
    entries: &[OutgoingEntry],
) -> Result<(), LanError>
where
    S: AsyncWrite + Unpin,
{
    for (page_index, page) in entries.chunks(MANIFEST_PAGE_ENTRIES).enumerate() {
        let wire_entries = page
            .iter()
            .map(|entry| wire::ManifestEntry {
                item_id: entry.item_id.clone(),
                relative_path: entry.relative_path.clone(),
                kind: match entry.kind {
                    EntryKind::File => wire::EntryKind::File as i32,
                    EntryKind::Directory => wire::EntryKind::Directory as i32,
                },
                size: entry.size,
                modified_unix_ms: entry.modified_unix_ms,
                source_revision: entry.source_revision.clone(),
                random_access: entry.random_access,
            })
            .collect();
        write_envelope(
            stream,
            &envelope(wire::envelope::Payload::ManifestPage(wire::ManifestPage {
                transfer_id: transfer_id.to_string(),
                page_index: page_index as u32,
                is_last: (page_index + 1) * MANIFEST_PAGE_ENTRIES >= entries.len(),
                entries: wire_entries,
            })),
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct OutgoingJob {
    pub(crate) entries: Vec<OutgoingEntry>,
    pub(crate) top_level_names: Vec<String>,
    pub(crate) item_count: u32,
    pub(crate) total_bytes: u64,
}

#[derive(Debug)]
pub(crate) struct OutgoingEntry {
    pub(crate) item_id: String,
    pub(crate) relative_path: String,
    pub(crate) kind: EntryKind,
    pub(crate) size: u64,
    pub(crate) modified_unix_ms: i64,
    pub(crate) source_revision: String,
    pub(crate) source_path: Option<PathBuf>,
    pub(crate) persistent_source: Option<String>,
    pub(crate) source_file: Option<Arc<File>>,
    pub(crate) random_access: bool,
}

pub struct JobControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
    changed: Notify,
}

impl JobControl {
    pub(crate) fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }

    pub(crate) fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }

    pub(crate) fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.changed.notify_waiters();
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }

    async fn checkpoint(&self, inner: &CoreInner) -> Result<(), LanError> {
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(LanError::Cancelled);
            }
            if !self.paused.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::select! {
                () = self.changed.notified() => {}
                () = inner.shutdown.cancelled() => return Err(LanError::Stopped),
            }
        }
    }
}

pub(crate) fn scan_sources(sources: &[SourceHandle]) -> Result<OutgoingJob, LanError> {
    let mut entries = Vec::new();
    let mut top_level_names = Vec::with_capacity(sources.len());
    let mut seen_roots = HashSet::new();

    if sources
        .iter()
        .any(|source| source.relative_path.is_some() || source.token.starts_with("android-fd:"))
    {
        for source in sources {
            let fallback_relative;
            let relative_path = match source.relative_path.as_deref() {
                Some(path) => path,
                None => {
                    fallback_relative = safe_manifest_name(&source.display_name);
                    fallback_relative.as_str()
                }
            };
            let kind = if source.is_directory {
                EntryKind::Directory
            } else {
                EntryKind::File
            };
            let (size, modified_unix_ms, source_revision, source_file) = if source.is_directory {
                (
                    0,
                    source.modified_unix_ms.unwrap_or_default(),
                    "directory".to_owned(),
                    None,
                )
            } else {
                let file = take_platform_file(&source.token)?;
                let metadata = file.metadata()?;
                let size = source.size.unwrap_or(metadata.len());
                let modified = source
                    .modified_unix_ms
                    .unwrap_or_else(|| modified_unix_ms(&metadata));
                (
                    size,
                    modified,
                    format!("{size}:{modified}"),
                    Some(Arc::new(file)),
                )
            };
            entries.push(OutgoingEntry {
                item_id: Uuid::new_v4().to_string(),
                relative_path: relative_path.to_owned(),
                kind,
                size,
                modified_unix_ms,
                source_revision,
                source_path: None,
                persistent_source: source.persistent_token.clone(),
                source_file,
                random_access: source.random_access.unwrap_or(true),
            });
            if let Some(root) = relative_path.split('/').next()
                && seen_roots.insert(root.to_lowercase())
            {
                top_level_names.push(root.to_owned());
            }
        }
    } else {
        for source in sources {
            let path = PathBuf::from(&source.token);
            let metadata = fs::symlink_metadata(&path)?;
            if is_link_or_reparse(&metadata) {
                continue;
            }
            let fallback = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("未命名项目");
            let root_name = safe_manifest_name(if source.display_name.trim().is_empty() {
                fallback
            } else {
                source.display_name.trim()
            });
            let root_name = unique_manifest_root(root_name, &mut seen_roots);
            top_level_names.push(root_name.clone());
            scan_path(&path, &root_name, &mut entries)?;
        }
    }
    if entries.is_empty() {
        return Err(LanError::NoTransferableSources);
    }
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let manifest = TransferManifest::new(
        entries
            .iter()
            .map(|entry| ManifestEntry {
                relative_path: entry.relative_path.clone(),
                kind: entry.kind,
                size: entry.size,
                modified_unix_ms: entry.modified_unix_ms,
            })
            .collect(),
    );
    manifest.validate()?;
    let item_count = u32::try_from(entries.len()).map_err(|_| LanError::TooManyItems)?;
    let total_bytes = entries.iter().try_fold(0_u64, |total, entry| {
        total
            .checked_add(entry.size)
            .ok_or(LanError::TotalSizeOverflow)
    })?;
    Ok(OutgoingJob {
        entries,
        top_level_names,
        item_count,
        total_bytes,
    })
}

fn scan_path(
    path: &Path,
    relative_path: &str,
    entries: &mut Vec<OutgoingEntry>,
) -> Result<(), LanError> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_or_reparse(&metadata) {
        return Ok(());
    }
    let modified_unix_ms = modified_unix_ms(&metadata);
    if metadata.is_file() {
        entries.push(OutgoingEntry {
            item_id: Uuid::new_v4().to_string(),
            relative_path: relative_path.to_owned(),
            kind: EntryKind::File,
            size: metadata.len(),
            modified_unix_ms,
            source_revision: source_revision(&metadata),
            source_path: Some(path.to_owned()),
            persistent_source: None,
            source_file: Some(Arc::new(File::open(path)?)),
            random_access: true,
        });
    } else if metadata.is_dir() {
        entries.push(OutgoingEntry {
            item_id: Uuid::new_v4().to_string(),
            relative_path: relative_path.to_owned(),
            kind: EntryKind::Directory,
            size: 0,
            modified_unix_ms,
            source_revision: source_revision(&metadata),
            source_path: None,
            persistent_source: None,
            source_file: None,
            random_access: true,
        });
        let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let child_metadata = fs::symlink_metadata(child.path())?;
            if is_link_or_reparse(&child_metadata) {
                continue;
            }
            let child_name = safe_manifest_name(&child.file_name().to_string_lossy());
            scan_path(
                &child.path(),
                &format!("{relative_path}/{child_name}"),
                entries,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn persist_outgoing_items(
    repository: &TransferRepository,
    transfer_id: Uuid,
    job: &OutgoingJob,
) -> Result<(), RepositoryError> {
    for entry in &job.entries {
        repository.save_item(&StoredTransferItem {
            id: entry.item_id.clone(),
            transfer_id,
            relative_path: entry.relative_path.clone(),
            kind: entry.kind,
            size: entry.size,
            modified_unix_ms: entry.modified_unix_ms,
            source_revision: Some(entry.source_revision.clone()),
            temporary_ref: entry
                .source_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .or_else(|| {
                    entry
                        .persistent_source
                        .as_ref()
                        .map(|token| format!("android-source:{token}"))
                }),
            final_ref: None,
        })?;
    }
    Ok(())
}

pub(crate) fn restore_outgoing_job(
    repository: &TransferRepository,
    transfer_id: Uuid,
) -> Result<OutgoingJob, LanError> {
    let items = repository.list_items(transfer_id)?;
    if items.is_empty() {
        return Err(LanError::MissingPersistedSources);
    }
    let mut entries = Vec::with_capacity(items.len());
    let mut top_level_names = Vec::new();
    let mut seen_roots = HashSet::new();
    for item in items {
        let source_revision = item
            .source_revision
            .ok_or_else(|| LanError::MissingPersistedSource(item.relative_path.clone()))?;
        let (source_path, persistent_source, source_file, random_access) =
            if item.kind == EntryKind::File {
                let source_reference = item
                    .temporary_ref
                    .ok_or_else(|| LanError::MissingPersistedSource(item.relative_path.clone()))?;
                reopen_persisted_source(&source_reference, &item.relative_path)?
            } else {
                (None, None, None, true)
            };
        if let Some(root) = item.relative_path.split('/').next()
            && seen_roots.insert(root.to_lowercase())
        {
            top_level_names.push(root.to_owned());
        }
        entries.push(OutgoingEntry {
            item_id: item.id,
            relative_path: item.relative_path,
            kind: item.kind,
            size: item.size,
            modified_unix_ms: item.modified_unix_ms,
            source_revision,
            source_path,
            persistent_source,
            source_file,
            random_access,
        });
    }
    let manifest = TransferManifest::new(
        entries
            .iter()
            .map(|entry| ManifestEntry {
                relative_path: entry.relative_path.clone(),
                kind: entry.kind,
                size: entry.size,
                modified_unix_ms: entry.modified_unix_ms,
            })
            .collect(),
    );
    manifest.validate()?;
    let item_count = u32::try_from(entries.len()).map_err(|_| LanError::TooManyItems)?;
    let total_bytes = entries.iter().try_fold(0_u64, |total, entry| {
        total
            .checked_add(entry.size)
            .ok_or(LanError::TotalSizeOverflow)
    })?;
    Ok(OutgoingJob {
        entries,
        top_level_names,
        item_count,
        total_bytes,
    })
}

type ReopenedSource = (Option<PathBuf>, Option<String>, Option<Arc<File>>, bool);

#[cfg(not(target_os = "android"))]
fn reopen_persisted_source(
    source_reference: &str,
    relative_path: &str,
) -> Result<ReopenedSource, LanError> {
    let path = PathBuf::from(source_reference);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || is_link_or_reparse(&metadata) {
        return Err(LanError::SourceChanged(relative_path.to_owned()));
    }
    let file = Arc::new(File::open(&path)?);
    Ok((Some(path), None, Some(file), true))
}

#[cfg(target_os = "android")]
fn reopen_persisted_source(
    source_reference: &str,
    relative_path: &str,
) -> Result<ReopenedSource, LanError> {
    use std::os::fd::FromRawFd;

    let uri = source_reference
        .strip_prefix("android-source:")
        .ok_or_else(|| LanError::MissingPersistedSource(relative_path.to_owned()))?;
    let prepared = crate::android_storage::open_source(uri)?;
    if prepared.fd < 0 {
        return Err(LanError::InvalidPlatformToken(prepared.fd.to_string()));
    }
    // SAFETY: Kotlin transfers ownership with ParcelFileDescriptor.detachFd(), and this File
    // owns the reopened descriptor for the lifetime of the restored outgoing job.
    let file = Arc::new(unsafe { File::from_raw_fd(prepared.fd) });
    Ok((
        None,
        Some(uri.to_owned()),
        Some(file),
        prepared.random_access,
    ))
}

pub(crate) fn delete_partial_data(
    repository: &TransferRepository,
    transfer_id: Uuid,
) -> Result<(), LanError> {
    for item in repository.list_items(transfer_id)? {
        let Some(temporary) = item.temporary_ref else {
            continue;
        };
        #[cfg(target_os = "android")]
        if let Some(uri) = temporary.strip_prefix("android-saf:") {
            crate::android_storage::delete_target(uri)?;
            continue;
        }
        let path = PathBuf::from(temporary);
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    repository.clear_completed_chunks(transfer_id)?;
    Ok(())
}

pub(crate) async fn run_outgoing(
    inner: Arc<CoreInner>,
    transfer_id: Uuid,
    peer: PeerSummary,
    job: Arc<OutgoingJob>,
    control: Arc<JobControl>,
) -> Result<(), LanError> {
    let _active_permit = inner
        .active_tasks
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| LanError::Stopped)?;
    inner.transition_transfer(transfer_id, TransferState::Connecting)?;
    // 对端可能广播多个地址（WiFi 与移动数据混在一起），逐个尝试直到连上。
    let addresses = parse_peer_addresses(&peer.address)?;
    let connector = TlsConnector::from(Arc::new(inner.identity.client_config()?));
    let (socket, address) = connect_any(&addresses).await?;
    log::info!("发送 {transfer_id}: 控制通道已连接 {address}");
    socket.set_nodelay(true)?;
    let server_name = "transassist.local".try_into().expect("static DNS name");
    let mut tls = connector.connect(server_name, socket).await?;
    let remote_fingerprint = tls_peer_fingerprint(tls.get_ref().1.peer_certificates())?;
    write_envelope(
        &mut tls,
        &connection_open(wire::ConnectionKind::Control, transfer_id, &[], 0),
    )
    .await?;
    write_envelope(&mut tls, &hello_envelope(&inner)?).await?;
    let remote_hello = expect_hello(read_envelope(&mut tls).await?)?;
    validate_hello(&remote_hello, &remote_fingerprint)?;
    let trusted = inner.is_trusted(&remote_hello.device_id, &remote_fingerprint)?;
    inner.update_peer_identity(
        Some(&peer.id),
        remote_hello.device_id.clone(),
        remote_hello.display_name.clone(),
        address.to_string(),
        device_kind(remote_hello.capabilities),
        trusted,
    )?;

    write_envelope(&mut tls, &offer_envelope(transfer_id, &job)).await?;
    write_manifest(&mut tls, transfer_id, &job.entries).await?;

    let pairing_answer = if trusted {
        None
    } else {
        inner.transition_transfer(transfer_id, TransferState::Pairing)?;
        let pairing_code = pairing_code(&inner, &remote_fingerprint, &remote_hello.device_id);
        let answer = inner
            .request_answer(TransferOffer {
                id: transfer_id.to_string(),
                peer_name: remote_hello.display_name.clone(),
                item_count: job.item_count,
                total_bytes: job.total_bytes,
                top_level_names: job.top_level_names.clone(),
                pairing_code: Some(pairing_code),
                direction: TransferDirection::Outgoing,
            })
            .await?;
        write_envelope(
            &mut tls,
            &pairing_confirmation(answer.accept, answer.remember_peer),
        )
        .await?;
        let remote_confirmation = expect_pairing(read_envelope(&mut tls).await?)?;
        if !answer.accept || !remote_confirmation.confirmed {
            return Err(LanError::PairingRejected);
        }
        if answer.remember_peer {
            inner.trust_peer(
                remote_hello.device_id.clone(),
                remote_hello.display_name.clone(),
                remote_fingerprint,
            )?;
        }
        Some(answer)
    };
    let _ = pairing_answer;
    inner.transition_transfer(transfer_id, TransferState::WaitingForAcceptance)?;
    log::info!("发送 {transfer_id}: 等待接收方决策");
    let decision = expect_decision(read_envelope(&mut tls).await?)?;
    if decision.transfer_id != transfer_id.to_string() || !decision.accepted {
        return Err(LanError::OfferRejected(decision.reason));
    }
    let resume = expect_resume(read_envelope(&mut tls).await?)?;
    log::info!(
        "发送 {transfer_id}: 决策接受，恢复 {} 个文件条目，通道数 {}",
        resume.files.len(),
        decision.data_channel_count
    );
    let jobs = build_chunk_jobs(&job, &resume)?;
    let remaining_bytes = jobs.iter().try_fold(0_u64, |total, chunk| {
        total
            .checked_add(u64::from(chunk.spec.length))
            .ok_or(LanError::TotalSizeOverflow)
    })?;
    inner.set_completed_bytes(transfer_id, job.total_bytes.saturating_sub(remaining_bytes))?;
    inner.transition_transfer(transfer_id, TransferState::Transferring)?;
    let mut channel_count = usize::try_from(decision.data_channel_count)
        .unwrap_or(MAX_DATA_CHANNELS)
        .min(MAX_DATA_CHANNELS)
        .min(jobs.len().max(1));
    if job.entries.iter().any(|entry| !entry.random_access) {
        channel_count = channel_count.min(1);
    }
    if !jobs.is_empty() && (decision.transfer_token.len() != 32 || channel_count == 0) {
        return Err(LanError::InvalidTransferToken);
    }
    let mut tasks = Vec::with_capacity(channel_count);
    for channel_index in 0..channel_count {
        let assigned = jobs
            .iter()
            .skip(channel_index)
            .step_by(channel_count)
            .cloned()
            .collect::<Vec<_>>();
        if assigned.is_empty() {
            continue;
        }
        let channel_inner = inner.clone();
        let channel_job = job.clone();
        let channel_control = control.clone();
        let token = decision.transfer_token.clone();
        tasks.push(tokio::spawn(async move {
            send_data_channel(SendDataChannelRequest {
                inner: channel_inner,
                transfer_id,
                address,
                channel_index: channel_index as u32,
                token,
                job: channel_job,
                chunks: assigned,
                control: channel_control,
            })
            .await
        }));
    }
    for task in tasks {
        task.await??;
    }
    log::info!("发送 {transfer_id}: 数据通道完成，等待结果");
    let result = expect_result(read_envelope(&mut tls).await?)?;
    if !result.completed {
        return Err(LanError::RemoteTransferFailed(result.error));
    }
    inner.transition_transfer(transfer_id, TransferState::Verifying)?;
    inner.transition_transfer(transfer_id, TransferState::Completed)?;
    Ok(())
}

struct SendDataChannelRequest {
    inner: Arc<CoreInner>,
    transfer_id: Uuid,
    address: SocketAddr,
    channel_index: u32,
    token: Vec<u8>,
    job: Arc<OutgoingJob>,
    chunks: Vec<ChunkJob>,
    control: Arc<JobControl>,
}

async fn send_data_channel(request: SendDataChannelRequest) -> Result<(), LanError> {
    let SendDataChannelRequest {
        inner,
        transfer_id,
        address,
        channel_index,
        token,
        job,
        chunks,
        control,
    } = request;
    let connector = TlsConnector::from(Arc::new(inner.identity.client_config()?));
    let socket = TcpStream::connect(address).await?;
    socket.set_nodelay(true)?;
    let server_name = "transassist.local".try_into().expect("static DNS name");
    let mut tls = connector.connect(server_name, socket).await?;
    write_envelope(
        &mut tls,
        &connection_open(
            wire::ConnectionKind::Data,
            transfer_id,
            &token,
            channel_index,
        ),
    )
    .await?;
    for chunk_job in chunks {
        control.checkpoint(&inner).await?;
        let entry = &job.entries[chunk_job.entry_index];
        verify_source_revision(entry)?;
        let source = entry
            .source_file
            .as_ref()
            .ok_or(LanError::MissingSourcePath)?
            .try_clone()?;
        let spec = chunk_job.spec;
        let random_access = entry.random_access;
        let chunk = tokio::task::spawn_blocking(move || {
            if random_access {
                read_chunk(&source, spec).map_err(LanError::from)
            } else {
                read_sequential_chunk(&source, spec).map_err(LanError::from)
            }
        })
        .await??;
        let header = wire::ChunkHeader {
            transfer_id: transfer_id.to_string(),
            item_id: entry.item_id.clone(),
            chunk_index: chunk.spec.index,
            offset: chunk.spec.offset,
            length: chunk.spec.length,
            blake3_hash: chunk.blake3_hash.to_vec(),
        };
        write_header(&mut tls, &header).await?;
        tls.write_all(&chunk.bytes).await?;
        inner.update_completed_bytes(transfer_id, u64::from(chunk.spec.length))?;
    }
    tls.shutdown().await?;
    Ok(())
}

#[derive(Clone)]
pub(crate) struct ChunkJob {
    entry_index: usize,
    spec: ChunkSpec,
}

pub(crate) fn build_chunk_jobs(
    job: &OutgoingJob,
    resume: &wire::ResumeState,
) -> Result<Vec<ChunkJob>, LanError> {
    let maps = resume
        .files
        .iter()
        .map(|file| (file.item_id.as_str(), file.completed_bitmap.as_slice()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut jobs = Vec::new();
    for (entry_index, entry) in job.entries.iter().enumerate() {
        if entry.kind != EntryKind::File {
            continue;
        }
        let plan = ChunkPlan::new(entry.size, DEFAULT_CHUNK_SIZE)?;
        let resume_map = match maps.get(entry.item_id.as_str()) {
            Some(bytes) => ResumeMap::from_bitmap_bytes(plan.chunk_count(), bytes)?,
            None => ResumeMap::new(plan.chunk_count()),
        };
        jobs.extend(
            plan.missing_chunks(&resume_map)?
                .into_iter()
                .map(|spec| ChunkJob { entry_index, spec }),
        );
    }
    Ok(jobs)
}

fn verify_source_revision(entry: &OutgoingEntry) -> Result<(), LanError> {
    let metadata = entry
        .source_file
        .as_ref()
        .ok_or(LanError::MissingSourcePath)?
        .metadata()?;
    #[cfg(target_os = "android")]
    let current_revision = if let Some(uri) = entry.persistent_source.as_deref() {
        crate::android_storage::source_revision(uri)?
    } else if entry.source_path.is_some() {
        source_revision(&metadata)
    } else {
        format!("{}:{}", metadata.len(), entry.modified_unix_ms)
    };
    #[cfg(not(target_os = "android"))]
    let current_revision = if entry.source_path.is_some() {
        source_revision(&metadata)
    } else {
        format!("{}:{}", metadata.len(), entry.modified_unix_ms)
    };
    if current_revision != entry.source_revision {
        return Err(LanError::SourceChanged(entry.relative_path.clone()));
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn take_platform_file(token: &str) -> Result<File, LanError> {
    use std::os::fd::FromRawFd;

    let descriptor = token
        .strip_prefix("android-fd:")
        .ok_or_else(|| LanError::InvalidPlatformToken(token.to_owned()))?
        .parse::<i32>()
        .map_err(|_| LanError::InvalidPlatformToken(token.to_owned()))?;
    if descriptor < 0 {
        return Err(LanError::InvalidPlatformToken(token.to_owned()));
    }
    // SAFETY: Kotlin transfers ownership with ParcelFileDescriptor.detachFd(); every token is
    // consumed exactly once while building the outgoing job.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(not(target_os = "android"))]
fn take_platform_file(token: &str) -> Result<File, LanError> {
    Err(LanError::InvalidPlatformToken(token.to_owned()))
}
