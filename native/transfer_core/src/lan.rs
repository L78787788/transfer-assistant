use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
    time::UNIX_EPOCH,
};

use if_addrs::get_if_addrs;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinHandle,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use uuid::Uuid;

use crate::{
    chunk::{ChunkPlan, ChunkSpec, DEFAULT_CHUNK_SIZE, MAX_DATA_CHANNELS, ResumeMap},
    core::{
        CoreError, CoreInner, OfferAnswer, PeerSummary, SourceHandle, TransferOffer, now_unix_ms,
    },
    identity::{certificate_fingerprint, derive_pairing_code},
    manifest::{EntryKind, ManifestEntry, TransferManifest},
    model::{TransferDirection, TransferSnapshot, TransferState},
    persistence::{RepositoryError, StoredTransferItem, TransferRepository},
    protocol::{self, PROTOCOL_MAJOR, PROTOCOL_MINOR, read_envelope, wire, write_envelope},
    storage::{
        VerifiedChunk, read_chunk, read_sequential_chunk, write_sequential_verified_chunk,
        write_verified_chunk,
    },
    transfer::{self, read_header, validate_header, write_header},
};

const DEFAULT_PORT: u16 = 53_317;
const MANIFEST_PAGE_ENTRIES: usize = 1_000;
const MAX_MANIFEST_ENTRIES: usize = 1_000_000;
const SERVICE_TYPE: &str = "_transassist._tcp.local.";

pub struct NetworkHandle {
    address: SocketAddr,
    task: JoinHandle<()>,
    mdns: Option<MdnsHandle>,
}

impl NetworkHandle {
    pub(crate) async fn start(inner: Arc<CoreInner>) -> Result<Self, LanError> {
        let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)).await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await?
            }
            Err(error) => return Err(error.into()),
        };
        let bound = listener.local_addr()?;
        let advertised = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bound.port());
        let acceptor = TlsAcceptor::from(Arc::new(inner.identity.server_config()?));
        let state = Arc::new(NetworkState::default());
        let task_inner = inner.clone();
        let task = tokio::spawn(async move {
            accept_loop(listener, acceptor, task_inner, state).await;
        });
        let mdns = match MdnsHandle::start(inner.clone(), bound.port()) {
            Ok(handle) => Some(handle),
            Err(error) => {
                let _ = inner.queue_event(crate::core::CoreEvent::Failure {
                    message: format!("局域网自动发现启动失败，可使用 IP 地址连接: {error}"),
                });
                None
            }
        };
        Ok(Self {
            address: advertised,
            task,
            mdns,
        })
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn shutdown(mut self) {
        self.task.abort();
        if let Some(mdns) = self.mdns.take() {
            mdns.shutdown();
        }
    }
}

struct MdnsHandle {
    daemon: ServiceDaemon,
    fullname: String,
    stopped: Arc<AtomicBool>,
    browser: Option<thread::JoinHandle<()>>,
}

impl MdnsHandle {
    fn start(inner: Arc<CoreInner>, port: u16) -> Result<Self, LanError> {
        let daemon = ServiceDaemon::new()?;
        let short_id = &inner.identity.device_id()[..12];
        let instance = format!("transassist-{short_id}");
        let hostname = format!("transassist-{short_id}");
        let config = inner.config()?;
        let properties = HashMap::from([
            ("id".to_owned(), inner.identity.device_id().to_owned()),
            ("name".to_owned(), config.device_name),
            (
                "kind".to_owned(),
                if cfg!(target_os = "android") {
                    "phone"
                } else {
                    "computer"
                }
                .to_owned(),
            ),
            ("version".to_owned(), PROTOCOL_MAJOR.to_string()),
        ]);
        // Collect non-loopback IPv4 addresses for mDNS announcement.
        let addrs: Vec<String> = get_if_addrs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|iface| match iface.addr {
                if_addrs::IfAddr::V4(ref v4) if !v4.ip.is_loopback() => Some(v4.ip.to_string()),
                _ => None,
            })
            .collect();
        let addr_str = addrs.join(",");
        let service = if addr_str.is_empty() {
            ServiceInfo::new(
                SERVICE_TYPE,
                &instance,
                &hostname,
                "",
                port,
                Some(properties),
            )?
            .enable_addr_auto()
        } else {
            ServiceInfo::new(
                SERVICE_TYPE,
                &instance,
                &hostname,
                &addr_str,
                port,
                Some(properties),
            )?
        };
        let fullname = service.get_fullname().to_owned();
        daemon.register(service)?;
        let receiver = daemon.browse(SERVICE_TYPE)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let browser_stopped = stopped.clone();
        let local_id = inner.identity.device_id().to_owned();
        let browser = thread::Builder::new()
            .name("transassist-mdns".to_owned())
            .spawn(move || {
                let mut service_peers = HashMap::<String, String>::new();
                while !browser_stopped.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(500)) {
                        Ok(ServiceEvent::ServiceResolved(info)) => {
                            let Some(peer_id) = info.get_property_val_str("id") else {
                                continue;
                            };
                            if peer_id == local_id {
                                continue;
                            }
                            let Some(ip) = info
                                .get_addresses_v4()
                                .into_iter()
                                .find(|address| !address.is_loopback())
                                .or_else(|| info.get_addresses_v4().into_iter().next())
                            else {
                                continue;
                            };
                            let name = info
                                .get_property_val_str("name")
                                .unwrap_or(peer_id)
                                .to_owned();
                            let kind = info
                                .get_property_val_str("kind")
                                .unwrap_or("computer")
                                .to_owned();
                            let trusted = inner
                                .repository
                                .trusted_peer(peer_id)
                                .ok()
                                .flatten()
                                .is_some();
                            let _ = inner.upsert_peer(PeerSummary {
                                id: peer_id.to_owned(),
                                name,
                                address: SocketAddr::new(IpAddr::V4(ip), info.get_port())
                                    .to_string(),
                                device_kind: kind,
                                trusted,
                                online: true,
                            });
                            service_peers
                                .insert(info.get_fullname().to_owned(), peer_id.to_owned());
                        }
                        Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                            if let Some(peer_id) = service_peers.remove(&fullname) {
                                let _ = inner.mark_peer_offline(&peer_id);
                            }
                        }
                        Ok(_) => {}
                        Err(flume::RecvTimeoutError::Timeout) => {}
                        Err(flume::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self {
            daemon,
            fullname,
            stopped,
            browser: Some(browser),
        })
    }

