use super::prelude::*;
use super::{options::DiscoveryOptions, wire::DiscoveryMessage};

pub async fn peers(
    share_id: ShareId,
    discovery_port: u16,
    timeout: Duration,
) -> std::io::Result<Vec<SocketAddr>> {
    peers_with(
        share_id,
        DiscoveryOptions::new(discovery_port).with_timeout(timeout),
    )
    .await
}

pub async fn peers_with(
    share_id: ShareId,
    options: DiscoveryOptions,
) -> std::io::Result<Vec<SocketAddr>> {
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).await?;
    socket.set_broadcast(true)?;

    let query = DiscoveryMessage::Query {
        magic: DISCOVERY_MAGIC.to_string(),
        share_id,
    };
    let payload = super::network::encode_message(&query)?;

    for target in super::network::query_targets(options.port, options.multicast) {
        let _ = socket.send_to(&payload, target).await;
    }

    let deadline = time::Instant::now() + options.timeout;
    let mut peers_by_instance = BTreeMap::<String, SocketAddr>::new();
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
            listen_addr,
            listen_port,
            instance_id,
            ..
        }) = super::network::decode_message(&buffer[..read])
        else {
            continue;
        };

        if magic != DISCOVERY_MAGIC || response_share_id != share_id {
            continue;
        }

        let peer_addr = if listen_addr.ip().is_unspecified() {
            SocketAddr::new(source.ip(), listen_port)
        } else {
            listen_addr
        };

        if instance_id.is_empty() {
            peers.insert(peer_addr);
        } else {
            super::network::insert_preferred_peer(&mut peers_by_instance, instance_id, peer_addr);
        }
    }

    peers.extend(peers_by_instance.into_values());
    Ok(peers.into_iter().collect())
}
