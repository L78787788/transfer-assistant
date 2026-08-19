use std::{
    collections::{HashMap, VecDeque},
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    runtime::Runtime,
    sync::{Semaphore, oneshot},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    identity::{DeviceIdentity, IdentityError},
    lan::{self, JobControl, LanError, NetworkHandle, OutgoingJob},
    model::{
        LifecycleError, TransferCommand, TransferDirection, TransferSnapshot, TransferState,
        TrustedPeer,
    },
    persistence::{RepositoryError, TransferRepository},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    pub data_directory: PathBuf,
    pub device_name: String,
    pub receive_directory: PathBuf,
    pub background_receive: bool,
    pub auto_accept_trusted: bool,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    #[serde(default = "default_theme_style")]
    pub theme_style: String,
    #[serde(skip)]
    pub identity_wrap_key: Option<Vec<u8>>,
}

fn default_theme_mode() -> String {
    "system".to_owned()
}

fn default_theme_style() -> String {
    "radar".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHandle {
    pub token: String,
    #[serde(default)]
    pub persistent_token: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub is_directory: bool,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub modified_unix_ms: Option<i64>,
    #[serde(default)]
    pub random_access: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerSummary {
    pub id: String,
    pub name: String,
    pub address: String,
    pub device_kind: String,
    pub trusted: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferOffer {
    pub id: String,
    pub peer_name: String,
    pub item_count: u32,
    pub total_bytes: u64,
    pub top_level_names: Vec<String>,
    pub pairing_code: Option<String>,
    pub direction: TransferDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreEvent {
    Ready,
    SettingsLoaded {
        device_name: String,
        receive_directory: String,
        background_receive: bool,
        auto_accept_trusted: bool,
        theme_mode: String,
        theme_style: String,
    },
    PeersChanged {
        peers: Vec<PeerSummary>,
    },
    TransfersChanged {
        transfers: Vec<TransferSnapshot>,
    },
    IncomingOffer {
        offer: TransferOffer,
    },
    Failure {
        message: String,
    },
}

pub struct TransferCore {
    pub(crate) inner: Arc<CoreInner>,
    runtime: Arc<Runtime>,
    network: Mutex<Option<NetworkHandle>>,
    stopped: AtomicBool,
}

pub(crate) struct CoreInner {
    pub(crate) repository: Arc<TransferRepository>,
    pub(crate) identity: Arc<DeviceIdentity>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) active_tasks: Arc<Semaphore>,
    state: Mutex<CoreState>,
}

struct CoreState {
    config: CoreConfig,
    peers: HashMap<String, PeerSummary>,
    transfers: HashMap<Uuid, TransferSnapshot>,
    events: VecDeque<CoreEvent>,
    pending_answers: HashMap<Uuid, oneshot::Sender<OfferAnswer>>,
    outgoing_jobs: HashMap<Uuid, StoredOutgoingJob>,
    incoming_controls: HashMap<Uuid, Arc<JobControl>>,
    progress_samples: HashMap<Uuid, ProgressSample>,
}

struct ProgressSample {
    at: Instant,
    completed_bytes: u64,
    last_saved_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSettings {
    device_name: String,
    receive_directory: PathBuf,
    background_receive: bool,
    auto_accept_trusted: bool,
    #[serde(default = "default_theme_mode")]
    theme_mode: String,
    #[serde(default = "default_theme_style")]
    theme_style: String,
}

struct StoredOutgoingJob {
    peer: PeerSummary,
    job: Arc<OutgoingJob>,
    control: Arc<JobControl>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OfferAnswer {
    pub(crate) accept: bool,
    pub(crate) remember_peer: bool,
}

impl TransferCore {
    pub fn initialize(mut config: CoreConfig) -> Result<Self, CoreError> {
        validate_config(&config)?;
        fs::create_dir_all(&config.data_directory)?;
        ensure_receive_directory(&config.receive_directory)?;

        let repository = Arc::new(TransferRepository::open(
            config.data_directory.join("transfers.db"),
        )?);
        if let Some(saved) = repository.setting::<PersistedSettings>("core_settings")? {
            config.device_name = saved.device_name;
            config.receive_directory = saved.receive_directory;
            config.background_receive = saved.background_receive;
            config.auto_accept_trusted = saved.auto_accept_trusted;
            config.theme_mode = saved.theme_mode;
            config.theme_style = saved.theme_style;
            validate_config(&config)?;
            ensure_receive_directory(&config.receive_directory)?;
        }
        let identity = Arc::new(DeviceIdentity::load_or_generate(
            &config.data_directory.join("device-identity.dat"),
            config.identity_wrap_key.as_deref(),
        )?);
        let mut restored = repository.list_transfers()?;
        for transfer in &mut restored {
            if !matches!(
                transfer.state,
                TransferState::Completed | TransferState::Cancelled | TransferState::Failed
            ) {
                transfer.state = TransferState::Interrupted;
                transfer.bytes_per_second = 0;
                transfer.error = Some("应用曾退出，可重试以继续缺失的数据块".to_owned());
                transfer.updated_unix_ms = now_unix_ms();
                repository.save_transfer(transfer)?;
            }
        }
        let transfers = restored
            .into_iter()
            .map(|transfer| (transfer.id, transfer))
            .collect();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("transassist-core")
                .worker_threads(4)
                .build()?,
        );
        let inner = Arc::new(CoreInner {
            repository,
            identity,
            shutdown: CancellationToken::new(),
            active_tasks: Arc::new(Semaphore::new(2)),
            state: Mutex::new(CoreState {
                config,
                peers: HashMap::new(),
                transfers,
                events: VecDeque::new(),
                pending_answers: HashMap::new(),
                outgoing_jobs: HashMap::new(),
                incoming_controls: HashMap::new(),
                progress_samples: HashMap::new(),
            }),
        });
        let network = runtime.block_on(NetworkHandle::start(inner.clone()))?;
        let effective = inner.config()?;
        inner.queue_event(CoreEvent::SettingsLoaded {
            device_name: effective.device_name,
            receive_directory: effective.receive_directory.to_string_lossy().into_owned(),
            background_receive: effective.background_receive,
            auto_accept_trusted: effective.auto_accept_trusted,
            theme_mode: effective.theme_mode,
            theme_style: effective.theme_style,
        })?;
        inner.queue_event(CoreEvent::Ready)?;

        Ok(Self {
            inner,
            runtime,
            network: Mutex::new(Some(network)),
            stopped: AtomicBool::new(false),
        })
    }

    pub fn next_event(&self) -> Option<CoreEvent> {
        self.inner.state.lock().ok()?.events.pop_front()
    }

    pub fn refresh_peers(&self) -> Result<(), CoreError> {
        self.ensure_running()?;
        if let Some(network) = self
            .network
            .lock()
            .map_err(|_| CoreError::LockPoisoned)?
            .as_ref()
        {
            network.refresh_peers();
        }
        self.inner.queue_peers_changed()
    }

    pub fn connect_address(&self, address: &str) -> Result<String, CoreError> {
        self.ensure_running()?;
        let address = normalize_address(address)?;
        let id = format!("manual:{address}");
        let peer = PeerSummary {
            id: id.clone(),
            name: address.clone(),
            address,
            device_kind: "computer".to_owned(),
            trusted: false,
            online: true,
        };
        self.inner.upsert_peer(peer)?;
        Ok(id)
    }

    pub fn send(&self, peer_id: &str, sources: Vec<SourceHandle>) -> Result<Uuid, CoreError> {
        self.ensure_running()?;
        if sources.is_empty() {
            return Err(CoreError::NoSources);
        }
        let job = Arc::new(lan::scan_sources(&sources)?);
        let peer = self
            .inner
            .peer(peer_id)?
            .ok_or_else(|| CoreError::UnknownPeer(peer_id.to_owned()))?;
        let transfer = TransferSnapshot::new_outgoing(
            peer.id.clone(),
            peer.name.clone(),
            job.item_count,
            job.total_bytes,
            now_unix_ms(),
        );
        let id = transfer.id;
        self.inner.repository.save_transfer(&transfer)?;
        lan::persist_outgoing_items(&self.inner.repository, id, &job)?;
        let control = Arc::new(JobControl::new());
        {
            let mut state = self.inner.lock()?;
            state.transfers.insert(id, transfer);
            state.outgoing_jobs.insert(
                id,
                StoredOutgoingJob {
                    peer: peer.clone(),
                    job: job.clone(),
                    control: control.clone(),
                },
            );
            queue_transfers_changed(&mut state);
        }
        self.spawn_outgoing(id, peer, job, control);
        Ok(id)
    }

    pub fn answer_offer(
        &self,
        offer_id: Uuid,
        accept: bool,
        remember_peer: bool,
    ) -> Result<(), CoreError> {
        self.ensure_running()?;
        let sender = self
            .inner
            .lock()?
            .pending_answers
            .remove(&offer_id)
            .ok_or(CoreError::UnknownOffer(offer_id))?;
        sender
            .send(OfferAnswer {
                accept,
                remember_peer,
            })
            .map_err(|_| CoreError::OfferExpired(offer_id))
    }

    pub fn command_transfer(
        &self,
        transfer_id: Uuid,
        command: TransferCommand,
    ) -> Result<(), CoreError> {
        self.ensure_running()?;
        if command == TransferCommand::Retry {
            self.restore_outgoing_job(transfer_id)?;
        }
        let retry;
        let cancel_incoming;
        {
            let mut state = self.inner.lock()?;
            let transfer = state
                .transfers
                .get_mut(&transfer_id)
                .ok_or(CoreError::UnknownTransfer(transfer_id))?;
            transfer.apply_command(command)?;
            if transfer.state != TransferState::Transferring {
                transfer.bytes_per_second = 0;
            }
            if command == TransferCommand::Cancel {
                transfer.error = Some("已取消".to_owned());
            }
            transfer.updated_unix_ms = now_unix_ms();
            cancel_incoming = command == TransferCommand::Cancel
                && transfer.direction == TransferDirection::Incoming;
            self.inner.repository.save_transfer(transfer)?;

            if let Some(control) = state.incoming_controls.get(&transfer_id) {
                match command {
                    TransferCommand::Pause => control.pause(),
                    TransferCommand::Resume => control.resume(),
                    TransferCommand::Cancel => control.cancel(),
                    TransferCommand::Retry => {}
                }
            }

            retry = match state.outgoing_jobs.get_mut(&transfer_id) {
                Some(stored) => {
                    match command {
                        TransferCommand::Pause => stored.control.pause(),
                        TransferCommand::Resume => stored.control.resume(),
                        TransferCommand::Cancel => stored.control.cancel(),
                        TransferCommand::Retry => {
                            stored.control = Arc::new(JobControl::new());
                        }
                    }
                    (command == TransferCommand::Retry).then(|| {
                        (
                            stored.peer.clone(),
                            stored.job.clone(),
                            stored.control.clone(),
                        )
                    })
                }
                None => None,
            };
            queue_transfers_changed(&mut state);
        }
        if let Some((peer, job, control)) = retry {
            self.spawn_outgoing(transfer_id, peer, job, control);
        }
        if cancel_incoming {
            lan::delete_partial_data(&self.inner.repository, transfer_id)?;
        }
        Ok(())
    }

    fn restore_outgoing_job(&self, transfer_id: Uuid) -> Result<(), CoreError> {
        let peer = {
            let state = self.inner.lock()?;
            if state.outgoing_jobs.contains_key(&transfer_id) {
                return Ok(());
            }
            let transfer = state
                .transfers
                .get(&transfer_id)
                .ok_or(CoreError::UnknownTransfer(transfer_id))?;
            if transfer.direction != TransferDirection::Outgoing {
                return Ok(());
            }
            state
                .peers
                .get(&transfer.peer_id)
                .cloned()
                .ok_or_else(|| CoreError::UnknownPeer(transfer.peer_id.clone()))?
        };
        let job = Arc::new(lan::restore_outgoing_job(
            &self.inner.repository,
            transfer_id,
        )?);
        let mut state = self.inner.lock()?;
        state
            .outgoing_jobs
            .entry(transfer_id)
            .or_insert_with(|| StoredOutgoingJob {
                peer,
                job,
                control: Arc::new(JobControl::new()),
            });
        Ok(())
    }

    pub fn update_settings(&self, config: CoreConfig) -> Result<(), CoreError> {
        self.ensure_running()?;
        validate_config(&config)?;
        ensure_receive_directory(&config.receive_directory)?;
        self.inner.repository.save_setting(
            "core_settings",
            &PersistedSettings {
                device_name: config.device_name.clone(),
                receive_directory: config.receive_directory.clone(),
                background_receive: config.background_receive,
                auto_accept_trusted: config.auto_accept_trusted,
                theme_mode: config.theme_mode.clone(),
                theme_style: config.theme_style.clone(),
            },
        )?;
        self.inner.lock()?.config = config;
        Ok(())
    }

    pub fn transfers(&self) -> Vec<TransferSnapshot> {
        self.inner
            .state
            .lock()
            .map(|state| sorted_transfers(&state))
            .unwrap_or_default()
    }

    pub fn listening_address(&self) -> Result<SocketAddr, CoreError> {
        self.network
            .lock()
            .map_err(|_| CoreError::LockPoisoned)?
            .as_ref()
            .map(NetworkHandle::address)
            .ok_or(CoreError::Stopped)
    }

    pub fn remove_trusted_peer(&self, peer_id: &str) -> Result<bool, CoreError> {
        self.ensure_running()?;
        let removed = self.inner.repository.remove_trusted_peer(peer_id)?;
        if removed {
            let mut state = self.inner.lock()?;
            state.peers.remove(peer_id);
            queue_peers_changed(&mut state);
        }
        Ok(removed)
    }

    pub fn list_history_files(
        &self,
    ) -> Result<Vec<crate::persistence::HistoryFileRecord>, CoreError> {
        self.ensure_running()?;
        Ok(self.inner.repository.list_history_files()?)
    }

    pub fn list_transfer_items(
        &self,
        transfer_id: Uuid,
    ) -> Result<Vec<crate::persistence::StoredTransferItem>, CoreError> {
        self.ensure_running()?;
        Ok(self.inner.repository.list_items(transfer_id)?)
    }

    pub fn clear_history(&self) -> Result<usize, CoreError> {
        self.ensure_running()?;
        let count = self.inner.repository.clear_history()?;
        let mut state = self.inner.lock()?;
        state.transfers.retain(|_, t| t.state.is_active());
        queue_transfers_changed(&mut state);
        Ok(count)
    }

    pub fn delete_history_transfer(&self, transfer_id: Uuid) -> Result<bool, CoreError> {
        self.ensure_running()?;
        let id_str = transfer_id.to_string();
        let deleted = self.inner.repository.delete_history_transfer(&id_str)?;
        if deleted {
            let mut state = self.inner.lock()?;
            state.transfers.remove(&transfer_id);
            queue_transfers_changed(&mut state);
        }
        Ok(deleted)
    }

    pub fn shutdown(&self) -> Result<(), CoreError> {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        self.inner.shutdown.cancel();
        self.inner.lock()?.pending_answers.clear();
        if let Some(network) = self
            .network
            .lock()
            .map_err(|_| CoreError::LockPoisoned)?
            .take()
        {
            network.shutdown();
        }
        Ok(())
    }

    fn spawn_outgoing(
        &self,
        transfer_id: Uuid,
        peer: PeerSummary,
        job: Arc<OutgoingJob>,
        control: Arc<JobControl>,
    ) {
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            if let Err(error) =
                lan::run_outgoing(inner.clone(), transfer_id, peer, job, control).await
            {
                if error.is_cancelled() {
                    log::info!("发送任务已取消 {transfer_id}: {error}");
                    let _ = inner.cancel_transfer_with_error(
                        transfer_id,
                        match error {
                            LanError::RemoteCancelled(reason) => reason,
                            _ => "已取消".to_owned(),
                        },
                    );
                } else if error.is_retryable() {
                    log::warn!("发送任务中断 {transfer_id}: {error}");
                    let _ = inner.interrupt_transfer(transfer_id, error.to_string());
                } else {
                    log::warn!("发送任务失败 {transfer_id}: {error}");
                    let _ = inner.fail_transfer(transfer_id, error.to_string());
                }
            }
        });
    }

    fn ensure_running(&self) -> Result<(), CoreError> {
        if self.stopped.load(Ordering::Acquire) {
            Err(CoreError::Stopped)
        } else {
            Ok(())
        }
    }
}

impl Drop for TransferCore {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

impl CoreInner {
    pub(crate) fn config(&self) -> Result<CoreConfig, CoreError> {
        Ok(self.lock()?.config.clone())
    }

    pub(crate) fn peer(&self, peer_id: &str) -> Result<Option<PeerSummary>, CoreError> {
        Ok(self.lock()?.peers.get(peer_id).cloned())
    }

    pub(crate) fn upsert_peer(&self, peer: PeerSummary) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        state.peers.insert(peer.id.clone(), peer);
        queue_peers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn queue_peers_changed(&self) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        queue_peers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn mark_peer_offline(&self, peer_id: &str) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        if let Some(peer) = state.peers.get_mut(peer_id) {
            peer.online = false;
        }
        queue_peers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn queue_event(&self, event: CoreEvent) -> Result<(), CoreError> {
        self.lock()?.events.push_back(event);
        Ok(())
    }

    pub(crate) async fn request_answer(
        &self,
        offer: TransferOffer,
    ) -> Result<OfferAnswer, CoreError> {
        let id =
            Uuid::parse_str(&offer.id).map_err(|_| CoreError::InvalidOfferId(offer.id.clone()))?;
        let (sender, receiver) = oneshot::channel();
        {
            let mut state = self.lock()?;
            state.pending_answers.insert(id, sender);
            state.events.push_back(CoreEvent::IncomingOffer { offer });
        }
        tokio::select! {
            answer = receiver => answer.map_err(|_| CoreError::OfferExpired(id)),
            () = self.shutdown.cancelled() => Err(CoreError::Stopped),
        }
    }

    pub(crate) fn add_incoming_transfer(
        &self,
        transfer: TransferSnapshot,
    ) -> Result<(), CoreError> {
        self.repository.save_transfer(&transfer)?;
        let mut state = self.lock()?;
        state.transfers.insert(transfer.id, transfer);
        queue_transfers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn transition_transfer(
        &self,
        id: Uuid,
        next: TransferState,
    ) -> Result<(), CoreError> {
        self.update_transfer(id, |transfer| {
            transfer.transition_to(next)?;
            transfer.error = None;
            if next != TransferState::Transferring {
                transfer.bytes_per_second = 0;
            }
            Ok(())
        })
    }

    pub(crate) fn update_completed_bytes(&self, id: Uuid, added: u64) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        let (previous_completed, total_bytes, current_speed) = {
            let transfer = state
                .transfers
                .get(&id)
                .ok_or(CoreError::UnknownTransfer(id))?;
            (
                transfer.completed_bytes,
                transfer.total_bytes,
                transfer.bytes_per_second,
            )
        };
        let completed = previous_completed.saturating_add(added).min(total_bytes);
        let now = Instant::now();
        let sample = state.progress_samples.entry(id).or_insert(ProgressSample {
            at: now,
            completed_bytes: previous_completed,
            last_saved_at: now,
        });
        let elapsed = now.duration_since(sample.at);
        let speed = if elapsed >= Duration::from_millis(250) {
            let bytes = completed.saturating_sub(sample.completed_bytes);
            sample.at = now;
            sample.completed_bytes = completed;
            (bytes as f64 / elapsed.as_secs_f64()) as u64
        } else {
            current_speed
        };

        // 关键性能优化：节流写入 SQLite。避免高并发/高速网络下逐块频繁同步触发磁盘 SQLite 写事务
        let should_save_db = completed >= total_bytes
            || now.duration_since(sample.last_saved_at) >= Duration::from_millis(500);
        if should_save_db {
            sample.last_saved_at = now;
        }

        let transfer = state
            .transfers
            .get_mut(&id)
            .ok_or(CoreError::UnknownTransfer(id))?;
        transfer.completed_bytes = completed;
        transfer.bytes_per_second = speed;
        transfer.updated_unix_ms = now_unix_ms();
        if should_save_db {
            self.repository.save_transfer(transfer)?;
        }
        queue_transfers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn set_completed_bytes(&self, id: Uuid, completed: u64) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        let completed = {
            let transfer = state
                .transfers
                .get_mut(&id)
                .ok_or(CoreError::UnknownTransfer(id))?;
            let completed = completed.min(transfer.total_bytes);
            transfer.completed_bytes = completed;
            transfer.bytes_per_second = 0;
            transfer.updated_unix_ms = now_unix_ms();
            self.repository.save_transfer(transfer)?;
            completed
        };
        let now = Instant::now();
        state.progress_samples.insert(
            id,
            ProgressSample {
                at: now,
                completed_bytes: completed,
                last_saved_at: now,
            },
        );
        queue_transfers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn transfer_is_cancelled(&self, id: Uuid) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.transfers.get(&id).map(|transfer| transfer.state))
            == Some(TransferState::Cancelled)
    }

    pub(crate) fn transfer_is_failed(&self, id: Uuid) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.transfers.get(&id).map(|transfer| transfer.state))
            .is_some_and(|state| {
                matches!(state, TransferState::Failed | TransferState::Interrupted)
            })
    }

    pub(crate) fn update_peer_identity(
        &self,
        old_peer_id: Option<&str>,
        peer_id: String,
        name: String,
        address: String,
        device_kind: String,
        trusted: bool,
    ) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        if let Some(old) = old_peer_id
            && old != peer_id
        {
            state.peers.remove(old);
        }
        state.peers.insert(
            peer_id.clone(),
            PeerSummary {
                id: peer_id,
                name,
                address,
                device_kind,
                trusted,
                online: true,
            },
        );
        queue_peers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn trust_peer(
        &self,
        peer_id: String,
        display_name: String,
        certificate_fingerprint: [u8; 32],
    ) -> Result<(), CoreError> {
        let now = now_unix_ms();
        let created = self
            .repository
            .trusted_peer(&peer_id)?
            .map_or(now, |peer| peer.created_unix_ms);
        self.repository.trust_peer(&TrustedPeer {
            peer_id: peer_id.clone(),
            display_name,
            certificate_fingerprint,
            created_unix_ms: created,
            last_seen_unix_ms: now,
            auto_accept: false,
        })?;
        let mut state = self.lock()?;
        if let Some(peer) = state.peers.get_mut(&peer_id) {
            peer.trusted = true;
        }
        queue_peers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn is_trusted(
        &self,
        peer_id: &str,
        fingerprint: &[u8; 32],
    ) -> Result<bool, CoreError> {
        Ok(self
            .repository
            .trusted_peer(peer_id)?
            .is_some_and(|peer| peer.certificate_fingerprint == *fingerprint))
    }

    pub(crate) fn fail_transfer(&self, id: Uuid, message: String) -> Result<(), CoreError> {
        self.update_transfer(id, |transfer| {
            if !transfer.state.is_terminal() {
                transfer.state = TransferState::Failed;
                transfer.bytes_per_second = 0;
                transfer.error = Some(message);
            }
            Ok(())
        })
    }

    pub(crate) fn interrupt_transfer(&self, id: Uuid, message: String) -> Result<(), CoreError> {
        self.update_transfer(id, |transfer| {
            if !transfer.state.is_terminal() {
                transfer.state = TransferState::Interrupted;
                transfer.bytes_per_second = 0;
                transfer.error = Some(message);
            }
            Ok(())
        })
    }

    pub(crate) fn cancel_transfer_with_error(
        &self,
        id: Uuid,
        message: String,
    ) -> Result<(), CoreError> {
        self.update_transfer(id, |transfer| {
            if !transfer.state.is_terminal() {
                transfer.state = TransferState::Cancelled;
                transfer.bytes_per_second = 0;
                transfer.error = Some(message);
            }
            Ok(())
        })
    }

    pub(crate) fn register_incoming_control(
        &self,
        id: Uuid,
        control: Arc<JobControl>,
    ) -> Result<(), CoreError> {
        self.lock()?.incoming_controls.insert(id, control);
        Ok(())
    }

    pub(crate) fn unregister_incoming_control(&self, id: Uuid) -> Result<(), CoreError> {
        self.lock()?.incoming_controls.remove(&id);
        Ok(())
    }

    pub(crate) fn pause_from_remote(&self, id: Uuid) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        if let Some(stored) = state.outgoing_jobs.get(&id) {
            stored.control.pause_from_remote();
        }
        if let Some(control) = state.incoming_controls.get(&id) {
            control.pause_from_remote();
        }
        if let Some(transfer) = state.transfers.get_mut(&id)
            && !transfer.state.is_terminal()
        {
            transfer.state = TransferState::Paused;
            transfer.bytes_per_second = 0;
            transfer.updated_unix_ms = now_unix_ms();
            self.repository.save_transfer(transfer)?;
        }
        queue_transfers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn resume_from_remote(&self, id: Uuid) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        if let Some(stored) = state.outgoing_jobs.get(&id) {
            stored.control.resume_from_remote();
        }
        if let Some(control) = state.incoming_controls.get(&id) {
            control.resume_from_remote();
        }
        if let Some(transfer) = state.transfers.get_mut(&id)
            && transfer.state == TransferState::Paused
        {
            transfer.state = TransferState::Transferring;
            transfer.updated_unix_ms = now_unix_ms();
            self.repository.save_transfer(transfer)?;
        }
        queue_transfers_changed(&mut state);
        Ok(())
    }

    pub(crate) fn cancel_from_remote(&self, id: Uuid, reason: &str) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        if let Some(stored) = state.outgoing_jobs.get(&id) {
            stored.control.cancel_from_remote(reason.to_owned());
        }
        if let Some(control) = state.incoming_controls.get(&id) {
            control.cancel_from_remote(reason.to_owned());
        }
        if let Some(transfer) = state.transfers.get_mut(&id)
            && !transfer.state.is_terminal()
        {
            transfer.state = TransferState::Cancelled;
            transfer.bytes_per_second = 0;
            transfer.error = Some(reason.to_owned());
            transfer.updated_unix_ms = now_unix_ms();
            self.repository.save_transfer(transfer)?;
        }
        queue_transfers_changed(&mut state);
        Ok(())
    }

    fn update_transfer(
        &self,
        id: Uuid,
        update: impl FnOnce(&mut TransferSnapshot) -> Result<(), LifecycleError>,
    ) -> Result<(), CoreError> {
        let mut state = self.lock()?;
        let transfer = state
            .transfers
            .get_mut(&id)
            .ok_or(CoreError::UnknownTransfer(id))?;
        update(transfer)?;
        transfer.updated_unix_ms = now_unix_ms();
        self.repository.save_transfer(transfer)?;
        queue_transfers_changed(&mut state);
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, CoreState>, CoreError> {
        self.state.lock().map_err(|_| CoreError::LockPoisoned)
    }
}

