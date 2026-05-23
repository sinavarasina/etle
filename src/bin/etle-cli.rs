#[cfg(feature = "cli")]
use std::{net::SocketAddr, path::PathBuf};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
use etle::{
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    ipc::{
        IpcCommand, IpcEvent, IpcResponse, IpcShareSummary, default_ipc_socket_path,
        send_ipc_command, subscribe_ipc_events,
    },
    state::default_library_root,
};

#[cfg(feature = "cli")]
#[derive(Debug, Parser)]
#[command(
    name = "etle-cli",
    version,
    about = "ETLE command client for the etled daemon"
)]
struct Cli {
    /// Show detailed CLI logs.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Local etled IPC socket path. Defaults to $ETLE_LIBRARY_ROOT/.etle/etled.sock or ~/Downloads/ETLE/.etle/etled.sock.
    #[arg(long, global = true)]
    ipc_socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[cfg(feature = "cli")]
#[derive(Debug, Subcommand)]
enum Command {
    /// Ask etled to add a file into the local library and make it seedable.
    Seed {
        /// File to add to the daemon library.
        file: PathBuf,

        /// Chunk size in bytes.
        #[arg(long, default_value_t = DEFAULT_CHUNK_SIZE)]
        chunk_size: usize,
    },

    /// List local shares through etled.
    List,

    /// Ask etled to download a share from one or more peers.
    Download {
        /// Seeder peer address. Can be repeated for fallback or parallel download.
        #[arg(long, required = true)]
        peer: Vec<SocketAddr>,

        /// Share ID to request from a multi-share library server.
        #[arg(long)]
        share_id: ShareId,

        /// Output file path. Defaults to <daemon-library-root>/output/<file-name-from-manifest>.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Reuse verified encrypted chunks from the daemon library when available.
        /// This is enabled by default; the flag is accepted for explicitness.
        #[arg(long)]
        resume: bool,

        /// Ignore existing local chunks and fetch chunks from peers again.
        #[arg(long)]
        no_resume: bool,

        /// Number of parallel peer workers for multi-peer download.
        #[arg(long, default_value_t = 1)]
        parallel: usize,

        /// Number of chunk requests kept in flight per peer connection.
        #[arg(long, default_value_t = 16)]
        request_window: usize,
    },

    /// Send direct daemon control commands.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[cfg(feature = "cli")]
#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Check whether etled is reachable.
    Ping,

    /// List local shares through etled.
    List,

    /// Ask etled to shut down.
    Shutdown,

    /// Stream daemon events and transfer progress.
    Watch,
}

#[cfg(feature = "cli")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket_path = cli
        .ipc_socket
        .unwrap_or_else(|| default_ipc_socket_path(default_library_root()));

    if cli.verbose {
        println!("[cli] ipc socket: {}", socket_path.display());
    }

    if matches!(
        &cli.command,
        Command::Daemon {
            command: DaemonCommand::Watch
        }
    ) {
        subscribe_ipc_events(&socket_path, print_event).await?;
        return Ok(());
    }

    let command = match cli.command {
        Command::Seed { file, chunk_size } => IpcCommand::SeedFile {
            input: file,
            chunk_size,
        },
        Command::List => IpcCommand::ListShares,
        Command::Download {
            peer,
            share_id,
            output,
            resume: _,
            no_resume,
            parallel,
            request_window,
        } => {
            if no_resume {
                IpcCommand::DownloadFresh {
                    share_id,
                    peers: peer,
                    output,
                    parallelism: parallel,
                    request_window,
                }
            } else {
                IpcCommand::Download {
                    share_id,
                    peers: peer,
                    output,
                    parallelism: parallel,
                    request_window,
                }
            }
        }
        Command::Daemon { command } => match command {
            DaemonCommand::Ping => IpcCommand::Ping,
            DaemonCommand::List => IpcCommand::ListShares,
            DaemonCommand::Shutdown => IpcCommand::Shutdown,
            DaemonCommand::Watch => IpcCommand::SubscribeEvents,
        },
    };

    let response = send_ipc_command(&socket_path, command).await?;
    print_response(response)?;

    Ok(())
}

#[cfg(feature = "cli")]
fn print_shares(shares: Vec<IpcShareSummary>) {
    if shares.is_empty() {
        println!("[library] no shares found");
        return;
    }

    for share in shares {
        print_share(&share);
    }
}

#[cfg(feature = "cli")]
fn print_share(share: &IpcShareSummary) {
    let mode = share.mode.as_deref().unwrap_or("unknown");
    let secret = if share.has_secret {
        "key=yes"
    } else {
        "key=no"
    };

    println!(
        "[library] {}  {mode}  chunks={}/{}  {secret}  name=\"{}\"",
        share.share_id, share.completed_chunks, share.total_chunks, share.name
    );
}

#[cfg(feature = "cli")]
fn print_response(response: IpcResponse) -> anyhow::Result<()> {
    match response {
        IpcResponse::Pong => println!("[daemon] pong"),
        IpcResponse::Ack { message } => println!("[daemon] {message}"),
        IpcResponse::Shares { shares } => print_shares(shares),
        IpcResponse::ShareAdded { share } => {
            println!("[daemon] share added");
            print_share(&share);
        }
        IpcResponse::TransferQueued { share_id } => {
            println!("[daemon] transfer queued: {share_id}");
        }
        IpcResponse::TransferCompleted {
            share_id,
            output,
            file_name,
            file_size,
            chunks,
        } => {
            println!("[daemon] transfer completed");
            println!("[daemon] share_id: {share_id}");
            println!("[daemon] output: {}", output.display());
            println!("[daemon] file: {file_name}");
            println!("[daemon] file size: {file_size} bytes");
            println!("[daemon] chunks: {chunks}");
        }
        IpcResponse::Error { message } => anyhow::bail!(message),
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn print_event(event: IpcEvent) {
    match event {
        IpcEvent::ServerStarted { listen } => println!("[event] server started: {listen}"),
        IpcEvent::ServerStopped => println!("[event] server stopped"),
        IpcEvent::ShareUpdated { share } => {
            println!("[event] share updated");
            print_share(&share);
        }
        IpcEvent::PeerConnected { peer_id } => println!("[event] peer connected: {peer_id}"),
        IpcEvent::ChunkCompleted {
            share_id,
            completed_chunks,
            total_chunks,
        } => {
            println!("[event] chunk completed: {share_id} chunks={completed_chunks}/{total_chunks}")
        }
        IpcEvent::TransferProgress {
            share_id,
            completed_chunks,
            total_chunks,
            bytes_done,
            total_bytes,
            bytes_per_second,
        } => println!(
            "[event] progress: {share_id} chunks={completed_chunks}/{total_chunks} bytes={bytes_done}/{total_bytes} speed={bytes_per_second} B/s"
        ),
        IpcEvent::TransferCompleted { share_id, output } => println!(
            "[event] transfer completed: {share_id} output={}",
            output.display()
        ),
        IpcEvent::Error { message } => eprintln!("[event] error: {message}"),
    }
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("etle-cli requires the `cli` feature");
}
