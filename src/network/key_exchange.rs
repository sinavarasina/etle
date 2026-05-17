use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    crypto::{
        aead::SymmetricKey,
        hash::FileId,
        key_exchange::{EphemeralKeypair, SharedSecretBytes, derive_file_key},
    },
    network::NetworkError,
    protocol::{WireMessage, receive_message, send_message},
};

pub async fn client_shared_secret_exchange<S>(
    stream: &mut S,
) -> Result<SharedSecretBytes, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let keypair = EphemeralKeypair::generate();
    let public_key = keypair.public_key();

    send_message(stream, &WireMessage::KeyExchange { public_key }).await?;

    let server_public_key = match receive_message(stream).await? {
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
    let client_public_key = match receive_message(stream).await? {
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

    send_message(stream, &WireMessage::KeyExchange { public_key }).await?;

    Ok(keypair.diffie_hellman(client_public_key)?)
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
