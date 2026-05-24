use std::{
    fs,
    net::{Ipv4Addr, SocketAddr, UdpSocket as StdUdpSocket},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use etle::{
    discovery::{
        client as discovery_client, options::DiscoveryOptions, server as discovery_server,
    },
    file::chunker::DEFAULT_CHUNK_SIZE,
    network::{
        tcp::bind_listener,
        transfer::{
            download,
            options::{DownloadFileOptions, ServeFileOptions, TransferLogLevel},
            seed, serve,
        },
    },
};

#[tokio::test]
async fn discovery_finds_one_local_seeder_and_downloads_without_manual_peer() {
    let root = temp_dir("discovery-download");
    let seeder_root = root.join("seeder");
    let downloader_root = root.join("downloader");
    fs::create_dir_all(&seeder_root).unwrap();
    fs::create_dir_all(&downloader_root).unwrap();

    let input = root.join("sample.bin");
    fs::write(&input, deterministic_bytes(2 * 1024 * 1024 + 777)).unwrap();

    let descriptor = seed::add(
        &input,
        DEFAULT_CHUNK_SIZE / 4,
        &seeder_root,
        TransferLogLevel::Quiet,
    )
    .unwrap();

    let listener = bind_listener((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let p2p_addr = listener.local_addr().unwrap();
    let discovery_port = free_udp_port();

    let p2p_root = seeder_root.clone();
    let p2p_task = tokio::spawn(async move {
        serve::library_forever(
            listener,
            p2p_root,
            ServeFileOptions::new("discovery-test-seeder", TransferLogLevel::Quiet),
        )
        .await
    });

    let discovery_root = seeder_root.clone();
    let discovery_task = tokio::spawn(async move {
        discovery_server::serve_with(
            discovery_root,
            p2p_addr,
            "discovery-test-seeder",
            DiscoveryOptions::new(discovery_port)
                .with_timeout(Duration::from_millis(500))
                .without_multicast(),
        )
        .await
    });

    let peers = wait_for_discovered_peers(descriptor.share_id, discovery_port)
        .await
        .expect("discovery should return one local seeder");

    assert_eq!(peers, vec![p2p_addr]);

    let output = root.join("downloaded.bin");
    let manifest = download::from_peers(
        peers,
        &output,
        DownloadFileOptions::new("discovery-test-downloader", TransferLogLevel::Quiet)
            .with_library_root(&downloader_root)
            .with_requested_share_id(Some(descriptor.share_id)),
    )
    .await
    .unwrap();

    assert_eq!(manifest.file_name, "sample.bin");
    assert_eq!(fs::read(&output).unwrap(), fs::read(&input).unwrap());

    p2p_task.abort();
    discovery_task.abort();
    let _ = fs::remove_dir_all(root);
}

async fn wait_for_discovered_peers(
    share_id: etle::file::descriptor::ShareId,
    discovery_port: u16,
) -> Option<Vec<SocketAddr>> {
    for _ in 0..20 {
        let peers = discovery_client::peers_with(
            share_id,
            DiscoveryOptions::new(discovery_port)
                .with_timeout(Duration::from_millis(150))
                .without_multicast(),
        )
        .await
        .unwrap();

        if !peers.is_empty() {
            return Some(peers);
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    None
}

fn free_udp_port() -> u16 {
    let socket = StdUdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket.local_addr().unwrap().port()
}

fn deterministic_bytes(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index.wrapping_mul(31).wrapping_add(7) % 251) as u8)
        .collect()
}

fn temp_dir(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("etle-{name}-{}-{millis}", std::process::id()))
}
