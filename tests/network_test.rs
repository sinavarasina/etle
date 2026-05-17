use etle::crypto::hash::FileId;
use etle::network::{
    accept_peer, bind_listener, client_hello, client_key_exchange, connect_peer, server_hello,
    server_key_exchange,
};
use etle::protocol::{WireMessage, send_message};

#[tokio::test]
async fn tcp_listener_accepts_client_connection() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (_stream, remote_addr) = accept_peer(&listener).await.unwrap();
        remote_addr
    });

    let client = connect_peer(addr).await.unwrap();
    let client_addr = client.local_addr().unwrap();
    let remote_addr = server.await.unwrap();

    assert_eq!(remote_addr, client_addr);
}

#[tokio::test]
async fn hello_handshake_over_tcp_succeeds() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _remote_addr) = accept_peer(&listener).await.unwrap();
        server_hello(&mut stream, "seeder-01").await.unwrap()
    });

    let mut client = connect_peer(addr).await.unwrap();
    let server_peer = client_hello(&mut client, "peer-01").await.unwrap();
    let client_peer = server.await.unwrap();

    assert_eq!(server_peer.peer_id, "seeder-01");
    assert_eq!(client_peer.peer_id, "peer-01");
}

#[tokio::test]
async fn server_rejects_non_hello_handshake_message() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _remote_addr) = accept_peer(&listener).await.unwrap();
        server_hello(&mut stream, "seeder-01").await
    });

    let mut client = connect_peer(addr).await.unwrap();
    send_message(&mut client, &WireMessage::RequestManifest)
        .await
        .unwrap();

    let result = server.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn key_exchange_over_tcp_derives_same_file_key() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let file_id = FileId([42_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _remote_addr) = accept_peer(&listener).await.unwrap();
        server_key_exchange(&mut stream, file_id).await.unwrap()
    });

    let mut client = connect_peer(addr).await.unwrap();
    let client_key = client_key_exchange(&mut client, file_id).await.unwrap();
    let server_key = server.await.unwrap();

    assert_eq!(client_key.file_key, server_key.file_key);
    assert_ne!(client_key.remote_public_key, server_key.remote_public_key);
}

#[tokio::test]
async fn server_rejects_non_key_exchange_message() {
    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let file_id = FileId([42_u8; 32]);

    let server = tokio::spawn(async move {
        let (mut stream, _remote_addr) = accept_peer(&listener).await.unwrap();
        server_key_exchange(&mut stream, file_id).await
    });

    let mut client = connect_peer(addr).await.unwrap();
    send_message(
        &mut client,
        &WireMessage::Hello {
            peer_id: "not-key-exchange".to_string(),
        },
    )
    .await
    .unwrap();

    let result = server.await.unwrap();
    assert!(result.is_err());
}
