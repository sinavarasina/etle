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
    let (mut client, mut server) = tokio::io::duplex(4096);
    let psk = AuthPsk::from_passphrase("same out-of-band passphrase");
    let server_psk = psk.clone();

    let server = tokio::spawn(async move {
        server_authenticated_session_key_exchange(&mut server, &server_psk).await
    });

    let client_key = client_authenticated_session_key_exchange(&mut client, &psk)
        .await
        .unwrap();
    let server_key = server.await.unwrap().unwrap();

    assert_eq!(client_key, server_key);
}

#[tokio::test]
async fn authenticated_session_key_exchange_rejects_wrong_psk() {
    let (mut client, mut server) = tokio::io::duplex(4096);
    let client_psk = AuthPsk::from_passphrase("client passphrase");
    let server_psk = AuthPsk::from_passphrase("server passphrase");

    let server = tokio::spawn(async move {
        server_authenticated_session_key_exchange(&mut server, &server_psk).await
    });

    let client = tokio::spawn(async move {
        client_authenticated_session_key_exchange(&mut client, &client_psk).await
    });

    let (client_result, server_result) = tokio::join!(client, server);

    assert!(client_result.unwrap().is_err());
    assert!(matches!(
        server_result.unwrap(),
        Err(NetworkError::PeerAuthenticationFailed)
    ));
}
