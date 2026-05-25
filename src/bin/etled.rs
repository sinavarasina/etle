use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use clap::{Parser, Subcommand};

use etle::{
    build_info,
    config::load,
    crypto::key_exchange::AuthPsk,
    discovery::{options::DiscoveryOptions, server},
    ipc::{path::default_ipc_socket_path, server::listener},
    network::{
        tcp::bind_listener,
        transfer::{
            options::{ServeFileOptions, TransferLogLevel},
            serve,
        },
    },
    state::{library, paths::default_library_root},
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
    /// Print detailed build and version information.
    Version,

    /// Serve any local library share requested by peers over one P2P port.
    Serve {
        /// TCP listen address for the P2P transfer server. Defaults to 0.0.0.0:7000 so LAN peers can connect.
        #[arg(long, value_name = "ADDR:PORT")]
        listen: Option<SocketAddr>,

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
        #[arg(long, value_name = "PORT")]
        discovery_port: Option<u16>,

        /// IPv4 multicast group used in addition to broadcast for LAN peer discovery.
        #[arg(long, value_name = "IPv4")]
        discovery_multicast: Option<Ipv4Addr>,

        /// Pre-shared passphrase used to authenticate incoming P2P key exchange.
        /// If omitted, ETLE_AUTH_PSK/config auth_psk is used when present.
        #[arg(long, value_name = "PASSPHRASE")]
        auth_psk: Option<String>,

        /// Disable LAN peer discovery.
        #[arg(long)]
        no_discovery: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if build_info::args_request_version(std::env::args().skip(1)) {
        build_info::print("etled");
        return Ok(());
    }

    let cli = Cli::parse();
    let log_level = if cli.verbose {
        TransferLogLevel::Verbose
    } else {
        TransferLogLevel::Normal
    };

    match cli.command {
        Command::Version => {
            build_info::print("etled");
        }
        Command::Serve {
            listen,
            peer_id,
            library_root,
            ipc_socket,
            no_ipc,
            discovery_port,
            discovery_multicast,
            auth_psk,
            no_discovery,
        } => {
            let config = load::load()?;

            let library_root = library_root
                .or_else(|| config.library_root())
                .unwrap_or_else(default_library_root);
            let listen =
                listen.unwrap_or_else(|| config.listen.unwrap_or_else(load::default_listen_addr));
            let discovery_port = discovery_port.unwrap_or_else(|| config.discovery_port());
            let discovery_multicast =
                discovery_multicast.unwrap_or_else(|| config.discovery_multicast());
            let ipc_socket = ipc_socket
                .or_else(|| config.ipc_socket())
                .unwrap_or_else(|| default_ipc_socket_path(&library_root));
            let auth_psk = auth_psk
                .or_else(|| std::env::var("ETLE_AUTH_PSK").ok())
                .or_else(|| config.auth_psk());
            print_startup_banner(&library_root, listen)?;

            let listener = bind_listener(listen).await?;
            let mut serve_options = ServeFileOptions::new(peer_id.clone(), log_level);
            if let Some(auth_psk) = auth_psk {
                println!("[daemon] p2p auth: PSK enabled");
                serve_options = serve_options.with_auth_psk(AuthPsk::from_passphrase(auth_psk));
            } else {
                println!("[daemon] p2p auth: disabled; key exchange is not MITM-resistant");
            }
            let discovery_task = if no_discovery {
                println!("[daemon] discovery: disabled");
                None
            } else {
                println!("[daemon] discovery udp: 0.0.0.0:{discovery_port}");
                println!("[daemon] discovery multicast: {discovery_multicast}");
                let discovery_library_root = library_root.clone();
                let discovery_options = DiscoveryOptions::new(discovery_port)
                    .with_multicast(discovery_multicast)
                    .with_verbose(log_level.is_verbose());
                Some(tokio::spawn(async move {
                    if let Err(error) = server::serve_with(
                        discovery_library_root,
                        listen,
                        peer_id,
                        discovery_options,
                    )
                    .await
                    {
                        eprintln!("[daemon] discovery stopped: {error}");
                    }
                }))
            };

            if no_ipc {
                println!("[daemon] ipc: disabled");
                let result = serve::library_forever(listener, &library_root, serve_options).await;
                if let Some(task) = discovery_task {
                    task.abort();
                }
                result?;
            } else {
                println!("[daemon] ipc socket: {}", ipc_socket.display());
                println!("[daemon] ipc commands: Ping, ListShares, Shutdown");

                let p2p_library_root = library_root.clone();
                let p2p_task = tokio::spawn(async move {
                    serve::library_forever(listener, p2p_library_root, serve_options).await
                });

                let ipc_result = listener::forever(&ipc_socket, &library_root).await;
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

    let shares = library::list(library_root)?;
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
