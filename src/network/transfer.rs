use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    io::{ErrorKind, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::net::{TcpListener, TcpStream};

use crate::{
    crypto::{
        aead::{SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk, generate_nonce},
        hash::{FileId, hash_chunk, hash_file},
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
    pub requested_share_id: Option<ShareId>,
}

impl DownloadFileOptions {
    #[must_use]
    pub fn new(peer_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            peer_id: peer_id.into(),
            log_level,
            library_root: None,
            resume: true,
            requested_share_id: None,
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

    #[must_use]
    pub const fn with_requested_share_id(mut self, share_id: Option<ShareId>) -> Self {
        self.requested_share_id = share_id;
        self
    }
}

const STAGING_DIR_NAME: &str = "staging";

pub fn add_file_to_library(
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    library_root: impl AsRef<Path>,
    log_level: TransferLogLevel,
) -> Result<EtleDescriptor, NetworkError> {
    let input_path = input_path.as_ref();

    if log_level.is_normal() {
        println!("[daemon] adding file to library: {}", input_path.display());
        println!("[daemon] chunk size: {chunk_size} bytes");
    }

    let file_key = generate_file_key();
    add_file_to_library_streaming(
        input_path,
        file_key,
        chunk_size,
        library_root.as_ref(),
        log_level,
    )
}

fn add_file_to_library_streaming(
    input_path: &Path,
    file_key: SymmetricKey,
    chunk_size: usize,
    library_root: &Path,
    log_level: TransferLogLevel,
) -> Result<EtleDescriptor, NetworkError> {
    if chunk_size == 0 {
        return Err(FileError::InvalidChunkSize(chunk_size).into());
    }

    let file_id = hash_file(input_path)?;
    let file_size = fs::metadata(input_path)?.len();
    let file_name = manifest_file_name(input_path);
    let staging = StagedChunkDir::create(library_root)?;
    let mut input = File::open(input_path)?;
    let mut buffer = vec![0_u8; chunk_size];
    let mut chunk_metas = Vec::new();
    let mut index = 0_u32;

    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        let nonce = generate_nonce();
        let aad = build_chunk_aad(file_id, index, read as u64);
        let ciphertext = encrypt_chunk(&file_key, nonce, &buffer[..read], &aad)?;
        let encrypted_hash = hash_chunk(&ciphertext);
        let encrypted_size = ciphertext.len() as u64;

        staging.write_chunk(index, &ciphertext)?;
        chunk_metas.push(ChunkMeta {
            index,
            plain_size: read as u64,
            encrypted_size,
            nonce,
            blake3_hash: encrypted_hash,
        });

        if log_level.is_verbose() {
            println!(
                "[daemon] encrypted staged chunk {} (plain={} bytes, encrypted={} bytes)",
                index, read, encrypted_size
            );
        }

        index = index.saturating_add(1);
    }

    let manifest = Manifest {
        file_id,
        file_name,
        file_size,
        chunk_size: chunk_size as u64,
        chunks: chunk_metas,
    };
    let descriptor = descriptor_from_manifest(&manifest);
    let paths = initialize_share_library(
        library_root,
        &descriptor,
        file_key,
        ShareMode::Seeding,
        None,
    )?;

    for meta in &manifest.chunks {
        staging.move_chunk(meta.index, &paths.chunk_path(meta.index))?;
    }

    if log_level.is_normal() {
        println!(
            "[daemon] seed state stored: {}",
            paths.share_dir().display()
        );
    }

    Ok(descriptor)
}

struct StagedChunkDir {
    path: PathBuf,
}

impl StagedChunkDir {
    fn create(library_root: &Path) -> Result<Self, std::io::Error> {
        let base = library_root
            .join(crate::state::ETLE_DIR_NAME)
            .join(STAGING_DIR_NAME);
        fs::create_dir_all(&base)?;

        for attempt in 0_u32.. {
            let candidate = base.join(format!(
                "seed-{}-{}-{attempt}",
                std::process::id(),
                staging_timestamp()
            ));

            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        unreachable!("unbounded staging directory loop must return before overflowing")
    }

    fn chunk_path(&self, index: u32) -> PathBuf {
        self.path
            .join(format!("{index:06}.{}", crate::state::CHUNK_EXTENSION))
    }

    fn write_chunk(&self, index: u32, data: &[u8]) -> Result<(), std::io::Error> {
        fs::write(self.chunk_path(index), data)
    }

    fn move_chunk(&self, index: u32, destination: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        if destination.exists() {
            fs::remove_file(destination)?;
        }

        fs::rename(self.chunk_path(index), destination)
    }
}

impl Drop for StagedChunkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn staging_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn manifest_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unnamed".to_string())
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

    let available_chunks = encrypted.chunks.keys().copied().collect::<Vec<_>>();
    send_message(
        &mut stream,
        &WireMessage::Have {
            chunks: available_chunks,
        },
    )
    .await?;

    if log_level.is_normal() {
        println!("[seeder] advertised {total_chunks}/{total_chunks} available chunks");
    }

    let mut served_or_known = BTreeSet::new();

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
    serve_library_share_to_one_peer_from_listener(&listener, library_root, share_id, options).await
}

pub async fn serve_library_share_forever(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    share_id: ShareId,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        match serve_library_share_to_one_peer_from_listener(
            &listener,
            &library_root,
            share_id,
            options.clone(),
        )
        .await
        {
            Ok(descriptor) => {
                if options.log_level.is_normal() {
                    println!(
                        "[seeder] ready for next peer: share=\"{}\", share_id={}",
                        descriptor.name, descriptor.share_id
                    );
                }
            }
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[seeder] peer session failed: {error}");
                    println!("[seeder] keeping listener alive for the next peer");
                }
            }
        }
    }
}

