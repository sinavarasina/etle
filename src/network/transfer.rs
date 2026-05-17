use std::{
    collections::{BTreeMap, VecDeque},
    io::ErrorKind,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::net::{TcpListener, TcpStream};

use crate::{
    crypto::{
        aead::SymmetricKey,
        hash::hash_chunk,
        key_exchange::derive_session_key,
        key_wrap::{WrappedFileKey, generate_file_key, unwrap_file_key, wrap_file_key},
    },
    file::{
        descriptor::{EtleDescriptor, FileEntry, ShareId},
        error::FileError,
        manifest::{ChunkMeta, Manifest},
        storage::{EncryptedChunk, EncryptedFile, decrypt_to_file, encrypt_file},
    },
    network::{
        NetworkError, accept_peer, client_hello_handshake, client_shared_secret_exchange,
        connect_peer, server_hello_handshake, server_shared_secret_exchange,
    },
    protocol::{ProtocolError, WireMessage, receive_message, send_message},
    state::{
        DownloadProgress, LibraryPaths, ShareMode, ShareState, has_encrypted_chunk,
        initialize_share_library, read_descriptor, read_encrypted_chunk, read_progress,
        read_secret, write_encrypted_chunk, write_progress, write_state,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransferLogLevel {
    #[default]
    Quiet,
    Normal,
    Verbose,
}

impl TransferLogLevel {
    #[must_use]
    pub const fn is_normal(self) -> bool {
        matches!(self, Self::Normal | Self::Verbose)
    }

    #[must_use]
    pub const fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

#[derive(Clone, Debug)]
pub struct ServeFileOptions {
    pub seeder_id: String,
    pub log_level: TransferLogLevel,
    pub library_root: Option<PathBuf>,
}

impl ServeFileOptions {
    #[must_use]
    pub fn new(seeder_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            seeder_id: seeder_id.into(),
            log_level,
            library_root: None,
        }
    }

    #[must_use]
    pub fn with_library_root(mut self, library_root: impl Into<PathBuf>) -> Self {
        self.library_root = Some(library_root.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct DownloadFileOptions {
    pub peer_id: String,
    pub log_level: TransferLogLevel,
    pub library_root: Option<PathBuf>,
    pub resume: bool,
}

impl DownloadFileOptions {
    #[must_use]
    pub fn new(peer_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            peer_id: peer_id.into(),
            log_level,
            library_root: None,
            resume: false,
        }
    }

    #[must_use]
    pub fn with_library_root(mut self, library_root: impl Into<PathBuf>) -> Self {
        self.library_root = Some(library_root.into());
        self
    }

    #[must_use]
    pub const fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }
}

pub async fn serve_file_to_one_peer(
    listener: TcpListener,
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    seeder_id: impl Into<String>,
) -> Result<(), NetworkError> {
    serve_file_to_one_peer_with_options(
        listener,
        input_path,
        chunk_size,
        ServeFileOptions::new(seeder_id, TransferLogLevel::Quiet),
    )
    .await
}

pub async fn serve_file_to_one_peer_with_options(
    listener: TcpListener,
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root,
    } = options;
    let input_path = input_path.as_ref();
    let (mut stream, peer_addr) = accept_peer(&listener).await?;

    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
    }

    let remote_peer_id = server_hello_handshake(&mut stream, seeder_id).await?;
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let shared_secret = server_shared_secret_exchange(&mut stream).await?;
    if log_level.is_normal() {
        println!("[seeder] key exchange completed");
    }

    if log_level.is_normal() {
        println!("[seeder] hashing and encrypting file...");
    }

    // The file key is now independent from the peer session. This makes
    // encrypted chunks reusable by future swarm/partial-seeder flows.
    let session_key = derive_session_key(shared_secret);
    let file_key = generate_file_key();
    let encrypted = encrypt_file(input_path, &file_key, chunk_size)?;
    let total_chunks = encrypted.manifest.chunks.len();

    persist_seed_library_state(
        library_root.as_deref(),
        &encrypted.manifest,
        file_key,
        &encrypted.chunks,
        log_level,
    )?;

    if log_level.is_normal() {
        println!(
            "[seeder] encrypted manifest ready: file=\"{}\", size={} bytes, chunks={}",
            encrypted.manifest.file_name, encrypted.manifest.file_size, total_chunks
        );
    }

    if log_level.is_verbose() {
        println!("[seeder] file_id: {}", encrypted.manifest.file_id);
        println!(
            "[seeder] manifest chunk_size: {} bytes",
            encrypted.manifest.chunk_size
        );
        println!("[seeder] generated reusable file key");
    }

    send_message(
        &mut stream,
        &WireMessage::Manifest {
            manifest: encrypted.manifest.clone(),
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] manifest sent");
    }

    let wrapped_file_key = wrap_file_key(&session_key, encrypted.manifest.file_id, &file_key)?;
    send_message(
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

    let mut served_or_known = std::collections::BTreeSet::new();

    while served_or_known.len() < total_chunks {
        let message = match receive_message(&mut stream).await {
            Ok(message) => message,
            Err(error) if is_peer_closed_protocol_error(&error) => {
                if log_level.is_normal() {
                    println!("[seeder] peer disconnected before requesting all chunks");
                }
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };

        match message {
            WireMessage::Have { chunks } => {
                for index in chunks {
                    if encrypted.chunks.contains_key(&index) {
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
                let chunk = encrypted
                    .chunks
                    .get(&index)
                    .ok_or(NetworkError::MissingEncryptedChunk(index))?;

                send_message(
                    &mut stream,
                    &WireMessage::Chunk {
                        index,
                        data: chunk.data.clone(),
                    },
                )
                .await?;

                served_or_known.insert(index);
                let served_count = served_or_known.len();
                log_chunk_progress(
                    "seeder",
                    "served",
                    log_level,
                    served_count,
                    total_chunks,
                    index,
                    chunk.data.len(),
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

    Ok(())
}

pub async fn serve_library_share_to_one_peer(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
    } = options;
    let paths = LibraryPaths::for_share(library_root, share_id);
    let descriptor = read_descriptor(&paths)?;
    let secret = read_secret(&paths)?;
    let manifest = manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();

    let (mut stream, peer_addr) = accept_peer(&listener).await?;
    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
        println!(
            "[seeder] loading share state: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    let remote_peer_id = server_hello_handshake(&mut stream, seeder_id).await?;
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let shared_secret = server_shared_secret_exchange(&mut stream).await?;
    let session_key = derive_session_key(shared_secret);
    if log_level.is_normal() {
        println!("[seeder] key exchange completed");
    }

    send_message(
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
    send_message(
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

    let mut served_or_known = std::collections::BTreeSet::new();
    while served_or_known.len() < total_chunks {
        let message = match receive_message(&mut stream).await {
            Ok(message) => message,
            Err(error) if is_peer_closed_protocol_error(&error) => {
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
                    if descriptor.chunks.iter().any(|chunk| chunk.index == index) {
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
                let meta = descriptor
                    .chunks
                    .iter()
                    .find(|chunk| chunk.index == index)
                    .ok_or(NetworkError::MissingEncryptedChunk(index))?;
                let chunk = read_encrypted_chunk(&paths, index, meta.encrypted_size)?;

                send_message(
                    &mut stream,
                    &WireMessage::Chunk {
                        index,
                        data: chunk.data.clone(),
                    },
                )
                .await?;

                served_or_known.insert(index);
                log_chunk_progress(
                    "seeder",
                    "served-from-state",
                    log_level,
                    served_or_known.len(),
                    total_chunks,
                    index,
                    chunk.data.len(),
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

pub async fn download_file_from_peer(
    peer_addr: SocketAddr,
    output_path: impl AsRef<Path>,
    peer_id: impl Into<String>,
) -> Result<Manifest, NetworkError> {
    download_file_from_peer_with_options(
        peer_addr,
        output_path,
        DownloadFileOptions::new(peer_id, TransferLogLevel::Quiet),
    )
    .await
}

pub async fn download_file_from_peers_with_options(
    peer_addrs: impl IntoIterator<Item = SocketAddr>,
    output_path: impl AsRef<Path>,
    options: DownloadFileOptions,
) -> Result<Manifest, NetworkError> {
    let peer_addrs = peer_addrs.into_iter().collect::<Vec<_>>();
    if peer_addrs.is_empty() {
        return Err(NetworkError::NoPeersProvided);
    }

    let output_path = output_path.as_ref();
    let total_peers = peer_addrs.len();
    let mut last_error = None;

    for (attempt, peer_addr) in peer_addrs.into_iter().enumerate() {
        let attempt_number = attempt + 1;
        let mut attempt_options = options.clone();

        // Multi-peer fallback relies on persisted encrypted chunks between
        // attempts. Even if the user did not explicitly pass --resume, force
        // resume after the first peer so chunks fetched from a failed/partial
        // peer can be reused by the next peer.
        if total_peers > 1 || attempt > 0 {
            attempt_options = attempt_options.with_resume(true);
        }

        if attempt_options.log_level.is_normal() && total_peers > 1 {
            println!("[peer] trying peer {attempt_number}/{total_peers}: {peer_addr}");
        }

        match download_file_from_peer_with_options(peer_addr, output_path, attempt_options).await {
            Ok(manifest) => return Ok(manifest),
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[peer] peer {attempt_number}/{total_peers} failed: {error}");
                }
                last_error = Some(error.to_string());
            }
        }
    }

    Err(NetworkError::AllPeersFailed {
        attempts: total_peers,
        last_error: last_error.unwrap_or_else(|| "unknown error".to_string()),
    })
}

pub async fn download_file_from_peers_parallel_with_options(
    peer_addrs: impl IntoIterator<Item = SocketAddr>,
    output_path: impl AsRef<Path>,
    options: DownloadFileOptions,
    parallelism: usize,
) -> Result<Manifest, NetworkError> {
    let peer_addrs = peer_addrs.into_iter().collect::<Vec<_>>();
    if peer_addrs.is_empty() {
        return Err(NetworkError::NoPeersProvided);
    }

    if parallelism <= 1 || peer_addrs.len() == 1 {
        return download_file_from_peers_with_options(
            peer_addrs,
            output_path,
            options.with_resume(true),
        )
        .await;
    }

    let output_path = output_path.as_ref();
    let worker_limit = parallelism.min(peer_addrs.len());

    if options.log_level.is_normal() {
        println!(
            "[peer] parallel download enabled: workers={worker_limit}, peers={}",
            peer_addrs.len()
        );
    }

    let mut connected_peers = Vec::new();
    let mut reference_manifest = None;
    let mut reference_file_key = None;
    let mut last_error = None;

    for peer_addr in peer_addrs.into_iter().take(worker_limit) {
        match connect_download_peer(peer_addr, options.peer_id.clone(), options.log_level).await {
            Ok(peer) => {
                if let Some(reference) = &reference_manifest {
                    if !manifests_are_compatible(reference, &peer.manifest) {
                        if options.log_level.is_normal() {
                            println!("[peer] skipping {peer_addr}: manifest mismatch");
                        }
                        continue;
                    }
                } else {
                    reference_file_key = Some(peer.file_key);
                    reference_manifest = Some(peer.manifest.clone());
                }

                if let Some(file_key) = reference_file_key {
                    if peer.file_key != file_key {
                        if options.log_level.is_normal() {
                            println!("[peer] skipping {peer_addr}: file key mismatch");
                        }
                        continue;
                    }
                }

                connected_peers.push(peer);
            }
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[peer] failed to prepare parallel peer {peer_addr}: {error}");
                }
                last_error = Some(error.to_string());
            }
        }
    }

    let Some(manifest) = reference_manifest else {
        return Err(NetworkError::AllPeersFailed {
            attempts: worker_limit,
            last_error: last_error.unwrap_or_else(|| "no compatible peer prepared".to_string()),
        });
    };

    let Some(file_key) = reference_file_key else {
        return Err(NetworkError::AllPeersFailed {
            attempts: worker_limit,
            last_error: last_error.unwrap_or_else(|| "file key was not received".to_string()),
        });
    };

    if connected_peers.is_empty() {
        return Err(NetworkError::AllPeersFailed {
            attempts: worker_limit,
            last_error: last_error.unwrap_or_else(|| "no compatible peer prepared".to_string()),
        });
    }

    let descriptor = descriptor_from_manifest(&manifest);
    let output_state_dir = output_parent_dir(output_path);
    let mut library_state = initialize_download_library_state(
        options.library_root.as_deref(),
        &descriptor,
        file_key,
        output_state_dir.clone(),
        true,
        options.log_level,
    )?;

    let existing_chunks = load_resumable_chunks(
        &mut library_state,
        &manifest,
        output_state_dir.clone(),
        options.log_level,
    )?;

    let missing_chunks = manifest
        .chunks
        .iter()
        .filter(|meta| !existing_chunks.contains_key(&meta.index))
        .map(|meta| meta.index)
        .collect::<VecDeque<_>>();

    if missing_chunks.is_empty() {
        if options.log_level.is_normal() {
            println!("[peer] all chunks already available in local state");
            println!("[peer] decrypting and reconstructing file...");
        }

        let encrypted = EncryptedFile {
            manifest: manifest.clone(),
            chunks: existing_chunks,
        };
        decrypt_to_file(&encrypted, &file_key, output_path)?;
        mark_download_library_complete(&library_state, output_state_dir)?;
        return Ok(manifest);
    }

    let total_chunks = manifest.chunks.len();
    let queue = Arc::new(Mutex::new(missing_chunks));
    let chunks = Arc::new(Mutex::new(existing_chunks));
    let state = Arc::new(Mutex::new(library_state));
    let manifest = Arc::new(manifest);

    let mut handles = Vec::with_capacity(connected_peers.len());
    for peer in connected_peers {
        let queue = Arc::clone(&queue);
        let chunks = Arc::clone(&chunks);
        let state = Arc::clone(&state);
        let manifest = Arc::clone(&manifest);
        let output_state_dir = output_state_dir.clone();
        let log_level = options.log_level;

        handles.push(tokio::spawn(async move {
            parallel_download_worker(
                peer,
                manifest,
                queue,
                chunks,
                state,
                output_state_dir,
                log_level,
            )
            .await
        }));
    }

    let mut worker_errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => worker_errors.push(error.to_string()),
            Err(error) => worker_errors.push(error.to_string()),
        }
    }

    let chunks = match Arc::try_unwrap(chunks) {
        Ok(chunks) => chunks.into_inner().expect("parallel chunk mutex poisoned"),
        Err(chunks) => chunks
            .lock()
            .expect("parallel chunk mutex poisoned")
            .clone(),
    };

    if chunks.len() != total_chunks {
        if let Some(meta) = manifest
            .chunks
            .iter()
            .find(|meta| !chunks.contains_key(&meta.index))
        {
            if options.log_level.is_normal() && !worker_errors.is_empty() {
                println!(
                    "[peer] parallel worker errors: {}",
                    worker_errors.join("; ")
                );
            }
            return Err(NetworkError::MissingEncryptedChunk(meta.index));
        }
    }

    if options.log_level.is_normal() {
        println!("[peer] decrypting and reconstructing file...");
    }

    let encrypted = EncryptedFile {
        manifest: (*manifest).clone(),
        chunks,
    };

    decrypt_to_file(&encrypted, &file_key, output_path)?;

    {
        let state = state.lock().expect("parallel state mutex poisoned");
        mark_download_library_complete(&state, output_state_dir)?;
    }

    if options.log_level.is_normal() {
        println!("[peer] final hash verified: {}", manifest.file_id);
        println!("[peer] output written: {}", output_path.display());
    }

    Ok((*manifest).clone())
}

pub async fn download_file_from_peer_with_options(
    peer_addr: SocketAddr,
    output_path: impl AsRef<Path>,
    options: DownloadFileOptions,
) -> Result<Manifest, NetworkError> {
    let DownloadFileOptions {
        peer_id,
        log_level,
        library_root,
        resume,
    } = options;
    let output_path = output_path.as_ref();
    let mut stream = connect_peer(peer_addr).await?;

    if log_level.is_normal() {
        println!("[peer] connected to {peer_addr}");
    }

    let remote_peer_id = client_hello_handshake(&mut stream, peer_id).await?;
    if log_level.is_normal() {
        println!("[peer] hello handshake completed with {remote_peer_id}");
    }

    let shared_secret = client_shared_secret_exchange(&mut stream).await?;
    if log_level.is_normal() {
        println!("[peer] key exchange completed");
    }

    let manifest = match receive_message(&mut stream).await? {
        WireMessage::Manifest { manifest } => manifest,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Manifest",
                actual,
            });
        }
    };

    let total_chunks = manifest.chunks.len();
    if log_level.is_normal() {
        println!(
            "[peer] manifest received: file=\"{}\", size={} bytes, chunks={}",
            manifest.file_name, manifest.file_size, total_chunks
        );
    }

    if log_level.is_verbose() {
        println!("[peer] file_id: {}", manifest.file_id);
        println!("[peer] manifest chunk_size: {} bytes", manifest.chunk_size);
        println!("[peer] output path: {}", output_path.display());
    }

    let session_key = derive_session_key(shared_secret);
    let wrapped_file_key = match receive_message(&mut stream).await? {
        WireMessage::WrappedFileKey { nonce, data } => WrappedFileKey { nonce, data },
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "WrappedFileKey",
                actual,
            });
        }
    };

    let file_key = unwrap_file_key(&session_key, manifest.file_id, &wrapped_file_key)?;
    if log_level.is_normal() {
        println!("[peer] wrapped file key received and unlocked");
    }

    let descriptor = descriptor_from_manifest(&manifest);
    let output_state_dir = output_parent_dir(output_path);
    let mut library_state = initialize_download_library_state(
        library_root.as_deref(),
        &descriptor,
        file_key,
        output_state_dir.clone(),
        resume,
        log_level,
    )?;

    let mut chunks = load_resumable_chunks(
        &mut library_state,
        &manifest,
        output_state_dir.clone(),
        log_level,
    )?;

    if !chunks.is_empty() {
        let have_chunks = chunks.keys().copied().collect::<Vec<_>>();
        send_message(
            &mut stream,
            &WireMessage::Have {
                chunks: have_chunks,
            },
        )
        .await?;
    }

    for meta in &manifest.chunks {
        if chunks.contains_key(&meta.index) {
            log_chunk_progress(
                "peer",
                "reused",
                log_level,
                chunks.len(),
                total_chunks,
                meta.index,
                meta.encrypted_size as usize,
            );
            continue;
        }

        send_message(
            &mut stream,
            &WireMessage::RequestChunk { index: meta.index },
        )
        .await?;

        match receive_message(&mut stream).await? {
            WireMessage::Chunk { index, data } => {
                if index != meta.index {
                    return Err(NetworkError::UnexpectedChunkIndex {
                        expected: meta.index,
                        actual: index,
                    });
                }

                let actual_hash = hash_chunk(&data);
                if actual_hash != meta.blake3_hash {
                    return Err(FileError::ChunkHashMismatch(meta.index).into());
                }

                let chunk_len = data.len();
                if log_level.is_verbose() {
                    println!("[peer] chunk {} hash verified: {}", meta.index, actual_hash);
                }

                let encrypted_chunk = EncryptedChunk { index, data };
                persist_downloaded_chunk(
                    &mut library_state,
                    &encrypted_chunk,
                    output_state_dir.clone(),
                )?;

                chunks.insert(index, encrypted_chunk);
                log_chunk_progress(
                    "peer",
                    "received+verified",
                    log_level,
                    chunks.len(),
                    total_chunks,
                    index,
                    chunk_len,
                );
            }
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Chunk",
                    actual,
                });
            }
        }
    }

    if log_level.is_normal() {
        println!("[peer] decrypting and reconstructing file...");
    }

    let encrypted = EncryptedFile {
        manifest: manifest.clone(),
        chunks,
    };

    decrypt_to_file(&encrypted, &file_key, output_path)?;
    mark_download_library_complete(&library_state, output_state_dir)?;

    if log_level.is_normal() {
        println!("[peer] final hash verified: {}", manifest.file_id);
        println!("[peer] output written: {}", output_path.display());
    }

    Ok(manifest)
}

struct ConnectedDownloadPeer {
    peer_addr: SocketAddr,
    stream: TcpStream,
    manifest: Manifest,
    file_key: SymmetricKey,
}

async fn connect_download_peer(
    peer_addr: SocketAddr,
    peer_id: String,
    log_level: TransferLogLevel,
) -> Result<ConnectedDownloadPeer, NetworkError> {
    let mut stream = connect_peer(peer_addr).await?;

    if log_level.is_normal() {
        println!("[peer] connected to {peer_addr}");
    }

    let remote_peer_id = client_hello_handshake(&mut stream, peer_id).await?;
    if log_level.is_normal() {
        println!("[peer] hello handshake completed with {remote_peer_id}");
    }

    let shared_secret = client_shared_secret_exchange(&mut stream).await?;
    if log_level.is_normal() {
        println!("[peer] key exchange completed");
    }

    let manifest = match receive_message(&mut stream).await? {
        WireMessage::Manifest { manifest } => manifest,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Manifest",
                actual,
            });
        }
    };

    if log_level.is_normal() {
        println!(
            "[peer] manifest received from {peer_addr}: file=\"{}\", size={} bytes, chunks={}",
            manifest.file_name,
            manifest.file_size,
            manifest.chunks.len()
        );
    }

    let session_key = derive_session_key(shared_secret);
    let wrapped_file_key = match receive_message(&mut stream).await? {
        WireMessage::WrappedFileKey { nonce, data } => WrappedFileKey { nonce, data },
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "WrappedFileKey",
                actual,
            });
        }
    };

    let file_key = unwrap_file_key(&session_key, manifest.file_id, &wrapped_file_key)?;
    if log_level.is_normal() {
        println!("[peer] wrapped file key received and unlocked from {peer_addr}");
    }

    Ok(ConnectedDownloadPeer {
        peer_addr,
        stream,
        manifest,
        file_key,
    })
}