fn validate_config(config: &CoreConfig) -> Result<(), CoreError> {
    if config.device_name.trim().is_empty() {
        return Err(CoreError::EmptyDeviceName);
    }
    if !matches!(config.theme_mode.as_str(), "system" | "light" | "dark") {
        return Err(CoreError::InvalidThemeMode(config.theme_mode.clone()));
    }
    if !matches!(
        config.theme_style.as_str(),
        "radar" | "chat" | "workspace" | "matrix" | "glass" | "cyber" | "swiss" | "warm"
    ) {
        return Err(CoreError::InvalidThemeMode(config.theme_style.clone()));
    }
    Ok(())
}

fn ensure_receive_directory(path: &std::path::Path) -> Result<(), std::io::Error> {
    if cfg!(target_os = "android") && path.to_string_lossy().starts_with("android-saf:") {
        return Ok(());
    }
    fs::create_dir_all(path)
}

fn normalize_address(address: &str) -> Result<String, CoreError> {
    let trimmed = address.trim();
    if let Ok(socket) = SocketAddr::from_str(trimmed) {
        return Ok(socket.to_string());
    }
    let ip =
        IpAddr::from_str(trimmed).map_err(|_| CoreError::InvalidAddress(trimmed.to_owned()))?;
    Ok(SocketAddr::new(ip, 53_317).to_string())
}

