use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use if_addrs::get_if_addrs;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    core::{CoreInner, PeerSummary},
    protocol::PROTOCOL_MAJOR,
};

pub(crate) const SERVICE_TYPE: &str = "_transassist._tcp.local.";
const DISCOVERY_PORT: u16 = 53_317;
const BROADCAST_INTERVAL: Duration = Duration::from_secs(3);
const BROADCAST_PEER_TIMEOUT: Duration = Duration::from_secs(12);
const DISCOVERY_MAGIC: &[u8] = b"TRANSASSIST-DISCOVERY/1\0";

/// 移动数据接口名（Android 常见前缀），与局域网发现无关。
fn is_cellular_interface(name: &str) -> bool {
    const CELLULAR_PREFIXES: &[&str] = &["ccmni", "rmnet", "pdp", "swlan", "wwan", "radio"];
    CELLULAR_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn should_advertise_interface(name: &str, address: IpAddr, is_oper_up: bool) -> bool {
    is_oper_up && address.is_ipv4() && !address.is_loopback() && !is_cellular_interface(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DiscoveryInterface {
    address: Ipv4Addr,
    broadcast: Ipv4Addr,
}

#[derive(Debug, Deserialize, Serialize)]
struct BroadcastAnnouncement {
    id: String,
    name: String,
    kind: String,
    port: u16,
    version: u32,
    #[serde(default)]
    reply_requested: bool,
}

fn encode_broadcast_announcement(
    announcement: &BroadcastAnnouncement,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut packet = DISCOVERY_MAGIC.to_vec();
    packet.extend(serde_json::to_vec(announcement)?);
    Ok(packet)
}

fn decode_broadcast_announcement(packet: &[u8]) -> Option<BroadcastAnnouncement> {
    packet
        .strip_prefix(DISCOVERY_MAGIC)
        .and_then(|payload| serde_json::from_slice(payload).ok())
}

pub(crate) struct MdnsHandle {
    daemon: ServiceDaemon,
    fullname: String,
    stopped: Arc<AtomicBool>,
    refresh_requested: Arc<AtomicBool>,
    browser: Option<thread::JoinHandle<()>>,
}

impl MdnsHandle {
    pub(crate) fn start(inner: Arc<CoreInner>, port: u16) -> Result<Self, MdnsError> {
        let daemon = ServiceDaemon::new()?;
        let short_id = &inner.identity.device_id()[..12];
        let instance = format!("transassist-{short_id}");
        // mdns_sd 要求主机名以 .local. 结尾（多播 DNS 域名后缀）。
        let hostname = format!("transassist-{short_id}.local.");
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
        let announcement_name = properties
            .get("name")
            .cloned()
            .unwrap_or_else(|| "传输助手".to_owned());
        let announcement_kind = properties
            .get("kind")
            .cloned()
            .unwrap_or_else(|| "computer".to_owned());
        // 让 mDNS 自动跟踪网卡地址变化，避免切换 Wi-Fi 后仍广播旧地址。
        // 移动数据接口（ccmni/rmnet 等）与局域网无关，不加入服务广播。
        let discovery_interfaces = discovery_interfaces();
        let interface_addresses = discovery_interfaces
            .iter()
            .map(|interface| IpAddr::V4(interface.address))
            .collect::<Vec<_>>();
        let interface_filters: Vec<IfKind> = interface_addresses
            .iter()
            .copied()
            .map(IfKind::Addr)
            .collect();
        let broadcast_socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))
            .ok()
            .and_then(|socket| {
                socket.set_broadcast(true).ok()?;
                socket.set_nonblocking(true).ok()?;
                Some(socket)
            });
        if broadcast_socket.is_none() {
            log::warn!("局域网广播发现启动失败，将继续使用 mDNS");
        }
        let announcement = BroadcastAnnouncement {
            id: inner.identity.device_id().to_owned(),
            name: announcement_name,
            kind: announcement_kind,
            port,
            version: PROTOCOL_MAJOR,
            reply_requested: true,
        };
        let announcement_packet = encode_broadcast_announcement(&announcement)?;
        let response_packet = encode_broadcast_announcement(&BroadcastAnnouncement {
            reply_requested: false,
            ..announcement
        })?;
        let mut service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &hostname,
            "",
            port,
            Some(properties),
        )?
        .enable_addr_auto();
        if !interface_filters.is_empty() {
            service.set_interfaces(interface_filters);
        }
        let fullname = service.get_fullname().to_owned();
        daemon.register(service)?;
        let receiver = daemon.browse(SERVICE_TYPE)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let browser_stopped = stopped.clone();
        let refresh_requested = Arc::new(AtomicBool::new(false));
        let browser_refresh_requested = refresh_requested.clone();
        let local_id = inner.identity.device_id().to_owned();
        let browser_daemon = daemon.clone();
        let browser = thread::Builder::new()
            .name("transassist-mdns".to_owned())
            .spawn(move || {
                let mut service_peers = HashMap::<String, String>::new();
                let mut broadcast_peers = HashMap::<String, Instant>::new();
                let mut receiver = receiver;
                let mut next_refresh = Instant::now() + Duration::from_secs(30);
                let mut next_broadcast = Instant::now();
                while !browser_stopped.load(Ordering::Acquire) {
                    let now = Instant::now();
                    if now >= next_broadcast {
                        if let Some(socket) = &broadcast_socket {
                            let broadcast_targets = broadcast_targets(DISCOVERY_PORT);
                            for target in &broadcast_targets {
                                if let Err(error) = socket.send_to(&announcement_packet, target) {
                                    log::warn!("局域网广播发现发送失败 {target}: {error}");
                                }
                            }
                        }
                        next_broadcast = now + BROADCAST_INTERVAL;
                    }
                    if let Some(socket) = &broadcast_socket {
                        let mut packet = [0_u8; 2048];
                        loop {
                            match socket.recv_from(&mut packet) {
                                Ok((length, source)) => {
                                    let Some(peer) =
                                        decode_broadcast_announcement(&packet[..length])
                                    else {
                                        continue;
                                    };
                                    if peer.id == local_id
                                        || peer.version != PROTOCOL_MAJOR
                                        || peer.port == 0
                                    {
                                        continue;
                                    }
                                    let Some(source_ip) = (match source.ip() {
                                        IpAddr::V4(address) if !address.is_loopback() => {
                                            Some(address)
                                        }
                                        _ => None,
                                    }) else {
                                        continue;
                                    };
                                    let address = SocketAddr::new(IpAddr::V4(source_ip), peer.port)
                                        .to_string();
                                    if peer.reply_requested
                                        && let Err(error) = socket.send_to(&response_packet, source)
                                    {
                                        log::debug!("局域网发现单播响应失败 {source}: {error}");
                                    }
                                    if let Err(error) =
                                        upsert_peer(&inner, &peer.id, peer.name, peer.kind, address)
                                    {
                                        log::debug!("登记广播发现的设备失败: {error}");
                                    }
                                    broadcast_peers.insert(peer.id, Instant::now());
                                }
                                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                    break;
                                }
                                Err(error) => {
                                    log::debug!("局域网广播发现接收失败: {error}");
                                    break;
                                }
                            }
                        }
                    }
                    let stale_peers = broadcast_peers
                        .iter()
                        .filter_map(|(peer_id, last_seen)| {
                            (last_seen.elapsed() > BROADCAST_PEER_TIMEOUT)
                                .then_some(peer_id.clone())
                        })
                        .collect::<Vec<_>>();
                    for peer_id in stale_peers {
                        broadcast_peers.remove(&peer_id);
                        if !service_peers.values().any(|id| id == &peer_id) {
                            let _ = inner.mark_peer_offline(&peer_id);
                        }
                    }
                    if browser_refresh_requested.swap(false, Ordering::AcqRel)
                        || Instant::now() >= next_refresh
                    {
                        let _ = browser_daemon.stop_browse(SERVICE_TYPE);
                        match browser_daemon.browse(SERVICE_TYPE) {
                            Ok(next_receiver) => {
                                receiver = next_receiver;
                                next_refresh = Instant::now() + Duration::from_secs(30);
                                log::debug!("mDNS 已重新发起局域网设备搜索");
                            }
                            Err(error) => {
                                log::warn!("mDNS 重新搜索失败: {error}");
                                next_refresh = Instant::now() + Duration::from_secs(5);
                            }
                        }
                    }
                    match receiver.recv_timeout(Duration::from_millis(150)) {
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
                            // 保留对端广播的全部地址，发送端连接时逐个尝试；
                            // 单一地址可能落在移动数据接口上导致连接失败。
                            let mut addresses = info
                                .get_addresses_v4()
                                .into_iter()
                                .filter(|address| !address.is_loopback())
                                .map(|address| {
                                    SocketAddr::new(IpAddr::V4(address), info.get_port())
                                        .to_string()
                                })
                                .collect::<Vec<_>>();
                            addresses.sort_unstable();
                            let address_text = if addresses.is_empty() {
                                SocketAddr::new(IpAddr::V4(ip), info.get_port()).to_string()
                            } else {
                                addresses.join(",")
                            };
                            let name = info
                                .get_property_val_str("name")
                                .unwrap_or(peer_id)
                                .to_owned();
                            let kind = info
                                .get_property_val_str("kind")
                                .unwrap_or("computer")
                                .to_owned();
                            if let Err(error) =
                                upsert_peer(&inner, peer_id, name, kind, address_text)
                            {
                                log::debug!("登记 mDNS 发现的设备失败: {error}");
                            }
                            service_peers
                                .insert(info.get_fullname().to_owned(), peer_id.to_owned());
                        }
                        Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                            if let Some(peer_id) = service_peers.remove(&fullname)
                                && !broadcast_peers.contains_key(&peer_id)
                            {
                                let _ = inner.mark_peer_offline(&peer_id);
                            }
                        }
                        Ok(ServiceEvent::ServiceFound(_, fullname)) => {
                            log::debug!("mDNS 找到设备服务: {fullname}");
                        }
                        Ok(ServiceEvent::SearchStarted(_)) | Ok(ServiceEvent::SearchStopped(_)) => {
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
            refresh_requested,
            browser: Some(browser),
        })
    }

    /// 请求浏览线程重新发起一次 mDNS 查询。查询在独立线程中执行，避免阻塞 UI 调用。
    pub(crate) fn refresh(&self) {
        self.refresh_requested.store(true, Ordering::Release);
    }

    pub(crate) fn shutdown(mut self) {
        self.stopped.store(true, Ordering::Release);
        let daemon = self.daemon.clone();
        let fullname = self.fullname.clone();
        let _ = thread::Builder::new()
            .name("transassist-mdns-shutdown".to_owned())
            .spawn(move || {
                let _ = daemon.stop_browse(SERVICE_TYPE);
                let _ = daemon.unregister(&fullname);
                let _ = daemon.shutdown();
            });
        drop(self.browser.take());
    }
}