    fn shutdown(mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.daemon.stop_browse(SERVICE_TYPE);
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
        if let Some(browser) = self.browser.take() {
            let _ = browser.join();
        }
    }
}

#[derive(Default)]
struct NetworkState {
    incoming: Mutex<HashMap<Uuid, Arc<IncomingContext>>>,
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
    item_id: String,
    relative_path: String,
    kind: EntryKind,
    size: u64,
    modified_unix_ms: i64,
    source_revision: String,
    source_path: Option<PathBuf>,
    persistent_source: Option<String>,
    source_file: Option<Arc<File>>,
    random_access: bool,
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
    let address: SocketAddr = peer
        .address
        .parse()
        .map_err(|_| LanError::InvalidPeerAddress(peer.address.clone()))?;
    let connector = TlsConnector::from(Arc::new(inner.identity.client_config()?));
    let socket = TcpStream::connect(address).await?;
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
    let decision = expect_decision(read_envelope(&mut tls).await?)?;
    if decision.transfer_id != transfer_id.to_string() || !decision.accepted {
        return Err(LanError::OfferRejected(decision.reason));
    }
    let resume = expect_resume(read_envelope(&mut tls).await?)?;
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
struct ChunkJob {
    entry_index: usize,
    spec: ChunkSpec,
}

fn build_chunk_jobs(
    job: &OutgoingJob,
    resume: &wire::ResumeState,
) -> Result<Vec<ChunkJob>, LanError> {
    let maps = resume
        .files
        .iter()
        .map(|file| (file.item_id.as_str(), file.completed_bitmap.as_slice()))
        .collect::<HashMap<_, _>>();
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

async fn accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    inner: Arc<CoreInner>,
    state: Arc<NetworkState>,
) {
    loop {
        let accepted = tokio::select! {
            result = listener.accept() => result,
            () = inner.shutdown.cancelled() => break,
        };
        let (socket, remote_address) = match accepted {
            Ok(value) => value,
            Err(error) => {
                let _ = inner.queue_event(crate::core::CoreEvent::Failure {
                    message: format!("接收局域网连接失败: {error}"),
                });
                continue;
            }
        };
        let connection_acceptor = acceptor.clone();
        let connection_inner = inner.clone();
        let connection_state = state.clone();
        tokio::spawn(async move {
            let handler_inner = connection_inner.clone();
            let result = async {
                socket.set_nodelay(true)?;
                let mut tls = connection_acceptor.accept(socket).await?;
                let peer_fingerprint = tls_peer_fingerprint(tls.get_ref().1.peer_certificates())?;
                let open = expect_connection_open(read_envelope(&mut tls).await?)?;
                let transfer_id = Uuid::parse_str(&open.transfer_id)
                    .map_err(|_| LanError::InvalidTransferId(open.transfer_id.clone()))?;
                let outcome = match wire::ConnectionKind::try_from(open.kind) {
                    Ok(wire::ConnectionKind::Control) => {
                        handle_incoming_control(
                            &mut tls,
                            handler_inner.clone(),
                            connection_state,
                            transfer_id,
                            remote_address,
                            peer_fingerprint,
                        )
                        .await
                    }
                    Ok(wire::ConnectionKind::Data) => {
                        handle_incoming_data(
                            &mut tls,
                            handler_inner.clone(),
                            connection_state,
                            transfer_id,
                            open.transfer_token,
                            peer_fingerprint,
                        )
                        .await
                    }
                    _ => Err(LanError::UnexpectedMessage("connection kind")),
                };
                if let Err(error) = &outcome {
                    if error.is_retryable() {
                        let _ = handler_inner.interrupt_transfer(transfer_id, error.to_string());
                    } else {
                        let _ = handler_inner.fail_transfer(transfer_id, error.to_string());
                    }
                }
                outcome
            }
            .await;
            if let Err(error) = result {
                let _ = connection_inner.queue_event(crate::core::CoreEvent::Failure {
                    message: format!("局域网连接失败: {error}"),
                });
            }
        });
    }
}

async fn handle_incoming_control<S>(
    tls: &mut S,
    inner: Arc<CoreInner>,
    state: Arc<NetworkState>,
    transfer_id: Uuid,
    remote_address: SocketAddr,
    peer_fingerprint: [u8; 32],
) -> Result<(), LanError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_hello = expect_hello(read_envelope(tls).await?)?;
    validate_hello(&remote_hello, &peer_fingerprint)?;
    write_envelope(tls, &hello_envelope(&inner)?).await?;
    let trusted = inner.is_trusted(&remote_hello.device_id, &peer_fingerprint)?;
    inner.update_peer_identity(
        None,
        remote_hello.device_id.clone(),
        remote_hello.display_name.clone(),
        remote_address.to_string(),
        device_kind(remote_hello.capabilities),
        trusted,
    )?;

