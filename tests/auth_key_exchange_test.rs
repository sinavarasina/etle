mod common;

use common::{print_banner, print_kv, print_result, print_step};
use etle::{
    crypto::key_exchange::AuthPsk,
    network::{
        error::NetworkError,
        key_exchange::{
            client_authenticated_session_key_exchange, server_authenticated_session_key_exchange,
        },
    },
};

#[tokio::test]
async fn authenticated_session_key_exchange_derives_same_key() {
    print_banner("authenticated_session_key_exchange_derives_same_key");

    print_step(1, "prepare in-memory duplex stream and shared PSK");
    let (mut client, mut server) = tokio::io::duplex(4096);
    let psk = AuthPsk::from_passphrase("same out-of-band passphrase");
    let server_psk = psk.clone();
    print_kv("transport", "tokio::io::duplex");
    print_kv("psk", "same passphrase on both peers");

    print_step(2, "run authenticated key exchange concurrently");
    let server = tokio::spawn(async move {
        server_authenticated_session_key_exchange(&mut server, &server_psk).await
    });
    let client_key = client_authenticated_session_key_exchange(&mut client, &psk)
        .await
        .unwrap();
    let server_key = server.await.unwrap().unwrap();
    print_kv("session_keys_equal", client_key == server_key);

    assert_eq!(client_key, server_key);
    print_result("authenticated_session_key_exchange_derives_same_key", "ok");
}

#[tokio::test]
async fn authenticated_session_key_exchange_rejects_wrong_psk() {
    print_banner("authenticated_session_key_exchange_rejects_wrong_psk");

    print_step(1, "prepare peers with different PSK values");
    let (mut client, mut server) = tokio::io::duplex(4096);
    let client_psk = AuthPsk::from_passphrase("client passphrase");
    let server_psk = AuthPsk::from_passphrase("server passphrase");
    print_kv("client_psk", "client passphrase");
    print_kv("server_psk", "server passphrase");

    print_step(2, "run authenticated key exchange concurrently");
    let server = tokio::spawn(async move {
        server_authenticated_session_key_exchange(&mut server, &server_psk).await
    });
    let client = tokio::spawn(async move {
        client_authenticated_session_key_exchange(&mut client, &client_psk).await
    });
    let (client_result, server_result) = tokio::join!(client, server);
    let client_result = client_result.unwrap();
    let server_result = server_result.unwrap();
    print_kv("client_rejected", client_result.is_err());
    print_kv(
        "server_rejected_peer_auth",
        matches!(&server_result, Err(NetworkError::PeerAuthenticationFailed)),
    );

    assert!(client_result.is_err());
    assert!(matches!(
        server_result,
        Err(NetworkError::PeerAuthenticationFailed)
    ));
    print_result("authenticated_session_key_exchange_rejects_wrong_psk", "ok");
}
