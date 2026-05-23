use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};

use get_if_addrs::{IfAddr, get_if_addrs};
use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket, time};

use crate::{file::descriptor::ShareId, state::list_library_shares};

pub const DEFAULT_DISCOVERY_PORT: u16 = 37037;
pub const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 3000;
pub const DEFAULT_DISCOVERY_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 0, 86);

const DISCOVERY_MAGIC: &str = "etle-discovery-v1";
const MAX_DISCOVERY_PACKET_SIZE: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoveryOptions {
    pub port: u16,
    pub timeout: Duration,
    pub multicast: Option<Ipv4Addr>,
}

impl DiscoveryOptions {
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            timeout: Duration::from_millis(DEFAULT_DISCOVERY_TIMEOUT_MS),
            multicast: Some(DEFAULT_DISCOVERY_MULTICAST_ADDR),
        }
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_multicast(mut self, multicast: Ipv4Addr) -> Self {
        self.multicast = multicast.is_multicast().then_some(multicast);
        self
    }

    #[must_use]
    pub const fn without_multicast(mut self) -> Self {
        self.multicast = None;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiscoveryInterface {
    ip: Ipv4Addr,
    broadcast: Option<Ipv4Addr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum DiscoveryMessage {
    Query {
        magic: String,
        share_id: ShareId,
    },
    Response {
        magic: String,
        share_id: ShareId,
        listen_port: u16,
        peer_id: String,
        name: String,
    },
}

pub async fn serve_discovery_forever(
    library_root: impl AsRef<Path>,
    p2p_listen: SocketAddr,
    peer_id: impl Into<String>,
    discovery_port: u16,
) -> std::io::Result<()> {
    serve_discovery_forever_with_options(
        library_root,
        p2p_listen,
        peer_id,
        DiscoveryOptions::new(discovery_port),
    )
    .await
}

pub async fn serve_discovery_forever_with_options(
    library_root: impl AsRef<Path>,
    p2p_listen: SocketAddr,
    peer_id: impl Into<String>,
    options: DiscoveryOptions,
) -> std::io::Result<()> {
    let library_root = library_root.as_ref().to_path_buf();
    let peer_id = peer_id.into();
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), options.port);
    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

    if let Some(multicast) = options.multicast {
        join_multicast_on_active_interfaces(&socket, multicast);
    }

    let mut buffer = [0_u8; MAX_DISCOVERY_PACKET_SIZE];
    loop {
        let (read, source) = socket.recv_from(&mut buffer).await?;
        let Ok(message) = decode_message(&buffer[..read]) else {
            continue;
        };

        let DiscoveryMessage::Query { magic, share_id } = message else {
            continue;
        };
        if magic != DISCOVERY_MAGIC {
            continue;
        }

        let Some(name) = local_share_name(&library_root, share_id) else {
            continue;
        };

        let response = DiscoveryMessage::Response {
            magic: DISCOVERY_MAGIC.to_string(),
            share_id,
            listen_port: p2p_listen.port(),
            peer_id: peer_id.clone(),
            name,
        };

        if let Ok(payload) = encode_message(&response) {
            let _ = socket.send_to(&payload, source).await;
        }
    }
}

pub async fn discover_peers_for_share(
    share_id: ShareId,
    discovery_port: u16,
    timeout: Duration,
) -> std::io::Result<Vec<SocketAddr>> {
    discover_peers_for_share_with_options(
        share_id,
        DiscoveryOptions::new(discovery_port).with_timeout(timeout),
    )
    .await
}

