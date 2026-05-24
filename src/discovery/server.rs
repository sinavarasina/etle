use super::prelude::*;
use super::{options::DiscoveryOptions, wire::DiscoveryMessage};

pub async fn serve(
    library_root: impl AsRef<Path>,
    p2p_listen: SocketAddr,
    peer_id: impl Into<String>,
    discovery_port: u16,
) -> std::io::Result<()> {
    serve_with(
        library_root,
        p2p_listen,
        peer_id,
        DiscoveryOptions::new(discovery_port),
    )
    .await
}

pub async fn serve_with(
    library_root: impl AsRef<Path>,
    p2p_listen: SocketAddr,
    peer_id: impl Into<String>,
    options: DiscoveryOptions,
) -> std::io::Result<()> {
    let library_root = library_root.as_ref().to_path_buf();
    let peer_id = peer_id.into();
    let instance_id = super::network::discovery_instance_id(&library_root, p2p_listen, &peer_id);
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), options.port);
    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

    if let Some(multicast) = options.multicast {
        super::network::join_multicast_on_active_interfaces(&socket, multicast);
    }

    let mut buffer = [0_u8; MAX_DISCOVERY_PACKET_SIZE];
    loop {
        let (read, source) = socket.recv_from(&mut buffer).await?;
        let Ok(message) = super::network::decode_message(&buffer[..read]) else {
            continue;
        };

        let DiscoveryMessage::Query { magic, share_id } = message else {
            continue;
        };
        if magic != DISCOVERY_MAGIC {
            continue;
        }

        let Some(name) = super::network::local_share_name(&library_root, share_id) else {
            continue;
        };

        let response = DiscoveryMessage::Response {
            magic: DISCOVERY_MAGIC.to_string(),
            share_id,
            listen_addr: super::network::advertised_p2p_addr(p2p_listen, source.ip()),
            listen_port: p2p_listen.port(),
            peer_id: peer_id.clone(),
            instance_id: instance_id.clone(),
            name,
        };

        if let Ok(payload) = super::network::encode_message(&response) {
            let _ = socket.send_to(&payload, source).await;
        }
    }
}
