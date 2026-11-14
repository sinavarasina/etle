#[cfg(feature = "cli")]
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
use etle::{
    build_info,
    config::load,
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    ipc::{
        client::{EventSubscription, send_ipc_command, subscribe_ipc_events},
        message::{
            IpcActiveJob, IpcCommand, IpcDaemonStatus, IpcEvent, IpcJobKind, IpcResponse,
            IpcShareSummary,
        },
        path::default_ipc_socket_path,
    },
    state::paths::default_library_root,
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
    /// Print detailed build and version information.
    Version,

    /// Ask etled to add a file into the local library and make it seedable.
    Seed {
        /// File to add to the daemon library.
        file: PathBuf,

        /// Chunk size in bytes.
        #[arg(long, default_value_t = DEFAULT_CHUNK_SIZE)]
        chunk_size: usize,

        /// Follow live seed progress until the job completes.
        #[arg(short, long)]
        follow: bool,
    },

    /// List local shares through etled.
    List,

    /// Delete a share from the daemon local library.
    Delete {
        /// Share ID to delete from the local daemon library.
        #[arg(long)]
        share_id: ShareId,
    },

    /// Ask etled to download a share from one or more peers.
    Download {
        /// Seeder peer address. Can be repeated for fallback or parallel download.
        /// If omitted, etled will try LAN auto-discovery for the requested share.
        #[arg(long)]
        peer: Vec<SocketAddr>,

        /// Share ID to request from a multi-share library server.
        #[arg(long)]
        share_id: ShareId,

        /// Output file path. Defaults to <daemon-library-root>/output/<file-name-from-manifest>.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Reuse verified encrypted chunks from the daemon library when available.
        /// This is enabled by default; the flag is accepted for explicitness.
        #[arg(long, conflicts_with = "no_resume")]
        resume: bool,

        /// Ignore existing local chunks and fetch chunks from peers again.
        #[arg(long)]
        no_resume: bool,

        /// Parallel peer workers: 0=auto by resolved peer count, 1=sequential fallback, N=parallel workers.
        #[arg(long, value_name = "N")]
        parallel: Option<usize>,

        /// Number of chunk requests kept in flight per peer connection.
        #[arg(long, value_name = "CHUNKS")]
        request_window: Option<usize>,

        /// Pre-shared passphrase used to authenticate the P2P key exchange.
        /// If omitted, ETLE_AUTH_PSK/config auth_psk is used when present.
        #[arg(long, value_name = "PASSPHRASE")]
        auth_psk: Option<String>,

        /// UDP port used for LAN peer discovery when --peer is omitted.
        #[arg(long, value_name = "PORT")]
        discovery_port: Option<u16>,

        /// LAN discovery timeout in milliseconds when --peer is omitted.
        #[arg(long, value_name = "MS")]
        discovery_timeout_ms: Option<u64>,

        /// IPv4 multicast group used in addition to broadcast for LAN peer discovery.
        #[arg(long, value_name = "IPv4")]
        discovery_multicast: Option<Ipv4Addr>,

        /// Follow live download progress until the job completes.
        #[arg(short, long)]
        follow: bool,
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

    /// Show live daemon configuration and active transfer jobs.
    Status,

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
    if build_info::args_request_version(std::env::args().skip(1)) {
        build_info::print("etle-cli");
        return Ok(());
    }

    let cli = Cli::parse();

    if matches!(cli.command, Command::Version) {
        build_info::print("etle-cli");
        return Ok(());
    }

    let config = load::load()?;
    let library_root = config.library_root().unwrap_or_else(default_library_root);
    let socket_path = cli
        .ipc_socket
        .or_else(|| config.ipc_socket())
        .unwrap_or_else(|| default_ipc_socket_path(&library_root));

    if cli.verbose {
        println!("[cli] ipc socket: {}", socket_path.display());
    }

    let (command, follow) = match cli.command {
        Command::Version => unreachable!("version handled before loading config"),
        Command::Seed {
            file,
            chunk_size,
            follow,
        } => (
            IpcCommand::SeedFile {
                input: absolutize_cli_path(file)?,
                chunk_size,
            },
            follow,
        ),
        Command::List => (IpcCommand::ListShares, false),
        Command::Delete { share_id } => (IpcCommand::DeleteShare { share_id }, false),
        Command::Download {
            peer,
            share_id,
            output,
            resume: _,
            no_resume,
            parallel,
            request_window,
            auth_psk,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            follow,
        } => {
            let output = output.map(absolutize_cli_path).transpose()?;
            let parallel = parallel.unwrap_or_else(|| config.parallel());
            let request_window = request_window.unwrap_or_else(|| config.request_window());
            let discovery_port = discovery_port.unwrap_or_else(|| config.discovery_port());
            let discovery_timeout_ms =
                discovery_timeout_ms.unwrap_or_else(|| config.discovery_timeout_ms());
            let discovery_multicast =
                discovery_multicast.unwrap_or_else(|| config.discovery_multicast());
            let auth_psk = auth_psk
                .filter(|psk| !psk.trim().is_empty())
                .or_else(|| {
                    std::env::var("ETLE_AUTH_PSK")
                        .ok()
                        .filter(|psk| !psk.trim().is_empty())
                })
                .or_else(|| config.auth_psk().filter(|psk| !psk.trim().is_empty()));

            let command = if no_resume {
                IpcCommand::DownloadFresh {
                    share_id,
                    peers: peer,
                    output,
                    parallelism: parallel,
                    request_window,
                    discovery_port,
                    discovery_timeout_ms,
                    discovery_multicast,
                    auth_psk,
                }
            } else {
                IpcCommand::Download {
                    share_id,
                    peers: peer,
                    output,
                    parallelism: parallel,
                    request_window,
                    discovery_port,
                    discovery_timeout_ms,
                    discovery_multicast,
                    auth_psk,
                }
            };
            (command, follow)
        }
        Command::Daemon {
            command: DaemonCommand::Watch,
        } => {
            subscribe_ipc_events(&socket_path, print_event).await?;
            return Ok(());
        }
        Command::Daemon { command } => (
            match command {
                DaemonCommand::Ping => IpcCommand::Ping,
                DaemonCommand::Status => IpcCommand::GetStatus,
                DaemonCommand::List => IpcCommand::ListShares,
                DaemonCommand::Shutdown => IpcCommand::Shutdown,
                DaemonCommand::Watch => unreachable!("watch handled above"),
            },
            false,
        ),
    };

    if follow {
        let mut subscription = EventSubscription::connect(&socket_path).await?;
        let response = send_ipc_command(&socket_path, command).await?;
        let target_job_id = match &response {
            IpcResponse::SeedQueued { job_id, .. } | IpcResponse::TransferQueued { job_id, .. } => {
                Some(job_id.clone())
            }
            _ => None,
        };
        print_response(response)?;
        if let Some(job_id) = target_job_id {
            follow_job(&mut subscription, &socket_path, &job_id).await?;
        }
        return Ok(());
    }

    let response = send_ipc_command(&socket_path, command).await?;
    print_response(response)?;

    Ok(())
}

#[cfg(feature = "cli")]
fn absolutize_cli_path(path: PathBuf) -> anyhow::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(std::env::current_dir()?.join(path))
}