    let offer = expect_offer(read_envelope(tls).await?)?;
    if offer.transfer_id != transfer_id.to_string() {
        return Err(LanError::InvalidTransferId(offer.transfer_id));
    }
    let entries = read_manifest(tls, transfer_id).await?;
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
    let mut snapshot = TransferSnapshot::new_incoming(
        transfer_id,
        remote_hello.device_id.clone(),
        remote_hello.display_name.clone(),
        offer.item_count,
        offer.total_bytes,
        now_unix_ms(),
    );
    if !trusted {
        snapshot.state = TransferState::Pairing;
    }
    inner.add_incoming_transfer(snapshot)?;
    let incoming_offer = TransferOffer {
        id: transfer_id.to_string(),
        peer_name: remote_hello.display_name.clone(),
        item_count: offer.item_count,
        total_bytes: offer.total_bytes,
        top_level_names: offer.top_level_names.clone(),
        pairing_code: (!trusted)
            .then(|| pairing_code(&inner, &peer_fingerprint, &remote_hello.device_id)),
        direction: TransferDirection::Incoming,
    };
    let answer = if trusted && inner.config()?.auto_accept_trusted {
        OfferAnswer {
            accept: true,
            remember_peer: false,
        }
    } else {
        inner.request_answer(incoming_offer).await?
    };

    if !trusted {
        let client_confirmation = expect_pairing(read_envelope(tls).await?)?;
        let pairing_accepted = answer.accept && client_confirmation.confirmed;
        write_envelope(
            tls,
            &pairing_confirmation(pairing_accepted, answer.remember_peer),
        )
        .await?;
        if !pairing_accepted {
            write_envelope(
                tls,
                &decision_envelope(transfer_id, false, "配对或接收已拒绝", &[], 0),
            )
            .await?;
            inner.fail_transfer(transfer_id, "配对或接收已拒绝".to_owned())?;
            return Ok(());
        }
        if answer.remember_peer {
            inner.trust_peer(
                remote_hello.device_id.clone(),
                remote_hello.display_name.clone(),
                peer_fingerprint,
            )?;
        }
        inner.transition_transfer(transfer_id, TransferState::WaitingForAcceptance)?;
    }

    if !answer.accept {
        write_envelope(
            tls,
            &decision_envelope(transfer_id, false, "接收方已拒绝", &[], 0),
        )
        .await?;
        inner.fail_transfer(transfer_id, "接收方已拒绝".to_owned())?;
        return Ok(());
    }

    let _active_permit = inner
        .active_tasks
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| LanError::Stopped)?;

    let context = Arc::new(prepare_incoming(
        inner.clone(),
        transfer_id,
        peer_fingerprint,
        entries,
    )?);
    let token = transfer_token(
        transfer_id,
        &inner.identity.fingerprint(),
        &peer_fingerprint,
    );
    context.set_token(token);
    let channel_count = context
        .remaining_chunks
        .load(Ordering::Acquire)
        .min(MAX_DATA_CHANNELS as u64) as u32;
    let channel_count = if context.files.values().any(|file| !file.random_access) {
        channel_count.min(1)
    } else {
        channel_count
    };
    state
        .incoming
        .lock()
        .map_err(|_| LanError::LockPoisoned)?
        .insert(transfer_id, context.clone());
    write_envelope(
        tls,
        &decision_envelope(transfer_id, true, "", &token, channel_count),
    )
    .await?;
    write_envelope(tls, &context.resume_envelope()?).await?;
    inner.transition_transfer(transfer_id, TransferState::Transferring)?;

    if context.remaining_chunks.load(Ordering::Acquire) != 0 {
        loop {
            if context.remaining_chunks.load(Ordering::Acquire) == 0 {
                break;
            }
            if inner.transfer_is_cancelled(transfer_id) {
                return Err(LanError::Cancelled);
            }
            tokio::select! {
                () = context.completed.notified() => {}
                () = tokio::time::sleep(Duration::from_millis(200)) => {}
                () = inner.shutdown.cancelled() => return Err(LanError::Stopped),
            }
        }
    }
    inner.transition_transfer(transfer_id, TransferState::Verifying)?;
    finalize_incoming(&context)?;
    inner.transition_transfer(transfer_id, TransferState::Completed)?;
    write_envelope(tls, &result_envelope(transfer_id, true, "")).await?;
    state
        .incoming
        .lock()
        .map_err(|_| LanError::LockPoisoned)?
        .remove(&transfer_id);
    Ok(())
}

async fn handle_incoming_data<S>(
    tls: &mut S,
    inner: Arc<CoreInner>,
    state: Arc<NetworkState>,
    transfer_id: Uuid,
    token: Vec<u8>,
    peer_fingerprint: [u8; 32],
) -> Result<(), LanError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let context = state
        .incoming
        .lock()
        .map_err(|_| LanError::LockPoisoned)?
        .get(&transfer_id)
        .cloned()
        .ok_or(LanError::UnknownIncomingTransfer(transfer_id))?;
    if context.token() != token.as_slice() || context.peer_fingerprint != peer_fingerprint {
        return Err(LanError::InvalidTransferToken);
    }
    loop {
        let header = match read_header(tls).await {
            Ok(header) => header,
            Err(transfer::TransferIoError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };
        let file = context
            .files
            .get(&header.item_id)
            .ok_or_else(|| LanError::UnknownIncomingItem(header.item_id.clone()))?;
        if inner.transfer_is_cancelled(transfer_id) {
            return Err(LanError::Cancelled);
        }
        validate_header(
            &header,
            &transfer_id.to_string(),
            &header.item_id,
            file.entry.size,
        )?;
        let expected = file.plan.chunk(header.chunk_index)?;
        if expected.offset != header.offset || expected.length != header.length {
            return Err(LanError::ChunkPlanMismatch);
        }
        let mut bytes = vec![0_u8; header.length as usize];
        tls.read_exact(&mut bytes).await?;
        let hash: [u8; 32] = header
            .blake3_hash
            .try_into()
            .map_err(|value: Vec<u8>| LanError::InvalidHashLength(value.len()))?;
        let chunk = VerifiedChunk {
            spec: expected,
            blake3_hash: hash,
            bytes,
        };
        let _write_guard = file.write_lock.lock().await;
        let already_complete = file
            .resume
            .lock()
            .map_err(|_| LanError::LockPoisoned)?
            .contains(header.chunk_index);
        if already_complete {
            if *blake3::hash(&chunk.bytes).as_bytes() != chunk.blake3_hash {
                return Err(crate::storage::StorageError::HashMismatch {
                    index: header.chunk_index,
                }
                .into());
            }
            continue;
        }
        let target = file.target.try_clone()?;
        let written_length = u64::from(chunk.spec.length);
        let random_access = file.random_access;
        tokio::task::spawn_blocking(move || {
            if random_access {
                write_verified_chunk(&target, &chunk)
            } else {
                write_sequential_verified_chunk(&target, &chunk)
            }
        })
        .await??;
        let mut resume = file.resume.lock().map_err(|_| LanError::LockPoisoned)?;
        if resume.mark_complete(header.chunk_index)? {
            inner.repository.mark_chunk_complete(
                transfer_id,
                &header.item_id,
                header.chunk_index,
                &hash,
            )?;
            inner.update_completed_bytes(transfer_id, written_length)?;
            if context.remaining_chunks.fetch_sub(1, Ordering::AcqRel) == 1 {
                context.completed.notify_waiters();
            }
        }
    }
    Ok(())
}

