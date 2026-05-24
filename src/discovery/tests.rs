use super::prelude::*;

#[test]
fn includes_global_loopback_and_multicast_targets() {
    let targets = super::network::query_targets(37037, Some(DEFAULT_DISCOVERY_MULTICAST_ADDR));

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
        super::network::subnet_broadcast(
            Ipv4Addr::new(192, 168, 1, 23),
            Ipv4Addr::new(255, 255, 255, 0)
        ),
        Some(Ipv4Addr::new(192, 168, 1, 255))
    );
}

#[test]
fn advertised_addr_uses_explicit_loopback_listen_addr() {
    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7000);
    let source = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

    assert_eq!(super::network::advertised_p2p_addr(listen, source), listen);
}

#[test]
fn advertised_addr_keeps_unspecified_listen_addr_for_client_resolution() {
    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7000);
    let source = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

    assert_eq!(super::network::advertised_p2p_addr(listen, source), listen);
}

#[test]
fn preferred_peer_deduplicates_same_discovery_instance() {
    let mut peers = BTreeMap::new();
    super::network::insert_preferred_peer(
        &mut peers,
        "same".to_string(),
        "192.168.1.20:7000".parse().unwrap(),
    );
    super::network::insert_preferred_peer(
        &mut peers,
        "same".to_string(),
        "127.0.0.1:7000".parse().unwrap(),
    );

    assert_eq!(peers.len(), 1);
    assert_eq!(peers["same"], "127.0.0.1:7000".parse().unwrap());
}
