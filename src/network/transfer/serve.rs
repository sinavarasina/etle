use super::prelude::*;

async fn server_transfer_session_key_exchange(
    stream: &mut TcpStream,
    auth_psk: Option<&AuthPsk>,
) -> Result<SymmetricKey, NetworkError> {
    match auth_psk {
        Some(psk) => server_authenticated_session_key_exchange(stream, psk).await,
        None => server_session_key_exchange(stream).await,
    }
}

pub async fn file_once(
    listener: TcpListener,
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    seeder_id: impl Into<String>,
) -> Result<(), NetworkError> {
    file_once_with(
        listener,
        input_path,
        chunk_size,
        ServeFileOptions::new(seeder_id, TransferLogLevel::Quiet),
    )
    .await
}

pub async fn file_once_with(
    listener: TcpListener,
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root,
        auth_psk,
    } = options;
    let input_path = input_path.as_ref().to_path_buf();
    let (library_root, _temporary_root) = match library_root {
        Some(root) => (root, None),
        None => {
            let temporary_root = super::seed::TemporaryLibraryRoot::create()?;
            (temporary_root.path.clone(), Some(temporary_root))
        }
    };

    if log_level.is_normal() {
        println!(
            "[seeder] direct one-peer serve stages via library root: {}",
            library_root.display()
        );
    }

    let descriptor = super::seed::add(&input_path, chunk_size, &library_root, log_level)?;
    let mut options =
        ServeFileOptions::new(seeder_id, log_level).with_library_root(library_root.clone());
    if let Some(auth_psk) = auth_psk {
        options = options.with_auth_psk(auth_psk);
    }

    share_once_from_listener(&listener, &library_root, descriptor.share_id, options)
        .await
        .map(|_| ())
}

pub async fn share_once(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    share_once_from_listener(&listener, library_root, share_id, options).await
}

pub async fn share_forever(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        let (stream, peer_addr) = match accept_peer(&listener).await {
            Ok(accepted) => accepted,
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[seeder] transient accept error: {error}");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let peer_library_root = library_root.clone();
        let peer_options = options.clone();
        let log_level = options.log_level;

        tokio::spawn(async move {
            match share_connected_peer(stream, peer_addr, peer_library_root, share_id, peer_options)
                .await
            {
                Ok(descriptor) => {
                    if log_level.is_normal() {
                        println!(
                            "[seeder] peer session completed: share=\"{}\", share_id={}",
                            descriptor.name, descriptor.share_id
                        );
                    }
                }
                Err(error) if log_level.is_normal() => {
                    println!("[seeder] peer session failed: {error}");
                }
                Err(_) => {}
            }
        });

        if options.log_level.is_normal() {
            println!("[seeder] spawned single-share peer session for {peer_addr}");
        }
    }
}

pub async fn library_once(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    library_once_from_listener(&listener, library_root, options).await
}

pub async fn library_forever(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        let (stream, peer_addr) = match accept_peer(&listener).await {
            Ok(accepted) => accepted,
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[seeder] transient accept error: {error}");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let peer_library_root = library_root.clone();
        let peer_options = options.clone();
        let log_level = options.log_level;

        tokio::spawn(async move {
            match library_connected_peer(stream, peer_addr, peer_library_root, peer_options).await {
                Ok(descriptor) => {
                    if log_level.is_normal() {
                        println!(
                            "[seeder] peer session completed: share=\"{}\", share_id={}",
                            descriptor.name, descriptor.share_id
                        );
                    }
                }
                Err(error) if log_level.is_normal() => {
                    println!("[seeder] peer session failed: {error}");
                }
                Err(_) => {}
            }
        });

        if options.log_level.is_normal() {
            println!("[seeder] spawned peer session for {peer_addr}; listener remains available");
        }
    }
}