struct IncomingContext {
    transfer_id: Uuid,
    peer_fingerprint: [u8; 32],
    token: Mutex<[u8; 32]>,
    files: HashMap<String, IncomingFile>,
    directories: Vec<PathBuf>,
    temporary_root: PathBuf,
    remaining_chunks: AtomicU64,
    completed: Notify,
    repository: Arc<TransferRepository>,
    #[cfg(target_os = "android")]
    android_saf: bool,
}

struct IncomingFile {
    entry: IncomingEntry,
    plan: ChunkPlan,
    resume: Mutex<ResumeMap>,
    write_lock: tokio::sync::Mutex<()>,
    target: File,
    temporary_path: PathBuf,
    final_path: PathBuf,
    #[cfg(target_os = "android")]
    android_temporary_uri: Option<String>,
    #[cfg(target_os = "android")]
    android_final_name: Option<String>,
    random_access: bool,
}

#[derive(Clone)]
struct IncomingEntry {
    item_id: String,
    relative_path: String,
    kind: EntryKind,
    size: u64,
    modified_unix_ms: i64,
    source_revision: String,
}

impl IncomingContext {
    fn set_token(&self, token: [u8; 32]) {
        if let Ok(mut stored) = self.token.lock() {
            *stored = token;
        }
    }

    fn token(&self) -> [u8; 32] {
        self.token.lock().map(|token| *token).unwrap_or_default()
    }

    fn resume_envelope(&self) -> Result<wire::Envelope, LanError> {
        let mut files = Vec::with_capacity(self.files.len());
        for file in self.files.values() {
            files.push(wire::FileResume {
                item_id: file.entry.item_id.clone(),
                completed_bitmap: file
                    .resume
                    .lock()
                    .map_err(|_| LanError::LockPoisoned)?
                    .to_bitmap_bytes(),
            });
        }
        files.sort_by(|left, right| left.item_id.cmp(&right.item_id));
        Ok(envelope(wire::envelope::Payload::ResumeState(
            wire::ResumeState {
                transfer_id: self.transfer_id.to_string(),
                files,
            },
        )))
    }
}

fn prepare_incoming(
    inner: Arc<CoreInner>,
    transfer_id: Uuid,
    peer_fingerprint: [u8; 32],
    entries: Vec<IncomingEntry>,
) -> Result<IncomingContext, LanError> {
    let receive_directory = inner.config()?.receive_directory;
    #[cfg(target_os = "android")]
    {
        let receive_token = receive_directory.to_string_lossy();
        if let Some(tree_uri) = receive_token.strip_prefix("android-saf:") {
            return prepare_incoming_android(
                inner,
                transfer_id,
                peer_fingerprint,
                tree_uri,
                entries,
            );
        }
    }
    fs::create_dir_all(&receive_directory)?;
    let temporary_root = receive_directory
        .join(".transassist")
        .join(transfer_id.to_string());
    fs::create_dir_all(&temporary_root)?;
    let final_paths = allocate_final_paths(&receive_directory, &entries)?;
    let mut files = HashMap::new();
    let mut directories = Vec::new();
    let mut remaining_chunks = 0_u64;
    let mut resumed_bytes = 0_u64;

    for entry in entries {
        let final_path = final_paths
            .get(&entry.item_id)
            .cloned()
            .ok_or(LanError::MissingFinalPath)?;
        if entry.kind == EntryKind::Directory {
            directories.push(final_path.clone());
            inner.repository.save_item(&StoredTransferItem {
                id: entry.item_id,
                transfer_id,
                relative_path: entry.relative_path,
                kind: entry.kind,
                size: 0,
                modified_unix_ms: entry.modified_unix_ms,
                source_revision: Some(entry.source_revision),
                temporary_ref: None,
                final_ref: Some(final_path.to_string_lossy().into_owned()),
            })?;
            continue;
        }
        let temporary_path = temporary_root.join(format!("{}.part", entry.item_id));
        let target = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&temporary_path)?;
        target.set_len(entry.size)?;
        let plan = ChunkPlan::new(entry.size, DEFAULT_CHUNK_SIZE)?;
        let mut resume = ResumeMap::new(plan.chunk_count());
        if temporary_path.exists() {
            for index in inner
                .repository
                .completed_chunks(transfer_id, &entry.item_id)?
            {
                if index < plan.chunk_count() {
                    resume.mark_complete(index)?;
                }
            }
        }
        resumed_bytes = resumed_bytes.saturating_add(resume.completed_bytes(&plan)?);
        remaining_chunks = remaining_chunks
            .checked_add(u64::from(
                plan.chunk_count() - resume_count(&resume, plan.chunk_count()),
            ))
            .ok_or(LanError::TooManyChunks)?;
        inner.repository.save_item(&StoredTransferItem {
            id: entry.item_id.clone(),
            transfer_id,
            relative_path: entry.relative_path.clone(),
            kind: entry.kind,
            size: entry.size,
            modified_unix_ms: entry.modified_unix_ms,
            source_revision: Some(entry.source_revision.clone()),
            temporary_ref: Some(temporary_path.to_string_lossy().into_owned()),
            final_ref: Some(final_path.to_string_lossy().into_owned()),
        })?;
        files.insert(
            entry.item_id.clone(),
            IncomingFile {
                entry,
                plan,
                resume: Mutex::new(resume),
                write_lock: tokio::sync::Mutex::new(()),
                target,
                temporary_path,
                final_path,
                #[cfg(target_os = "android")]
                android_temporary_uri: None,
                #[cfg(target_os = "android")]
                android_final_name: None,
                random_access: true,
            },
        );
    }
    if resumed_bytes != 0 {
        inner.update_completed_bytes(transfer_id, resumed_bytes)?;
    }
    Ok(IncomingContext {
        transfer_id,
        peer_fingerprint,
        token: Mutex::new([0; 32]),
        files,
        directories,
        temporary_root,
        remaining_chunks: AtomicU64::new(remaining_chunks),
        completed: Notify::new(),
        repository: inner.repository.clone(),
        #[cfg(target_os = "android")]
        android_saf: false,
    })
}

