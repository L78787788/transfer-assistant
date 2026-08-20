use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
};

use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;
use uuid::Uuid;

use crate::{
    chunk::MAX_DATA_CHANNELS,
    core::{CoreError, CoreInner, OfferAnswer, TransferOffer, now_unix_ms},
    incoming::{self, IncomingContext},
    manifest::{ManifestEntry, TransferManifest},
    mdns::MdnsHandle,
    model::{TransferDirection, TransferSnapshot, TransferState},
    persistence::RepositoryError,
    protocol::{self, read_envelope, wire, write_envelope},
    transfer::{self},
    wire::{
        control_envelope, decision_envelope, device_kind, expect_connection_open, expect_hello,
        expect_offer, expect_pairing, hello_envelope, pairing_code, pairing_confirmation,
        result_envelope, tls_peer_fingerprint, transfer_token, validate_hello,
    },
};

const DEFAULT_PORT: u16 = 53_317;

// 发送端接口从 outgoing 模块重导出，保持 core 的调用路径不变。
pub(crate) use crate::outgoing::{
    JobControl, OutgoingJob, delete_partial_data, persist_outgoing_items, restore_outgoing_job,
    run_outgoing, scan_sources,
};

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

    pub(crate) fn refresh_peers(&self) {
        if let Some(mdns) = &self.mdns {
            mdns.refresh();
        }
    }

    pub(crate) fn shutdown(mut self) {
        self.task.abort();
        if let Some(mdns) = self.mdns.take() {
            mdns.shutdown();
        }
    }
}

