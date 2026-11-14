use std::path::Path;

use crate::ipc::{
    error::IpcError,
    message::{IpcCommand, IpcEvent, IpcResponse},
};

#[cfg(any(unix, windows))]
use crate::ipc::codec::{receive_ipc_message, send_ipc_message};

#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

/// Live streaming IPC event subscription returned after `SubscribeEvents` ack.
pub struct EventSubscription {
    #[cfg(unix)]
    stream: UnixStream,
    #[cfg(windows)]
    stream: NamedPipeClient,
}

impl EventSubscription {
    /// Connects to the daemon IPC endpoint and starts an `IpcCommand::SubscribeEvents` stream.
    #[cfg(unix)]
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let mut stream = UnixStream::connect(socket_path).await?;
        send_ipc_message(&mut stream, &IpcCommand::SubscribeEvents).await?;
        validate_subscription_ack(receive_ipc_message(&mut stream).await?)?;
        Ok(Self { stream })
    }

    /// Connects to the daemon named pipe and starts an `IpcCommand::SubscribeEvents` stream.
    #[cfg(windows)]
    pub async fn connect(pipe_name: impl AsRef<Path>) -> Result<Self, IpcError> {
        let mut stream = connect_named_pipe(pipe_name.as_ref()).await?;
        send_ipc_message(&mut stream, &IpcCommand::SubscribeEvents).await?;
        validate_subscription_ack(receive_ipc_message(&mut stream).await?)?;
        Ok(Self { stream })
    }

    #[cfg(not(any(unix, windows)))]
    pub async fn connect(_socket_path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let _ = _socket_path;
        Err(IpcError::UnsupportedPlatform(
            "local daemon IPC currently supports Unix sockets and Windows named pipes only",
        ))
    }

    /// Receives the next event emitted by the daemon.
    #[cfg(any(unix, windows))]
    pub async fn next_event(&mut self) -> Result<IpcEvent, IpcError> {
        receive_ipc_message(&mut self.stream).await
    }

    #[cfg(not(any(unix, windows)))]
    pub async fn next_event(&mut self) -> Result<IpcEvent, IpcError> {
        Err(IpcError::UnsupportedPlatform(
            "local daemon IPC currently supports Unix sockets and Windows named pipes only",
        ))
    }
}

#[cfg(unix)]
pub async fn send_ipc_command(
    socket_path: impl AsRef<Path>,
    command: IpcCommand,
) -> Result<IpcResponse, IpcError> {
    let mut stream = UnixStream::connect(socket_path).await?;
    send_ipc_message(&mut stream, &command).await?;
    receive_ipc_message(&mut stream).await
}

#[cfg(windows)]
pub async fn send_ipc_command(
    pipe_name: impl AsRef<Path>,
    command: IpcCommand,
) -> Result<IpcResponse, IpcError> {
    let mut stream = connect_named_pipe(pipe_name.as_ref()).await?;
    send_ipc_message(&mut stream, &command).await?;
    receive_ipc_message(&mut stream).await
}

#[cfg(any(unix, windows))]
fn validate_subscription_ack(response: IpcResponse) -> Result<(), IpcError> {
    match response {
        IpcResponse::Ack { .. } => Ok(()),
        IpcResponse::Error { message } => Err(IpcError::RemoteError(message)),
        other => Err(IpcError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unexpected subscription handshake response: {other:?}"),
        ))),
    }
}

#[cfg(windows)]
async fn connect_named_pipe(pipe_name: &Path) -> Result<NamedPipeClient, IpcError> {
    const ERROR_PIPE_BUSY: i32 = 231;
    const MAX_ATTEMPTS: usize = 80;

    for attempt in 0..MAX_ATTEMPTS {
        match ClientOptions::new().open(pipe_name) {
            Ok(client) => return Ok(client),
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempt + 1 < MAX_ATTEMPTS =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(error) => return Err(IpcError::Io(error)),
        }
    }

    unreachable!("loop always returns before exhausting MAX_ATTEMPTS")
}

#[cfg(not(any(unix, windows)))]
pub async fn send_ipc_command(
    _socket_path: impl AsRef<Path>,
    _command: IpcCommand,
) -> Result<IpcResponse, IpcError> {
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently supports Unix sockets and Windows named pipes only",
    ))
}

/// Subscribes to daemon events indefinitely until the daemon stops, stream closes, or errors.
pub async fn subscribe_ipc_events<F>(
    socket_path: impl AsRef<Path>,
    mut on_event: F,
) -> Result<(), IpcError>
where
    F: FnMut(IpcEvent),
{
    let mut subscription = EventSubscription::connect(socket_path).await?;
    loop {
        let event = subscription.next_event().await?;
        let stopped = matches!(&event, IpcEvent::ServerStopped);
        on_event(event);
        if stopped {
            return Ok(());
        }
    }
}