#[cfg(target_os = "android")]
fn prepare_incoming_android(
    inner: Arc<CoreInner>,
    transfer_id: Uuid,
    peer_fingerprint: [u8; 32],
    tree_uri: &str,
    entries: Vec<IncomingEntry>,
) -> Result<IncomingContext, LanError> {
    use std::os::fd::FromRawFd;

    let requests = entries
        .iter()
        .map(|entry| crate::android_storage::TargetRequest {
            id: &entry.item_id,
            relative_path: &entry.relative_path,
            is_directory: entry.kind == EntryKind::Directory,
            size: entry.size,
        })
        .collect::<Vec<_>>();
    let prepared =
        crate::android_storage::prepare_targets(tree_uri, &transfer_id.to_string(), &requests)?
            .into_iter()
            .map(|target| (target.id.clone(), target))
            .collect::<HashMap<_, _>>();
    let mut files = HashMap::new();
    let mut remaining_chunks = 0_u64;
    let mut resumed_bytes = 0_u64;

    for entry in entries {
        if entry.kind == EntryKind::Directory {
            inner.repository.save_item(&StoredTransferItem {
                id: entry.item_id,
                transfer_id,
                relative_path: entry.relative_path,
                kind: EntryKind::Directory,
                size: 0,
                modified_unix_ms: entry.modified_unix_ms,
                source_revision: Some(entry.source_revision),
                temporary_ref: None,
                final_ref: Some(format!("android-saf:{tree_uri}")),
            })?;
            continue;
        }
        let prepared = prepared
            .get(&entry.item_id)
            .ok_or(LanError::MissingFinalPath)?;
        if prepared.fd < 0 {
            return Err(LanError::InvalidPlatformToken(prepared.fd.to_string()));
        }
        // SAFETY: Kotlin transfers ownership with ParcelFileDescriptor.detachFd(), and this File
        // is the sole owner until the incoming context is dropped.
        let target = unsafe { File::from_raw_fd(prepared.fd) };
        let plan = ChunkPlan::new(entry.size, DEFAULT_CHUNK_SIZE)?;
        let mut resume = ResumeMap::new(plan.chunk_count());
        if prepared.existed && prepared.random_access {
            for index in inner
                .repository
                .completed_chunks(transfer_id, &entry.item_id)?
            {
                if index < plan.chunk_count() {
                    resume.mark_complete(index)?;
                }
            }
        }
        resumed_bytes = resumed_bytes.saturating_add(resume.completed_bytes(&plan)?);
        remaining_chunks = remaining_chunks
            .checked_add(u64::from(
                plan.chunk_count() - resume_count(&resume, plan.chunk_count()),
            ))
            .ok_or(LanError::TooManyChunks)?;
        let temporary_ref = format!("android-saf:{}", prepared.temporary_uri);
        let final_ref = format!("android-saf:{tree_uri}/{}", prepared.final_path);
        inner.repository.save_item(&StoredTransferItem {
            id: entry.item_id.clone(),
            transfer_id,
            relative_path: entry.relative_path.clone(),
            kind: EntryKind::File,
            size: entry.size,
            modified_unix_ms: entry.modified_unix_ms,
            source_revision: Some(entry.source_revision.clone()),
            temporary_ref: Some(temporary_ref.clone()),
            final_ref: Some(final_ref.clone()),
        })?;
        files.insert(
            entry.item_id.clone(),
            IncomingFile {
                entry,
                plan,
                resume: Mutex::new(resume),
                write_lock: tokio::sync::Mutex::new(()),
                target,
                temporary_path: PathBuf::from(temporary_ref),
                final_path: PathBuf::from(final_ref),
                android_temporary_uri: Some(prepared.temporary_uri.clone()),
                android_final_name: Some(prepared.final_name.clone()),
                random_access: prepared.random_access,
            },
        );
    }
    if resumed_bytes != 0 {
        inner.update_completed_bytes(transfer_id, resumed_bytes)?;
    }
    Ok(IncomingContext {
        transfer_id,
        peer_fingerprint,
        token: Mutex::new([0; 32]),
        files,
        directories: Vec::new(),
        temporary_root: PathBuf::new(),
        remaining_chunks: AtomicU64::new(remaining_chunks),
        completed: Notify::new(),
        repository: inner.repository.clone(),
        android_saf: true,
    })
}