#[derive(Default)]
pub(crate) struct NetworkState {
    pub(crate) incoming: Mutex<HashMap<Uuid, Arc<IncomingContext>>>,
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
                        log::info!("接收 {transfer_id}: 数据通道接入（来自 {remote_address}）");
                        incoming::handle_incoming_data(
                            &mut tls,
                            handler_inner.clone(),
                            connection_state,
                            transfer_id,
                            open.transfer_token,
                            open.channel_index,
                            peer_fingerprint,
                        )
                        .await
                    }
                    _ => Err(LanError::UnexpectedMessage("connection kind")),
                };
                let is_cancelled = !handler_inner.shutdown.is_cancelled()
                    && (outcome.as_ref().err().is_some_and(|e| e.is_cancelled())
                        || handler_inner.transfer_is_cancelled(transfer_id));
                if let Err(error) = &outcome {
                    if handler_inner.shutdown.is_cancelled() {
                        log::info!("核心系统正在关机，保持接收任务状态以供重启恢复 {transfer_id}");
                    } else if is_cancelled {
                        let _ = handler_inner.cancel_transfer_with_error(
                            transfer_id,
                            match error {
                                LanError::RemoteCancelled(reason) => reason.clone(),
                                _ => "已取消".to_owned(),
                            },
                        );
                        let _ = delete_partial_data(&handler_inner.repository, transfer_id);
                        return Err(LanError::Cancelled);
                    } else if error.is_retryable() {
                        let _ = handler_inner.interrupt_transfer(transfer_id, error.to_string());
                    } else {
                        let _ = handler_inner.fail_transfer(transfer_id, error.to_string());
                    }
                }
                outcome
            }
            .await;
            if let Err(error) = result {
                if !error.is_cancelled() {
                    log::warn!("局域网连接处理失败: {error}");
                } else {
                    log::info!("局域网传输已取消: {error}");
                }
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
    let entries = incoming::read_manifest(tls, transfer_id).await?;
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

    let context = Arc::new(incoming::prepare_incoming(
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
    inner.register_incoming_control(transfer_id, context.control.clone())?;
    let _context_guard = IncomingContextGuard {
        inner: inner.clone(),
        state: state.clone(),
        transfer_id,
        context: context.clone(),
    };
    context.configure_channels(channel_count)?;
    write_envelope(
        tls,
        &decision_envelope(transfer_id, true, "", &token, channel_count),
    )
    .await?;
    write_envelope(tls, &context.resume_envelope()?).await?;
    log::info!(
        "接收 {transfer_id}: 已接受并进入传输（{} 个剩余块，{} 通道）",
        context.remaining_chunks.load(Ordering::Acquire),
        channel_count
    );
    inner.transition_transfer(transfer_id, TransferState::Transferring)?;

    let mut control_rx = context.control.register_channel();

    if context.remaining_chunks.load(Ordering::Acquire) != 0 {
        loop {
            if context.remaining_chunks.load(Ordering::Acquire) == 0 {
                break;
            }
            if context.control.is_cancelled() || inner.transfer_is_cancelled(transfer_id) {
                let _ = write_envelope(
                    tls,
                    &control_envelope(transfer_id, wire::ControlAction::Cancel),
                )
                .await;
                return Err(LanError::Cancelled);
            }
            if inner.transfer_is_failed(transfer_id) {
                // 数据通道已失败（accept_loop 已 fail_transfer），主动告知发送端，避免其无限等待。
                let _ = write_envelope(
                    tls,
                    &result_envelope(transfer_id, false, "数据通道传输失败，接收中止"),
                )
                .await;
                return Err(LanError::Core("数据通道传输失败，接收中止".to_owned()));
            }
            tokio::select! {
                Some(action) = control_rx.recv() => {
                    log::info!("接收 {transfer_id}: 向发送端发送控制指令: {action:?}");
                    let _ = write_envelope(tls, &control_envelope(transfer_id, action)).await;
                    if action == wire::ControlAction::Cancel {
                        return Err(LanError::Cancelled);
                    }
                }
                envelope_res = read_envelope(tls) => {
                    let envelope = match envelope_res {
                        Ok(env) => env,
                        Err(err) => {
                            if context.control.is_cancelled() {
                                return Err(LanError::Cancelled);
                            }
                            if inner.transfer_is_cancelled(transfer_id) {
                                return Err(LanError::RemoteCancelled("对方已取消传输".to_owned()));
                            }
                            return Err(err.into());
                        }
                    };
                    if let Some(wire::envelope::Payload::TransferControl(ctrl)) = envelope.payload {
                        match wire::ControlAction::try_from(ctrl.action) {
                            Ok(wire::ControlAction::Pause) => {
                                log::info!("接收 {transfer_id}: 收到发送端暂停指令");
                                inner.pause_from_remote(transfer_id)?;
                            }
                            Ok(wire::ControlAction::Resume) => {
                                log::info!("接收 {transfer_id}: 收到发送端继续指令");
                                inner.resume_from_remote(transfer_id)?;
                            }
                            Ok(wire::ControlAction::Cancel) => {
                                log::info!("接收 {transfer_id}: 收到发送端取消指令");
                                inner.cancel_from_remote(transfer_id, "对方已取消传输")?;
                                return Err(LanError::RemoteCancelled("对方已取消传输".to_owned()));
                            }
                            _ => {}
                        }
                    }
                }
                () = context.completed.notified() => {
                    if context.remaining_chunks.load(Ordering::Acquire) == 0 {
                        break;
                    }
                }
                () = inner.shutdown.cancelled() => return Err(LanError::Stopped),
            }
        }
    }
    inner.transition_transfer(transfer_id, TransferState::Verifying)?;
    if let Err(error) = incoming::finalize_incoming(&context) {
        // finalize 失败时主动告知发送端失败结果，避免发送端无限等待。
        let _ = write_envelope(
            tls,
            &result_envelope(transfer_id, false, &error.to_string()),
        )
        .await;
        return Err(error);
    }
    inner.transition_transfer(transfer_id, TransferState::Completed)?;
    write_envelope(tls, &result_envelope(transfer_id, true, "")).await?;
    log::info!("接收 {transfer_id}: 任务完成");
    Ok(())
}

struct IncomingContextGuard {
    inner: Arc<CoreInner>,
    state: Arc<NetworkState>,
    transfer_id: Uuid,
    context: Arc<IncomingContext>,
}

impl Drop for IncomingContextGuard {
    fn drop(&mut self) {
        let _ = self.inner.unregister_incoming_control(self.transfer_id);
        if let Ok(mut incoming) = self.state.incoming.lock()
            && incoming
                .get(&self.transfer_id)
                .is_some_and(|current| Arc::ptr_eq(current, &self.context))
        {
            incoming.remove(&self.transfer_id);
        }
    }
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
    #[error("对方已取消传输: {0}")]
    RemoteCancelled(String),
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
    #[error("数据通道 {0} 提前结束")]
    IncompleteDataChannel(u32),
    #[error("无效的数据通道")]
    InvalidDataChannel,
    #[error("数据通道 {0} 发送了超出计划的数据")]
    UnexpectedDataChannelEnd(u32),
    #[error("校验期间目标路径被其他文件占用: {0}")]
    TargetAppeared(PathBuf),
    #[cfg(target_os = "android")]
    #[error("Android SAF 失败: {0}")]
    AndroidStorage(#[from] crate::android_storage::AndroidStorageError),
}

impl LanError {
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled | Self::RemoteCancelled(_))
    }

    pub(crate) fn is_retryable(&self) -> bool {
        if self.is_cancelled() {
            return false;
        }
        matches!(
            self,
            Self::Io(_)
                | Self::Tls(_)
                | Self::Protocol(crate::protocol::ProtocolError::Io(_))
                | Self::Stopped
                | Self::RemoteTransferFailed(_)
                | Self::SourceChanged(_)
                | Self::IncompleteDataChannel(_)
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

    use crate::path_safety::{sanitize_target_segment, suffixed_name, unique_target_name};

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
