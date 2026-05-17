use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    crypto::{
        aead::SymmetricKey,
        hash::FileId,
        key_exchange::{EphemeralKeypair, PublicKeyBytes, derive_file_key},
    },
    network::error::NetworkError,
    protocol::{WireMessage, receive_message, send_message},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EstablishedKey {
    pub remote_public_key: PublicKeyBytes,
    pub file_key: SymmetricKey,
}

/// Client-side key exchange.
///
/// The client sends its ephemeral X25519 public key first, then waits for the
/// server public key. Both sides derive the same file key from the shared
/// secret and the file id.
pub async fn client_key_exchange<S>(
    stream: &mut S,
    file_id: FileId,
) -> Result<EstablishedKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let local_keypair = EphemeralKeypair::generate();
    let local_public_key = local_keypair.public_key();

    send_message(
        stream,
        &WireMessage::KeyExchange {
            public_key: local_public_key,
        },
    )
    .await?;

    let remote_public_key = receive_key_exchange(stream).await?;
    let shared_secret = local_keypair.diffie_hellman(remote_public_key)?;
    let file_key = derive_file_key(shared_secret, file_id);

    Ok(EstablishedKey {
        remote_public_key,
        file_key,
    })
}

/// Server-side key exchange.
///
/// The server waits for the client ephemeral public key first, then sends its
/// own public key. Both sides derive the same file key from the shared secret
/// and the file id.
pub async fn server_key_exchange<S>(
    stream: &mut S,
    file_id: FileId,
) -> Result<EstablishedKey, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_public_key = receive_key_exchange(stream).await?;

    let local_keypair = EphemeralKeypair::generate();
    let local_public_key = local_keypair.public_key();

    send_message(
        stream,
        &WireMessage::KeyExchange {
            public_key: local_public_key,
        },
    )
    .await?;

    let shared_secret = local_keypair.diffie_hellman(remote_public_key)?;
    let file_key = derive_file_key(shared_secret, file_id);

    Ok(EstablishedKey {
        remote_public_key,
        file_key,
    })
}

async fn receive_key_exchange<S>(stream: &mut S) -> Result<PublicKeyBytes, NetworkError>
where
    S: AsyncRead + Unpin,
{
    match receive_message(stream).await? {
        WireMessage::KeyExchange { public_key } => Ok(public_key),
        _ => Err(NetworkError::ExpectedKeyExchange),
    }
}