#[cfg(feature = "cli")]
async fn follow_job(
    subscription: &mut EventSubscription,
    socket_path: &Path,
    target_job_id: &str,
) -> anyhow::Result<()> {
    let mut last_progress: Option<(usize, usize, u64, u64)> = None;
    loop {
        match subscription.next_event().await? {
            IpcEvent::TaskProgress {
                job_id: Some(ref id),
                ref task,
                completed_chunks,
                total_chunks,
                bytes_done,
                total_bytes,
                bytes_per_second,
                ..
            } if id == target_job_id => {
                last_progress = Some((completed_chunks, total_chunks, bytes_done, total_bytes));
                println!(
                    "[progress] {task}  chunks={completed_chunks}/{total_chunks}  data={}/{}  speed={}",
                    format_bytes_cli(bytes_done),
                    format_bytes_cli(total_bytes),
                    format_rate_cli(bytes_per_second),
                );
            }
            IpcEvent::TransferProgress {
                job_id: Some(ref id),
                completed_chunks,
                total_chunks,
                bytes_done,
                total_bytes,
                bytes_per_second,
                ..
            } if id == target_job_id => {
                let snapshot = (completed_chunks, total_chunks, bytes_done, total_bytes);
                if last_progress == Some(snapshot) {
                    continue;
                }
                last_progress = Some(snapshot);
                println!(
                    "[progress] transferring  chunks={completed_chunks}/{total_chunks}  data={}/{}  speed={}",
                    format_bytes_cli(bytes_done),
                    format_bytes_cli(total_bytes),
                    format_rate_cli(bytes_per_second),
                );
            }
            IpcEvent::JobCompleted {
                job_id,
                share_id,
                summary,
                ..
            } if job_id == target_job_id => {
                if let Some(share_id) = share_id {
                    println!("[done] {summary} (share_id={share_id})");
                } else {
                    println!("[done] {summary}");
                }
                return Ok(());
            }
            IpcEvent::JobFailed { job_id, error, .. } if job_id == target_job_id => {
                anyhow::bail!(error);
            }
            IpcEvent::Error { message } => {
                eprintln!("[warn] {message}");
                if let Ok(IpcResponse::Status(status)) =
                    send_ipc_command(socket_path, IpcCommand::GetStatus).await
                    && !status
                        .active_jobs
                        .iter()
                        .any(|job| job.job_id == target_job_id)
                {
                    println!("[done] job {target_job_id} is no longer active");
                    return Ok(());
                }
            }
            IpcEvent::ServerStopped => {
                anyhow::bail!("daemon stopped before job {target_job_id} finished");
            }
            _ => {}
        }
    }
}

