use std::path::Path;

use crate::ipc::{IpcCommand, IpcError, IpcResponse};

#[cfg(unix)]
use crate::ipc::{receive_ipc_message, send_ipc_message};

#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(unix)]
pub async fn send_ipc_command(
    socket_path: impl AsRef<Path>,
    command: IpcCommand,
) -> Result<IpcResponse, IpcError> {
    let mut stream = UnixStream::connect(socket_path).await?;
    send_ipc_message(&mut stream, &command).await?;
    receive_ipc_message(&mut stream).await
}

#[cfg(not(unix))]
pub async fn send_ipc_command(
    _socket_path: impl AsRef<Path>,
    _command: IpcCommand,
) -> Result<IpcResponse, IpcError> {
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently uses Unix sockets; Windows named pipes will be added later",
    ))
}
