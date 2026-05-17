use std::{fs, path::PathBuf};

use etle::{
    crypto::hash::{FileId, hash_file},
    network::{
        NetworkError, bind_listener, client_hello_handshake, client_key_exchange, connect_peer,
        download_file_from_peer, serve_file_to_one_peer, server_hello_handshake,
        server_key_exchange,
    },
    protocol::{WireMessage, send_message},
};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-network-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn tcp_listener_accepts_client_connection() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (_stream, _addr) = listener.accept().await.unwrap();
    });

    let _client = connect_peer(addr).await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn hello_handshake_over_tcp_succeeds() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_hello_handshake(&mut stream, "seeder").await.unwrap()
    });

    let mut client = connect_peer(addr).await.unwrap();
    let server_peer_id = client_hello_handshake(&mut client, "peer").await.unwrap();
    let client_peer_id = server.await.unwrap();

    assert_eq!(server_peer_id, "seeder");
    assert_eq!(client_peer_id, "peer");
}

#[tokio::test]
async fn server_rejects_non_hello_handshake_message() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_hello_handshake(&mut stream, "seeder").await
    });

    let mut client = connect_peer(addr).await.unwrap();
    send_message(&mut client, &WireMessage::RequestManifest)
        .await
        .unwrap();

    assert!(matches!(
        server.await.unwrap(),
        Err(NetworkError::UnexpectedMessage { .. })
    ));
}

#[tokio::test]
async fn key_exchange_over_tcp_derives_same_file_key() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let file_id = FileId([11_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_key_exchange(&mut stream, file_id).await.unwrap()
    });

    let mut client = connect_peer(addr).await.unwrap();
    let client_key = client_key_exchange(&mut client, file_id).await.unwrap();
    let server_key = server.await.unwrap();

    assert_eq!(client_key, server_key);
}

#[tokio::test]
async fn server_rejects_non_key_exchange_message() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let file_id = FileId([11_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_key_exchange(&mut stream, file_id).await
    });

    let mut client = connect_peer(addr).await.unwrap();
    send_message(
        &mut client,
        &WireMessage::Hello {
            peer_id: "not-a-key-exchange".to_string(),
        },
    )
    .await
    .unwrap();

    assert!(matches!(
        server.await.unwrap(),
        Err(NetworkError::UnexpectedMessage { .. })
    ));
}

#[tokio::test]
async fn encrypted_file_transfer_over_tcp_reconstructs_output() {
    let input = temp_file_name("transfer-input.bin");
    let output = temp_file_name("transfer-output.bin");

    fs::write(
        &input,
        b"ETLE network transfer: chunk, encrypt, send, verify, decrypt, reconstruct.",
    )
    .unwrap();

    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_input = input.clone();

    let server = tokio::spawn(async move {
        serve_file_to_one_peer(listener, server_input, 8, "seeder")
            .await
            .unwrap();
    });

    let manifest = download_file_from_peer(addr, &output, "peer")
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(hash_file(&input).unwrap(), hash_file(&output).unwrap());
    let expected_chunks = manifest.file_size.div_ceil(manifest.chunk_size) as usize;

    assert_eq!(manifest.chunks.len(), expected_chunks);

    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
}
