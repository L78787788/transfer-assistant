use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use if_addrs::get_if_addrs;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;

use crate::{
    core::{CoreInner, PeerSummary},
    protocol::PROTOCOL_MAJOR,
};

pub(crate) const SERVICE_TYPE: &str = "_transassist._tcp.local.";

pub(crate) struct MdnsHandle {
    daemon: ServiceDaemon,
    fullname: String,
    stopped: Arc<AtomicBool>,
    browser: Option<thread::JoinHandle<()>>,
}

impl MdnsHandle {
    pub(crate) fn start(inner: Arc<CoreInner>, port: u16) -> Result<Self, MdnsError> {
        let daemon = ServiceDaemon::new()?;
        let short_id = &inner.identity.device_id()[..12];
        let instance = format!("transassist-{short_id}");
        let hostname = format!("transassist-{short_id}");
        let config = inner.config()?;
        let properties = HashMap::from([
            ("id".to_owned(), inner.identity.device_id().to_owned()),
            (
                "name".to_owned(),
                if config.device_name.trim().is_empty() {
                    "传输助手".to_owned()
                } else {
                    config.device_name
                },
            ),
            (
                "kind".to_owned(),
                if cfg!(target_os = "android") {
                    "phone".to_owned()
                } else {
                    "computer".to_owned()
                },
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

    pub(crate) fn shutdown(mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.daemon.stop_browse(SERVICE_TYPE);
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
        if let Some(browser) = self.browser.take() {
            let _ = browser.join();
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum MdnsError {
    #[error("mDNS 失败: {0}")]
    Daemon(#[from] mdns_sd::Error),
    #[error("核心失败: {0}")]
    Core(#[from] crate::core::CoreError),
    #[error("文件系统错误: {0}")]
    Io(#[from] std::io::Error),
}
