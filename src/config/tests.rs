use super::{load::default_listen_addr, parse::parse, prelude::*};

#[test]
fn parses_basic_config_file() {
    let config = parse(
        r#"
        library_root = "~/Downloads/ETLE"
        ipc_socket = "/tmp/etled.sock"
        listen = "0.0.0.0:7000"
        discovery_port = 37037
        discovery_multicast = "239.255.0.86"
        discovery_timeout_ms = 5000
        request_window = 32
        parallel = 0
        "#,
    )
    .unwrap();

    assert_eq!(config.library_root, Some(PathBuf::from("~/Downloads/ETLE")));
    assert_eq!(config.ipc_socket, Some(PathBuf::from("/tmp/etled.sock")));
    assert_eq!(config.listen, Some(default_listen_addr()));
    assert_eq!(config.discovery_port, Some(37037));
    assert_eq!(
        config.discovery_multicast,
        Some(DEFAULT_DISCOVERY_MULTICAST_ADDR)
    );
    assert_eq!(config.discovery_timeout_ms, Some(5000));
    assert_eq!(config.request_window, Some(32));
    assert_eq!(config.parallel, Some(0));
}

#[test]
fn ignores_comments_outside_strings() {
    let config = parse(
        r#"
        library_root = "/tmp/#not-comment" # comment
        request_window = 16 # comment
        "#,
    )
    .unwrap();

    assert_eq!(
        config.library_root,
        Some(PathBuf::from("/tmp/#not-comment"))
    );
    assert_eq!(config.request_window, Some(16));
}

#[test]
fn rejects_non_multicast_discovery_multicast() {
    assert!(parse(r#"discovery_multicast = "192.168.1.1""#).is_err());
}
