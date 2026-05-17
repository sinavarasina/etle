use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use tokio::net::UnixListener;

#[cfg(windows)]
use tokio::net::windows::named_pipe::ServerOptions;

use crate::{
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId, manifest::Manifest},
    ipc::{IpcCommand, IpcError, IpcResponse, IpcShareSummary},
    network::{
        DownloadFileOptions, TransferLogLevel, add_file_to_library,
        download_file_from_peers_parallel_with_options, download_file_from_peers_with_options,
    },
    state::{LocalShareSummary, OUTPUT_DIR_NAME, ShareMode, list_library_shares},
};

#[cfg(any(unix, windows))]
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

#[cfg(windows)]
pub async fn serve_ipc_forever(
    pipe_name: impl AsRef<Path>,
    library_root: impl AsRef<Path>,
) -> Result<(), IpcError> {
    let pipe_name = pipe_name.as_ref().to_path_buf();
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        let should_shutdown = serve_ipc_once_named_pipe(&pipe_name, &library_root).await?;
        if should_shutdown {
            break;
        }
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub async fn serve_ipc_forever(
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
pub async fn serve_ipc_once(
    listener: &UnixListener,
    library_root: impl AsRef<Path>,
) -> Result<bool, IpcError> {
    let (mut stream, _) = listener.accept().await?;
    let command: IpcCommand = receive_ipc_message(&mut stream).await?;
    let should_shutdown = matches!(command, IpcCommand::Shutdown);
    let response = handle_ipc_command_async(command, library_root.as_ref()).await;
    send_ipc_message(&mut stream, &response).await?;
    Ok(should_shutdown)
}

#[cfg(windows)]
async fn serve_ipc_once_named_pipe(
    pipe_name: &Path,
    library_root: &Path,
) -> Result<bool, IpcError> {
    let mut stream = ServerOptions::new().create(pipe_name)?;
    stream.connect().await?;

    let command: IpcCommand = receive_ipc_message(&mut stream).await?;
    let should_shutdown = matches!(command, IpcCommand::Shutdown);
    let response = handle_ipc_command_async(command, library_root).await;
    send_ipc_message(&mut stream, &response).await?;

    Ok(should_shutdown)
}

pub fn handle_ipc_command(command: IpcCommand, library_root: &Path) -> IpcResponse {
    match command {
        IpcCommand::Ping => IpcResponse::Pong,
        IpcCommand::ListShares => list_shares_response(library_root),
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
        IpcCommand::SeedFile { .. } => IpcResponse::Error {
            message: "seed requires async daemon handling".to_string(),
        },
        IpcCommand::Download { .. } | IpcCommand::DownloadFresh { .. } => IpcResponse::Error {
            message: "download requires async daemon handling".to_string(),
        },
        IpcCommand::Pause { .. } | IpcCommand::Resume { .. } => IpcResponse::Error {
            message: "pause/resume control is not implemented yet".to_string(),
        },
        IpcCommand::SubscribeEvents => IpcResponse::Error {
            message: "event subscription is not implemented yet".to_string(),
        },
    }
}

pub async fn handle_ipc_command_async(command: IpcCommand, library_root: &Path) -> IpcResponse {
    match command {
        IpcCommand::SeedFile { input, chunk_size } => {
            queue_seed_file_command(input, chunk_size, library_root)
        }
        IpcCommand::Download {
            share_id,
            peers,
            output,
            parallelism,
        } => queue_download_command(share_id, peers, output, parallelism, true, library_root),
        IpcCommand::DownloadFresh {
            share_id,
            peers,
            output,
            parallelism,
        } => queue_download_command(share_id, peers, output, parallelism, false, library_root),
        command => handle_ipc_command(command, library_root),
    }
}

fn queue_seed_file_command(input: PathBuf, chunk_size: usize, library_root: &Path) -> IpcResponse {
    let input_label = input.display().to_string();
    let library_root = library_root.to_path_buf();

    tokio::task::spawn_blocking(move || {
        match run_seed_file_command(input, chunk_size, &library_root) {
            IpcResponse::ShareAdded { share } => {
                println!("[daemon] seed job completed");
                println!(
                    "[daemon] share {}  chunks={}/{}  key={}  name=\"{}\"",
                    share.share_id,
                    share.completed_chunks,
                    share.total_chunks,
                    if share.has_secret { "yes" } else { "no" },
                    share.name
                );
            }
            IpcResponse::Error { message } => eprintln!("[daemon] seed job failed: {message}"),
            other => println!("[daemon] seed job finished: {other:?}"),
        }
    });

    IpcResponse::Ack {
        message: format!("seed job queued: {input_label}"),
    }
}

fn run_seed_file_command(input: PathBuf, chunk_size: usize, library_root: &Path) -> IpcResponse {
    let chunk_size = if chunk_size == 0 {
        DEFAULT_CHUNK_SIZE
    } else {
        chunk_size
    };

    match add_file_to_library(&input, chunk_size, library_root, TransferLogLevel::Normal) {
        Ok(descriptor) => match share_summary_by_id(library_root, descriptor.share_id) {
            Ok(Some(share)) => IpcResponse::ShareAdded { share },
            Ok(None) => IpcResponse::Error {
                message: format!(
                    "share {} was added but cannot be found in the local library",
                    descriptor.share_id
                ),
            },
            Err(error) => IpcResponse::Error { message: error },
        },
        Err(error) => IpcResponse::Error {
            message: error.to_string(),
        },
    }
}

fn queue_download_command(
    share_id: ShareId,
    peers: Vec<SocketAddr>,
    output: Option<PathBuf>,
    parallelism: usize,
    resume: bool,
    library_root: &Path,
) -> IpcResponse {
    let library_root = library_root.to_path_buf();

    tokio::spawn(async move {
        match handle_download_command(share_id, peers, output, parallelism, resume, &library_root)
            .await
        {
            IpcResponse::TransferCompleted {
                output,
                file_name,
                file_size,
                chunks,
                ..
            } => {
                println!("[daemon] download job completed: {share_id}");
                println!("[daemon] output: {}", output.display());
                println!("[daemon] file: {file_name}");
                println!("[daemon] file size: {file_size} bytes");
                println!("[daemon] chunks: {chunks}");
            }
            IpcResponse::Error { message } => eprintln!("[daemon] download job failed: {message}"),
            other => println!("[daemon] download job finished: {other:?}"),
        }
    });

    IpcResponse::TransferQueued { share_id }
}

async fn handle_download_command(
    share_id: ShareId,
    peers: Vec<SocketAddr>,
    output: Option<PathBuf>,
    parallelism: usize,
    resume: bool,
    library_root: &Path,
) -> IpcResponse {
    let auto_output = output.is_none();
    let output_path = output.unwrap_or_else(|| temporary_download_output_path(library_root));

    if let Err(error) = create_output_parent_dir(&output_path) {
        return IpcResponse::Error {
            message: error.to_string(),
        };
    }

    let options = DownloadFileOptions::new("etle-daemon", TransferLogLevel::Normal)
        .with_library_root(library_root.to_path_buf())
        .with_resume(resume)
        .with_requested_share_id(Some(share_id));

    let result = if parallelism > 1 {
        download_file_from_peers_parallel_with_options(peers, &output_path, options, parallelism)
            .await
    } else {
        download_file_from_peers_with_options(peers, &output_path, options).await
    };

    let manifest = match result {
        Ok(manifest) => manifest,
        Err(error) => {
            return IpcResponse::Error {
                message: error.to_string(),
            };
        }
    };

    let final_output = if auto_output {
        match move_auto_download_output(&output_path, library_root, &manifest.file_name) {
            Ok(path) => path,
            Err(error) => {
                return IpcResponse::Error {
                    message: error.to_string(),
                };
            }
        }
    } else {
        output_path
    };

    transfer_completed_response(share_id, final_output, &manifest)
}

fn list_shares_response(library_root: &Path) -> IpcResponse {
    match list_library_shares(library_root) {
        Ok(shares) => IpcResponse::Shares {
            shares: shares
                .into_iter()
                .map(ipc_share_summary_from_local)
                .collect(),
        },
        Err(error) => IpcResponse::Error {
            message: error.to_string(),
        },
    }
}

fn share_summary_by_id(
    library_root: &Path,
    share_id: ShareId,
) -> Result<Option<IpcShareSummary>, String> {
    let shares = list_library_shares(library_root).map_err(|error| error.to_string())?;
    Ok(shares
        .into_iter()
        .find(|share| share.descriptor.share_id == share_id)
        .map(ipc_share_summary_from_local))
}

fn transfer_completed_response(
    share_id: ShareId,
    output: PathBuf,
    manifest: &Manifest,
) -> IpcResponse {
    IpcResponse::TransferCompleted {
        share_id,
        output,
        file_name: manifest.file_name.clone(),
        file_size: manifest.file_size,
        chunks: manifest.chunks.len(),
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

fn default_download_output_dir(library_root: &Path) -> PathBuf {
    library_root.join(OUTPUT_DIR_NAME)
}

fn temporary_download_output_path(library_root: &Path) -> PathBuf {
    default_download_output_dir(library_root)
        .join(format!(".etle-download-daemon-{}.part", std::process::id()))
}

fn move_auto_download_output(
    temporary_output: &Path,
    library_root: &Path,
    manifest_file_name: &str,
) -> std::io::Result<PathBuf> {
    let final_output = unique_output_path(default_download_output_path(
        library_root,
        manifest_file_name,
    ));
    create_output_parent_dir(&final_output)?;
    fs::rename(temporary_output, &final_output)?;

    Ok(final_output)
}

fn default_download_output_path(library_root: &Path, manifest_file_name: &str) -> PathBuf {
    default_download_output_dir(library_root).join(safe_output_file_name(manifest_file_name))
}

fn safe_output_file_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "downloaded-file".to_string())
}

fn create_output_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}

fn unique_output_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }

    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "downloaded-file".to_string());
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());

    for copy_index in 1_u32.. {
        let file_name = match &extension {
            Some(extension) => format!("{stem} ({copy_index}).{extension}"),
            None => format!("{stem} ({copy_index})"),
        };
        let candidate = parent.join(file_name);

        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded copy index loop must return before overflowing")
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
