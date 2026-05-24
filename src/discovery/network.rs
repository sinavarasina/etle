use super::prelude::*;
use super::wire::{DiscoveryInterface, DiscoveryMessage};

pub(super) fn advertised_p2p_addr(p2p_listen: SocketAddr, _source_ip: IpAddr) -> SocketAddr {
    // Keep 0.0.0.0/:: unspecified in the discovery response.
    //
    // The discovery client receives the UDP response from the seeder's real
    // interface address and replaces an unspecified listen address with that
    // UDP source IP. Filling this with the query source IP here would advertise
    // the downloader's own address back to itself.
    p2p_listen
}

pub(super) fn insert_preferred_peer(
    peers: &mut BTreeMap<String, SocketAddr>,
    instance_id: String,
    candidate: SocketAddr,
) {
    let Some(existing) = peers.get_mut(&instance_id) else {
        peers.insert(instance_id, candidate);
        return;
    };

    if candidate.ip().is_loopback() && !existing.ip().is_loopback() {
        *existing = candidate;
    }
}

pub(super) fn discovery_instance_id(
    library_root: &Path,
    p2p_listen: SocketAddr,
    peer_id: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DISCOVERY_MAGIC.as_bytes());
    hasher.update(peer_id.as_bytes());
    hasher.update(library_root.to_string_lossy().as_bytes());
    hasher.update(p2p_listen.to_string().as_bytes());
    hasher.update(std::process::id().to_string().as_bytes());
    hasher.finalize().to_hex().to_string()
}

#[must_use]
pub fn query_targets(port: u16, multicast: Option<Ipv4Addr>) -> Vec<SocketAddr> {
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

pub(super) fn active_ipv4_interfaces() -> std::io::Result<Vec<DiscoveryInterface>> {
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

pub(super) fn subnet_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Option<Ipv4Addr> {
    let mask = u32::from(netmask);
    if mask == u32::MAX {
        return None;
    }

    Some(Ipv4Addr::from(u32::from(ip) | !mask))
}

pub(super) fn join_multicast_on_active_interfaces(socket: &UdpSocket, multicast: Ipv4Addr) {
    if !multicast.is_multicast() {
        return;
    }

    for interface in active_ipv4_interfaces().unwrap_or_default() {
        let _ = socket.join_multicast_v4(multicast, interface.ip);
    }

    let _ = socket.join_multicast_v4(multicast, Ipv4Addr::UNSPECIFIED);
}

pub(super) fn local_share_name(library_root: &Path, share_id: ShareId) -> Option<String> {
    let shares = library::list(library_root).ok()?;
    shares
        .into_iter()
        .find(|share| {
            share.descriptor.share_id == share_id
                && share.has_secret
                && share.completed_chunks() > 0
        })
        .map(|share| share.descriptor.name)
}

pub(super) fn encode_message(message: &DiscoveryMessage) -> std::io::Result<Vec<u8>> {
    bincode::serde::encode_to_vec(message, bincode::config::standard())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub(super) fn decode_message(
    bytes: &[u8],
) -> Result<DiscoveryMessage, bincode::error::DecodeError> {
    let (message, bytes_read): (DiscoveryMessage, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

    if bytes_read != bytes.len() {
        return Err(bincode::error::DecodeError::OtherString(format!(
            "discovery message has trailing bytes: decoded {bytes_read} of {} bytes",
            bytes.len()
        )));
    }

    Ok(message)
}