async fn library_connected_peer(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    library_root: PathBuf,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
        auth_psk,
    } = options;
    let library_root = library_root.as_path();

    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
    }

    let remote_peer_id =
        server_transfer_handshake(&mut stream, seeder_id, auth_psk.is_some()).await?;
    events::publish(IpcEvent::PeerConnected {
        peer_id: format!("{remote_peer_id} ({peer_addr})"),
    });
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let session_key = server_transfer_session_key_exchange(&mut stream, auth_psk.as_ref()).await?;
    if log_level.is_normal() {
        println!("[seeder] key exchange completed");
    }

    let share_id = match receive(&mut stream).await? {
        WireMessage::RequestShare { share_id } => share_id,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "RequestShare",
                actual,
            });
        }
    };

    let paths = LibraryPaths::for_share(library_root, share_id);
    let descriptor = read_descriptor(&paths)?;
    let secret = read_secret(&paths)?;
    let manifest = super::download::manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();
    let available_chunks = super::download::available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

    if log_level.is_normal() {
        println!(
            "[seeder] peer requested share: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    send(
        &mut stream,
        &WireMessage::Manifest {
            manifest: manifest.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] manifest sent from multi-share library");
    }

    let wrapped_file_key = wrap_file_key(&session_key, manifest.file_id, &secret.file_key)?;
    send(
        &mut stream,
        &WireMessage::WrappedFileKey {
            nonce: wrapped_file_key.nonce,
            data: wrapped_file_key.data,
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] wrapped file key sent");
    }

    send(
        &mut stream,
        &WireMessage::Have {
            chunks: available_chunks.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!(
            "[seeder] advertised {}/{} available chunks",
            available_chunks.len(),
            total_chunks
        );
    }

    let mut served_or_known = BTreeSet::new();
    let mut chunks_served = 0_usize;
    let total_encrypted_bytes: u64 = descriptor
        .chunks
        .iter()
        .fold(0_u64, |acc, chunk| acc.saturating_add(chunk.encrypted_size));
    let mut upload_session: Option<UploadSessionGuard> = None;

    while served_or_known.len() < total_chunks {
        let message = match receive(&mut stream).await {
            Ok(message) => message,
            Err(error) if super::download::is_peer_closed_protocol_error(&error) => {
                if log_level.is_normal() {
                    println!("[seeder] peer disconnected before requesting all chunks");
                }
                if let Some(session) = upload_session.take() {
                    session.complete(format!(
                        "peer disconnected after serving {chunks_served} chunk(s)"
                    ));
                }
                return Ok(descriptor);
            }
            Err(error) => return Err(error.into()),
        };

        match message {
            WireMessage::Have { chunks } => {
                for index in chunks {
                    if available_set.contains(&index) {
                        served_or_known.insert(index);
                    }
                }
            }
            WireMessage::RequestChunk { index } => {
                if !available_set.contains(&index) {
                    send(
                        &mut stream,
                        &WireMessage::Error {
                            message: format!("chunk {index} is not available from this peer"),
                        },
                    )
                    .await?;
                    continue;
                }

                let meta = descriptor
                    .chunks
                    .iter()
                    .find(|chunk| chunk.index == index)
                    .ok_or(NetworkError::MissingEncryptedChunk(index))?;

                let session = upload_session.get_or_insert_with(|| {
                    UploadSessionGuard::start(
                        share_id,
                        &descriptor.name,
                        &remote_peer_id,
                        total_chunks,
                        &served_or_known,
                        &descriptor.chunks,
                        total_encrypted_bytes,
                    )
                });

                send_chunk_file(
                    &mut stream,
                    index,
                    paths.chunk_path(index),
                    meta.encrypted_size,
                )
                .await?;

                served_or_known.insert(index);
                chunks_served += 1;
                super::progress::with_context_label_for_job(
                    session.job_id.clone(),
                    Some(share_id),
                    Some(session.job_id.clone()),
                    "seeder",
                    "served-from-library",
                    log_level,
                    served_or_known.len(),
                    total_chunks,
                    index,
                    meta.encrypted_size as usize,
                );
            }
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Have or RequestChunk",
                    actual,
                });
            }
        }
    }

    if let Some(session) = upload_session.take() {
        session.complete(format!(
            "uploaded {chunks_served} chunk(s) to {remote_peer_id}"
        ));
    }

    Ok(descriptor)
}

struct UploadSessionGuard {
    job_id: String,
    total_chunks: usize,
    finished: bool,
}

impl UploadSessionGuard {
    fn start(
        share_id: ShareId,
        share_name: &str,
        remote_peer_id: &str,
        total_chunks: usize,
        known_chunks: &BTreeSet<u32>,
        chunks: &[ChunkMeta],
        total_bytes: u64,
    ) -> Self {
        static NEXT_UPLOAD_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let seq = NEXT_UPLOAD_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let job_id = format!("upload-{seq}");
        let baseline_bytes: u64 = chunks
            .iter()
            .filter(|meta| known_chunks.contains(&meta.index))
            .map(|meta| meta.encrypted_size)
            .sum();

        super::progress::init_context_session(
            &job_id,
            "seeder",
            "served-from-library",
            total_chunks,
            known_chunks.len(),
            baseline_bytes,
            Some(total_bytes),
            Some(job_id.clone()),
        );
        super::jobs::start_job(IpcActiveJob {
            job_id: job_id.clone(),
            kind: IpcJobKind::Upload,
            share_id: Some(share_id),
            label: format!("{share_name} -> {remote_peer_id}"),
            phase: "seeder:served-from-library".to_string(),
            completed_chunks: known_chunks.len(),
            total_chunks,
            bytes_done: baseline_bytes,
            total_bytes,
            bytes_per_second: 0,
        });

        Self {
            job_id,
            total_chunks,
            finished: false,
        }
    }

