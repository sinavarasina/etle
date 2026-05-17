#[cfg(feature = "cli")]
use std::{net::SocketAddr, path::PathBuf};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
use etle::{
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        client_hello_handshake, connect_peer, download_file_from_peer_with_options,
        serve_file_to_one_peer_with_options, serve_library_share_to_one_peer,
    },
    state::{ShareMode, list_library_shares},
};

#[cfg(feature = "cli")]
#[derive(Debug, Parser)]
#[command(
    name = "etle-cli",
    version,
    about = "Experimental torrent-like encrypted file transfer"
)]
struct Cli {
    /// Show detailed transfer logs.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[cfg(feature = "cli")]
#[derive(Debug, Subcommand)]
enum Command {
    /// Seed one file to one peer over TCP.
    Seed {
        /// File to seed.
        file: PathBuf,

        /// TCP listen address.
        #[arg(long, default_value = "127.0.0.1:7000")]
        listen: SocketAddr,

        /// Chunk size in bytes.
        #[arg(long, default_value_t = DEFAULT_CHUNK_SIZE)]
        chunk_size: usize,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-seeder")]
        peer_id: String,

        /// Root directory for the local .etle library state.
        #[arg(long, default_value = ".")]
        library_root: PathBuf,
    },

    /// Seed an already persisted share from the local .etle library.
    SeedState {
        /// Share ID to seed from .etle/library/<share_id>.
        share_id: ShareId,

        /// TCP listen address.
        #[arg(long, default_value = "127.0.0.1:7000")]
        listen: SocketAddr,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-seeder")]
        peer_id: String,

        /// Root directory for the local .etle library state.
        #[arg(long, default_value = ".")]
        library_root: PathBuf,
    },

    /// List local shares stored in the .etle library.
    List {
        /// Root directory for the local .etle library state.
        #[arg(long, default_value = ".")]
        library_root: PathBuf,
    },

    /// Download a file from one seeder peer.
    Download {
        /// Seeder peer address.
        #[arg(long)]
        peer: SocketAddr,

        /// Output file path.
        #[arg(long)]
        output: PathBuf,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-peer")]
        peer_id: String,

        /// Root directory for the local .etle library state.
        #[arg(long, default_value = ".")]
        library_root: PathBuf,
    },

    /// Perform a basic TCP + hello handshake probe.
    Connect {
        /// Seeder peer address.
        #[arg(long)]
        peer: SocketAddr,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-probe")]
        peer_id: String,
    },
}

#[cfg(feature = "cli")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let log_level = if cli.verbose {
        TransferLogLevel::Verbose
    } else {
        TransferLogLevel::Normal
    };

    match cli.command {
        Command::Seed {
            file,
            listen,
            chunk_size,
            peer_id,
            library_root,
        } => {
            println!("[seeder] loading file: {}", file.display());
            println!("[seeder] listen address: {listen}");
            println!("[seeder] chunk size: {chunk_size} bytes");
            println!("[seeder] library root: {}", library_root.display());
            println!("[seeder] waiting for one peer...");

            let listener = bind_listener(listen).await?;
            serve_file_to_one_peer_with_options(
                listener,
                &file,
                chunk_size,
                ServeFileOptions::new(peer_id, log_level).with_library_root(library_root),
            )
            .await?;

            println!("[seeder] transfer completed");
        }

        Command::SeedState {
            share_id,
            listen,
            peer_id,
            library_root,
        } => {
            println!("[seeder] loading share from state: {share_id}");
            println!("[seeder] listen address: {listen}");
            println!("[seeder] library root: {}", library_root.display());
            println!("[seeder] waiting for one peer...");

            let listener = bind_listener(listen).await?;
            let descriptor = serve_library_share_to_one_peer(
                listener,
                &library_root,
                share_id,
                ServeFileOptions::new(peer_id, log_level),
            )
            .await?;

            println!("[seeder] transfer completed");
            println!("[seeder] share: {}", descriptor.name);
            println!("[seeder] share_id: {}", descriptor.share_id);
            println!("[seeder] chunks: {}", descriptor.chunks.len());
        }

        Command::List { library_root } => {
            let shares = list_library_shares(&library_root)?;
            println!("[library] root: {}", library_root.display());

            if shares.is_empty() {
                println!("[library] no shares found");
            } else {
                for share in shares {
                    let mode = format_share_mode(share.mode());
                    let secret = if share.has_secret {
                        "key=yes"
                    } else {
                        "key=no"
                    };
                    println!(
                        "[library] {}  {mode}  chunks={}/{}  {secret}  name=\"{}\"",
                        share.descriptor.share_id,
                        share.completed_chunks(),
                        share.total_chunks(),
                        share.descriptor.name
                    );
                }
            }
        }

        Command::Download {
            peer,
            output,
            peer_id,
            library_root,
        } => {
            println!("[peer] connecting to {peer}");
            println!("[peer] output path: {}", output.display());
            println!("[peer] library root: {}", library_root.display());

            let manifest = download_file_from_peer_with_options(
                peer,
                &output,
                DownloadFileOptions::new(peer_id, log_level).with_library_root(library_root),
            )
            .await?;

            println!("[peer] transfer completed");
            println!("[peer] file: {}", manifest.file_name);
            println!("[peer] file_id: {}", manifest.file_id);
            println!("[peer] file size: {} bytes", manifest.file_size);
            println!("[peer] chunks: {}", manifest.chunks.len());
        }

        Command::Connect { peer, peer_id } => {
            println!("[peer] connecting to {peer}");

            let mut stream = connect_peer(peer).await?;
            let remote_peer_id = client_hello_handshake(&mut stream, peer_id).await?;

            println!("[peer] hello handshake completed with {remote_peer_id}");
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn format_share_mode(mode: Option<ShareMode>) -> &'static str {
    match mode {
        Some(ShareMode::Seeding) => "seeding",
        Some(ShareMode::Downloading) => "downloading",
        Some(ShareMode::Completed) => "completed",
        Some(ShareMode::Paused) => "paused",
        None => "unknown",
    }
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("etle-cli requires the `cli` feature");
}
