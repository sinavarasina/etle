use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    network::error::NetworkError,
    protocol::{WireMessage, receive_message, send_message},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloPeer {
    pub peer_id: String,
}

pub async fn client_hello<S>(
    stream: &mut S,
    local_peer_id: impl Into<String>,
) -> Result<HelloPeer, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send_message(
        stream,
        &WireMessage::Hello {
            peer_id: local_peer_id.into(),
        },
    )
    .await?;

    receive_hello(stream).await
}

pub async fn server_hello<S>(
    stream: &mut S,
    local_peer_id: impl Into<String>,
) -> Result<HelloPeer, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer = receive_hello(stream).await?;

    send_message(
        stream,
        &WireMessage::Hello {
            peer_id: local_peer_id.into(),
        },
    )
    .await?;

    Ok(remote_peer)
}

async fn receive_hello<S>(stream: &mut S) -> Result<HelloPeer, NetworkError>
where
    S: AsyncRead + Unpin,
{
    match receive_message(stream).await? {
        WireMessage::Hello { peer_id } => Ok(HelloPeer { peer_id }),
        _ => Err(NetworkError::ExpectedHello),
    }
}
