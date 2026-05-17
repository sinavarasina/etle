#[cfg(feature = "cli")]
use std::{net::SocketAddr, path::PathBuf};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
use etle::{
    file::chunker::DEFAULT_CHUNK_SIZE,
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        client_hello_handshake, connect_peer, download_file_from_peer_with_options,
        serve_file_to_one_peer_with_options,
    },
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
        } => {
            println!("[seeder] loading file: {}", file.display());
            println!("[seeder] listen address: {listen}");
            println!("[seeder] chunk size: {chunk_size} bytes");
            println!("[seeder] waiting for one peer...");

            let listener = bind_listener(listen).await?;
            serve_file_to_one_peer_with_options(
                listener,
                &file,
                chunk_size,
                ServeFileOptions::new(peer_id, log_level),
            )
            .await?;

            println!("[seeder] transfer completed");
        }

        Command::Download {
            peer,
            output,
            peer_id,
        } => {
            println!("[peer] connecting to {peer}");
            println!("[peer] output path: {}", output.display());

            let manifest = download_file_from_peer_with_options(
                peer,
                &output,
                DownloadFileOptions::new(peer_id, log_level),
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

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("etle-cli requires the `cli` feature");
}