    fn complete(mut self, summary: String) {
        self.finished = true;
        super::progress::clear_context_session(
            &self.job_id,
            "seeder",
            "served-from-library",
            self.total_chunks,
        );
        super::jobs::complete_job(&self.job_id, summary);
    }
}

impl Drop for UploadSessionGuard {
    fn drop(&mut self) {
        if !self.finished {
            super::progress::clear_context_session(
                &self.job_id,
                "seeder",
                "served-from-library",
                self.total_chunks,
            );
            super::jobs::fail_job(&self.job_id, "upload interrupted by peer error");
        }
    }
}

async fn share_once_from_listener(
    listener: &TcpListener,
    library_root: impl AsRef<Path>,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let (stream, peer_addr) = accept_peer(listener).await?;
    share_connected_peer(
        stream,
        peer_addr,
        library_root.as_ref().to_path_buf(),
        share_id,
        options,
    )
    .await
}

async fn share_connected_peer(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    library_root: PathBuf,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
        auth_psk,
    } = options;
    let paths = LibraryPaths::for_share(&library_root, share_id);
    let descriptor = read_descriptor(&paths)?;
    let secret = read_secret(&paths)?;
    let manifest = super::download::manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();
    let available_chunks = super::download::available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
        println!(
            "[seeder] loading share state: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    let remote_peer_id =
        server_transfer_handshake(&mut stream, seeder_id, auth_psk.is_some()).await?;
    events::publish(IpcEvent::PeerConnected {
        peer_id: format!("{remote_peer_id} ({peer_addr})"),
    });
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let session_key = server_transfer_session_key_exchange(&mut stream, auth_psk.as_ref()).await?;
    if log_level.is_normal() {
        println!("[seeder] key exchange completed");
    }

    send(
        &mut stream,
        &WireMessage::Manifest {
            manifest: manifest.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] manifest sent from persisted state");
    }

    let wrapped_file_key = wrap_file_key(&session_key, manifest.file_id, &secret.file_key)?;
    send(
        &mut stream,
        &WireMessage::WrappedFileKey {
            nonce: wrapped_file_key.nonce,
            data: wrapped_file_key.data,
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] wrapped file key sent");
    }

    send(
        &mut stream,
        &WireMessage::Have {
            chunks: available_chunks.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!(
            "[seeder] advertised {}/{} available chunks",
            available_chunks.len(),
            total_chunks
        );
    }

    static NEXT_SHARE_SESSION_SEQ: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(1);
    let session_context = format!(
        "{share_id}:state:{}",
        NEXT_SHARE_SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let _progress_guard = super::progress::ProgressSessionGuard::new(
        session_context.clone(),
        "seeder",
        "served-from-state",
        total_chunks,
    );

    static NEXT_LIBRARY_SESSION_SEQ: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(1);
    let session_context = format!(
        "{share_id}:library:{}",
        NEXT_LIBRARY_SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let _progress_guard = super::progress::ProgressSessionGuard::new(
        session_context.clone(),
        "seeder",
        "served-from-library",
        total_chunks,
    );

    let mut served_or_known = BTreeSet::new();
    while served_or_known.len() < total_chunks {
        let message = match receive(&mut stream).await {
            Ok(message) => message,
            Err(error) if super::download::is_peer_closed_protocol_error(&error) => {
                if log_level.is_normal() {
                    println!("[seeder] peer disconnected before requesting all chunks");
                }
                return Ok(descriptor);
            }
            Err(error) => return Err(error.into()),
        };

        match message {
            WireMessage::Have { chunks } => {
                for index in chunks {
                    if available_set.contains(&index) {
                        served_or_known.insert(index);
                    }
                }
            }
            WireMessage::RequestChunk { index } => {
                if !available_set.contains(&index) {
                    send(
                        &mut stream,
                        &WireMessage::Error {
                            message: format!("chunk {index} is not available from this peer"),
                        },
                    )
                    .await?;
                    continue;
                }

                let meta = descriptor
                    .chunks
                    .iter()
                    .find(|chunk| chunk.index == index)
                    .ok_or(NetworkError::MissingEncryptedChunk(index))?;
                send_chunk_file(
                    &mut stream,
                    index,
                    paths.chunk_path(index),
                    meta.encrypted_size,
                )
                .await?;

                served_or_known.insert(index);
                super::progress::with_context_label_for_job(
                    session_context.clone(),
                    Some(share_id),
                    None,
                    "seeder",
                    "served-from-state",
                    log_level,
                    served_or_known.len(),
                    total_chunks,
                    index,
                    meta.encrypted_size as usize,
                );
            }
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Have or RequestChunk",
                    actual,
                });
            }
        }
    }

    Ok(descriptor)
}