async fn parallel_download_worker(
    mut peer: ConnectedDownloadPeer,
    manifest: Arc<Manifest>,
    queue: Arc<Mutex<VecDeque<u32>>>,
    chunks: Arc<Mutex<BTreeMap<u32, EncryptedChunk>>>,
    state: Arc<Mutex<Option<ActiveDownloadLibraryState>>>,
    output_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    let initial_have = {
        let chunks = chunks.expect_lock("parallel chunk mutex poisoned");
        chunks.keys().copied().collect::<Vec<_>>()
    };

    if !initial_have.is_empty() {
        send_message(
            &mut peer.stream,
            &WireMessage::Have {
                chunks: initial_have,
            },
        )
        .await?;
    }

    loop {
        let Some(index) = pop_next_missing_chunk(&queue, &chunks) else {
            break;
        };

        let Some(meta) = manifest.chunks.iter().find(|meta| meta.index == index) else {
            continue;
        };

        match request_chunk_from_peer(&mut peer.stream, meta).await {
            Ok(encrypted_chunk) => {
                let chunk_len = encrypted_chunk.data.len();

                {
                    let mut state = state.expect_lock("parallel state mutex poisoned");
                    persist_downloaded_chunk(&mut state, &encrypted_chunk, output_dir.clone())?;
                }

                let done = {
                    let mut chunks = chunks.expect_lock("parallel chunk mutex poisoned");
                    chunks.insert(index, encrypted_chunk);
                    chunks.len()
                };

                log_chunk_progress(
                    "peer",
                    "parallel received+verified",
                    log_level,
                    done,
                    manifest.chunks.len(),
                    index,
                    chunk_len,
                );
            }
            Err(error) => {
                queue
                    .expect_lock("parallel queue mutex poisoned")
                    .push_back(index);
                return Err(error);
            }
        }
    }

    let final_have = {
        let chunks = chunks.expect_lock("parallel chunk mutex poisoned");
        chunks.keys().copied().collect::<Vec<_>>()
    };

    if !final_have.is_empty() {
        let _ = send_message(&mut peer.stream, &WireMessage::Have { chunks: final_have }).await;
    }

    if log_level.is_normal() {
        println!("[peer] parallel worker finished: {}", peer.peer_addr);
    }

    Ok(())
}