fn queue_peers_changed(state: &mut CoreState) {
    let mut peers: Vec<_> = state.peers.values().cloned().collect();
    peers.sort_by(|left, right| left.name.cmp(&right.name));
    state.events.push_back(CoreEvent::PeersChanged { peers });
}

fn queue_transfers_changed(state: &mut CoreState) {
    state.events.push_back(CoreEvent::TransfersChanged {
        transfers: sorted_transfers(state),
    });
}

fn sorted_transfers(state: &CoreState) -> Vec<TransferSnapshot> {
    let mut transfers: Vec<_> = state.transfers.values().cloned().collect();
    transfers.sort_by_key(|transfer| std::cmp::Reverse(transfer.updated_unix_ms));
    transfers
}

pub(crate) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("文件系统错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("持久化错误: {0}")]
    Repository(#[from] RepositoryError),
    #[error("设备身份错误: {0}")]
    Identity(#[from] IdentityError),
    #[error("网络错误: {0}")]
    Network(#[from] lan::LanError),
    #[error("任务状态错误: {0}")]
    Lifecycle(#[from] LifecycleError),
    #[error("设备名称不能为空")]
    EmptyDeviceName,
    #[error("无效的主题模式: {0}")]
    InvalidThemeMode(String),
    #[error("没有选择文件或文件夹")]
    NoSources,
    #[error("未知设备: {0}")]
    UnknownPeer(String),
    #[error("未知接收请求: {0}")]
    UnknownOffer(Uuid),
    #[error("接收请求已经失效: {0}")]
    OfferExpired(Uuid),
    #[error("接收请求 ID 无效: {0}")]
    InvalidOfferId(String),
    #[error("未知传输任务: {0}")]
    UnknownTransfer(Uuid),
    #[error("无效的设备地址: {0}")]
    InvalidAddress(String),
    #[error("传输内核已经停止")]
    Stopped,
    #[error("传输内核锁已损坏")]
    LockPoisoned,
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::Write,
        thread,
        time::{Duration, Instant},
    };

    use crate::model::{TransferCommand, TransferState};

    use super::{CoreConfig, CoreEvent, SourceHandle, TransferCore};

    fn config(root: &std::path::Path, name: &str) -> CoreConfig {
        CoreConfig {
            data_directory: root.join("state"),
            device_name: name.to_owned(),
            receive_directory: root.join("receive"),
            background_receive: false,
            auto_accept_trusted: false,
            theme_mode: "system".to_owned(),
            theme_style: "radar".to_owned(),
            identity_wrap_key: None,
        }
    }

    #[test]
    fn two_cores_transfer_a_file_over_mutual_tls() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("hello.txt");
        File::create(&source_path)
            .and_then(|mut file| file.write_all(b"hello over tls"))
            .expect("source file");
        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver address")
                    .to_string(),
            )
            .expect("manual peer");
        sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "hello.txt".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            for core in [&sender, &receiver] {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::IncomingOffer { offer } = event {
                        core.answer_offer(
                            uuid::Uuid::parse_str(&offer.id).expect("offer id"),
                            true,
                            true,
                        )
                        .expect("accept offer");
                    }
                }
            }
            if sender
                .transfers()
                .iter()
                .any(|transfer| transfer.state == TransferState::Completed)
                && receiver
                    .transfers()
                    .iter()
                    .any(|transfer| transfer.state == TransferState::Completed)
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(
            fs::read(receiver_root.path().join("receive/hello.txt")).expect("received bytes"),
            b"hello over tls"
        );
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn outgoing_transfer_can_retry_after_sender_restart() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("restart.txt");
        fs::write(&source_path, b"resume after process restart").expect("source file");
        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        let receiver_address = receiver.listening_address().expect("receiver address");
        let peer_id = sender
            .connect_address(&receiver_address.to_string())
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "restart.txt".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");
        sender.shutdown().expect("stop sender mid-task");
        drop(sender);

        let restarted = TransferCore::initialize(config(sender_root.path(), "发送电脑"))
            .expect("restart sender core");
        restarted
            .connect_address(&receiver_address.to_string())
            .expect("reconnect receiver");
        assert_eq!(
            restarted
                .transfers()
                .into_iter()
                .find(|transfer| transfer.id == transfer_id)
                .expect("restored transfer")
                .state,
            TransferState::Interrupted
        );
        restarted
            .command_transfer(transfer_id, crate::model::TransferCommand::Retry)
            .expect("retry restored transfer");

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            for core in [&restarted, &receiver] {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::IncomingOffer { offer } = event {
                        core.answer_offer(
                            uuid::Uuid::parse_str(&offer.id).expect("offer id"),
                            true,
                            true,
                        )
                        .expect("accept offer");
                    }
                }
            }
            if restarted.transfers().iter().any(|transfer| {
                transfer.id == transfer_id && transfer.state == TransferState::Completed
            }) && receiver.transfers().iter().any(|transfer| {
                transfer.id == transfer_id && transfer.state == TransferState::Completed
            }) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(
            fs::read(receiver_root.path().join("receive/restart.txt")).expect("received bytes"),
            b"resume after process restart"
        );
        restarted.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn two_cores_preserve_nested_files_and_empty_directories() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_directory = sender_root.path().join("source-folder");
        fs::create_dir_all(source_directory.join("空目录")).expect("empty directory");
        fs::create_dir_all(source_directory.join("nested")).expect("nested directory");
        fs::write(source_directory.join("nested/照片.txt"), b"folder payload")
            .expect("nested source");
        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver address")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_directory.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "相册".to_owned(),
                    relative_path: None,
                    is_directory: true,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send folder");

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            for core in [&sender, &receiver] {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::IncomingOffer { offer } = event {
                        core.answer_offer(
                            uuid::Uuid::parse_str(&offer.id).expect("offer id"),
                            true,
                            true,
                        )
                        .expect("accept offer");
                    }
                }
            }
            if sender.transfers().iter().any(|transfer| {
                transfer.id == transfer_id && transfer.state == TransferState::Completed
            }) && receiver.transfers().iter().any(|transfer| {
                transfer.id == transfer_id && transfer.state == TransferState::Completed
            }) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        let received_root = receiver_root.path().join("receive/相册");
        assert!(received_root.join("空目录").is_dir());
        assert_eq!(
            fs::read(received_root.join("nested/照片.txt")).expect("nested received bytes"),
            b"folder payload"
        );
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn cancelling_incoming_transfer_deletes_partial_files() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("cancel-me.bin");
        {
            let mut file = File::create(&source_path).expect("source");
            // Write enough data so transfer does not finish before we cancel.
            file.write_all(&[0xcc_u8; 8 * 1024 * 1024]).expect("data");
        }
        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender");
        let receiver =
            TransferCore::initialize(config(receiver_root.path(), "接收电脑")).expect("receiver");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver address")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "cancel-me.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        // Allow transfer to start then cancel.
        thread::sleep(Duration::from_millis(100));
        receiver
            .command_transfer(transfer_id, crate::model::TransferCommand::Cancel)
            .expect("cancel incoming");

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            for core in [&sender, &receiver] {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::IncomingOffer { offer } = event {
                        core.answer_offer(
                            uuid::Uuid::parse_str(&offer.id).expect("offer id"),
                            true,
                            true,
                        )
                        .expect("accept");
                    }
                }
            }
            if receiver
                .transfers()
                .iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Cancelled)
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            receiver
                .transfers()
                .iter()
                .find(|t| t.id == transfer_id)
                .expect("transfer")
                .state,
            TransferState::Cancelled
        );
        // No .part files should remain.
        let part_files: Vec<_> = walkdir::WalkDir::new(receiver_root.path())
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".part"))
            })
            .collect();
        assert!(
            part_files.is_empty(),
            "cancel must delete all .part files, found: {part_files:?}"
        );
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    /// 泵出事件并自动接受所有入站请求，模拟配对确认流程。
    fn pump_and_accept(cores: &[&TransferCore]) {
        for core in cores {
            while let Some(event) = core.next_event() {
                if let CoreEvent::IncomingOffer { offer } = event {
                    core.answer_offer(
                        uuid::Uuid::parse_str(&offer.id).expect("offer id"),
                        true,
                        true,
                    )
                    .expect("accept offer");
                }
            }
        }
    }

    /// 列出目录下的 .part 临时文件大小。
    fn part_file_sizes(root: &std::path::Path) -> Vec<u64> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".part"))
            })
            .map(|entry| entry.metadata().map(|meta| meta.len()).unwrap_or(0))
            .collect()
    }

    fn write_pattern(path: &std::path::Path, megabytes: usize) -> u64 {
        let mut file = File::create(path).expect("source file");
        let block = vec![0x5a_u8; 1024 * 1024];
        for _ in 0..megabytes {
            file.write_all(&block).expect("pattern write");
        }
        (megabytes * 1024 * 1024) as u64
    }

    #[test]
    fn interrupted_network_transfer_resumes_without_rewriting_completed_chunks() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("large.bin");
        let total_bytes = write_pattern(&source_path, 48);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        let receiver_address = receiver.listening_address().expect("receiver address");
        let peer_id = sender
            .connect_address(&receiver_address.to_string())
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "large.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        // 等到发送端已产生真实进度（部分块已到达接收端）后强制断线。
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut interrupted_bytes = 0_u64;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Transferring
                && snapshot.completed_bytes > 0
            {
                interrupted_bytes = snapshot.completed_bytes;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(interrupted_bytes > 0, "传输应当开始并产生进度后再断线");
        sender.shutdown().expect("stop sender mid-transfer");
        drop(sender);

        // 数据通道意外结束后，接收端必须尽快进入可重试状态，不能等待发送端结果超时。
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut receiver_interrupted = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&receiver]);
            if receiver.transfers().iter().any(|snapshot| {
                snapshot.id == transfer_id && snapshot.state == TransferState::Interrupted
            }) {
                receiver_interrupted = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            receiver_interrupted,
            "数据通道断开后接收端必须在 5 秒内进入中断状态"
        );

        // 中断后接收端必须保留部分数据（断点续传的前提）。
        let part_sizes = part_file_sizes(receiver_root.path());
        assert!(
            part_sizes.iter().any(|size| *size > 0),
            "中断后必须保留 .part 部分数据，实际: {part_sizes:?}"
        );

        // 重启发送端，用原任务 ID 恢复，只补缺失块。
        let restarted = TransferCore::initialize(config(sender_root.path(), "发送电脑"))
            .expect("restart sender core");
        restarted
            .connect_address(&receiver_address.to_string())
            .expect("reconnect receiver");
        let restored = restarted
            .transfers()
            .into_iter()
            .find(|t| t.id == transfer_id)
            .expect("restored transfer");
        assert_eq!(restored.state, TransferState::Interrupted);
        assert!(
            restored.completed_bytes > 0 && restored.completed_bytes < total_bytes,
            "恢复任务应保留部分进度，实际 {} / {total_bytes}",
            restored.completed_bytes
        );
        restarted
            .command_transfer(transfer_id, crate::model::TransferCommand::Retry)
            .expect("retry restored transfer");

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut completed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&restarted, &receiver]);
            let sender_done = restarted
                .transfers()
                .iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Completed);
            let receiver_done = receiver
                .transfers()
                .iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Completed);
            if sender_done && receiver_done {
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(completed, "断线续传应在 30 秒内恢复完成");

        assert_eq!(
            fs::read(receiver_root.path().join("receive/large.bin")).expect("received bytes"),
            fs::read(&source_path).expect("source bytes"),
            "恢复后的文件必须与源文件逐字节一致"
        );
        let final_snapshot = receiver
            .transfers()
            .into_iter()
            .find(|t| t.id == transfer_id)
            .expect("final snapshot");
        assert!(
            final_snapshot.completed_bytes <= final_snapshot.total_bytes,
            "完成字节 {} 不能超过总大小 {}",
            final_snapshot.completed_bytes,
            final_snapshot.total_bytes
        );
        restarted.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn source_change_with_same_size_stops_the_transfer() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("changing.bin");
        write_pattern(&source_path, 32);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver address")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "changing.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        // 传输产生进度后，改写源文件内容（保持大小不变）并更新修改时间。
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut changed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            let started = sender
                .transfers()
                .iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Transferring);
            if started {
                let block = vec![0x99_u8; 1024 * 1024];
                let mut file = File::options()
                    .write(true)
                    .open(&source_path)
                    .expect("reopen source");
                for _ in 0..32 {
                    file.write_all(&block).expect("rewrite source");
                }
                let future = std::time::SystemTime::now() + std::time::Duration::from_secs(30);
                filetime::set_file_mtime(
                    &source_path,
                    filetime::FileTime::from_system_time(future),
                )
                .expect("bump mtime");
                changed = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(changed, "传输应当开始后再改写源文件");

        // 发送端必须在下一块发送前发现源文件变化并失败，而不是继续传输。
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut failed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                && matches!(
                    snapshot.state,
                    TransferState::Failed | TransferState::Interrupted
                )
                && snapshot
                    .error
                    .as_deref()
                    .is_some_and(|message| message.contains("源文件"))
            {
                failed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(failed, "源文件变化必须让发送端任务失败并报告源文件错误");
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn cancel_during_concurrent_channels_leaves_no_partial_files() {
        for round in 0..3 {
            let sender_root = tempfile::tempdir().expect("sender root");
            let receiver_root = tempfile::tempdir().expect("receiver root");
            let source_path = sender_root.path().join("concurrent.bin");
            write_pattern(&source_path, 128);

            let sender = TransferCore::initialize(config(sender_root.path(), "发送电脑"))
                .expect("sender core");
            let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
                .expect("receiver core");
            let peer_id = sender
                .connect_address(
                    &receiver
                        .listening_address()
                        .expect("receiver address")
                        .to_string(),
                )
                .expect("manual peer");
            let transfer_id = sender
                .send(
                    &peer_id,
                    vec![SourceHandle {
                        token: source_path.to_string_lossy().into_owned(),
                        persistent_token: None,
                        display_name: "concurrent.bin".to_owned(),
                        relative_path: None,
                        is_directory: false,
                        size: None,
                        modified_unix_ms: None,
                        random_access: None,
                    }],
                )
                .expect("send transfer");

            // 等四条数据通道并发写入后取消。
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut started = false;
            while Instant::now() < deadline {
                pump_and_accept(&[&sender, &receiver]);
                if receiver
                    .transfers()
                    .iter()
                    .any(|t| t.id == transfer_id && t.state == TransferState::Transferring)
                {
                    receiver
                        .command_transfer(transfer_id, crate::model::TransferCommand::Cancel)
                        .expect("cancel incoming");
                    started = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            assert!(started, "第 {round} 轮：传输应进入 Transferring 后再取消");

            let deadline = Instant::now() + Duration::from_secs(15);
            let mut cancelled = false;
            while Instant::now() < deadline {
                pump_and_accept(&[&sender, &receiver]);
                if receiver
                    .transfers()
                    .iter()
                    .any(|t| t.id == transfer_id && t.state == TransferState::Cancelled)
                {
                    cancelled = true;
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            assert!(cancelled, "第 {round} 轮：取消未在 15 秒内完成");

            let part_sizes = part_file_sizes(receiver_root.path());
            assert!(
                part_sizes.is_empty(),
                "第 {round} 轮：并发通道写入中取消后不得残留 .part，实际: {part_sizes:?}"
            );
            // 发送端最终也必须终止，不能悬挂在活动状态。
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut sender_done = false;
            while Instant::now() < deadline {
                pump_and_accept(&[&sender, &receiver]);
                if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                    && (snapshot.state.is_terminal()
                        || matches!(
                            snapshot.state,
                            TransferState::Interrupted | TransferState::Failed
                        ))
                {
                    sender_done = true;
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            assert!(sender_done, "第 {round} 轮：发送端任务必须终止");

            sender.shutdown().expect("sender shutdown");
            receiver.shutdown().expect("receiver shutdown");
        }
    }

    #[test]
    fn receive_directory_write_failure_fails_cleanly() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("small.bin");
        fs::write(&source_path, b"payload for write failure").expect("source file");

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送电脑")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收电脑"))
            .expect("receiver core");
        // 内核初始化已创建接收目录；在传输开始前把它替换为同名文件，
        // 使接收端准备临时文件时的目录创建必然失败（写入失败路径）。
        let occupied = receiver_root.path().join("receive");
        fs::remove_dir_all(&occupied).expect("remove receive directory");
        fs::write(&occupied, b"occupied").expect("occupied file");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver address")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "small.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        // 接收端任务必须进入失败或可重试的中断状态，且携带明确错误。
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut receiver_failed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = receiver
                .transfers()
                .into_iter()
                .find(|t| t.id == transfer_id)
                && matches!(
                    snapshot.state,
                    TransferState::Failed | TransferState::Interrupted
                )
                && snapshot.error.is_some()
            {
                receiver_failed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            receiver_failed,
            "写入失败必须让接收端任务失败或中断并给出错误"
        );
        assert!(
            part_file_sizes(receiver_root.path()).is_empty(),
            "写入失败后不得残留 .part 临时文件"
        );

        // 发送端也必须结束，不能无限等待接收决定。
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut sender_done = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                && (snapshot.state.is_terminal()
                    || matches!(
                        snapshot.state,
                        TransferState::Interrupted | TransferState::Failed
                    ))
            {
                sender_done = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(sender_done, "接收失败后发送端任务也必须终止");
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn sender_pause_and_resume_syncs_to_receiver() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("pause_test.bin");
        write_pattern(&source_path, 16);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送端")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收端"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver addr")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "pause_test.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if sender
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Transferring)
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        sender
            .command_transfer(transfer_id, TransferCommand::Pause)
            .expect("pause sender");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut receiver_paused = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = receiver
                .transfers()
                .into_iter()
                .find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Paused
            {
                receiver_paused = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(receiver_paused, "发送端暂停后接收端应同步进入 Paused 状态");

        sender
            .command_transfer(transfer_id, TransferCommand::Resume)
            .expect("resume sender");

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut completed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if sender
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Completed)
                && receiver
                    .transfers()
                    .into_iter()
                    .any(|t| t.id == transfer_id && t.state == TransferState::Completed)
            {
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(completed, "恢复后双方应顺利完成传输");
        assert_eq!(
            fs::read(receiver_root.path().join("receive/pause_test.bin")).expect("received bytes"),
            fs::read(&source_path).expect("source bytes")
        );
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn receiver_pause_and_resume_syncs_to_sender() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("receiver_pause.bin");
        write_pattern(&source_path, 16);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送端")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收端"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver addr")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "receiver_pause.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if receiver
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Transferring)
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        receiver
            .command_transfer(transfer_id, TransferCommand::Pause)
            .expect("pause receiver");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut sender_paused = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Paused
            {
                sender_paused = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(sender_paused, "接收端暂停后发送端应同步进入 Paused 状态");

        receiver
            .command_transfer(transfer_id, TransferCommand::Resume)
            .expect("resume receiver");

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut completed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if sender
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Completed)
                && receiver
                    .transfers()
                    .into_iter()
                    .any(|t| t.id == transfer_id && t.state == TransferState::Completed)
            {
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(completed, "接收端恢复后双方应顺利完成传输");
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn sender_cancel_notifies_receiver_cleanly() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("cancel_test.bin");
        write_pattern(&source_path, 16);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送端")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收端"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver addr")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "cancel_test.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if sender
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Transferring)
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        sender
            .command_transfer(transfer_id, TransferCommand::Cancel)
            .expect("cancel sender");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut receiver_cancelled = false;
        let mut failures = Vec::new();
        while Instant::now() < deadline {
            for (idx, core) in [&sender, &receiver].iter().enumerate() {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::Failure { message } = event {
                        failures.push((idx, message));
                    }
                }
            }
            if let Some(snapshot) = receiver
                .transfers()
                .into_iter()
                .find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Cancelled
            {
                assert!(
                    snapshot.error.as_deref() == Some("对方已取消传输")
                        || snapshot.error.as_deref() == Some("已取消"),
                    "取消原因应为对方已取消传输，实际为 {:?}",
                    snapshot.error
                );
                receiver_cancelled = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(receiver_cancelled, "发送端取消后接收端任务应变为 Cancelled");
        assert!(
            failures.is_empty(),
            "取消操作不应弹出 Failure 错误事件: {failures:?}"
        );
        assert!(
            part_file_sizes(receiver_root.path()).is_empty(),
            "取消后不得残留 .part 临时文件"
        );
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn receiver_cancel_notifies_sender_cleanly() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("cancel_recv.bin");
        write_pattern(&source_path, 16);

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送端")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收端"))
            .expect("receiver core");
        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver addr")
                    .to_string(),
            )
            .expect("manual peer");
        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "cancel_recv.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: None,
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if receiver
                .transfers()
                .into_iter()
                .any(|t| t.id == transfer_id && t.state == TransferState::Transferring)
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        receiver
            .command_transfer(transfer_id, TransferCommand::Cancel)
            .expect("cancel receiver");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut sender_cancelled = false;
        let mut has_failure_event = false;
        while Instant::now() < deadline {
            for core in [&sender, &receiver] {
                while let Some(event) = core.next_event() {
                    if let CoreEvent::Failure { .. } = event {
                        has_failure_event = true;
                    }
                }
            }
            if let Some(snapshot) = sender.transfers().into_iter().find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Cancelled
            {
                assert!(
                    snapshot.error.as_deref() == Some("对方已取消传输")
                        || snapshot.error.as_deref() == Some("已取消"),
                    "取消原因应为对方已取消传输，实际为 {:?}",
                    snapshot.error
                );
                sender_cancelled = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(sender_cancelled, "接收端取消后发送端任务应变为 Cancelled");
        assert!(!has_failure_event, "取消操作不应弹出 Failure 错误事件");
        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }

    #[test]
    fn two_cores_transfer_large_file_with_work_stealing_pipeline() {
        let sender_root = tempfile::tempdir().expect("sender root");
        let receiver_root = tempfile::tempdir().expect("receiver root");
        let source_path = sender_root.path().join("large_pipeline.bin");
        // 写入 16 MiB (4 个 4MB 块)，确保多数据通道能够充分并发工作窃取并执行预取流水线
        let total_bytes = write_pattern(&source_path, 16);
        let _ = total_bytes;
        let expected_hash = *blake3::hash(&fs::read(&source_path).expect("read source")).as_bytes();

        let sender =
            TransferCore::initialize(config(sender_root.path(), "发送端")).expect("sender core");
        let receiver = TransferCore::initialize(config(receiver_root.path(), "接收端"))
            .expect("receiver core");

        let peer_id = sender
            .connect_address(
                &receiver
                    .listening_address()
                    .expect("receiver addr")
                    .to_string(),
            )
            .expect("manual peer");

        let transfer_id = sender
            .send(
                &peer_id,
                vec![SourceHandle {
                    token: source_path.to_string_lossy().into_owned(),
                    persistent_token: None,
                    display_name: "large_pipeline.bin".to_owned(),
                    relative_path: None,
                    is_directory: false,
                    size: None,
                    modified_unix_ms: None,
                    random_access: Some(true),
                }],
            )
            .expect("send transfer");

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut completed = false;
        while Instant::now() < deadline {
            pump_and_accept(&[&sender, &receiver]);
            if let Some(snapshot) = receiver
                .transfers()
                .into_iter()
                .find(|t| t.id == transfer_id)
                && snapshot.state == TransferState::Completed
            {
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }

        assert!(completed, "16 MiB 大文件应在流水线工作窃取模式下顺利完成");
        let target_path = receiver_root.path().join("receive/large_pipeline.bin");
        assert!(target_path.exists(), "目标文件应已原子落盘");
        let target_hash = *blake3::hash(&fs::read(target_path).expect("read target")).as_bytes();
        assert_eq!(
            expected_hash, target_hash,
            "目标文件 BLAKE3 哈希必须与源文件完全一致"
        );

        sender.shutdown().expect("sender shutdown");
        receiver.shutdown().expect("receiver shutdown");
    }
}