fn resume_count(resume: &ResumeMap, chunk_count: u32) -> u32 {
    (0..chunk_count)
        .filter(|index| resume.contains(*index))
        .count() as u32
}

fn finalize_incoming(context: &IncomingContext) -> Result<(), LanError> {
    #[cfg(target_os = "android")]
    if context.android_saf {
        for file in context.files.values() {
            if !file
                .resume
                .lock()
                .map_err(|_| LanError::LockPoisoned)?
                .is_complete()
            {
                return Err(LanError::IncompleteTransfer(
                    file.entry.relative_path.clone(),
                ));
            }
            file.target.sync_all()?;
            crate::android_storage::finalize_target(
                file.android_temporary_uri
                    .as_deref()
                    .ok_or(LanError::MissingFinalPath)?,
                file.android_final_name
                    .as_deref()
                    .ok_or(LanError::MissingFinalPath)?,
            )?;
        }
        context
            .repository
            .clear_completed_chunks(context.transfer_id)?;
        return Ok(());
    }
    let mut directories = context.directories.clone();
    directories.sort_by_key(|path| path.components().count());
    for directory in directories {
        fs::create_dir_all(directory)?;
    }
    for file in context.files.values() {
        if !file
            .resume
            .lock()
            .map_err(|_| LanError::LockPoisoned)?
            .is_complete()
        {
            return Err(LanError::IncompleteTransfer(
                file.entry.relative_path.clone(),
            ));
        }
        file.target.sync_all()?;
        if let Some(parent) = file.final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if file.final_path.exists() {
            return Err(LanError::TargetAppeared(file.final_path.clone()));
        }
        fs::rename(&file.temporary_path, &file.final_path)?;
    }
    context
        .repository
        .clear_completed_chunks(context.transfer_id)?;
    let _ = fs::remove_dir_all(&context.temporary_root);
    Ok(())
}

fn allocate_final_paths(
    receive_directory: &Path,
    entries: &[IncomingEntry],
) -> Result<HashMap<String, PathBuf>, LanError> {
    let mut roots = HashMap::<String, String>::new();
    let mut reserved = HashSet::new();
    let mut result = HashMap::with_capacity(entries.len());
    let mut sorted = entries.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    for entry in sorted {
        let segments = entry
            .relative_path
            .split('/')
            .map(sanitize_target_segment)
            .collect::<Vec<_>>();
        let source_root = entry
            .relative_path
            .split('/')
            .next()
            .ok_or(LanError::MissingFinalPath)?;
        let target_root = match roots.get(source_root) {
            Some(root) => root.clone(),
            None => {
                let unique = unique_target_name(receive_directory, &segments[0], &mut reserved);
                roots.insert(source_root.to_owned(), unique.clone());
                unique
            }
        };
        let mut path = receive_directory.join(target_root);
        for segment in segments.iter().skip(1) {
            path.push(segment);
        }
        result.insert(entry.item_id.clone(), path);
    }
    Ok(result)
}

fn unique_target_name(directory: &Path, requested: &str, reserved: &mut HashSet<String>) -> String {
    for index in 0_u32.. {
        let candidate = if index == 0 {
            requested.to_owned()
        } else {
            suffixed_name(requested, index)
        };
        let key = if cfg!(windows) {
            candidate.to_lowercase()
        } else {
            candidate.clone()
        };
        if !directory.join(&candidate).exists() && reserved.insert(key) {
            return candidate;
        }
    }
    unreachable!("u32 name suffix space exhausted")
}

fn suffixed_name(name: &str, index: u32) -> String {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => format!("{stem} ({index}).{extension}"),
        _ => format!("{name} ({index})"),
    }
}

