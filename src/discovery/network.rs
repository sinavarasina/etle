use std::io::ErrorKind;

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

pub(super) fn resolve_discovered_peer_addr(
    listen_addr: SocketAddr,
    listen_port: u16,
    source_ip: IpAddr,
) -> Option<SocketAddr> {
    let effective_port = if listen_port != 0 {
        listen_port
    } else {
        listen_addr.port()
    };
    if effective_port == 0 {
        return None;
    }

    let candidate = if listen_addr.ip().is_unspecified() {
        SocketAddr::new(source_ip, effective_port)
    } else {
        SocketAddr::new(listen_addr.ip(), effective_port)
    };

    if candidate.ip().is_unspecified() || candidate.ip().is_multicast() {
        return None;
    }

    if candidate.ip().is_loopback() && !source_ip.is_loopback() {
        return None;
    }

    Some(candidate)
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
    let mut random = [0_u8; 32];
    if getrandom::fill(&mut random).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        random[..16].copy_from_slice(&nanos.to_le_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(DISCOVERY_MAGIC.as_bytes());
    hasher.update(peer_id.as_bytes());
    hasher.update(library_root.to_string_lossy().as_bytes());
    hasher.update(p2p_listen.to_string().as_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&random);
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
        if ip.is_unspecified() || ip.is_loopback() {
            continue;
        }

        let broadcast = ipv4
            .broadcast
            .filter(|addr| !addr.is_loopback())
            .or_else(|| subnet_broadcast(ip, ipv4.netmask));

        interfaces.push(DiscoveryInterface { ip, broadcast });
    }

    Ok(interfaces)
}

pub(super) fn subnet_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Option<Ipv4Addr> {
    let mask = u32::from(netmask);
    if mask == 0 || mask >= 0xffff_fffe {
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
    let paths = LibraryPaths::for_share(library_root, share_id);
    if !paths.descriptor_path().is_file() || !paths.secret_path().is_file() {
        return None;
    }

    let descriptor = read_descriptor(&paths).ok()?;
    let progress = match read_progress(&paths) {
        Ok(progress) => Some(progress),
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => return None,
    };
    let state = match read_state(&paths) {
        Ok(state) => Some(state),
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => return None,
    };

    let summary = LocalShareSummary {
        has_secret: true,
        paths,
        descriptor,
        progress,
        state,
    };

    is_discoverable_share(&summary).then_some(summary.descriptor.name)
}

fn is_discoverable_share(share: &LocalShareSummary) -> bool {
    if !share.has_secret {
        return false;
    }

    if !matches!(
        share.mode(),
        Some(ShareMode::Seeding | ShareMode::Completed | ShareMode::Downloading)
    ) {
        return false;
    }

    if share.total_chunks() == 0 {
        return false;
    }

    share.completed_chunks() > 0
        || share
            .descriptor
            .chunks
            .iter()
            .any(|chunk| share.paths.chunk_path(chunk.index).is_file())
}

pub(super) fn encode_message(message: &DiscoveryMessage) -> std::io::Result<Vec<u8>> {
    bincode_next::serde::encode_to_vec(message, bincode_next::config::standard())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub(super) fn decode_message(
    bytes: &[u8],
) -> Result<DiscoveryMessage, bincode_next::error::DecodeError> {
    if bytes.len() > MAX_DISCOVERY_PACKET_SIZE {
        return Err(bincode_next::error::DecodeError::LimitExceeded);
    }

    let (message, bytes_read): (DiscoveryMessage, usize) = bincode_next::serde::decode_from_slice(
        bytes,
        bincode_next::config::standard().with_limit::<MAX_DISCOVERY_PACKET_SIZE>(),
    )?;

    if bytes_read != bytes.len() {
        return Err(bincode_next::error::DecodeError::OtherString(format!(
            "discovery message has trailing bytes: decoded {bytes_read} of {} bytes",
            bytes.len()
        )));
    }

    Ok(message)
}