#[cfg(feature = "cli")]
fn format_bytes_cli(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for candidate in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = candidate;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {unit}")
    }
}

#[cfg(feature = "cli")]
fn format_rate_cli(bytes_per_second: u64) -> String {
    if bytes_per_second == 0 {
        "0 B/s".to_string()
    } else {
        format!("{}/s", format_bytes_cli(bytes_per_second))
    }
}

#[cfg(feature = "cli")]
fn job_kind_label(kind: IpcJobKind) -> &'static str {
    match kind {
        IpcJobKind::Seed => "seed",
        IpcJobKind::Download => "download",
        IpcJobKind::Upload => "upload",
    }
}

#[cfg(feature = "cli")]
fn print_status(status: IpcDaemonStatus) {
    println!("[daemon] status: online");
    println!("[daemon] peer_id: {}", status.peer_id);
    match status.listen {
        Some(addr) => println!("[daemon] listen: {addr}"),
        None => println!("[daemon] listen: unknown"),
    }
    println!("[daemon] library_root: {}", status.library_root.display());
    println!(
        "[daemon] discovery: {} (udp port {})",
        if status.auto_discovery {
            "enabled"
        } else {
            "disabled"
        },
        status.discovery_port
    );
    println!(
        "[daemon] p2p_auth: {}",
        if status.auth_required {
            "PSK required"
        } else {
            "open (no PSK)"
        }
    );
    println!("[daemon] shares: {}", status.share_count);
    println!("[daemon] active_jobs: {}", status.active_jobs.len());
    for job in status.active_jobs {
        print_active_job(&job);
    }
}

#[cfg(feature = "cli")]
fn print_active_job(job: &IpcActiveJob) {
    println!(
        "  - [{}] {}  phase={}  chunks={}/{}  data={}/{}  speed={}",
        job_kind_label(job.kind),
        job.label,
        job.phase,
        job.completed_chunks,
        job.total_chunks,
        format_bytes_cli(job.bytes_done),
        format_bytes_cli(job.total_bytes),
        format_rate_cli(job.bytes_per_second),
    );
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
        "[library] {}  {mode}  size={}  chunks={}/{}  {secret}  name=\"{}\"",
        share.share_id,
        format_bytes_cli(share.total_bytes),
        share.completed_chunks,
        share.total_chunks,
        share.name
    );
}

