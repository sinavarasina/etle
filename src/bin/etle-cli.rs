#[cfg(feature = "cli")]
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
use etle::{
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        client_hello_handshake, connect_peer, download_file_from_peers_parallel_with_options,
        download_file_from_peers_with_options, serve_file_to_one_peer_with_options,
        serve_library_forever, serve_library_share_forever, serve_library_share_to_one_peer,
    },
    state::{OUTPUT_DIR_NAME, ShareMode, default_library_root, list_library_shares},
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

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,
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

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,

        /// Keep serving this share after one peer disconnects.
        #[arg(long)]
        forever: bool,
    },

    /// List local shares stored in the .etle library.
    List {
        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,
    },

    /// Download a file from one or more seeder peers.
    Download {
        /// Seeder peer address. Can be repeated for sequential fallback.
        #[arg(long, required = true)]
        peer: Vec<SocketAddr>,

        /// Share ID to request from a multi-share library server.
        #[arg(long)]
        share_id: Option<ShareId>,

        /// Output file path. Defaults to <library-root>/output/<file-name-from-manifest>.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-peer")]
        peer_id: String,

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,

        /// Reuse verified encrypted chunks from the local .etle library when available.
        /// This is enabled by default; the flag is accepted for explicitness.
        #[arg(long)]
        resume: bool,

        /// Ignore existing local chunks and fetch chunks from peers again.
        #[arg(long)]
        no_resume: bool,

        /// Number of parallel peer workers for multi-peer download.
        #[arg(long, default_value_t = 1)]
        parallel: usize,
    },

    /// Serve any local library share requested by peers over one P2P port.
    ServeLibrary {
        /// TCP listen address.
        #[arg(long, default_value = "127.0.0.1:7000")]
        listen: SocketAddr,

        /// Local peer identifier sent during hello handshake.
        #[arg(long, default_value = "etle-seeder")]
        peer_id: String,

        /// Root directory for local state. Defaults to $ETLE_LIBRARY_ROOT or ~/Downloads/ETLE.
        #[arg(long)]
        library_root: Option<PathBuf>,
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
            let library_root = library_root.unwrap_or_else(default_library_root);

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
            forever,
            library_root,
        } => {
            let library_root = library_root.unwrap_or_else(default_library_root);

            println!("[seeder] loading share from state: {share_id}");
            println!("[seeder] listen address: {listen}");
            println!("[seeder] library root: {}", library_root.display());

            let listener = bind_listener(listen).await?;
            let serve_options = ServeFileOptions::new(peer_id, log_level);

            if forever {
                println!("[seeder] serving continuously; press Ctrl+C to stop");
                serve_library_share_forever(listener, &library_root, share_id, serve_options)
                    .await?;
            } else {
                println!("[seeder] waiting for one peer...");

                let descriptor = serve_library_share_to_one_peer(
                    listener,
                    &library_root,
                    share_id,
                    serve_options,
                )
                .await?;

                println!("[seeder] transfer completed");
                println!("[seeder] share: {}", descriptor.name);
                println!("[seeder] share_id: {}", descriptor.share_id);
                println!("[seeder] chunks: {}", descriptor.chunks.len());
            }
        }

        Command::List { library_root } => {
            let library_root = library_root.unwrap_or_else(default_library_root);

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
            share_id,
            output,
            peer_id,
            library_root,
            resume,
            no_resume,
            parallel,
        } => {
            if resume && no_resume {
                anyhow::bail!("--resume and --no-resume cannot be used together");
            }

            let resume_enabled = !no_resume;

            let library_root = library_root.unwrap_or_else(default_library_root);

            println!("[peer] peer count: {}", peer.len());
            for (index, peer_addr) in peer.iter().enumerate() {
                println!("[peer] peer {}: {peer_addr}", index + 1);
            }
            if let Some(share_id) = share_id {
                println!("[peer] requested share_id: {share_id}");
            }
            let auto_output = output.is_none();
            let output = output.unwrap_or_else(|| temporary_download_output_path(&library_root));
            create_output_parent_dir(&output)?;

            if auto_output {
                println!(
                    "[peer] output path: automatic ({} / manifest file name)",
                    default_download_output_dir(&library_root).display()
                );
            } else {
                println!("[peer] output path: {}", output.display());
            }
            println!("[peer] library root: {}", library_root.display());
            if resume_enabled {
                println!("[peer] resume enabled");
            } else {
                println!("[peer] resume disabled: existing local chunks will be ignored");
            }
            if parallel > 1 {
                println!("[peer] parallel workers: {parallel}");
            }

            let options = DownloadFileOptions::new(peer_id, log_level)
                .with_library_root(library_root.clone())
                .with_resume(resume_enabled)
                .with_requested_share_id(share_id);

            let manifest = if parallel > 1 {
                download_file_from_peers_parallel_with_options(peer, &output, options, parallel)
                    .await?
            } else {
                download_file_from_peers_with_options(peer, &output, options).await?
            };

            let final_output = if auto_output {
                move_auto_download_output(&output, &library_root, &manifest.file_name)?
            } else {
                output.clone()
            };

            println!("[peer] transfer completed");
            println!("[peer] output: {}", final_output.display());
            println!("[peer] file: {}", manifest.file_name);
            println!("[peer] file_id: {}", manifest.file_id);
            println!("[peer] file size: {} bytes", manifest.file_size);
            println!("[peer] chunks: {}", manifest.chunks.len());
        }

        Command::ServeLibrary {
            listen,
            peer_id,
            library_root,
        } => {
            let library_root = library_root.unwrap_or_else(default_library_root);

            println!("[seeder] serving local library");
            println!("[seeder] listen address: {listen}");
            println!("[seeder] library root: {}", library_root.display());
            println!("[seeder] peers must request a share_id with RequestShare");
            println!("[seeder] serving continuously; press Ctrl+C to stop");

            let listener = bind_listener(listen).await?;
            serve_library_forever(
                listener,
                &library_root,
                ServeFileOptions::new(peer_id, log_level),
            )
            .await?;
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
fn default_download_output_dir(library_root: &Path) -> PathBuf {
    library_root.join(OUTPUT_DIR_NAME)
}

#[cfg(feature = "cli")]
fn temporary_download_output_path(library_root: &Path) -> PathBuf {
    default_download_output_dir(library_root)
        .join(format!(".etle-download-{}.part", std::process::id()))
}

#[cfg(feature = "cli")]
fn move_auto_download_output(
    temporary_output: &Path,
    library_root: &Path,
    manifest_file_name: &str,
) -> anyhow::Result<PathBuf> {
    let final_output = unique_output_path(default_download_output_path(
        library_root,
        manifest_file_name,
    ));
    create_output_parent_dir(&final_output)?;
    fs::rename(temporary_output, &final_output)?;

    Ok(final_output)
}

#[cfg(feature = "cli")]
fn default_download_output_path(library_root: &Path, manifest_file_name: &str) -> PathBuf {
    default_download_output_dir(library_root).join(safe_output_file_name(manifest_file_name))
}

#[cfg(feature = "cli")]
fn safe_output_file_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "downloaded-file".to_string())
}

#[cfg(feature = "cli")]
fn create_output_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}

#[cfg(feature = "cli")]
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
