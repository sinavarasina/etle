use std::path::Path;

#[cfg(unix)]
use std::{fs, path::PathBuf};

#[cfg(unix)]
use tokio::net::UnixListener;

use crate::{
    ipc::{IpcCommand, IpcError, IpcResponse, IpcShareSummary},
    state::{LocalShareSummary, ShareMode, list_library_shares},
};

#[cfg(unix)]
use crate::ipc::{receive_ipc_message, send_ipc_message};

#[cfg(unix)]
pub async fn serve_ipc_forever(
    socket_path: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let socket_path = socket_path.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();
    let listener = bind_ipc_listener(&socket_path)?;
    let _cleanup = IpcSocketCleanup::new(socket_path);

    loop {
        let should_shutdown = serve_ipc_once(&listener, &library_root).await?;
        if should_shutdown {
            break;
        }
    }

    Ok(())
}

#[cfg(not(unix))]
pub async fn serve_ipc_forever(
    _socket_path: impl AsRef<Path>,
    _library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    Err(IpcError::UnsupportedPlatform(
        "local daemon IPC currently uses Unix sockets; Windows named pipes will be added later",
    ))
}

#[cfg(unix)]
pub async fn serve_ipc_once(
    listener: &UnixListener,
    library_root: impl AsRef<Path>,
) -> Result<bool, IpcError> {
    let (mut stream, _) = listener.accept().await?;
    let command: IpcCommand = receive_ipc_message(&mut stream).await?;
    let should_shutdown = matches!(command, IpcCommand::Shutdown);
    let response = handle_ipc_command(command, library_root.as_ref());
    send_ipc_message(&mut stream, &response).await?;
    Ok(should_shutdown)
}

pub fn handle_ipc_command(command: IpcCommand, library_root: &Path) -> IpcResponse {
    match command {
        IpcCommand::Ping => IpcResponse::Pong,
        IpcCommand::ListShares => match list_library_shares(library_root) {
            Ok(shares) => IpcResponse::Shares {
                shares: shares
                    .into_iter()
                    .map(ipc_share_summary_from_local)
                    .collect(),
            },
            Err(error) => IpcResponse::Error {
                message: error.to_string(),
            },
        },
        IpcCommand::Shutdown => IpcResponse::Ack {
            message: "daemon shutdown requested".to_string(),
        },
        IpcCommand::StartServing { .. } => IpcResponse::Ack {
            message: "P2P serving is already managed by etled serve".to_string(),
        },
        IpcCommand::StopServing => IpcResponse::Error {
            message: "stopping only the P2P server is not implemented yet; use Shutdown"
                .to_string(),
        },
        IpcCommand::Download { .. } => IpcResponse::Error {
            message: "daemon-managed downloads are not implemented yet".to_string(),
        },
        IpcCommand::Pause { .. } | IpcCommand::Resume { .. } => IpcResponse::Error {
            message: "pause/resume control is not implemented yet".to_string(),
        },
        IpcCommand::SubscribeEvents => IpcResponse::Error {
            message: "event subscription is not implemented yet".to_string(),
        },
    }
}

#[cfg(unix)]
fn bind_ipc_listener(socket_path: &Path) -> Result<UnixListener, IpcError> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if socket_path.exists() {
        fs::remove_file(socket_path)?;
    }

    Ok(UnixListener::bind(socket_path)?)
}

fn ipc_share_summary_from_local(share: LocalShareSummary) -> IpcShareSummary {
    let completed_chunks = share.completed_chunks();
    let total_chunks = share.total_chunks();
    let mode = share.mode().map(format_share_mode).map(str::to_string);

    IpcShareSummary {
        share_id: share.descriptor.share_id,
        name: share.descriptor.name,
        completed_chunks,
        total_chunks,
        has_secret: share.has_secret,
        mode,
    }
}

fn format_share_mode(mode: ShareMode) -> &'static str {
    match mode {
        ShareMode::Seeding => "seeding",
        ShareMode::Downloading => "downloading",
        ShareMode::Completed => "completed",
        ShareMode::Paused => "paused",
    }
}

#[cfg(unix)]
struct IpcSocketCleanup {
    socket_path: PathBuf,
}

#[cfg(unix)]
impl IpcSocketCleanup {
    fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }
}

#[cfg(unix)]
impl Drop for IpcSocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}
