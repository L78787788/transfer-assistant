use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite},
    sync::Notify,
};
use uuid::Uuid;

use crate::{
    chunk::{ChunkPlan, DEFAULT_CHUNK_SIZE, ResumeMap},
    core::CoreInner,
    lan::{LanError, NetworkState},
    manifest::EntryKind,
    path_safety::sanitize_target_segment,
    persistence::StoredTransferItem,
    protocol::{read_envelope, wire},
    storage::{VerifiedChunk, write_sequential_verified_chunk, write_verified_chunk},
    transfer::{read_header, validate_header},
    wire::{envelope, expect_manifest},
};

#[derive(Clone)]
pub(crate) struct IncomingEntry {
    pub(crate) item_id: String,
    pub(crate) relative_path: String,
    pub(crate) kind: EntryKind,
    pub(crate) size: u64,
    pub(crate) modified_unix_ms: i64,
    pub(crate) source_revision: String,
}

pub(crate) struct IncomingContext {
    pub(crate) transfer_id: Uuid,
    pub(crate) peer_fingerprint: [u8; 32],
    token: Mutex<[u8; 32]>,
    pub(crate) files: HashMap<String, IncomingFile>,
    pub(crate) directories: Vec<PathBuf>,
    pub(crate) temporary_root: PathBuf,
    pub(crate) remaining_chunks: AtomicU64,
    pub(crate) active_channels: AtomicU32,
    pub(crate) completed: Notify,
    pub(crate) repository: Arc<crate::persistence::TransferRepository>,
    pub(crate) control: Arc<crate::outgoing::JobControl>,
    #[cfg(target_os = "android")]
    pub(crate) android_saf: bool,
}

pub(crate) struct IncomingFile {
    pub(crate) entry: IncomingEntry,
    pub(crate) plan: ChunkPlan,
    pub(crate) resume: Mutex<ResumeMap>,
    pub(crate) write_lock: tokio::sync::Mutex<()>,
    pub(crate) target: File,
    pub(crate) temporary_path: PathBuf,
    pub(crate) final_path: PathBuf,
    #[cfg(target_os = "android")]
    pub(crate) android_temporary_uri: Option<String>,
    #[cfg(target_os = "android")]
    pub(crate) android_final_name: Option<String>,
    pub(crate) random_access: bool,
}

impl IncomingContext {
    pub(crate) fn set_token(&self, token: [u8; 32]) {
        if let Ok(mut stored) = self.token.lock() {
            *stored = token;
        }
    }

    pub(crate) fn token(&self) -> [u8; 32] {
        self.token.lock().map(|token| *token).unwrap_or_default()
    }

    pub(crate) fn configure_channels(&self, _channel_count: u32) -> Result<(), LanError> {
        Ok(())
    }

    pub(crate) fn validate_channel(&self, channel_index: u32) -> Result<(), LanError> {
        let index = usize::try_from(channel_index).map_err(|_| LanError::InvalidDataChannel)?;
        if index >= crate::chunk::MAX_DATA_CHANNELS {
            return Err(LanError::InvalidDataChannel);
        }
        Ok(())
    }

    pub(crate) fn resume_envelope(&self) -> Result<wire::Envelope, LanError> {
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

pub(crate) async fn handle_incoming_data<S>(
    tls: &mut S,
    inner: Arc<CoreInner>,
    state: Arc<NetworkState>,
    transfer_id: Uuid,
    token: Vec<u8>,
    channel_index: u32,
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
    context.validate_channel(channel_index)?;
    context.active_channels.fetch_add(1, Ordering::SeqCst);
    struct ChannelGuard(Arc<IncomingContext>);
    impl Drop for ChannelGuard {
        fn drop(&mut self) {
            if self.0.active_channels.fetch_sub(1, Ordering::SeqCst) == 1 {
                self.0.completed.notify_waiters();
            }
        }
    }
    let _channel_guard = ChannelGuard(context.clone());
    loop {
        context.control.checkpoint(&inner).await?;
        let header = match read_header(tls).await {
            Ok(header) => header,
            Err(crate::transfer::TransferIoError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                if context.control.is_cancelled() || inner.transfer_is_cancelled(transfer_id) {
                    return Err(LanError::Cancelled);
                }
                // 对端该通道工作窃取任务发送完毕或关闭
                break;
            }
            Err(error) => {
                if context.control.is_cancelled() || inner.transfer_is_cancelled(transfer_id) {
                    return Err(LanError::Cancelled);
                }
                return Err(error.into());
            }
        };
        let file = context
            .files
            .get(&header.item_id)
            .ok_or_else(|| LanError::UnknownIncomingItem(header.item_id.clone()))?;
        if inner.transfer_is_cancelled(transfer_id) || context.control.is_cancelled() {
            return Err(LanError::Cancelled);
        }
        context.control.checkpoint(&inner).await?;
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
        if let Err(err) = tls.read_exact(&mut bytes).await {
            if context.control.is_cancelled() || inner.transfer_is_cancelled(transfer_id) {
                return Err(LanError::Cancelled);
            }
            return Err(err.into());
        }
        let hash: [u8; 32] = header
            .blake3_hash
            .try_into()
            .map_err(|value: Vec<u8>| LanError::InvalidHashLength(value.len()))?;
        let chunk = VerifiedChunk {
            spec: expected,
            blake3_hash: hash,
            bytes,
        };

        let target = file.target.try_clone()?;
        let written_length = u64::from(chunk.spec.length);
        let random_access = file.random_access;

        if random_access {
            // 支持定位 I/O：直接并发落盘，不争抢排他写锁
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

            tokio::task::spawn_blocking(move || write_verified_chunk(&target, &chunk)).await??;

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
        } else {
            // 不支持随机访问：使用 write_lock 保证顺序写安全
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

            tokio::task::spawn_blocking(move || write_sequential_verified_chunk(&target, &chunk))
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
    }
    Ok(())
}

pub(crate) fn prepare_incoming(
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
        active_channels: AtomicU32::new(0),
        completed: Notify::new(),
        repository: inner.repository.clone(),
        control: Arc::new(crate::outgoing::JobControl::new()),
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
        active_channels: AtomicU32::new(0),
        completed: Notify::new(),
        repository: inner.repository.clone(),
        control: Arc::new(crate::outgoing::JobControl::new()),
        android_saf: true,
    })
}

pub(crate) fn resume_count(resume: &ResumeMap, chunk_count: u32) -> u32 {
    (0..chunk_count)
        .filter(|index| resume.contains(*index))
        .count() as u32
}

pub(crate) fn finalize_incoming(context: &IncomingContext) -> Result<(), LanError> {
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
            // SAF 的 FUSE 文件描述符上 fsync 可能长时间阻塞甚至失败，
            // 因此 Android 由文档提供程序在重命名时完成落盘。
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
    receive_directory: &std::path::Path,
    entries: &[IncomingEntry],
) -> Result<HashMap<String, PathBuf>, LanError> {
    let mut roots = HashMap::<String, String>::new();
    let mut reserved = std::collections::HashSet::new();
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
                let unique = crate::path_safety::unique_target_name(
                    receive_directory,
                    &segments[0],
                    &mut reserved,
                );
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

pub(crate) async fn read_manifest<S>(
    stream: &mut S,
    transfer_id: Uuid,
) -> Result<Vec<IncomingEntry>, LanError>
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
        .collect::<std::collections::HashSet<_>>();
    if unique_ids.len() != entries.len() {
        return Err(LanError::DuplicateItemId);
    }
    Ok(entries)
}