async fn request_chunk_from_peer(
    stream: &mut TcpStream,
    meta: &ChunkMeta,
) -> Result<EncryptedChunk, NetworkError> {
    send_message(stream, &WireMessage::RequestChunk { index: meta.index }).await?;

    match receive_message(stream).await? {
        WireMessage::Chunk { index, data } => {
            if index != meta.index {
                return Err(NetworkError::UnexpectedChunkIndex {
                    expected: meta.index,
                    actual: index,
                });
            }

            let actual_hash = hash_chunk(&data);
            if actual_hash != meta.blake3_hash {
                return Err(FileError::ChunkHashMismatch(meta.index).into());
            }

            Ok(EncryptedChunk { index, data })
        }
        actual => Err(NetworkError::UnexpectedMessage {
            expected: "Chunk",
            actual,
        }),
    }
}

fn pop_next_missing_chunk(
    queue: &Mutex<VecDeque<u32>>,
    chunks: &Mutex<BTreeMap<u32, EncryptedChunk>>,
) -> Option<u32> {
    loop {
        let index = queue
            .expect_lock("parallel queue mutex poisoned")
            .pop_front()?;

        if !chunks
            .expect_lock("parallel chunk mutex poisoned")
            .contains_key(&index)
        {
            return Some(index);
        }
    }
}