fn discovery_interfaces() -> Vec<DiscoveryInterface> {
    get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| {
            if !iface.is_oper_up() || is_cellular_interface(&iface.name) {
                return None;
            }
            let if_addrs::IfAddr::V4(ref address) = iface.addr else {
                return None;
            };
            if !should_advertise_interface(&iface.name, IpAddr::V4(address.ip), iface.is_oper_up())
            {
                return None;
            }
            Some(DiscoveryInterface {
                address: address.ip,
                broadcast: address.broadcast.unwrap_or(Ipv4Addr::BROADCAST),
            })
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn broadcast_targets(port: u16) -> Vec<SocketAddr> {
    let mut targets = discovery_interfaces()
        .into_iter()
        .map(|interface| SocketAddr::new(IpAddr::V4(interface.broadcast), port))
        .collect::<HashSet<_>>();
    targets.insert(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
    targets.into_iter().collect()
}

fn upsert_peer(
    inner: &CoreInner,
    peer_id: &str,
    name: String,
    kind: String,
    address: String,
) -> Result<(), crate::core::CoreError> {
    let trusted = inner
        .repository
        .trusted_peer(peer_id)
        .ok()
        .flatten()
        .is_some();
    inner.upsert_peer(PeerSummary {
        id: peer_id.to_owned(),
        name,
        address,
        device_kind: kind,
        trusted,
        online: true,
    })
}

#[derive(Debug, Error)]
pub(crate) enum MdnsError {
    #[error("mDNS 失败: {0}")]
    Daemon(#[from] mdns_sd::Error),
    #[error("核心失败: {0}")]
    Core(#[from] crate::core::CoreError),
    #[error("文件系统错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("广播公告编码失败: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::{
        BroadcastAnnouncement, decode_broadcast_announcement, encode_broadcast_announcement,
        is_cellular_interface, should_advertise_interface,
    };

    #[test]
    fn cellular_interface_filter_keeps_lan_interfaces() {
        for name in [
            "ccmni4",
            "rmnet_data0",
            "pdp_ip0",
            "swlan0",
            "wwan0",
            "radio0",
        ] {
            assert!(is_cellular_interface(name), "应过滤移动数据接口 {name}");
        }
        for name in ["wlan0", "以太网", "Ethernet", "en0"] {
            assert!(!is_cellular_interface(name), "不应过滤局域网接口 {name}");
        }
    }

    #[test]
    fn discovery_filter_uses_operational_ipv4_lan_addresses() {
        assert!(should_advertise_interface(
            "WLAN",
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 16)),
            true,
        ));
        assert!(!should_advertise_interface(
            "wlan0",
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 21)),
            false,
        ));
        assert!(!should_advertise_interface(
            "ccmni0",
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            true,
        ));
        assert!(!should_advertise_interface(
            "lo",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            true,
        ));
        assert!(!should_advertise_interface(
            "wlan0",
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            true,
        ));
    }

    #[test]
    fn broadcast_announcement_round_trips_with_magic_prefix() {
        let announcement = BroadcastAnnouncement {
            id: "device-1".to_owned(),
            name: "测试设备".to_owned(),
            kind: "phone".to_owned(),
            port: 53317,
            version: 1,
            reply_requested: true,
        };

        let encoded = encode_broadcast_announcement(&announcement).expect("编码公告");
        let decoded = decode_broadcast_announcement(&encoded).expect("解码公告");
        assert_eq!(decoded.id, announcement.id);
        assert_eq!(decoded.name, announcement.name);
        assert_eq!(decoded.kind, announcement.kind);
        assert_eq!(decoded.port, announcement.port);
        assert_eq!(decoded.version, announcement.version);
        assert_eq!(decoded.reply_requested, announcement.reply_requested);
        assert!(decode_broadcast_announcement(b"invalid").is_none());
    }
}
