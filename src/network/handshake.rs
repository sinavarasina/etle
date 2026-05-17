use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    network::NetworkError,
    protocol::{WireMessage, receive_message, send_message},
};

pub async fn client_hello_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send_message(
        stream,
        &WireMessage::Hello {
            peer_id: peer_id.into(),
        },
    )
    .await?;

    match receive_message(stream).await? {
        WireMessage::Hello { peer_id } => Ok(peer_id),
        actual => Err(NetworkError::UnexpectedMessage {
            expected: "Hello",
            actual,
        }),
    }
}

pub async fn server_hello_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer_id = match receive_message(stream).await? {
        WireMessage::Hello { peer_id } => peer_id,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Hello",
                actual,
            });
        }
    };

    send_message(
        stream,
        &WireMessage::Hello {
            peer_id: peer_id.into(),
        },
    )
    .await?;

    Ok(remote_peer_id)
}