fn manifests_are_compatible(left: &Manifest, right: &Manifest) -> bool {
    left.file_id == right.file_id
        && left.file_name == right.file_name
        && left.file_size == right.file_size
        && left.chunk_size == right.chunk_size
        && left.chunks == right.chunks
}

trait MutexExpectLock<T> {
    fn expect_lock(&self, message: &str) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExpectLock<T> for Mutex<T> {
    fn expect_lock(&self, message: &str) -> std::sync::MutexGuard<'_, T> {
        self.lock().expect(message)
    }
}

fn descriptor_from_manifest(manifest: &Manifest) -> EtleDescriptor {
    EtleDescriptor::new(
        manifest.file_name.clone(),
        manifest.file_size,
        manifest.chunk_size,
        vec![FileEntry {
            path: manifest.file_name.clone(),
            size: manifest.file_size,
            offset: 0,
            blake3_hash: manifest.file_id,
        }],
        manifest.chunks.clone(),
    )
}

fn manifest_from_descriptor(descriptor: &EtleDescriptor) -> Result<Manifest, NetworkError> {
    if descriptor.files.len() != 1 {
        return Err(NetworkError::UnsupportedMultiFileDescriptor(
            descriptor.files.len(),
        ));
    }

    let file = &descriptor.files[0];
    Ok(Manifest {
        file_id: file.blake3_hash,
        file_name: file.path.clone(),
        file_size: descriptor.total_size,
        chunk_size: descriptor.chunk_size,
        chunks: descriptor.chunks.clone(),
    })
}