fn sanitize_target_segment(segment: &str) -> String {
    // Reject directory traversal on all platforms.
    if segment == "." || segment == ".." {
        return "_".to_owned();
    }
    if !cfg!(windows) {
        return segment.to_owned();
    }
    let mut sanitized = segment
        .chars()
        .map(|character| {
            if character < ' '
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    sanitized = sanitized.trim_end_matches([' ', '.']).to_owned();
    if sanitized.is_empty() {
        sanitized.push('_');
    }
    let stem = sanitized
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        sanitized.push('_');
    }
    sanitized
}

async fn write_manifest<S>(
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

async fn read_manifest<S>(stream: &mut S, transfer_id: Uuid) -> Result<Vec<IncomingEntry>, LanError>
where
    S: AsyncRead + Unpin,
{
    let mut entries = Vec::new();
    let mut expected_page = 0_u32;
    loop {
        let page = expect_manifest(read_envelope(stream).await?)?;
        if page.transfer_id != transfer_id.to_string() || page.page_index != expected_page {
            return Err(LanError::InvalidManifestPage);
        }
        if entries.len().saturating_add(page.entries.len()) > MAX_MANIFEST_ENTRIES {
            return Err(LanError::TooManyItems);
        }
        for entry in page.entries {
            let kind = match wire::EntryKind::try_from(entry.kind) {
                Ok(wire::EntryKind::File) => EntryKind::File,
                Ok(wire::EntryKind::Directory) => EntryKind::Directory,
                _ => return Err(LanError::InvalidManifestKind),
            };
            entries.push(IncomingEntry {
                item_id: entry.item_id,
                relative_path: entry.relative_path,
                kind,
                size: entry.size,
                modified_unix_ms: entry.modified_unix_ms,
                source_revision: entry.source_revision,
            });
        }
        if page.is_last {
            break;
        }
        expected_page = expected_page
            .checked_add(1)
            .ok_or(LanError::InvalidManifestPage)?;
    }
    let unique_ids = entries
        .iter()
        .map(|entry| entry.item_id.as_str())
        .collect::<HashSet<_>>();
    if unique_ids.len() != entries.len() {
        return Err(LanError::DuplicateItemId);
    }
    Ok(entries)
}

fn hello_envelope(inner: &CoreInner) -> Result<wire::Envelope, CoreError> {
    Ok(envelope(wire::envelope::Payload::Hello(wire::Hello {
        device_id: inner.identity.device_id().to_owned(),
        display_name: inner.config()?.device_name,
        certificate_fingerprint: inner.identity.fingerprint().to_vec(),
        capabilities: if cfg!(target_os = "android") { 1 } else { 0 },
    })))
}

fn connection_open(
    kind: wire::ConnectionKind,
    transfer_id: Uuid,
    token: &[u8],
    channel_index: u32,
) -> wire::Envelope {
    envelope(wire::envelope::Payload::ConnectionOpen(
        wire::ConnectionOpen {
            kind: kind as i32,
            transfer_id: transfer_id.to_string(),
            transfer_token: token.to_vec(),
            channel_index,
        },
    ))
}

fn offer_envelope(transfer_id: Uuid, job: &OutgoingJob) -> wire::Envelope {
    envelope(wire::envelope::Payload::TransferOffer(
        wire::TransferOffer {
            transfer_id: transfer_id.to_string(),
            item_count: job.item_count,
            directory_count: job
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Directory)
                .count() as u32,
            total_bytes: job.total_bytes,
            top_level_names: job.top_level_names.clone(),
        },
    ))
}

fn pairing_confirmation(confirmed: bool, remember_peer: bool) -> wire::Envelope {
    envelope(wire::envelope::Payload::PairingConfirmation(
        wire::PairingConfirmation {
            confirmed,
            remember_peer,
        },
    ))
}

fn decision_envelope(
    transfer_id: Uuid,
    accepted: bool,
    reason: &str,
    token: &[u8],
    channel_count: u32,
) -> wire::Envelope {
    envelope(wire::envelope::Payload::OfferDecision(
        wire::OfferDecision {
            transfer_id: transfer_id.to_string(),
            accepted,
            reason: reason.to_owned(),
            transfer_token: token.to_vec(),
            data_channel_count: channel_count,
        },
    ))
}

fn result_envelope(transfer_id: Uuid, completed: bool, error: &str) -> wire::Envelope {
    envelope(wire::envelope::Payload::TransferResult(
        wire::TransferResult {
            transfer_id: transfer_id.to_string(),
            completed,
            error: error.to_owned(),
        },
    ))
}

fn envelope(payload: wire::envelope::Payload) -> wire::Envelope {
    wire::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        payload: Some(payload),
    }
}

macro_rules! expect_payload {
    ($name:ident, $variant:ident, $type:ty) => {
        fn $name(envelope: wire::Envelope) -> Result<$type, LanError> {
            match envelope.payload {
                Some(wire::envelope::Payload::$variant(value)) => Ok(value),
                _ => Err(LanError::UnexpectedMessage(stringify!($variant))),
            }
        }
    };
}

expect_payload!(expect_connection_open, ConnectionOpen, wire::ConnectionOpen);
expect_payload!(expect_hello, Hello, wire::Hello);
expect_payload!(expect_offer, TransferOffer, wire::TransferOffer);
expect_payload!(expect_manifest, ManifestPage, wire::ManifestPage);
expect_payload!(
    expect_pairing,
    PairingConfirmation,
    wire::PairingConfirmation
);
expect_payload!(expect_decision, OfferDecision, wire::OfferDecision);
expect_payload!(expect_resume, ResumeState, wire::ResumeState);
expect_payload!(expect_result, TransferResult, wire::TransferResult);

fn validate_hello(hello: &wire::Hello, tls_fingerprint: &[u8; 32]) -> Result<(), LanError> {
    if hello.certificate_fingerprint.as_slice() != tls_fingerprint {
        return Err(LanError::CertificateFingerprintMismatch);
    }
    let expected_id = tls_fingerprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if hello.device_id != expected_id || hello.display_name.trim().is_empty() {
        return Err(LanError::InvalidHello);
    }
    Ok(())
}

