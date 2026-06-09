use super::prelude::*;

pub fn handle(command: IpcCommand, library_root: &Path) -> IpcResponse {
    match command {
        IpcCommand::Ping => IpcResponse::Pong,
        IpcCommand::ListShares => list_shares_response(library_root),
        IpcCommand::DeleteShare { share_id } => delete_share_response(library_root, share_id),
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
        IpcCommand::SubscribeEvents => IpcResponse::Ack {
            message: "use a streaming IPC connection to subscribe to events".to_string(),
        },
    }
}

pub async fn handle_async(command: IpcCommand, library_root: &Path) -> IpcResponse {
    match command {
        IpcCommand::SeedFile { input, chunk_size } => {
            queue_seed_file_command(input, chunk_size, library_root)
        }
        IpcCommand::Download {
            share_id,
            peers,
            output,
            parallelism,
            request_window,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            auth_psk,
        } => queue_download_command(
            share_id,
            peers,
            output,
            parallelism,
            request_window,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            auth_psk,
            true,
            library_root,
        ),
        IpcCommand::DownloadFresh {
            share_id,
            peers,
            output,
            parallelism,
            request_window,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            auth_psk,
        } => queue_download_command(
            share_id,
            peers,
            output,
            parallelism,
            request_window,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            auth_psk,
            false,
            library_root,
        ),
        command => handle(command, library_root),
    }
}