fn persist_seed_library_state(
    library_root: Option<&Path>,
    manifest: &Manifest,
    file_key: crate::crypto::aead::SymmetricKey,
    chunks: &BTreeMap<u32, EncryptedChunk>,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    let Some(root) = library_root else {
        return Ok(());
    };

    let descriptor = descriptor_from_manifest(manifest);
    let paths = initialize_share_library(root, &descriptor, file_key, ShareMode::Seeding, None)?;

    for chunk in chunks.values() {
        write_encrypted_chunk(&paths, chunk)?;
    }

    if log_level.is_normal() {
        println!(
            "[seeder] seed state stored: {}",
            paths.share_dir().display()
        );
    }

    Ok(())
}

struct ActiveDownloadLibraryState {
    paths: LibraryPaths,
    progress: DownloadProgress,
}

fn initialize_download_library_state(
    library_root: Option<&Path>,
    descriptor: &EtleDescriptor,
    file_key: crate::crypto::aead::SymmetricKey,
    output_dir: Option<PathBuf>,
    resume: bool,
    log_level: TransferLogLevel,
) -> Result<Option<ActiveDownloadLibraryState>, NetworkError> {
    let Some(root) = library_root else {
        return Ok(None);
    };

    let paths = LibraryPaths::for_share(root, descriptor.share_id);
    let progress = if resume && paths.progress_path().is_file() {
        if paths.descriptor_path().is_file() {
            let existing = read_descriptor(&paths)?;
            if existing != *descriptor && log_level.is_verbose() {
                println!("[peer] existing descriptor differs; resetting resume state");
            }
        }

        read_progress(&paths).unwrap_or_else(|_| DownloadProgress::empty(descriptor.share_id))
    } else {
        DownloadProgress::empty(descriptor.share_id)
    };

    let paths = initialize_share_library(
        root,
        descriptor,
        file_key,
        ShareMode::Downloading,
        output_dir.clone(),
    )?;

    if progress.completed_chunks.is_empty() {
        if log_level.is_normal() {
            println!(
                "[peer] download state initialized: {}",
                paths.share_dir().display()
            );
        }
        return Ok(Some(ActiveDownloadLibraryState { paths, progress }));
    }

    write_progress(&paths, &progress)?;
    write_state(
        &paths,
        &ShareState::from_progress(ShareMode::Downloading, output_dir, &progress),
    )?;

    if log_level.is_normal() {
        println!(
            "[peer] resume state loaded: {}/{} chunks",
            progress.completed_chunks.len(),
            descriptor.chunks.len()
        );
    }

    Ok(Some(ActiveDownloadLibraryState { paths, progress }))
}