pub async fn library_once_from_listener(
    listener: &TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
        auth_psk,
    } = options;
    let library_root = library_root.as_ref();

    let (mut stream, peer_addr) = accept_peer(listener).await?;
    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
    }

    let remote_peer_id =
        server_transfer_handshake(&mut stream, seeder_id, auth_psk.is_some()).await?;
    events::publish(IpcEvent::PeerConnected {
        peer_id: format!("{remote_peer_id} ({peer_addr})"),
    });
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let session_key = server_transfer_session_key_exchange(&mut stream, auth_psk.as_ref()).await?;
    if log_level.is_normal() {
        println!("[seeder] key exchange completed");
    }

    let share_id = match receive(&mut stream).await? {
        WireMessage::RequestShare { share_id } => share_id,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "RequestShare",
                actual,
            });
        }
    };

    let paths = LibraryPaths::for_share(library_root, share_id);
    let descriptor = read_descriptor(&paths)?;
    let secret = read_secret(&paths)?;
    let manifest = super::download::manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();
    let available_chunks = super::download::available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

    if log_level.is_normal() {
        println!(
            "[seeder] peer requested share: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    send(
        &mut stream,
        &WireMessage::Manifest {
            manifest: manifest.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] manifest sent from multi-share library");
    }

    let wrapped_file_key = wrap_file_key(&session_key, manifest.file_id, &secret.file_key)?;
    send(
        &mut stream,
        &WireMessage::WrappedFileKey {
            nonce: wrapped_file_key.nonce,
            data: wrapped_file_key.data,
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] wrapped file key sent");
    }

    send(
        &mut stream,
        &WireMessage::Have {
            chunks: available_chunks.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!(
            "[seeder] advertised {}/{} available chunks",
            available_chunks.len(),
            total_chunks
        );
    }

    static NEXT_LIBRARY_SESSION_SEQ: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(1);
    let session_context = format!(
        "{share_id}:library:{}",
        NEXT_LIBRARY_SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let _progress_guard = super::progress::ProgressSessionGuard::new(
        session_context.clone(),
        "seeder",
        "served-from-library",
        total_chunks,
    );

    let mut served_or_known = BTreeSet::new();
    while served_or_known.len() < total_chunks {
        let message = match receive(&mut stream).await {
            Ok(message) => message,
            Err(error) if super::download::is_peer_closed_protocol_error(&error) => {
                if log_level.is_normal() {
                    println!("[seeder] peer disconnected before requesting all chunks");
                }
                return Ok(descriptor);
            }
            Err(error) => return Err(error.into()),
        };

        match message {
            WireMessage::Have { chunks } => {
                for index in chunks {
                    if available_set.contains(&index) {
                        served_or_known.insert(index);
                    }
                }

                if log_level.is_normal() {
                    println!(
                        "[seeder] peer already has {}/{} chunks",
                        served_or_known.len(),
                        total_chunks
                    );
                }
            }
            WireMessage::RequestChunk { index } => {
                if !available_set.contains(&index) {
                    send(
                        &mut stream,
                        &WireMessage::Error {
                            message: format!("chunk {index} is not available from this peer"),
                        },
                    )
                    .await?;
                    continue;
                }

                let meta = descriptor
                    .chunks
                    .iter()
                    .find(|chunk| chunk.index == index)
                    .ok_or(NetworkError::MissingEncryptedChunk(index))?;
                send_chunk_file(
                    &mut stream,
                    index,
                    paths.chunk_path(index),
                    meta.encrypted_size,
                )
                .await?;

                served_or_known.insert(index);
                super::progress::with_context_label_for_job(
                    session_context.clone(),
                    Some(share_id),
                    None,
                    "seeder",
                    "served-from-library",
                    log_level,
                    served_or_known.len(),
                    total_chunks,
                    index,
                    meta.encrypted_size as usize,
                );
            }
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Have or RequestChunk",
                    actual,
                });
            }
        }
    }

    Ok(descriptor)
}