fn queue_seed_file_command(input: PathBuf, chunk_size: usize, library_root: &Path) -> IpcResponse {
    let input_label = input.display().to_string();
    let library_root = library_root.to_path_buf();

    tokio::task::spawn_blocking(move || {
        match run_seed_file_command(input, chunk_size, &library_root) {
            IpcResponse::ShareAdded { share } => {
                super::events::publish(IpcEvent::ShareUpdated {
                    share: share.clone(),
                });
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
            IpcResponse::Error { message } => {
                super::events::publish(IpcEvent::Error {
                    message: message.clone(),
                });
                eprintln!("[daemon] seed job failed: {message}");
            }
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

    match seed::add(&input, chunk_size, library_root, TransferLogLevel::Normal) {
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

fn generate_job_id(prefix: &str, share_id: ShareId) -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{prefix}-{share_id}-{millis}-{}", std::process::id())
}

#[allow(clippy::too_many_arguments)]
fn queue_download_command(
    share_id: ShareId,
    peers: Vec<SocketAddr>,
    output: Option<PathBuf>,
    parallelism: usize,
    request_window: usize,
    discovery_port: u16,
    discovery_timeout_ms: u64,
    discovery_multicast: Ipv4Addr,
    auth_psk: Option<String>,
    resume: bool,
    library_root: &Path,
) -> IpcResponse {
    let library_root = library_root.to_path_buf();
    let job_id = generate_job_id("download", share_id);
    jobs::register(share_id, job_id.clone());
    let task_job_id = job_id.clone();

    tokio::spawn(async move {
        match handle_download_command(
            share_id,
            peers,
            output,
            parallelism,
            request_window,
            discovery_port,
            discovery_timeout_ms,
            discovery_multicast,
            auth_psk,
            resume,
            &library_root,
        )
        .await
        {
            IpcResponse::TransferCompleted {
                output,
                file_name,
                file_size,
                chunks,
                ..
            } => {
                super::events::publish(IpcEvent::TaskProgress {
                    job_id: Some(task_job_id.clone()),
                    task: "peer:completed".to_string(),
                    label: share_id.to_string(),
                    completed_chunks: chunks,
                    total_chunks: chunks,
                    bytes_done: file_size,
                    total_bytes: file_size,
                    bytes_per_second: 0,
                });
                super::events::publish(IpcEvent::TransferCompleted {
                    job_id: Some(task_job_id.clone()),
                    share_id,
                    output: output.clone(),
                });
                if let Ok(Some(share)) = share_summary_by_id(&library_root, share_id) {
                    super::events::publish(IpcEvent::ShareUpdated { share });
                }
                println!("[daemon] download job completed: {share_id}");
                println!("[daemon] output: {}", output.display());
                println!("[daemon] file: {file_name}");
                println!("[daemon] file size: {file_size} bytes");
                println!("[daemon] chunks: {chunks}");
            }
            IpcResponse::Error { message } => {
                super::events::publish(IpcEvent::Error {
                    message: message.clone(),
                });
                eprintln!("[daemon] download job failed: {message}");
            }
            other => println!("[daemon] download job finished: {other:?}"),
        }

        jobs::unregister(share_id, &task_job_id);
    });

    IpcResponse::TransferQueued { share_id, job_id }
}

#[allow(clippy::too_many_arguments)]
async fn handle_download_command(
    share_id: ShareId,
    peers: Vec<SocketAddr>,
    output: Option<PathBuf>,
    parallelism: usize,
    request_window: usize,
    discovery_port: u16,
    discovery_timeout_ms: u64,
    discovery_multicast: Ipv4Addr,
    auth_psk: Option<String>,
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

    let peers = if peers.is_empty() {
        println!(
            "[daemon] no --peer supplied; discovering peers for share {share_id} on UDP port {discovery_port} for {discovery_timeout_ms} ms..."
        );
        let discovery_options = DiscoveryOptions::new(discovery_port)
            .with_timeout(Duration::from_millis(discovery_timeout_ms.max(1)))
            .with_multicast(discovery_multicast);

        match client::peers_with(share_id, discovery_options).await {
            Ok(discovered) if !discovered.is_empty() => {
                println!("[daemon] discovered {} peer(s)", discovered.len());
                discovered
            }
            Ok(_) => {
                return IpcResponse::Error {
                    message: format!(
                        "no peers discovered for share {share_id}; pass --peer manually or ensure etled discovery is enabled on the LAN"
                    ),
                };
            }
            Err(error) => {
                return IpcResponse::Error {
                    message: format!("peer discovery failed: {error}"),
                };
            }
        }
    } else {
        peers
    };

    let effective_parallelism = effective_download_parallelism(parallelism, peers.len());
    if parallelism == 0 && effective_parallelism > 1 {
        println!(
            "[daemon] auto parallelism: {effective_parallelism} worker(s) for {} peer(s)",
            peers.len()
        );
    }

    let mut options = DownloadFileOptions::new("etle-daemon", TransferLogLevel::Normal)
        .with_library_root(library_root.to_path_buf())
        .with_resume(resume)
        .with_requested_share_id(Some(share_id))
        .with_request_window(request_window);
    if let Some(auth_psk) = auth_psk {
        options = options.with_auth_psk(crate::crypto::key_exchange::AuthPsk::from_passphrase(
            auth_psk,
        ));
    }

    let result = if effective_parallelism > 1 {
        download::from_peers_parallel(peers, &output_path, options, effective_parallelism).await
    } else {
        download::from_peers(peers, &output_path, options).await
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

#[must_use]
fn effective_download_parallelism(requested_parallelism: usize, peer_count: usize) -> usize {
    if requested_parallelism == 0 {
        peer_count.max(1)
    } else {
        requested_parallelism
    }
}

fn list_shares_response(library_root: &Path) -> IpcResponse {
    match library::list(library_root) {
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

fn delete_share_response(library_root: &Path, share_id: ShareId) -> IpcResponse {
    println!("[daemon] ipc delete requested: share_id={share_id}");

    let share = match share_summary_by_id(library_root, share_id) {
        Ok(Some(share)) => share,
        Ok(None) => {
            println!("[daemon] ipc delete failed: share_id={share_id} not found");
            return IpcResponse::Error {
                message: format!("share {share_id} does not exist in the local library"),
            };
        }
        Err(message) => {
            println!("[daemon] ipc delete failed: share_id={share_id}: {message}");
            return IpcResponse::Error { message };
        }
    };

    match library::delete(library_root, share_id) {
        Ok(true) => {
            println!(
                "[daemon] ipc delete completed: share_id={share_id} name=\"{}\"",
                share.name
            );
            super::events::publish(IpcEvent::ShareDeleted { share_id });
            IpcResponse::ShareDeleted {
                share_id,
                name: share.name,
            }
        }
        Ok(false) => {
            println!("[daemon] ipc delete failed: share_id={share_id} disappeared");
            IpcResponse::Error {
                message: format!("share {share_id} disappeared before it could be deleted"),
            }
        }
        Err(error) => {
            println!("[daemon] ipc delete failed: share_id={share_id}: {error}");
            IpcResponse::Error {
                message: error.to_string(),
            }
        }
    }
}

fn share_summary_by_id(
    library_root: &Path,
    share_id: ShareId,
) -> Result<Option<IpcShareSummary>, String> {
    let shares = library::list(library_root).map_err(|error| error.to_string())?;
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
pub(super) fn bind_ipc_listener(socket_path: &Path) -> Result<UnixListener, IpcError> {
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