fn load_resumable_chunks(
    state: &mut Option<ActiveDownloadLibraryState>,
    manifest: &Manifest,
    output_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
) -> Result<BTreeMap<u32, EncryptedChunk>, NetworkError> {
    let Some(state) = state else {
        return Ok(BTreeMap::new());
    };

    let mut chunks = BTreeMap::new();
    let mut valid_progress = DownloadProgress::empty(state.paths.share_id);

    for meta in &manifest.chunks {
        if !state.progress.has_chunk(meta.index) || !has_encrypted_chunk(&state.paths, meta.index) {
            continue;
        }

        let chunk = read_encrypted_chunk(&state.paths, meta.index, meta.encrypted_size)?;
        let actual_hash = hash_chunk(&chunk.data);
        if actual_hash != meta.blake3_hash {
            if log_level.is_verbose() {
                println!("[peer] ignored invalid resumable chunk {}", meta.index);
            }
            continue;
        }

        valid_progress.mark_completed(meta.index);
        chunks.insert(meta.index, chunk);
    }

    state.progress = valid_progress;
    write_progress(&state.paths, &state.progress)?;
    write_state(
        &state.paths,
        &ShareState::from_progress(ShareMode::Downloading, output_dir, &state.progress),
    )?;

    if log_level.is_normal() && !chunks.is_empty() {
        println!(
            "[peer] reused {}/{} chunks from local state",
            chunks.len(),
            manifest.chunks.len()
        );
    }

    Ok(chunks)
}

