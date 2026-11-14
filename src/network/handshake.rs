use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    network::error::NetworkError,
    protocol::{
        codec::{receive_control, send},
        message::{
            CAPABILITY_PSK_AUTH, CAPABILITY_RAW_CHUNK_FRAME, CAPABILITY_WINDOWED_REQUESTS,
            ETLE_WIRE_PROTOCOL_VERSION, WireMessage,
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

    match receive_control(stream).await? {
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
    let remote_peer_id = match receive_control(stream).await? {
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
    exchange_capabilities_as_client(stream, None).await?;
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
    exchange_capabilities_as_server(stream, None).await?;
    Ok(remote_peer_id)
}

pub async fn client_transfer_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
    auth_required: bool,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer_id = client_hello_handshake(stream, peer_id).await?;
    exchange_capabilities_as_client(stream, Some(auth_required)).await?;
    Ok(remote_peer_id)
}

pub async fn server_transfer_handshake<S>(
    stream: &mut S,
    peer_id: impl Into<String>,
    auth_required: bool,
) -> Result<String, NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let remote_peer_id = server_hello_handshake(stream, peer_id).await?;
    exchange_capabilities_as_server(stream, Some(auth_required)).await?;
    Ok(remote_peer_id)
}

async fn exchange_capabilities_as_client<S>(
    stream: &mut S,
    auth_required: Option<bool>,
) -> Result<(), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send_local_capabilities(stream, auth_required.unwrap_or(false)).await?;
    receive_and_validate_peer_capabilities(stream, auth_required).await
}

async fn exchange_capabilities_as_server<S>(
    stream: &mut S,
    auth_required: Option<bool>,
) -> Result<(), NetworkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    receive_and_validate_peer_capabilities(stream, auth_required).await?;
    send_local_capabilities(stream, auth_required.unwrap_or(false)).await
}

async fn send_local_capabilities<S>(stream: &mut S, auth_required: bool) -> Result<(), NetworkError>
where
    S: AsyncWrite + Unpin,
{
    send(
        stream,
        &WireMessage::Capabilities {
            protocol_version: ETLE_WIRE_PROTOCOL_VERSION,
            features: local_capabilities(auth_required),
        },
    )
    .await?;

    Ok(())
}

async fn receive_and_validate_peer_capabilities<S>(
    stream: &mut S,
    expected_auth_required: Option<bool>,
) -> Result<(), NetworkError>
where
    S: AsyncRead + Unpin,
{
    let (protocol_version, features) = match receive_control(stream).await? {
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

    if let Some(auth_required) = expected_auth_required {
        let peer_auth_required = features
            .iter()
            .any(|feature| feature == CAPABILITY_PSK_AUTH);
        if peer_auth_required != auth_required {
            return Err(NetworkError::PeerAuthenticationFailed);
        }
    }

    Ok(())
}

fn local_capabilities(auth_required: bool) -> Vec<String> {
    let mut capabilities: Vec<String> = required_capabilities()
        .iter()
        .map(|capability| (*capability).to_string())
        .collect();
    if auth_required {
        capabilities.push(CAPABILITY_PSK_AUTH.to_string());
    }
    capabilities
}

fn required_capabilities() -> [&'static str; 2] {
    [CAPABILITY_RAW_CHUNK_FRAME, CAPABILITY_WINDOWED_REQUESTS]
}
