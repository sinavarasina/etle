use std::path::Path;

use crate::ipc::{IpcCommand, IpcError, IpcEvent, IpcResponse};

#[cfg(any(unix, windows))]
use crate::ipc::{receive_ipc_message, send_ipc_message};

#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(windows)]
use tokio::net::windows::named_pipe::ClientOptions;

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
    let mut stream = ClientOptions::new().open(pipe_name.as_ref())?;
    send_ipc_message(&mut stream, &command).await?;
    receive_ipc_message(&mut stream).await
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

#[cfg(unix)]
pub async fn subscribe_ipc_events<F>(
    socket_path: impl AsRef<Path>,
    mut on_event: F,
) -> Result<(), IpcError>
where
    F: FnMut(IpcEvent),
{
    let mut stream = UnixStream::connect(socket_path).await?;
    send_ipc_message(&mut stream, &IpcCommand::SubscribeEvents).await?;
    let _ack: IpcResponse = receive_ipc_message(&mut stream).await?;

    loop {
        let event: IpcEvent = receive_ipc_message(&mut stream).await?;
        on_event(event);
    }
}

#[cfg(windows)]
pub async fn subscribe_ipc_events<F>(
    pipe_name: impl AsRef<Path>,
    mut on_event: F,
) -> Result<(), IpcError>
where
    F: FnMut(IpcEvent),
{
    let mut stream = ClientOptions::new().open(pipe_name.as_ref())?;
    send_ipc_message(&mut stream, &IpcCommand::SubscribeEvents).await?;
    let _ack: IpcResponse = receive_ipc_message(&mut stream).await?;

    loop {
        let event: IpcEvent = receive_ipc_message(&mut stream).await?;
        on_event(event);
    }
}

#[cfg(not(any(unix, windows)))]
pub async fn subscribe_ipc_events<F>(
    _socket_path: impl AsRef<Path>,
    _on_event: F,
) -> Result<(), IpcError>
where
    F: FnMut(IpcEvent),
{
    let _ = _socket_path;
    let _ = _on_event;
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently supports Unix sockets and Windows named pipes only",
    ))
}