pub async fn serve_library_to_one_peer(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    serve_library_to_one_peer_from_listener(&listener, library_root, options).await
}

pub async fn serve_library_forever(
    listener: TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<(), NetworkError> {
    let library_root = library_root.as_ref().to_path_buf();

    loop {
        match serve_library_to_one_peer_from_listener(&listener, &library_root, options.clone())
            .await
        {
            Ok(descriptor) => {
                if options.log_level.is_normal() {
                    println!(
                        "[seeder] ready for next peer request: share=\"{}\", share_id={}",
                        descriptor.name, descriptor.share_id
                    );
                }
            }
            Err(error) => {
                if options.log_level.is_normal() {
                    println!("[seeder] peer session failed: {error}");
                    println!("[seeder] keeping multi-share listener alive");
                }
            }
        }
    }
}

async fn serve_library_to_one_peer_from_listener(
    listener: &TcpListener,
    library_root: impl AsRef<Path>,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
    } = options;
    let library_root = library_root.as_ref();

    let (mut stream, peer_addr) = accept_peer(listener).await?;
    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
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

    let share_id = match receive_message(&mut stream).await? {
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
    let manifest = manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();
    let available_chunks = available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

    if log_level.is_normal() {
        println!(
            "[seeder] peer requested share: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    send_message(
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

    send_message(
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
                    send_message(
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
                    "served-from-library",
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

async fn serve_library_share_to_one_peer_from_listener(
    listener: &TcpListener,
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
    let available_chunks = available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

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

    send_message(
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
                    send_message(
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
        match connect_download_peer(
            peer_addr,
            options.peer_id.clone(),
            options.log_level,
            options.requested_share_id,
        )
        .await
        {
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

    if library_state.is_none() {
        return Err(NetworkError::PeerError(
            "parallel streaming download requires a library root".to_string(),
        ));
    }

    let completed_chunks = load_resumable_chunk_indexes(
        &mut library_state,
        &manifest,
        output_state_dir.clone(),
        options.log_level,
    )?;

    let missing_chunks = manifest
        .chunks
        .iter()
        .filter(|meta| !completed_chunks.contains(&meta.index))
        .map(|meta| meta.index)
        .collect::<VecDeque<_>>();

    if missing_chunks.is_empty() {
        if options.log_level.is_normal() {
            println!("[peer] all chunks already available in local state");
            println!("[peer] decrypting and reconstructing file...");
        }

        let active_state = active_download_state(&library_state)?;
        decrypt_library_chunks_to_file(&active_state.paths, &manifest, &file_key, output_path)?;
        mark_download_library_complete(&library_state, output_state_dir)?;
        return Ok(manifest);
    }

    let total_chunks = manifest.chunks.len();
    let queue = Arc::new(Mutex::new(missing_chunks));
    let completed_chunks = Arc::new(Mutex::new(completed_chunks));
    let state = Arc::new(Mutex::new(library_state));
    let manifest = Arc::new(manifest);

    let mut handles = Vec::with_capacity(connected_peers.len());
    for peer in connected_peers {
        let queue = Arc::clone(&queue);
        let completed_chunks = Arc::clone(&completed_chunks);
        let state = Arc::clone(&state);
        let manifest = Arc::clone(&manifest);
        let output_state_dir = output_state_dir.clone();
        let log_level = options.log_level;

        handles.push(tokio::spawn(async move {
            parallel_download_worker(
                peer,
                manifest,
                queue,
                completed_chunks,
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

    let completed_chunks = match Arc::try_unwrap(completed_chunks) {
        Ok(completed_chunks) => completed_chunks
            .into_inner()
            .expect("parallel chunk mutex poisoned"),
        Err(completed_chunks) => completed_chunks
            .lock()
            .expect("parallel chunk mutex poisoned")
            .clone(),
    };

    if completed_chunks.len() != total_chunks {
        if let Some(meta) = manifest
            .chunks
            .iter()
            .find(|meta| !completed_chunks.contains(&meta.index))
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

    {
        let state = state.lock().expect("parallel state mutex poisoned");
        let active_state = active_download_state(&state)?;
        decrypt_library_chunks_to_file(&active_state.paths, &manifest, &file_key, output_path)?;
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
        requested_share_id,
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

    send_requested_share_if_needed(&mut stream, requested_share_id, log_level).await?;

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

    let peer_available = receive_peer_availability(&mut stream, log_level, peer_addr).await?;
    if log_level.is_normal() {
        println!(
            "[peer] peer availability: {}/{} chunks",
            peer_available.len(),
            total_chunks
        );
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

    let streaming_to_library = library_state.is_some();
    let mut chunks = BTreeMap::new();
    let mut completed_chunks = if streaming_to_library {
        load_resumable_chunk_indexes(
            &mut library_state,
            &manifest,
            output_state_dir.clone(),
            log_level,
        )?
    } else {
        chunks = load_resumable_chunks(
            &mut library_state,
            &manifest,
            output_state_dir.clone(),
            log_level,
        )?;
        chunks.keys().copied().collect::<BTreeSet<_>>()
    };

    if !completed_chunks.is_empty() {
        let have_chunks = completed_chunks.iter().copied().collect::<Vec<_>>();
        send_message(
            &mut stream,
            &WireMessage::Have {
                chunks: have_chunks,
            },
        )
        .await?;
    }

    for meta in &manifest.chunks {
        if completed_chunks.contains(&meta.index) {
            log_chunk_progress(
                "peer",
                "reused",
                log_level,
                completed_chunks.len(),
                total_chunks,
                meta.index,
                meta.encrypted_size as usize,
            );
            continue;
        }

        if !peer_available.contains(&meta.index) {
            if log_level.is_verbose() {
                println!(
                    "[peer] skipping chunk {}: unavailable on this peer",
                    meta.index
                );
            }
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

                completed_chunks.insert(index);
                if !streaming_to_library {
                    chunks.insert(index, encrypted_chunk);
                }
                log_chunk_progress(
                    "peer",
                    "received+verified",
                    log_level,
                    completed_chunks.len(),
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

    if completed_chunks.len() != total_chunks {
        if let Some(meta) = manifest
            .chunks
            .iter()
            .find(|meta| !completed_chunks.contains(&meta.index))
        {
            return Err(NetworkError::MissingEncryptedChunk(meta.index));
        }
    }

    if log_level.is_normal() {
        println!("[peer] decrypting and reconstructing file...");
    }

    if streaming_to_library {
        let active_state = active_download_state(&library_state)?;
        decrypt_library_chunks_to_file(&active_state.paths, &manifest, &file_key, output_path)?;
    } else {
        let encrypted = EncryptedFile {
            manifest: manifest.clone(),
            chunks,
        };

        decrypt_to_file(&encrypted, &file_key, output_path)?;
    }
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
    available_chunks: BTreeSet<u32>,
}

async fn send_requested_share_if_needed(
    stream: &mut TcpStream,
    share_id: Option<ShareId>,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    let Some(share_id) = share_id else {
        return Ok(());
    };

    send_message(stream, &WireMessage::RequestShare { share_id }).await?;

    if log_level.is_normal() {
        println!("[peer] requested share_id: {share_id}");
    }

    Ok(())
}

async fn connect_download_peer(
    peer_addr: SocketAddr,
    peer_id: String,
    log_level: TransferLogLevel,
    requested_share_id: Option<ShareId>,
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

    send_requested_share_if_needed(&mut stream, requested_share_id, log_level).await?;

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

    let available_chunks = receive_peer_availability(&mut stream, log_level, peer_addr).await?;
    if log_level.is_normal() {
        println!(
            "[peer] peer {peer_addr} has {}/{} chunks",
            available_chunks.len(),
            manifest.chunks.len()
        );
    }

    Ok(ConnectedDownloadPeer {
        peer_addr,
        stream,
        manifest,
        file_key,
        available_chunks,
    })
}

async fn parallel_download_worker(
    mut peer: ConnectedDownloadPeer,
    manifest: Arc<Manifest>,
    queue: Arc<Mutex<VecDeque<u32>>>,
    chunks: Arc<Mutex<BTreeSet<u32>>>,
    state: Arc<Mutex<Option<ActiveDownloadLibraryState>>>,
    output_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    let initial_have = {
        let chunks = chunks.expect_lock("parallel chunk mutex poisoned");
        chunks.iter().copied().collect::<Vec<_>>()
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
        let Some(index) = pop_next_available_missing_chunk(&queue, &chunks, &peer.available_chunks)
        else {
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
                    chunks.insert(index);
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
        chunks.iter().copied().collect::<Vec<_>>()
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
        WireMessage::Error { message } => Err(NetworkError::PeerError(message)),
        actual => Err(NetworkError::UnexpectedMessage {
            expected: "Chunk",
            actual,
        }),
    }
}

fn pop_next_available_missing_chunk(
    queue: &Mutex<VecDeque<u32>>,
    chunks: &Mutex<BTreeSet<u32>>,
    available_chunks: &BTreeSet<u32>,
) -> Option<u32> {
    let mut queue = queue.expect_lock("parallel queue mutex poisoned");
    let attempts = queue.len();

    for _ in 0..attempts {
        let index = queue.pop_front()?;

        if chunks
            .expect_lock("parallel chunk mutex poisoned")
            .contains(&index)
        {
            continue;
        }

        if available_chunks.contains(&index) {
            return Some(index);
        }

        queue.push_back(index);
    }

    None
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

async fn receive_peer_availability(
    stream: &mut TcpStream,
    log_level: TransferLogLevel,
    peer_addr: SocketAddr,
) -> Result<BTreeSet<u32>, NetworkError> {
    match receive_message(stream).await? {
        WireMessage::Have { chunks } => Ok(chunks.into_iter().collect()),
        WireMessage::Error { message } => Err(NetworkError::PeerError(message)),
        actual => {
            if log_level.is_normal() {
                println!("[peer] {peer_addr} did not advertise chunk availability");
            }

            Err(NetworkError::UnexpectedMessage {
                expected: "Have",
                actual,
            })
        }
    }
}

fn available_chunk_indexes(
    paths: &LibraryPaths,
    descriptor: &EtleDescriptor,
) -> Result<Vec<u32>, NetworkError> {
    let mut available = Vec::new();

    for meta in &descriptor.chunks {
        if !has_encrypted_chunk(paths, meta.index) {
            continue;
        }

        let Ok(chunk) = read_encrypted_chunk(paths, meta.index, meta.encrypted_size) else {
            continue;
        };

        if hash_chunk(&chunk.data) == meta.blake3_hash {
            available.push(meta.index);
        }
    }

    Ok(available)
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

fn load_resumable_chunk_indexes(
    state: &mut Option<ActiveDownloadLibraryState>,
    manifest: &Manifest,
    output_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
) -> Result<BTreeSet<u32>, NetworkError> {
    let Some(state) = state else {
        return Ok(BTreeSet::new());
    };

    let mut completed = BTreeSet::new();
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
        completed.insert(meta.index);
    }

    state.progress = valid_progress;
    write_progress(&state.paths, &state.progress)?;
    write_state(
        &state.paths,
        &ShareState::from_progress(ShareMode::Downloading, output_dir, &state.progress),
    )?;

    if log_level.is_normal() && !completed.is_empty() {
        println!(
            "[peer] reused {}/{} chunks from local state",
            completed.len(),
            manifest.chunks.len()
        );
    }

    Ok(completed)
}

fn active_download_state(
    state: &Option<ActiveDownloadLibraryState>,
) -> Result<&ActiveDownloadLibraryState, NetworkError> {
    state.as_ref().ok_or_else(|| {
        NetworkError::PeerError("download output reconstruction requires library state".to_string())
    })
}

fn decrypt_library_chunks_to_file(
    paths: &LibraryPaths,
    manifest: &Manifest,
    file_key: &SymmetricKey,
    output_path: &Path,
) -> Result<(), NetworkError> {
    let worker_count = default_decrypt_worker_count(manifest.chunks.len());

    if worker_count <= 1 {
        return decrypt_library_chunks_to_file_sequential(paths, manifest, file_key, output_path);
    }

    decrypt_library_chunks_to_file_parallel(paths, manifest, file_key, output_path, worker_count)
}

fn decrypt_library_chunks_to_file_sequential(
    paths: &LibraryPaths,
    manifest: &Manifest,
    file_key: &SymmetricKey,
    output_path: &Path,
) -> Result<(), NetworkError> {
    prepare_output_file_parent(output_path)?;

    let mut output = File::create(output_path)?;
    let mut final_hasher = blake3::Hasher::new();

    for meta in &manifest.chunks {
        let decrypted = decrypt_library_chunk(paths, manifest.file_id, file_key, meta)?;
        final_hasher.update(&decrypted.data);
        output.write_all(&decrypted.data)?;
    }

    finalize_streamed_output(output, final_hasher, manifest.file_id)
}

fn decrypt_library_chunks_to_file_parallel(
    paths: &LibraryPaths,
    manifest: &Manifest,
    file_key: &SymmetricKey,
    output_path: &Path,
    worker_count: usize,
) -> Result<(), NetworkError> {
    prepare_output_file_parent(output_path)?;

    let mut output = File::create(output_path)?;
    let mut final_hasher = blake3::Hasher::new();
    let queue = Arc::new(Mutex::new(VecDeque::from(manifest.chunks.clone())));
    let (tx, rx) = mpsc::sync_channel::<Result<DecryptedChunk, NetworkError>>(worker_count * 2);
    let paths = paths.clone();
    let file_id = manifest.file_id;
    let file_key = *file_key;
    let total_chunks = manifest.chunks.len();

    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let paths = paths.clone();

            handles.push(scope.spawn(move || {
                loop {
                    let Some(meta) = queue
                        .expect_lock("parallel decrypt queue mutex poisoned")
                        .pop_front()
                    else {
                        break;
                    };

                    let result = decrypt_library_chunk(&paths, file_id, &file_key, &meta);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            }));
        }

        drop(tx);

        let mut next_index = 0_u32;
        let mut pending = BTreeMap::<u32, Vec<u8>>::new();
        let mut first_error = None;

        for _ in 0..total_chunks {
            match rx.recv() {
                Ok(Ok(decrypted)) if first_error.is_none() => {
                    pending.insert(decrypted.index, decrypted.data);

                    while let Some(data) = pending.remove(&next_index) {
                        final_hasher.update(&data);
                        output.write_all(&data)?;
                        next_index = next_index.saturating_add(1);
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(_) => {
                    if first_error.is_none() {
                        first_error = Some(NetworkError::PeerError(
                            "parallel decrypt worker stopped unexpectedly".to_string(),
                        ));
                    }
                    break;
                }
            }
        }

        for handle in handles {
            handle.join().map_err(|_| {
                NetworkError::PeerError("parallel decrypt worker panicked".to_string())
            })?;
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        if !pending.is_empty() {
            return Err(NetworkError::PeerError(
                "parallel decrypt finished with unwritten chunks".to_string(),
            ));
        }

        finalize_streamed_output(output, final_hasher, manifest.file_id)
    })
}

fn decrypt_library_chunk(
    paths: &LibraryPaths,
    file_id: FileId,
    file_key: &SymmetricKey,
    meta: &ChunkMeta,
) -> Result<DecryptedChunk, NetworkError> {
    let chunk = read_encrypted_chunk(paths, meta.index, meta.encrypted_size)?;
    let actual_hash = hash_chunk(&chunk.data);
    if actual_hash != meta.blake3_hash {
        return Err(FileError::ChunkHashMismatch(meta.index).into());
    }

    let aad = build_chunk_aad(file_id, meta.index, meta.plain_size);
    let data = decrypt_chunk(file_key, meta.nonce, &chunk.data, &aad)?;

    Ok(DecryptedChunk {
        index: meta.index,
        data,
    })
}

fn prepare_output_file_parent(output_path: &Path) -> Result<(), NetworkError> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    Ok(())
}

fn finalize_streamed_output(
    mut output: File,
    final_hasher: blake3::Hasher,
    expected_file_id: FileId,
) -> Result<(), NetworkError> {
    output.flush()?;

    let final_hash = FileId(*final_hasher.finalize().as_bytes());
    if final_hash != expected_file_id {
        return Err(FileError::FinalHashMismatch.into());
    }

    Ok(())
}

fn default_decrypt_worker_count(total_chunks: usize) -> usize {
    if total_chunks <= 1 {
        return 1;
    }

    thread::available_parallelism()
        .map_or(2, usize::from)
        .min(4)
        .min(total_chunks)
}

struct DecryptedChunk {
    index: u32,
    data: Vec<u8>,
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
