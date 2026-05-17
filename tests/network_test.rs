use etle::network::{accept_peer, bind_listener, client_hello, connect_peer, server_hello};
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
