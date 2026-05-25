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
    let verbose = options.verbose;

    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

    if let Some(multicast) = options.multicast {
        super::network::join_multicast_on_active_interfaces(&socket, multicast);
    }

    if verbose {
        println!(
            "[discovery] server started: bind={bind_addr}, p2p_listen={p2p_listen}, library_root={}, peer_id={peer_id}, instance_id={instance_id}",
            library_root.display()
        );
    }

    let mut buffer = [0_u8; MAX_DISCOVERY_PACKET_SIZE];

    loop {
        let (read, source) = socket.recv_from(&mut buffer).await?;

        if verbose {
            println!("[discovery] udp packet from {source}, {read} bytes");
        }

        let Ok(message) = super::network::decode_message(&buffer[..read]) else {
            if verbose {
                println!("[discovery] drop: decode failed from {source}");
            }
            continue;
        };

        let DiscoveryMessage::Query { magic, share_id } = message else {
            if verbose {
                println!("[discovery] drop: not a query from {source}");
            }
            continue;
        };

        if verbose {
            println!("[discovery] query from {source}: share_id={share_id}");
        }

        if magic != DISCOVERY_MAGIC {
            if verbose {
                println!("[discovery] drop: bad magic from {source}: {magic:?}");
            }
            continue;
        }

        let Some(name) = super::network::local_share_name(&library_root, share_id) else {
            if verbose {
                println!("[discovery] drop: share not found locally: {share_id}");
            }
            continue;
        };

        let listen_addr = super::network::advertised_p2p_addr(p2p_listen, source.ip());

        if verbose {
            println!(
                "[discovery] responding to {source}: share_id={share_id}, name={name:?}, listen_addr={listen_addr}, listen_port={}",
                p2p_listen.port()
            );
        }

        let response = DiscoveryMessage::Response {
            magic: DISCOVERY_MAGIC.to_string(),
            share_id,
            listen_addr,
            listen_port: p2p_listen.port(),
            peer_id: peer_id.clone(),
            instance_id: instance_id.clone(),
            name,
        };

        match super::network::encode_message(&response) {
            Ok(payload) => {
                if let Err(error) = socket.send_to(&payload, source).await {
                    if verbose {
                        println!("[discovery] send response failed to {source}: {error}");
                    }
                } else if verbose {
                    println!(
                        "[discovery] response sent to {source}, {} bytes",
                        payload.len()
                    );
                }
            }
            Err(error) => {
                if verbose {
                    println!("[discovery] encode response failed for {source}: {error}");
                }
            }
        }
    }
}