fn tls_peer_fingerprint(
    certificates: Option<&[rustls::pki_types::CertificateDer<'static>]>,
) -> Result<[u8; 32], LanError> {
    let certificate = certificates
        .and_then(|values| values.first())
        .ok_or(LanError::MissingPeerCertificate)?;
    Ok(certificate_fingerprint(certificate.as_ref()))
}

fn pairing_code(inner: &CoreInner, remote: &[u8; 32], remote_id: &str) -> String {
    let mut ids = [inner.identity.device_id(), remote_id];
    ids.sort_unstable();
    derive_pairing_code(
        &inner.identity.fingerprint(),
        remote,
        format!("{}|{}", ids[0], ids[1]).as_bytes(),
    )
}

fn transfer_token(
    transfer_id: Uuid,
    local_fingerprint: &[u8; 32],
    remote_fingerprint: &[u8; 32],
) -> [u8; 32] {
    let nonce = Uuid::new_v4();
    *blake3::hash(
        &[
            transfer_id.as_bytes().as_slice(),
            nonce.as_bytes().as_slice(),
            local_fingerprint.as_slice(),
            remote_fingerprint.as_slice(),
        ]
        .concat(),
    )
    .as_bytes()
}

fn device_kind(capabilities: u64) -> String {
    if capabilities & 1 != 0 {
        "phone".to_owned()
    } else {
        "computer".to_owned()
    }
}

fn safe_manifest_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|character| {
            if matches!(character, '/' | '\\' | '\0') {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "未命名项目".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn unique_manifest_root(mut name: String, seen: &mut HashSet<String>) -> String {
    let original = name.clone();
    let mut index = 1_u32;
    while !seen.insert(name.to_lowercase()) {
        name = suffixed_name(&original, index);
        index += 1;
    }
    name
}

fn source_revision(metadata: &fs::Metadata) -> String {
    format!("{}:{}", metadata.len(), modified_unix_ms(metadata))
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

fn modified_unix_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[derive(Debug, Error)]
pub enum LanError {
    #[error("网络读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS 失败: {0}")]
    Tls(#[from] rustls::Error),
    #[error("设备身份失败: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("协议失败: {0}")]
    Protocol(#[from] protocol::ProtocolError),
    #[error("数据通道失败: {0}")]
    Transfer(#[from] transfer::TransferIoError),
    #[error("文件校验失败: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("数据块计划失败: {0}")]
    Chunk(#[from] crate::chunk::ChunkError),
    #[error("清单失败: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error("持久化失败: {0}")]
    Repository(#[from] RepositoryError),
    #[error("核心失败: {0}")]
    Core(String),
    #[error("后台任务失败: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("mDNS 失败: {0}")]
    Mdns(#[from] mdns_sd::Error),
    #[error("没有可传输的文件或文件夹")]
    NoTransferableSources,
    #[error("文件条目数量过多")]
    TooManyItems,
    #[error("数据块数量过多")]
    TooManyChunks,
    #[error("文件总大小溢出")]
    TotalSizeOverflow,
    #[error("无效的设备地址: {0}")]
    InvalidPeerAddress(String),
    #[error("无效的任务 ID: {0}")]
    InvalidTransferId(String),
    #[error("收到非预期协议消息: {0}")]
    UnexpectedMessage(&'static str),
    #[error("对端没有提供 TLS 证书")]
    MissingPeerCertificate,
    #[error("Hello 中的证书指纹与 TLS 证书不一致")]
    CertificateFingerprintMismatch,
    #[error("Hello 消息中的设备身份无效")]
    InvalidHello,
    #[error("配对已拒绝")]
    PairingRejected,
    #[error("接收方拒绝了任务: {0}")]
    OfferRejected(String),
    #[error("传输令牌无效")]
    InvalidTransferToken,
    #[error("远端传输失败: {0}")]
    RemoteTransferFailed(String),
    #[error("源文件路径缺失")]
    MissingSourcePath,
    #[error("重启后的任务没有可恢复的源文件清单")]
    MissingPersistedSources,
    #[error("重启后无法重新打开源文件，请重新选择: {0}")]
    MissingPersistedSource(String),
    #[error("平台文件句柄无效: {0}")]
    InvalidPlatformToken(String),
    #[error("发送期间源文件发生变化: {0}")]
    SourceChanged(String),
    #[error("内核已停止")]
    Stopped,
    #[error("任务已取消")]
    Cancelled,
    #[error("网络状态锁已损坏")]
    LockPoisoned,
    #[error("未知传入任务: {0}")]
    UnknownIncomingTransfer(Uuid),
    #[error("未知传入文件: {0}")]
    UnknownIncomingItem(String),
    #[error("数据块与清单计划不匹配")]
    ChunkPlanMismatch,
    #[error("BLAKE3 长度无效: {0}")]
    InvalidHashLength(usize),
    #[error("清单分页顺序无效")]
    InvalidManifestPage,
    #[error("清单项目类型无效")]
    InvalidManifestKind,
    #[error("清单包含重复项目 ID")]
    DuplicateItemId,
    #[error("无法生成目标路径")]
    MissingFinalPath,
    #[error("任务尚有文件未完整接收: {0}")]
    IncompleteTransfer(String),
    #[error("校验期间目标路径被其他文件占用: {0}")]
    TargetAppeared(PathBuf),
    #[cfg(target_os = "android")]
    #[error("Android SAF 失败: {0}")]
    AndroidStorage(#[from] crate::android_storage::AndroidStorageError),
}

impl LanError {
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::Tls(_)
                | Self::Stopped
                | Self::RemoteTransferFailed(_)
                | Self::SourceChanged(_)
        )
    }
}

impl From<CoreError> for LanError {
    fn from(error: CoreError) -> Self {
        Self::Core(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{sanitize_target_segment, suffixed_name, unique_target_name};

    #[test]
    fn target_names_are_deterministic_and_never_overwrite() {
        assert_eq!(suffixed_name("报告.pdf", 2), "报告 (2).pdf");
        if cfg!(windows) {
            assert_eq!(sanitize_target_segment("CON.txt"), "CON.txt_");
            assert_eq!(sanitize_target_segment("a:b?.txt"), "a_b_.txt");
        }
        let directory = tempfile::tempdir().expect("target directory");
        std::fs::write(directory.path().join("报告.pdf"), b"existing").expect("fixture");
        let mut reserved = HashSet::new();
        assert_eq!(
            unique_target_name(directory.path(), "报告.pdf", &mut reserved),
            "报告 (1).pdf"
        );
        assert_eq!(
            unique_target_name(directory.path(), "报告.pdf", &mut reserved),
            "报告 (2).pdf"
        );
    }

    #[test]
    fn sanitize_target_segment_rejects_directory_traversal() {
        assert_eq!(sanitize_target_segment("."), "_");
        assert_eq!(sanitize_target_segment(".."), "_");
        // Normal names pass through.
        assert_eq!(sanitize_target_segment("hello"), "hello");
        assert_eq!(sanitize_target_segment("hello.txt"), "hello.txt");
    }
}
