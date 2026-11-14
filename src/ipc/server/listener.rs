use super::prelude::*;

#[cfg(any(unix, windows))]
const IPC_COMMAND_READ_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(unix)]
pub async fn forever(
    socket_path: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let (_keep_alive_tx, rx) = tokio::sync::watch::channel(false);
    forever_with_shutdown(socket_path, library_root, rx).await
}

#[cfg(unix)]
pub async fn forever_with_shutdown(
    socket_path: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
    mut external_shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), IpcError> {
    let socket_path = socket_path.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();
    let bound = super::commands::bind_ipc_listener(&socket_path)?;
    let listener = bound.listener;
    let _cleanup = super::cleanup::IpcSocketCleanup::new(socket_path, bound.lock_file);
    let (local_shutdown_tx, mut local_shutdown_rx) = tokio::sync::watch::channel(false);

    loop {
        if *external_shutdown.borrow() || *local_shutdown_rx.borrow() {
            break;
        }

        tokio::select! {
            biased;
            () = wait_for_shutdown(&mut external_shutdown) => break,
            () = wait_for_shutdown(&mut local_shutdown_rx) => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let client_library_root = library_root.clone();
                let client_shutdown_tx = local_shutdown_tx.clone();
                tokio::spawn(async move {
                    match handle_client_stream(stream, &client_library_root).await {
                        Ok(true) => {
                            let _ = client_shutdown_tx.send(true);
                        }
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("[daemon] ipc client session error: {error}");
                        }
                    }
                });
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
pub async fn forever(
    pipe_name: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let (_keep_alive_tx, rx) = tokio::sync::watch::channel(false);
    forever_with_shutdown(pipe_name, library_root, rx).await
}

#[cfg(windows)]
pub async fn forever_with_shutdown(
    pipe_name: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
    mut external_shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), IpcError> {
    let pipe_name = pipe_name.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();
    let (local_shutdown_tx, mut local_shutdown_rx) = tokio::sync::watch::channel(false);
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)?;

    loop {
        if *external_shutdown.borrow() || *local_shutdown_rx.borrow() {
            break;
        }

        tokio::select! {
            biased;
            () = wait_for_shutdown(&mut external_shutdown) => break,
            () = wait_for_shutdown(&mut local_shutdown_rx) => break,
            connected = server.connect() => {
                connected?;
                let next_server = ServerOptions::new().create(&pipe_name)?;
                let connected_stream = std::mem::replace(&mut server, next_server);
                let client_library_root = library_root.clone();
                let client_shutdown_tx = local_shutdown_tx.clone();
                tokio::spawn(async move {
                    match handle_client_stream(connected_stream, &client_library_root).await {
                        Ok(true) => {
                            let _ = client_shutdown_tx.send(true);
                        }
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("[daemon] ipc client session error: {error}");
                        }
                    }
                });
            }
        }
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub async fn forever(
    _socket_path: impl AsRef<Path>,
    _library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let (_keep_alive_tx, rx) = tokio::sync::watch::channel(false);
    forever_with_shutdown(_socket_path, _library_root, rx).await
}

#[cfg(not(any(unix, windows)))]
pub async fn forever_with_shutdown(
    _socket_path: impl AsRef<Path>,
    _library_root: impl AsRef<Path>,
    _external_shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), IpcError> {
    let _ = (_socket_path, _library_root, _external_shutdown);
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently supports Unix sockets and Windows named pipes only",
    ))
}

#[cfg(any(unix, windows))]
async fn wait_for_shutdown(rx: &mut tokio::sync::watch::Receiver<bool>) {
    while !*rx.borrow_and_update() {
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(unix)]
pub async fn once(
    listener: &UnixListener,
    library_root: impl AsRef<Path>,
) -> Result<bool, IpcError> {
    let (stream, _) = listener.accept().await?;
    handle_client_stream(stream, library_root.as_ref()).await
}

#[cfg(any(unix, windows))]
async fn handle_client_stream<S>(mut stream: S, library_root: &Path) -> Result<bool, IpcError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let command: IpcCommand = match tokio::time::timeout(
        IPC_COMMAND_READ_TIMEOUT,
        receive_ipc_message(&mut stream),
    )
    .await
    {
        Ok(Ok(command)) => command,
        Ok(Err(error)) => {
            if !super::events::is_disconnected_client(&error) {
                eprintln!("[daemon] ignoring invalid or incomplete IPC client message: {error}");
            }
            return Ok(false);
        }
        Err(_) => {
            eprintln!("[daemon] timed out waiting for IPC client command");
            return Ok(false);
        }
    };

    if matches!(command, IpcCommand::SubscribeEvents) {
        let events = super::events::subscribe();
        tokio::spawn(async move {
            if let Err(error) = super::events::serve_subscription(&mut stream, events).await {
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
    let send_result = send_ipc_message(stream, response).await;
    if should_shutdown {
        super::events::publish(IpcEvent::ServerStopped);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    match send_result {
        Ok(()) => Ok(should_shutdown),
        Err(error) if super::events::is_disconnected_client(&error) => {
            eprintln!("[daemon] ipc client disconnected before response was delivered");
            Ok(should_shutdown)
        }
        Err(error) => {
            eprintln!("[daemon] ipc failed sending response to client: {error}");
            Ok(should_shutdown)
        }
    }
}
