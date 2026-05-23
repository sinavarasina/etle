use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};

use etle::{
    discovery::{DEFAULT_DISCOVERY_PORT, serve_discovery_forever},
    ipc::{default_ipc_socket_path, serve_ipc_forever},
    network::{ServeFileOptions, TransferLogLevel, bind_listener, serve_library_forever},
    state::{default_library_root, list_library_shares},
};

#[derive(Debug, Parser)]
#[command(
    name = "etled",
    version,
    about = "ETLE foreground daemon for serving local library shares"
)]
struct Cli {
    /// Show detailed daemon and transfer logs.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve any local library share requested by peers over one P2P port.
    Serve {
        /// TCP listen address for the P2P transfer server.
        #[arg(long, default_value = "0.0.0.0:7000")]
        listen: SocketAddr,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-daemon")]
        peer_id: String,

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,

        /// Local IPC socket path. Unix uses a filesystem socket; Windows named pipes come later.
        #[arg(long)]
        ipc_socket: Option<PathBuf>,

        /// Disable the local IPC command socket.
        #[arg(long)]
        no_ipc: bool,

        /// UDP port used for LAN peer discovery.
        #[arg(long, default_value_t = DEFAULT_DISCOVERY_PORT)]
        discovery_port: u16,

        /// Disable LAN peer discovery.
        #[arg(long)]
        no_discovery: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let log_level = if cli.verbose {
        TransferLogLevel::Verbose
    } else {
        TransferLogLevel::Normal
    };

    match cli.command {
        Command::Serve {
            listen,
            peer_id,
            library_root,
            ipc_socket,
            no_ipc,
            discovery_port,
            no_discovery,
        } => {
            let library_root = library_root.unwrap_or_else(default_library_root);
            let ipc_socket = ipc_socket.unwrap_or_else(|| default_ipc_socket_path(&library_root));
            print_startup_banner(&library_root, listen)?;

            let listener = bind_listener(listen).await?;
            let serve_options = ServeFileOptions::new(peer_id.clone(), log_level);
            let discovery_task = if no_discovery {
                println!("[daemon] discovery: disabled");
                None
            } else {
                println!("[daemon] discovery udp: 0.0.0.0:{discovery_port}");
                let discovery_library_root = library_root.clone();
                Some(tokio::spawn(async move {
                    if let Err(error) = serve_discovery_forever(
                        discovery_library_root,
                        listen,
                        peer_id,
                        discovery_port,
                    )
                    .await
                    {
                        eprintln!("[daemon] discovery stopped: {error}");
                    }
                }))
            };

            if no_ipc {
                println!("[daemon] ipc: disabled");
                let result = serve_library_forever(listener, &library_root, serve_options).await;
                if let Some(task) = discovery_task {
                    task.abort();
                }
                result?;
            } else {
                println!("[daemon] ipc socket: {}", ipc_socket.display());
                println!("[daemon] ipc commands: Ping, ListShares, Shutdown");

                let p2p_library_root = library_root.clone();
                let p2p_task = tokio::spawn(async move {
                    serve_library_forever(listener, p2p_library_root, serve_options).await
                });

                let ipc_result = serve_ipc_forever(&ipc_socket, &library_root).await;
                p2p_task.abort();
                if let Some(task) = discovery_task {
                    task.abort();
                }
                ipc_result?;
            }
        }
    }

    Ok(())
}

fn print_startup_banner(library_root: &std::path::Path, listen: SocketAddr) -> anyhow::Result<()> {
    println!("[daemon] etled starting");
    println!("[daemon] listen address: {listen}");
    println!("[daemon] library root: {}", library_root.display());
    println!("[daemon] mode: foreground; press Ctrl+C to stop");
    println!("[daemon] peers must request a share_id with RequestShare");

    let shares = list_library_shares(library_root)?;
    if shares.is_empty() {
        println!("[daemon] no local shares found yet");
    } else {
        println!("[daemon] loaded {} local share(s)", shares.len());
        for share in shares {
            let key_status = if share.has_secret {
                "key=yes"
            } else {
                "key=no"
            };
            println!(
                "[daemon] share {}  chunks={}/{}  {}  name=\"{}\"",
                share.descriptor.share_id,
                share.completed_chunks(),
                share.total_chunks(),
                key_status,
                share.descriptor.name
            );
        }
    }

    Ok(())
}
