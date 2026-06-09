mod common;

use common::{print_banner, print_kv, print_result, print_step};
use etle::{
    crypto::key_exchange::AuthPsk,
    network::key_exchange::{
        client_authenticated_session_key_exchange, server_authenticated_session_key_exchange,
    },
};

#[tokio::test]
async fn mitm_with_wrong_psk_is_rejected() {
    print_banner("mitm_with_wrong_psk_is_rejected");

    print_step(1, "prepare simulated client and server with mismatched PSK");
    let (mut client, mut server) = tokio::io::duplex(4096);
    let client_psk = AuthPsk::from_passphrase("correct passphrase");
    let server_psk = AuthPsk::from_passphrase("wrong passphrase");
    print_kv("client_psk", "correct passphrase");
    print_kv("server_psk", "wrong passphrase");

    print_step(2, "run authenticated session exchange");
    let server_handle = tokio::spawn(async move {
        server_authenticated_session_key_exchange(&mut server, &server_psk).await
    });
    let client_result = client_authenticated_session_key_exchange(&mut client, &client_psk).await;
    let server_result = server_handle.await.unwrap();
    print_kv("client_rejected", client_result.is_err());
    print_kv("server_rejected", server_result.is_err());

    assert!(client_result.is_err(), "client should reject wrong PSK");
    assert!(server_result.is_err(), "server should reject wrong PSK");
    print_result("mitm_with_wrong_psk_is_rejected", "ok");
}
