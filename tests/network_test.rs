mod common;

use common::{print_banner, print_kv, print_result, print_step};
use std::{fs, path::PathBuf};

use etle::{
    crypto::hash::{FileId, hash_file},
    network::{
        error::NetworkError,
        handshake::{client_hello_handshake, server_hello_handshake},
        key_exchange::{client_key_exchange, server_key_exchange},
        tcp::{bind_listener, connect_peer},
        transfer::{download, serve},
    },
    protocol::{codec::send, message::WireMessage},
};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-network-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn tcp_listener_accepts_client_connection() {
    print_banner("tcp_listener_accepts_client_connection");
    print_step(1, "bind TCP listener on an ephemeral localhost port");
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);

    let server = tokio::spawn(async move {
        let (_stream, _addr) = listener.accept().await.unwrap();
    });

    print_step(2, "connect client and accept server side");
    let _client = connect_peer(addr).await.unwrap();
    server.await.unwrap();
    print_result("tcp_listener_accepts_client_connection", "ok");
}

#[tokio::test]
async fn hello_handshake_over_tcp_succeeds() {
    print_banner("hello_handshake_over_tcp_succeeds");
    print_step(1, "bind listener and connect client");
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_hello_handshake(&mut stream, "seeder").await.unwrap()
    });

    print_step(2, "run hello handshake");
    let mut client = connect_peer(addr).await.unwrap();
    let server_peer_id = client_hello_handshake(&mut client, "peer").await.unwrap();
    let client_peer_id = server.await.unwrap();

    print_step(3, "verify peer ids");
    print_kv("server_peer_id", &server_peer_id);
    print_kv("client_peer_id", &client_peer_id);
    assert_eq!(server_peer_id, "seeder");
    assert_eq!(client_peer_id, "peer");
    print_result("hello_handshake_over_tcp_succeeds", "ok");
}

#[tokio::test]
async fn server_rejects_non_hello_handshake_message() {
    print_banner("server_rejects_non_hello_handshake_message");
    print_step(1, "bind listener and connect client");
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_hello_handshake(&mut stream, "seeder").await
    });

    print_step(2, "send wrong message before hello");
    let mut client = connect_peer(addr).await.unwrap();
    send(&mut client, &WireMessage::RequestManifest)
        .await
        .unwrap();

    let rejected = server.await.unwrap();
    print_step(3, "verify server rejects unexpected message");
    print_kv(
        "rejected",
        matches!(&rejected, Err(NetworkError::UnexpectedMessage { .. })),
    );
    assert!(matches!(
        &rejected,
        Err(NetworkError::UnexpectedMessage { .. })
    ));
    print_result("server_rejects_non_hello_handshake_message", "ok");
}

#[tokio::test]
async fn key_exchange_over_tcp_derives_same_file_key() {
    print_banner("key_exchange_over_tcp_derives_same_file_key");
    print_step(1, "bind listener and prepare file id");
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);
    let file_id = FileId([11_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_key_exchange(&mut stream, file_id).await.unwrap()
    });

    print_step(2, "run unauthenticated key exchange over TCP");
    let mut client = connect_peer(addr).await.unwrap();
    let client_key = client_key_exchange(&mut client, file_id).await.unwrap();
    let server_key = server.await.unwrap();

    print_step(3, "verify derived keys match");
    print_kv("keys_equal", client_key == server_key);
    assert_eq!(client_key, server_key);
    print_result("key_exchange_over_tcp_derives_same_file_key", "ok");
}

#[tokio::test]
async fn server_rejects_non_key_exchange_message() {
    print_banner("server_rejects_non_key_exchange_message");
    print_step(1, "bind listener and prepare expected file id");
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);
    let file_id = FileId([11_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_key_exchange(&mut stream, file_id).await
    });

    print_step(2, "send hello where key exchange is expected");
    let mut client = connect_peer(addr).await.unwrap();
    send(
        &mut client,
        &WireMessage::Hello {
            peer_id: "not-a-key-exchange".to_string(),
        },
    )
    .await
    .unwrap();

    let rejected = server.await.unwrap();
    print_step(3, "verify key exchange rejects wrong message");
    print_kv(
        "rejected",
        matches!(&rejected, Err(NetworkError::UnexpectedMessage { .. })),
    );
    assert!(matches!(
        &rejected,
        Err(NetworkError::UnexpectedMessage { .. })
    ));
    print_result("server_rejects_non_key_exchange_message", "ok");
}

#[tokio::test]
async fn encrypted_file_transfer_over_tcp_reconstructs_output() {
    print_banner("encrypted_file_transfer_over_tcp_reconstructs_output");
    print_step(1, "prepare input and output paths");
    let input = temp_file_name("transfer-input.bin");
    let output = temp_file_name("transfer-output.bin");

    print_kv("input", input.display());
    print_kv("output", output.display());
    fs::write(
        &input,
        b"ETLE network transfer: chunk, encrypt, send, verify, decrypt, reconstruct.",
    )
    .unwrap();

    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    print_kv("addr", addr);
    let server_input = input.clone();

    let server = tokio::spawn(async move {
        serve::file_once(listener, server_input, 8, "seeder")
            .await
            .unwrap();
    });

    print_step(3, "download from peer and wait for server task");
    let manifest = download::from_peer(addr, &output, "peer").await.unwrap();
    server.await.unwrap();

    print_step(4, "verify output hash and chunk count");
    let input_hash = hash_file(&input).unwrap();
    let output_hash = hash_file(&output).unwrap();
    print_kv("input_hash", input_hash);
    print_kv("output_hash", output_hash);
    assert_eq!(input_hash, output_hash);
    let expected_chunks = manifest.file_size.div_ceil(manifest.chunk_size) as usize;
    print_kv("expected_chunks", expected_chunks);
    print_kv("manifest_chunks", manifest.chunks.len());

    assert_eq!(manifest.chunks.len(), expected_chunks);

    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
    print_result("encrypted_file_transfer_over_tcp_reconstructs_output", "ok");
}