fn persist_downloaded_chunk(
    state: &mut Option<ActiveDownloadLibraryState>,
    chunk: &EncryptedChunk,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    let Some(state) = state else {
        return Ok(());
    };

    write_encrypted_chunk(&state.paths, chunk)?;
    state.progress.mark_completed(chunk.index);
    write_progress(&state.paths, &state.progress)?;
    write_state(
        &state.paths,
        &ShareState::from_progress(ShareMode::Downloading, output_dir, &state.progress),
    )?;

    Ok(())
}

fn mark_download_library_complete(
    state: &Option<ActiveDownloadLibraryState>,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    let Some(state) = state else {
        return Ok(());
    };

    write_state(
        &state.paths,
        &ShareState::from_progress(ShareMode::Completed, output_dir, &state.progress),
    )?;

    Ok(())
}

fn output_parent_dir(output_path: &Path) -> Option<PathBuf> {
    output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn is_peer_closed_protocol_error(error: &ProtocolError) -> bool {
    matches!(
        error,
        ProtocolError::Io(io_error)
            if matches!(
                io_error.kind(),
                ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            )
    )
}

fn log_chunk_progress(
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    if !should_log_progress(log_level, done, total) {
        return;
    }

    let percent = progress_percent(done, total);

    if log_level.is_verbose() {
        println!(
            "[{role}] {action} chunk {done}/{total} (index {index}, {bytes} bytes, {percent}%)"
        );
    } else {
        println!("[{role}] {action} chunk {done}/{total} ({percent}%)");
    }
}

fn should_log_progress(log_level: TransferLogLevel, done: usize, total: usize) -> bool {
    match log_level {
        TransferLogLevel::Quiet => false,
        TransferLogLevel::Verbose => true,
        TransferLogLevel::Normal => {
            if total <= 64 || done == 1 || done == total {
                return true;
            }

            let previous_percent = done.saturating_sub(1).saturating_mul(100) / total;
            let current_percent = done.saturating_mul(100) / total;

            current_percent / 10 != previous_percent / 10
        }
    }
}

fn progress_percent(done: usize, total: usize) -> usize {
    if total == 0 {
        100
    } else {
        done.saturating_mul(100) / total
    }
}
