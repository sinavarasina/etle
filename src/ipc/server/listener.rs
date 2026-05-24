use super::prelude::*;

#[cfg(unix)]
pub async fn forever(
    socket_path: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let socket_path = socket_path.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();
    let listener = super::commands::bind_ipc_listener(&socket_path)?;
    let _cleanup = super::cleanup::IpcSocketCleanup::new(socket_path);

    loop {
        let should_shutdown = once(&listener, &library_root).await?;
        if should_shutdown {
            break;
        }
    }

    Ok(())
}

#[cfg(windows)]
pub async fn forever(
    pipe_name: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let pipe_name = pipe_name.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        let should_shutdown = once_named_pipe(&pipe_name, &library_root).await?;
        if should_shutdown {
            break;
        }
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub async fn forever(
    _socket_path: impl AsRef<Path>,
    _library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let _ = _socket_path;
    let _ = _library_root;
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently supports Unix sockets and Windows named pipes only",
    ))
}

#[cfg(unix)]
pub async fn once(
    listener: &UnixListener,
    library_root: impl AsRef<Path>,
) -> Result<bool, IpcError> {
    let (mut stream, _) = listener.accept().await?;
    let command: IpcCommand = receive_ipc_message(&mut stream).await?;
    if matches!(command, IpcCommand::SubscribeEvents) {
        tokio::spawn(async move {
            if let Err(error) = super::events::serve_subscription(&mut stream).await {
                eprintln!("[daemon] ipc event subscription closed: {error}");
            }
        });
        return Ok(false);
    }

    let should_shutdown = matches!(command, IpcCommand::Shutdown);
    let response = super::commands::handle_async(command, library_root.as_ref()).await;
    send_response_or_ignore_closed_client(&mut stream, &response, should_shutdown).await
}

#[cfg(windows)]
async fn once_named_pipe(pipe_name: &Path, library_root: &Path) -> Result<bool, IpcError> {
    let mut stream = ServerOptions::new().create(pipe_name)?;
    stream.connect().await?;

    let command: IpcCommand = receive_ipc_message(&mut stream).await?;
    if matches!(command, IpcCommand::SubscribeEvents) {
        tokio::spawn(async move {
            if let Err(error) = super::events::serve_subscription(&mut stream).await {
                eprintln!("[daemon] ipc event subscription closed: {error}");
            }
        });
        return Ok(false);
    }

    let should_shutdown = matches!(command, IpcCommand::Shutdown);
    let response = super::commands::handle_async(command, library_root).await;
    send_response_or_ignore_closed_client(&mut stream, &response, should_shutdown).await
}

async fn send_response_or_ignore_closed_client<W>(
    stream: &mut W,
    response: &IpcResponse,
    should_shutdown: bool,
) -> Result<bool, IpcError>
where
    W: AsyncWrite + Unpin,
{
    match send_ipc_message(stream, response).await {
        Ok(()) => Ok(should_shutdown),
        Err(error) if super::events::is_disconnected_client(&error) => {
            eprintln!("[daemon] ipc client disconnected before response was delivered");
            Ok(should_shutdown)
        }
        Err(error) => Err(error),
    }
}
