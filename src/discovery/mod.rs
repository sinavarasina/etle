use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket, time};

use crate::{file::descriptor::ShareId, state::list_library_shares};

pub const DEFAULT_DISCOVERY_PORT: u16 = 37037;
const DISCOVERY_MAGIC: &str = "etle-discovery-v1";
const MAX_DISCOVERY_PACKET_SIZE: usize = 4096;

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
    let library_root = library_root.as_ref().to_path_buf();
    let peer_id = peer_id.into();
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), discovery_port);
    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

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
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).await?;
    socket.set_broadcast(true)?;

    let query = DiscoveryMessage::Query {
        magic: DISCOVERY_MAGIC.to_string(),
        share_id,
    };
    let payload = encode_message(&query)?;
    let broadcast = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), discovery_port);
    let localhost = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), discovery_port);

    let _ = socket.send_to(&payload, broadcast).await;
    let _ = socket.send_to(&payload, localhost).await;

    let deadline = time::Instant::now() + timeout;
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
