use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    crypto::{
        aead::SymmetricKey,
        hash::FileId,
        key_exchange::{
            AuthPsk, AuthRole, AuthTag, EphemeralKeypair, PublicKeyBytes, SharedSecretBytes,
            auth_tags_equal, derive_auth_tag, derive_file_key, derive_session_key_with_transcript,
        },
    },
    network::error::NetworkError,
    protocol::{
        codec::{receive, send},
        message::WireMessage,
    },
};

pub async fn client_shared_secret_exchange<S>(
    stream: &mut S,
) -> Result<SharedSecretBytes, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let keypair = EphemeralKeypair::generate();
    let public_key = keypair.public_key();

    send(stream, &WireMessage::KeyExchange { public_key }).await?;

    let server_public_key = match receive(stream).await? {
        WireMessage::KeyExchange { public_key } => public_key,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "KeyExchange",
                actual,
            });
        }
    };

    Ok(keypair.diffie_hellman(server_public_key)?)
}

pub async fn server_shared_secret_exchange<S>(
    stream: &mut S,
) -> Result<SharedSecretBytes, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let client_public_key = match receive(stream).await? {
        WireMessage::KeyExchange { public_key } => public_key,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "KeyExchange",
                actual,
            });
        }
    };

    let keypair = EphemeralKeypair::generate();
    let public_key = keypair.public_key();

    send(stream, &WireMessage::KeyExchange { public_key }).await?;

    Ok(keypair.diffie_hellman(client_public_key)?)
}

pub async fn client_session_key_exchange<S>(stream: &mut S) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (session_key, _, _) = client_unauthenticated_session_key_exchange(stream).await?;
    Ok(session_key)
}

pub async fn server_session_key_exchange<S>(stream: &mut S) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (session_key, _, _) = server_unauthenticated_session_key_exchange(stream).await?;
    Ok(session_key)
}

pub async fn client_authenticated_session_key_exchange<S>(
    stream: &mut S,
    psk: &AuthPsk,
) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (session_key, client_public_key, server_public_key) =
        client_unauthenticated_session_key_exchange(stream).await?;

    let client_tag = derive_auth_tag(
        psk,
        &session_key,
        client_public_key,
        server_public_key,
        AuthRole::Client,
    );
    send(stream, &WireMessage::AuthProof { tag: client_tag }).await?;

    let server_tag = receive_auth_proof(stream).await?;
    let expected = derive_auth_tag(
        psk,
        &session_key,
        client_public_key,
        server_public_key,
        AuthRole::Server,
    );
    verify_auth_tag(&expected, &server_tag)?;

    Ok(session_key)
}

pub async fn server_authenticated_session_key_exchange<S>(
    stream: &mut S,
    psk: &AuthPsk,
) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (session_key, client_public_key, server_public_key) =
        server_unauthenticated_session_key_exchange(stream).await?;

    let client_tag = receive_auth_proof(stream).await?;
    let expected_client_tag = derive_auth_tag(
        psk,
        &session_key,
        client_public_key,
        server_public_key,
        AuthRole::Client,
    );
    verify_auth_tag(&expected_client_tag, &client_tag)?;

    let server_tag = derive_auth_tag(
        psk,
        &session_key,
        client_public_key,
        server_public_key,
        AuthRole::Server,
    );
    send(stream, &WireMessage::AuthProof { tag: server_tag }).await?;

    Ok(session_key)
}

async fn client_unauthenticated_session_key_exchange<S>(
    stream: &mut S,
) -> Result<(SymmetricKey, PublicKeyBytes, PublicKeyBytes), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let keypair = EphemeralKeypair::generate();
    let client_public_key = keypair.public_key();

    send(
        stream,
        &WireMessage::KeyExchange {
            public_key: client_public_key,
        },
    )
    .await?;

    let server_public_key = match receive(stream).await? {
        WireMessage::KeyExchange { public_key } => public_key,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "KeyExchange",
                actual,
            });
        }
    };

    let shared_secret = keypair.diffie_hellman(server_public_key)?;
    let session_key =
        derive_session_key_with_transcript(shared_secret, client_public_key, server_public_key);

    Ok((session_key, client_public_key, server_public_key))
}

async fn server_unauthenticated_session_key_exchange<S>(
    stream: &mut S,
) -> Result<(SymmetricKey, PublicKeyBytes, PublicKeyBytes), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let client_public_key = match receive(stream).await? {
        WireMessage::KeyExchange { public_key } => public_key,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "KeyExchange",
                actual,
            });
        }
    };

    let keypair = EphemeralKeypair::generate();
    let server_public_key = keypair.public_key();

    send(
        stream,
        &WireMessage::KeyExchange {
            public_key: server_public_key,
        },
    )
    .await?;

    let shared_secret = keypair.diffie_hellman(client_public_key)?;
    let session_key =
        derive_session_key_with_transcript(shared_secret, client_public_key, server_public_key);

    Ok((session_key, client_public_key, server_public_key))
}

async fn receive_auth_proof<S>(stream: &mut S) -> Result<AuthTag, NetworkError>
where
    S: AsyncRead + Unpin,
{
    match receive(stream).await? {
        WireMessage::AuthProof { tag } => Ok(tag),
        actual => Err(NetworkError::UnexpectedMessage {
            expected: "AuthProof",
            actual,
        }),
    }
}

fn verify_auth_tag(expected: &AuthTag, actual: &AuthTag) -> Result<(), NetworkError> {
    if auth_tags_equal(expected, actual) {
        Ok(())
    } else {
        Err(NetworkError::PeerAuthenticationFailed)
    }
}

pub async fn client_key_exchange<S>(
    stream: &mut S,
    file_id: FileId,
) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let shared_secret = client_shared_secret_exchange(stream).await?;
    Ok(derive_file_key(shared_secret, file_id))
}

pub async fn server_key_exchange<S>(
    stream: &mut S,
    file_id: FileId,
) -> Result<SymmetricKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let shared_secret = server_shared_secret_exchange(stream).await?;
    Ok(derive_file_key(shared_secret, file_id))
}
