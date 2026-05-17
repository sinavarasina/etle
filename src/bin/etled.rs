use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};

use etle::{
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
        #[arg(long, default_value = "127.0.0.1:7000")]
        listen: SocketAddr,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-daemon")]
        peer_id: String,

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,
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
        } => {
            let library_root = library_root.unwrap_or_else(default_library_root);
            print_startup_banner(&library_root, listen)?;

            let listener = bind_listener(listen).await?;
            serve_library_forever(
                listener,
                &library_root,
                ServeFileOptions::new(peer_id, log_level),
            )
            .await?;
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
