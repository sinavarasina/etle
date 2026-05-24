use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    network::error::NetworkError,
    protocol::{
        codec::{receive, send},
        message::{
            CAPABILITY_RAW_CHUNK_FRAME, CAPABILITY_WINDOWED_REQUESTS, ETLE_WIRE_PROTOCOL_VERSION,
            WireMessage,
        },
    },
};

pub async fn client_hello_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send(
        stream,
        &WireMessage::Hello {
            peer_id: peer_id.into(),
        },
    )
    .await?;

    match receive(stream).await? {
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
    let remote_peer_id = match receive(stream).await? {
        WireMessage::Hello { peer_id } => peer_id,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Hello",
                actual,
            });
        }
    };

    send(
        stream,
        &WireMessage::Hello {
            peer_id: peer_id.into(),
        },
    )
    .await?;

    Ok(remote_peer_id)
}

pub async fn client_protocol_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer_id = client_hello_handshake(stream, peer_id).await?;
    exchange_capabilities_as_client(stream).await?;
    Ok(remote_peer_id)
}

pub async fn server_protocol_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer_id = server_hello_handshake(stream, peer_id).await?;
    exchange_capabilities_as_server(stream).await?;
    Ok(remote_peer_id)
}

async fn exchange_capabilities_as_client<S>(stream: &mut S) -> Result<(), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send_local_capabilities(stream).await?;
    receive_and_validate_peer_capabilities(stream).await
}

async fn exchange_capabilities_as_server<S>(stream: &mut S) -> Result<(), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    receive_and_validate_peer_capabilities(stream).await?;
    send_local_capabilities(stream).await
}

async fn send_local_capabilities<S>(stream: &mut S) -> Result<(), NetworkError>
where
    S: AsyncWrite + Unpin,
{
    send(
        stream,
        &WireMessage::Capabilities {
            protocol_version: ETLE_WIRE_PROTOCOL_VERSION,
            features: local_capabilities(),
        },
    )
    .await?;

    Ok(())
}

async fn receive_and_validate_peer_capabilities<S>(stream: &mut S) -> Result<(), NetworkError>
where
    S: AsyncRead + Unpin,
{
    let (protocol_version, features) = match receive(stream).await? {
        WireMessage::Capabilities {
            protocol_version,
            features,
        } => (protocol_version, features),
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Capabilities",
                actual,
            });
        }
    };

    if protocol_version != ETLE_WIRE_PROTOCOL_VERSION {
        return Err(NetworkError::UnsupportedProtocolVersion {
            peer: protocol_version,
            supported: ETLE_WIRE_PROTOCOL_VERSION,
        });
    }

    for required in required_capabilities() {
        if !features.iter().any(|feature| feature == required) {
            return Err(NetworkError::MissingPeerCapability(required.to_string()));
        }
    }

    Ok(())
}

fn local_capabilities() -> Vec<String> {
    required_capabilities()
        .iter()
        .map(|capability| (*capability).to_string())
        .collect()
}

fn required_capabilities() -> [&'static str; 2] {
    [CAPABILITY_RAW_CHUNK_FRAME, CAPABILITY_WINDOWED_REQUESTS]
}