pub async fn discover_peers_for_share_with_options(
    share_id: ShareId,
    options: DiscoveryOptions,
) -> std::io::Result<Vec<SocketAddr>> {
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).await?;
    socket.set_broadcast(true)?;

    let query = DiscoveryMessage::Query {
        magic: DISCOVERY_MAGIC.to_string(),
        share_id,
    };
    let payload = encode_message(&query)?;

    for target in discovery_query_targets(options.port, options.multicast) {
        let _ = socket.send_to(&payload, target).await;
    }

    let deadline = time::Instant::now() + options.timeout;
    let mut peers = BTreeSet::new();
    let mut buffer = [0_u8; MAX_DISCOVERY_PACKET_SIZE];

    loop {
        let now = time::Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline - now;
        let result = time::timeout(remaining, socket.recv_from(&mut buffer)).await;
        let Ok(Ok((read, source))) = result else {
            break;
        };

        let Ok(DiscoveryMessage::Response {
            magic,
            share_id: response_share_id,
            listen_port,
            ..
        }) = decode_message(&buffer[..read])
        else {
            continue;
        };

        if magic != DISCOVERY_MAGIC || response_share_id != share_id {
            continue;
        }

        peers.insert(SocketAddr::new(source.ip(), listen_port));
    }

    Ok(peers.into_iter().collect())
}

#[must_use]
pub fn discovery_query_targets(port: u16, multicast: Option<Ipv4Addr>) -> Vec<SocketAddr> {
    let mut targets = BTreeSet::new();

    targets.insert(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
    targets.insert(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));

    if let Some(multicast) = multicast
        && multicast.is_multicast()
    {
        targets.insert(SocketAddr::new(IpAddr::V4(multicast), port));
    }

    for interface in active_ipv4_interfaces().unwrap_or_default() {
        if let Some(broadcast) = interface.broadcast {
            targets.insert(SocketAddr::new(IpAddr::V4(broadcast), port));
        }
    }

    targets.into_iter().collect()
}

fn active_ipv4_interfaces() -> std::io::Result<Vec<DiscoveryInterface>> {
    let mut interfaces = Vec::new();

    for interface in get_if_addrs()? {
        let IfAddr::V4(ipv4) = interface.addr else {
            continue;
        };

        let ip = ipv4.ip;
        if ip.is_unspecified() {
            continue;
        }

        let broadcast = ipv4
            .broadcast
            .or_else(|| subnet_broadcast(ip, ipv4.netmask));

        interfaces.push(DiscoveryInterface { ip, broadcast });
    }

    Ok(interfaces)
}

fn subnet_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Option<Ipv4Addr> {
    let mask = u32::from(netmask);
    if mask == u32::MAX {
        return None;
    }

    Some(Ipv4Addr::from(u32::from(ip) | !mask))
}

fn join_multicast_on_active_interfaces(socket: &UdpSocket, multicast: Ipv4Addr) {
    if !multicast.is_multicast() {
        return;
    }

    for interface in active_ipv4_interfaces().unwrap_or_default() {
        let _ = socket.join_multicast_v4(multicast, interface.ip);
    }

    let _ = socket.join_multicast_v4(multicast, Ipv4Addr::UNSPECIFIED);
}

fn local_share_name(library_root: &Path, share_id: ShareId) -> Option<String> {
    let shares = list_library_shares(library_root).ok()?;
    shares
        .into_iter()
        .find(|share| {
            share.descriptor.share_id == share_id
                && share.has_secret
                && share.completed_chunks() > 0
        })
        .map(|share| share.descriptor.name)
}

fn encode_message(message: &DiscoveryMessage) -> std::io::Result<Vec<u8>> {
    bincode::serde::encode_to_vec(message, bincode::config::standard())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn decode_message(bytes: &[u8]) -> Result<DiscoveryMessage, bincode::error::DecodeError> {
    let (message, _read): (DiscoveryMessage, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_global_loopback_and_multicast_targets() {
        let targets = discovery_query_targets(37037, Some(DEFAULT_DISCOVERY_MULTICAST_ADDR));

        assert!(targets.contains(&SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), 37037)));
        assert!(targets.contains(&SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 37037)));
        assert!(targets.contains(&SocketAddr::new(
            IpAddr::V4(DEFAULT_DISCOVERY_MULTICAST_ADDR),
            37037
        )));
    }

    #[test]
    fn computes_subnet_broadcast_from_ip_and_netmask() {
        assert_eq!(
            subnet_broadcast(
                Ipv4Addr::new(192, 168, 1, 23),
                Ipv4Addr::new(255, 255, 255, 0)
            ),
            Some(Ipv4Addr::new(192, 168, 1, 255))
        );
    }
}