#[cfg(feature = "cli")]
fn print_response(response: IpcResponse) -> anyhow::Result<()> {
    match response {
        IpcResponse::Pong => println!("[daemon] pong"),
        IpcResponse::Status(status) => print_status(status),
        IpcResponse::Ack { message } => println!("[daemon] {message}"),
        IpcResponse::Shares { shares } => print_shares(shares),
        IpcResponse::ShareAdded { share } => {
            println!("[daemon] share added");
            print_share(&share);
        }
        IpcResponse::ShareDeleted { share_id, name } => {
            println!("[daemon] share deleted");
            println!("[daemon] share_id: {share_id}");
            println!("[daemon] name: {name}");
        }
        IpcResponse::SeedQueued { job_id, label } => {
            println!("[daemon] seed queued: {label}");
            println!("[daemon] job_id: {job_id}");
        }
        IpcResponse::TransferQueued { share_id, job_id } => {
            println!("[daemon] transfer queued: {share_id}");
            println!("[daemon] job_id: {job_id}");
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
        IpcEvent::ShareDeleted { share_id } => println!("[event] share deleted: {share_id}"),
        IpcEvent::PeerConnected { peer_id } => println!("[event] peer connected: {peer_id}"),
        IpcEvent::JobStarted { job } => println!(
            "[event] job started: [{}] {} ({})",
            job_kind_label(job.kind),
            job.label,
            job.job_id
        ),
        IpcEvent::JobCompleted {
            job_id,
            kind,
            label,
            summary,
            ..
        } => println!(
            "[event] job completed: [{}] {label} ({job_id}) - {summary}",
            job_kind_label(kind)
        ),
        IpcEvent::JobFailed {
            job_id,
            kind,
            label,
            error,
            ..
        } => eprintln!(
            "[event] job failed: [{}] {label} ({job_id}) - {error}",
            job_kind_label(kind)
        ),
        IpcEvent::ChunkCompleted {
            share_id,
            completed_chunks,
            total_chunks,
        } => {
            println!("[event] chunk completed: {share_id} chunks={completed_chunks}/{total_chunks}")
        }
        IpcEvent::TaskProgress {
            job_id,
            task,
            label,
            completed_chunks,
            total_chunks,
            bytes_done,
            total_bytes,
            bytes_per_second,
        } => println!(
            "[event] task progress: job={} task={} label=\"{}\" chunks={}/{} bytes={}/{} speed={} B/s",
            job_id.as_deref().unwrap_or("unknown"),
            task,
            label,
            completed_chunks,
            total_chunks,
            bytes_done,
            total_bytes,
            bytes_per_second,
        ),
        IpcEvent::TransferProgress {
            job_id,
            share_id,
            completed_chunks,
            total_chunks,
            bytes_done,
            total_bytes,
            bytes_per_second,
        } => println!(
            "[event] progress: job={} share={} chunks={}/{} bytes={}/{} speed={} B/s",
            job_id.as_deref().unwrap_or("unknown"),
            share_id,
            completed_chunks,
            total_chunks,
            bytes_done,
            total_bytes,
            bytes_per_second,
        ),
        IpcEvent::TransferCompleted {
            job_id,
            share_id,
            output,
        } => println!(
            "[event] transfer completed: job={} share={} output={}",
            job_id.as_deref().unwrap_or("unknown"),
            share_id,
            output.display()
        ),
        IpcEvent::Error { message } => eprintln!("[event] error: {message}"),
    }
}

#[cfg(not(feature = "cli"))]
fn main() {
    if etle::build_info::args_request_version(std::env::args().skip(1)) {
        etle::build_info::print("etle-cli");
        return;
    }

    eprintln!("etle-cli requires the `cli` feature");
}
