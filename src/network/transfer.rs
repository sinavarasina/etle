use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    io::{BufWriter, ErrorKind, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
        storage::{EncryptedChunk, EncryptedFile, decrypt_to_file},
    },
    network::{
        NetworkError, accept_peer, client_protocol_handshake, client_shared_secret_exchange,
        connect_peer, server_protocol_handshake, server_shared_secret_exchange,
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
    pub request_window: usize,
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
            request_window: DEFAULT_REQUEST_WINDOW,
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

    #[must_use]
    pub const fn with_request_window(mut self, request_window: usize) -> Self {
        self.request_window = request_window;
        self
    }
}


const STAGING_DIR_NAME: &str = "staging";
const DEFAULT_REQUEST_WINDOW: usize = 16;
const PROGRESS_FLUSH_CHUNK_INTERVAL: usize = 32;
const PROGRESS_FLUSH_TIME_INTERVAL: Duration = Duration::from_millis(750);
const RECONSTRUCT_WRITER_BUFFER_SIZE: usize = 8 * 1024 * 1024;

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

    let chunk_metas = encrypt_file_to_staging_parallel(
        input_path, file_id, &file_key, chunk_size, &staging, log_level,
    )?;

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

struct TemporaryLibraryRoot {
    path: PathBuf,
}

impl TemporaryLibraryRoot {
    fn create() -> Result<Self, std::io::Error> {
        let base = std::env::temp_dir().join("etle-legacy-serve");
        fs::create_dir_all(&base)?;

        for attempt in 0_u32.. {
            let candidate = base.join(format!(
                "{}-{}-{attempt}",
                std::process::id(),
                staging_timestamp()
            ));

            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        unreachable!("unbounded temporary library root loop must return before overflowing")
    }
}

impl Drop for TemporaryLibraryRoot {
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

fn encrypt_file_to_staging_parallel(
    input_path: &Path,
    file_id: FileId,
    file_key: &SymmetricKey,
    chunk_size: usize,
    staging: &StagedChunkDir,
    log_level: TransferLogLevel,
) -> Result<Vec<ChunkMeta>, NetworkError> {
    let worker_count = default_seed_worker_count();
    let total_size = fs::metadata(input_path)?.len();
    let total_chunks = total_chunks_for_size(total_size, chunk_size);

    if log_level.is_normal() {
        println!("[daemon] parallel seed encryption workers: {worker_count}");
    }

    let (plain_tx, plain_rx) = mpsc::sync_channel::<SeedPlainChunk>(worker_count * 2);
    let plain_rx = Arc::new(Mutex::new(plain_rx));
    let (encrypted_tx, encrypted_rx) =
        mpsc::sync_channel::<Result<SeedEncryptedChunk, NetworkError>>(worker_count * 2);
    let file_key = *file_key;
    let progress_context = file_id.to_string();

    thread::scope(|scope| {
        let staging_ref = staging;
        let progress_context = progress_context.clone();
        let writer = scope.spawn(move || -> Result<Vec<ChunkMeta>, NetworkError> {
            let mut chunk_metas = Vec::new();

            while let Ok(result) = encrypted_rx.recv() {
                let encrypted = result?;
                staging_ref.write_chunk(encrypted.meta.index, &encrypted.ciphertext)?;

                log_chunk_progress_with_context(
                    &progress_context,
                    "daemon",
                    "staged+encrypted",
                    log_level,
                    chunk_metas.len() + 1,
                    total_chunks,
                    encrypted.meta.index,
                    encrypted.meta.plain_size as usize,
                );

                if log_level.is_verbose() {
                    println!(
                        "[daemon] encrypted staged chunk {} (plain={} bytes, encrypted={} bytes)",
                        encrypted.meta.index,
                        encrypted.meta.plain_size,
                        encrypted.meta.encrypted_size
                    );
                }

                chunk_metas.push(encrypted.meta);
            }

            chunk_metas.sort_by_key(|meta| meta.index);
            Ok(chunk_metas)
        });

        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let plain_rx = Arc::clone(&plain_rx);
            let encrypted_tx = encrypted_tx.clone();

            workers.push(scope.spawn(move || {
                loop {
                    let chunk = match plain_rx
                        .expect_lock("parallel seed queue mutex poisoned")
                        .recv()
                    {
                        Ok(chunk) => chunk,
                        Err(_) => break,
                    };

                    let result = encrypt_seed_chunk(file_id, &file_key, chunk);
                    if encrypted_tx.send(result).is_err() {
                        break;
                    }
                }
            }));
        }

        drop(encrypted_tx);

        let mut input = File::open(input_path)?;
        let mut index = 0_u32;
        loop {
            let mut buffer = vec![0_u8; chunk_size];
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            buffer.truncate(read);
            plain_tx
                .send(SeedPlainChunk {
                    index,
                    data: buffer,
                })
                .map_err(|_| {
                    NetworkError::PeerError("parallel seed worker stopped unexpectedly".to_string())
                })?;
            index = index.saturating_add(1);
        }

        drop(plain_tx);

        for worker in workers {
            worker.join().map_err(|_| {
                NetworkError::PeerError("parallel seed worker panicked".to_string())
            })?;
        }

        writer
            .join()
            .map_err(|_| NetworkError::PeerError("parallel seed writer panicked".to_string()))?
    })
}

fn encrypt_seed_chunk(
    file_id: FileId,
    file_key: &SymmetricKey,
    chunk: SeedPlainChunk,
) -> Result<SeedEncryptedChunk, NetworkError> {
    let nonce = generate_nonce();
    let sample_len = chunk.data.len();
    let aad = build_chunk_aad(file_id, chunk.index, sample_len as u64);
    let ciphertext = encrypt_chunk(file_key, nonce, &chunk.data, &aad)?;
    let encrypted_hash = hash_chunk(&ciphertext);
    let encrypted_size = ciphertext.len() as u64;

    Ok(SeedEncryptedChunk {
        meta: ChunkMeta {
            index: chunk.index,
            plain_size: sample_len as u64,
            encrypted_size,
            nonce,
            blake3_hash: encrypted_hash,
        },
        ciphertext,
    })
}

fn default_seed_worker_count() -> usize {
    thread::available_parallelism()
        .map_or(2, usize::from)
        .min(4)
        .max(1)
}

struct SeedPlainChunk {
    index: u32,
    data: Vec<u8>,
}

struct SeedEncryptedChunk {
    meta: ChunkMeta,
    ciphertext: Vec<u8>,
}

fn total_chunks_for_size(total_size: u64, chunk_size: usize) -> usize {
    if total_size == 0 || chunk_size == 0 {
        return 0;
    }

    let chunk_size = chunk_size as u64;
    (total_size.saturating_add(chunk_size - 1) / chunk_size) as usize
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
    let input_path = input_path.as_ref().to_path_buf();
    let (library_root, _temporary_root) = match library_root {
        Some(root) => (root, None),
        None => {
            let temporary_root = TemporaryLibraryRoot::create()?;
            (temporary_root.path.clone(), Some(temporary_root))
        }
    };

    if log_level.is_normal() {
        println!(
            "[seeder] legacy one-peer serve now stages via library root: {}",
            library_root.display()
        );
    }

    let descriptor = add_file_to_library(&input_path, chunk_size, &library_root, log_level)?;
    let options =
        ServeFileOptions::new(seeder_id, log_level).with_library_root(library_root.clone());

    serve_library_share_to_one_peer_from_listener(
        &listener,
        &library_root,
        descriptor.share_id,
        options,
    )
    .await
    .map(|_| ())
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
        let (stream, peer_addr) = accept_peer(&listener).await?;
        let peer_library_root = library_root.clone();
        let peer_options = options.clone();
        let log_level = options.log_level;

        tokio::spawn(async move {
            match serve_library_share_connected_peer(
                stream,
                peer_addr,
                peer_library_root,
                share_id,
                peer_options,
            )
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
        let (stream, peer_addr) = accept_peer(&listener).await?;
        let peer_library_root = library_root.clone();
        let peer_options = options.clone();
        let log_level = options.log_level;

        tokio::spawn(async move {
            match serve_library_connected_peer(stream, peer_addr, peer_library_root, peer_options)
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
            println!("[seeder] spawned peer session for {peer_addr}; listener remains available");
        }
    }
}

async fn serve_library_connected_peer(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    library_root: PathBuf,
    options: ServeFileOptions,
) -> Result<EtleDescriptor, NetworkError> {
    let ServeFileOptions {
        seeder_id,
        log_level,
        library_root: _,
    } = options;
    let library_root = library_root.as_path();

    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
    }

    let remote_peer_id = server_protocol_handshake(&mut stream, seeder_id).await?;
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
    let progress_context = descriptor.share_id.to_string();
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
                log_chunk_progress_with_context(
                    &progress_context,
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
    let (stream, peer_addr) = accept_peer(listener).await?;
    serve_library_share_connected_peer(
        stream,
        peer_addr,
        library_root.as_ref().to_path_buf(),
        share_id,
        options,
    )
    .await
}

async fn serve_library_share_connected_peer(
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
    } = options;
    let paths = LibraryPaths::for_share(&library_root, share_id);
    let descriptor = read_descriptor(&paths)?;
    let secret = read_secret(&paths)?;
    let manifest = manifest_from_descriptor(&descriptor)?;
    let total_chunks = descriptor.chunks.len();
    let available_chunks = available_chunk_indexes(&paths, &descriptor)?;
    let available_set = available_chunks.iter().copied().collect::<BTreeSet<_>>();

    if log_level.is_normal() {
        println!("[seeder] peer connected: {peer_addr}");
        println!(
            "[seeder] loading share state: name=\"{}\", share_id={}",
            descriptor.name, descriptor.share_id
        );
    }

    let remote_peer_id = server_protocol_handshake(&mut stream, seeder_id).await?;
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
    let progress_context = descriptor.share_id.to_string();
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
                log_chunk_progress_with_context(
                    &progress_context,
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

pub async fn serve_library_to_one_peer_from_listener(
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

    let remote_peer_id = server_protocol_handshake(&mut stream, seeder_id).await?;
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
    let progress_context = descriptor.share_id.to_string();
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
                log_chunk_progress_with_context(
                    &progress_context,
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

    for peer_addr in peer_addrs.iter().copied().take(worker_limit) {
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
        decrypt_library_chunks_to_file(
            &active_state.paths,
            &manifest,
            &file_key,
            output_path,
            options.log_level,
        )?;
        mark_download_library_complete(&mut library_state, output_state_dir)?;
        return Ok(manifest);
    }

    let total_chunks = manifest.chunks.len();
    let progress_context = descriptor.share_id.to_string();
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
        let progress_context = progress_context.clone();

        handles.push(tokio::spawn(async move {
            parallel_download_worker(
                progress_context,
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
            if options.log_level.is_normal() {
                if !worker_errors.is_empty() {
                    println!(
                        "[peer] parallel worker errors: {}",
                        worker_errors.join("; ")
                    );
                }
                println!(
                    "[peer] retrying missing chunks with sequential peer fallback; first missing index={}",
                    meta.index
                );
            }

            return download_file_from_peers_with_options(
                peer_addrs,
                output_path,
                options.with_resume(true),
            )
            .await;
        }
    }

    if options.log_level.is_normal() {
        println!("[peer] decrypting and reconstructing file...");
    }

    {
        let mut state = state.lock().expect("parallel state mutex poisoned");
        let active_state = active_download_state(&state)?;
        decrypt_library_chunks_to_file(
            &active_state.paths,
            &manifest,
            &file_key,
            output_path,
            options.log_level,
        )?;
        mark_download_library_complete(&mut state, output_state_dir)?;
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
        request_window,
    } = options;
    let output_path = output_path.as_ref();
    let mut stream = connect_peer(peer_addr).await?;

    if log_level.is_normal() {
        println!("[peer] connected to {peer_addr}");
    }

    let remote_peer_id = client_protocol_handshake(&mut stream, peer_id).await?;
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

    let progress_context = descriptor.share_id.to_string();

    download_missing_chunks_windowed(
        &progress_context,
        &mut stream,
        &manifest,
        &peer_available,
        &mut library_state,
        &mut chunks,
        &mut completed_chunks,
        streaming_to_library,
        output_state_dir.clone(),
        log_level,
        request_window,
    )
    .await?;

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
        decrypt_library_chunks_to_file(
            &active_state.paths,
            &manifest,
            &file_key,
            output_path,
            log_level,
        )?;
    } else {
        let encrypted = EncryptedFile {
            manifest: manifest.clone(),
            chunks,
        };

        decrypt_to_file(&encrypted, &file_key, output_path)?;
    }
    mark_download_library_complete(&mut library_state, output_state_dir)?;

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

    let remote_peer_id = client_protocol_handshake(&mut stream, peer_id).await?;
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
    progress_context: String,
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

                log_chunk_progress_with_context(
                    &progress_context,
                    "peer",
                    "parallel received+verified",
                    log_level,
                    done,
                    manifest.chunks.len(),
                    index,
                    chunk_len,
                );
            }
            Result::Err(error) => {
                queue
                    .expect_lock("parallel queue mutex poisoned")
                    .push_back(index);

                if log_level.is_normal() {
                    println!(
                        "[peer] worker {} released chunk {} for another peer after error: {}",
                        peer.peer_addr, index, error
                    );
                }

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

async fn download_missing_chunks_windowed(
    progress_context: &str,
    stream: &mut TcpStream,
    manifest: &Manifest,
    peer_available: &BTreeSet<u32>,
    library_state: &mut Option<ActiveDownloadLibraryState>,
    chunks: &mut BTreeMap<u32, EncryptedChunk>,
    completed_chunks: &mut BTreeSet<u32>,
    streaming_to_library: bool,
    output_state_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
    request_window: usize,
) -> Result<(), NetworkError> {
    let request_window = request_window.max(1);
    let total_chunks = manifest.chunks.len();
    let mut next_meta = 0_usize;
    let mut in_flight = BTreeMap::<u32, ChunkMeta>::new();

    for meta in &manifest.chunks {
        if completed_chunks.contains(&meta.index) {
            log_chunk_progress_with_context(
                progress_context,
                "peer",
                "reused",
                log_level,
                completed_chunks.len(),
                total_chunks,
                meta.index,
                meta.encrypted_size as usize,
            );
        }
    }

    loop {
        while in_flight.len() < request_window && next_meta < manifest.chunks.len() {
            let meta = &manifest.chunks[next_meta];
            next_meta += 1;

            if completed_chunks.contains(&meta.index) {
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

            send_message(stream, &WireMessage::RequestChunk { index: meta.index }).await?;
            in_flight.insert(meta.index, meta.clone());
        }

        if in_flight.is_empty() {
            break;
        }

        match receive_message(stream).await? {
            WireMessage::Chunk { index, data } => {
                let expected = in_flight.keys().next().copied().unwrap_or(index);
                let meta = in_flight
                    .remove(&index)
                    .ok_or(NetworkError::UnexpectedChunkIndex {
                        expected,
                        actual: index,
                    })?;

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
                    library_state,
                    &encrypted_chunk,
                    output_state_dir.clone(),
                )?;

                completed_chunks.insert(index);
                if !streaming_to_library {
                    chunks.insert(index, encrypted_chunk);
                }

                log_chunk_progress_with_context(
                    progress_context,
                    "peer",
                    "received+verified",
                    log_level,
                    completed_chunks.len(),
                    total_chunks,
                    index,
                    chunk_len,
                );
            }
            WireMessage::Error { message } => return Err(NetworkError::PeerError(message)),
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Chunk",
                    actual,
                });
            }
        }
    }

    Ok(())
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

struct ActiveDownloadLibraryState {
    paths: LibraryPaths,
    progress: DownloadProgress,
    dirty_chunks: usize,
    last_progress_flush: Instant,
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
        return Ok(Some(ActiveDownloadLibraryState {
            paths,
            progress,
            dirty_chunks: 0,
            last_progress_flush: Instant::now(),
        }));
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

    Ok(Some(ActiveDownloadLibraryState {
        paths,
        progress,
        dirty_chunks: 0,
        last_progress_flush: Instant::now(),
    }))
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
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    let worker_count = default_decrypt_worker_count(manifest.chunks.len());

    if worker_count <= 1 {
        return decrypt_library_chunks_to_file_sequential(
            paths,
            manifest,
            file_key,
            output_path,
            log_level,
        );
    }

    let _ = decrypt_library_chunks_to_file_parallel(
        paths,
        manifest,
        file_key,
        output_path,
        worker_count,
        log_level,
    );
    Ok(())
}

fn decrypt_library_chunks_to_file_sequential(
    paths: &LibraryPaths,
    manifest: &Manifest,
    file_key: &SymmetricKey,
    output_path: &Path,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    prepare_output_file_parent(output_path)?;

    let output = File::create(output_path)?;
    let mut output = BufWriter::with_capacity(RECONSTRUCT_WRITER_BUFFER_SIZE, output);
    let mut final_hasher = blake3::Hasher::new();
    let total_chunks = manifest.chunks.len();
    let progress_context = manifest.file_id.to_string();

    for (offset, meta) in manifest.chunks.iter().enumerate() {
        let decrypted = decrypt_library_chunk(paths, manifest.file_id, file_key, meta)?;
        let decrypted_len = decrypted.data.len();
        final_hasher.update(&decrypted.data);
        output.write_all(&decrypted.data)?;
        log_chunk_progress_with_context(
            &progress_context,
            "peer",
            "decrypted+written",
            log_level,
            offset + 1,
            total_chunks,
            meta.index,
            decrypted_len,
        );
    }

    finalize_streamed_output(output, final_hasher, manifest.file_id)
}

fn decrypt_library_chunks_to_file_parallel(
    paths: &LibraryPaths,
    manifest: &Manifest,
    file_key: &SymmetricKey,
    output_path: &Path,
    worker_count: usize,
    log_level: TransferLogLevel,
) -> Result<(), NetworkError> {
    prepare_output_file_parent(output_path)?;

    let output = File::create(output_path)?;
    let mut output = BufWriter::with_capacity(RECONSTRUCT_WRITER_BUFFER_SIZE, output);
    let mut final_hasher = blake3::Hasher::new();
    let queue = Arc::new(Mutex::new(VecDeque::from(manifest.chunks.clone())));
    let (tx, rx) = mpsc::sync_channel::<Result<DecryptedChunk, NetworkError>>(worker_count * 2);
    let paths = paths.clone();
    let file_id = manifest.file_id;
    let file_key = *file_key;
    let total_chunks = manifest.chunks.len();
    let progress_context = manifest.file_id.to_string();

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
                        let bytes = data.len();
                        final_hasher.update(&data);
                        output.write_all(&data)?;
                        let done = (next_index as usize).saturating_add(1);
                        log_chunk_progress_with_context(
                            &progress_context,
                            "peer",
                            "decrypted+written",
                            log_level,
                            done,
                            total_chunks,
                            next_index,
                            bytes,
                        );
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
    mut output: impl Write,
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
    state.dirty_chunks = state.dirty_chunks.saturating_add(1);

    if should_flush_download_progress(state) {
        flush_download_progress(state, ShareMode::Downloading, output_dir)?;
    }

    Ok(())
}

fn should_flush_download_progress(state: &ActiveDownloadLibraryState) -> bool {
    state.dirty_chunks >= PROGRESS_FLUSH_CHUNK_INTERVAL
        || state.last_progress_flush.elapsed() >= PROGRESS_FLUSH_TIME_INTERVAL
}

fn flush_download_progress(
    state: &mut ActiveDownloadLibraryState,
    mode: ShareMode,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    write_progress(&state.paths, &state.progress)?;
    write_state(
        &state.paths,
        &ShareState::from_progress(mode, output_dir, &state.progress),
    )?;

    state.dirty_chunks = 0;
    state.last_progress_flush = Instant::now();

    Ok(())
}

fn mark_download_library_complete(
    state: &mut Option<ActiveDownloadLibraryState>,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    let Some(state) = state else {
        return Ok(());
    };

    if state.dirty_chunks > 0 {
        flush_download_progress(state, ShareMode::Downloading, output_dir.clone())?;
    }

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

pub fn log_chunk_progress(
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    log_chunk_progress_with_context("global", role, action, log_level, done, total, index, bytes);
}

fn log_chunk_progress_with_context(
    context: &str,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    if matches!(log_level, TransferLogLevel::Quiet) {
        return;
    }

    let key = ProgressKey::new(context, role, action, total);
    let mut states = progress_states()
        .lock()
        .expect("transfer progress state mutex poisoned");
    let now = Instant::now();
    let state = states
        .entry(key.clone())
        .or_insert_with(|| ProgressState::new(now));

    if done > state.last_done {
        state.bytes_done = state.bytes_done.saturating_add(bytes as u64);
        state.last_done = done;
    }

    if !should_log_progress(log_level, done, total, now.duration_since(state.last_log)) {
        return;
    }

    let line = format_progress_line(role, action, done, total, index, bytes, state, now);
    state.last_log = now;

    if done >= total {
        states.remove(&key);
    }

    println!("{line}");
}

fn progress_states() -> &'static Mutex<BTreeMap<ProgressKey, ProgressState>> {
    static STATES: OnceLock<Mutex<BTreeMap<ProgressKey, ProgressState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ProgressKey {
    context: String,
    role: String,
    action: String,
    total: usize,
}

impl ProgressKey {
    fn new(context: &str, role: &str, action: &str, total: usize) -> Self {
        Self {
            context: context.to_string(),
            role: role.to_string(),
            action: action.to_string(),
            total,
        }
    }
}

struct ProgressState {
    start: Instant,
    last_log: Instant,
    last_done: usize,
    bytes_done: u64,
}

impl ProgressState {
    fn new(now: Instant) -> Self {
        Self {
            start: now,
            last_log: now.checked_sub(Duration::from_secs(1)).unwrap_or(now),
            last_done: 0,
            bytes_done: 0,
        }
    }
}

fn should_log_progress(
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    since_last_log: Duration,
) -> bool {
    match log_level {
        TransferLogLevel::Quiet => false,
        TransferLogLevel::Verbose => true,
        TransferLogLevel::Normal => {
            done == 1 || done >= total || since_last_log >= Duration::from_millis(750)
        }
    }
}

fn format_progress_line(
    role: &str,
    action: &str,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
    state: &ProgressState,
    now: Instant,
) -> String {
    let total_bytes = estimated_total_bytes(state.bytes_done, done, total);
    let percent = progress_percent_bytes(state.bytes_done, total_bytes);
    let elapsed = now.duration_since(state.start);
    let average_rate = bytes_per_second(state.bytes_done, elapsed);
    let eta = estimate_eta(state.bytes_done, total_bytes, average_rate);

    format!(
        "[{role}] {action} chunk {done}/{total} ({percent:.2}%) | \
        index={index} | chunk={} | progress={}/{} | avg={} | eta={}",
        format_bytes(bytes as u64),
        format_bytes(state.bytes_done),
        format_bytes(total_bytes),
        format_rate(average_rate),
        format_duration(eta),
    )
}

fn estimated_total_bytes(done_bytes: u64, done: usize, total: usize) -> u64 {
    if total == 0 {
        return 0;
    }

    if done >= total {
        return done_bytes;
    }

    if done == 0 || done_bytes == 0 {
        return 0;
    }

    let average_chunk = (done_bytes / done as u64).max(1);
    average_chunk.saturating_mul(total as u64)
}

fn progress_percent_bytes(done_bytes: u64, total_bytes: u64) -> f64 {
    if total_bytes == 0 {
        return 100.0;
    }

    (done_bytes as f64 * 100.0 / total_bytes as f64).min(100.0)
}

fn bytes_per_second(done_bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= f64::EPSILON {
        return 0.0;
    }

    done_bytes as f64 / secs
}

fn estimate_eta(done_bytes: u64, total_bytes: u64, average_rate: f64) -> Option<Duration> {
    if done_bytes >= total_bytes || average_rate <= f64::EPSILON {
        return None;
    }

    let remaining = total_bytes.saturating_sub(done_bytes) as f64;
    Some(Duration::from_secs_f64(remaining / average_rate))
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];

    for candidate in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = candidate;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {unit}")
    }
}

fn format_rate(bytes_per_second: f64) -> String {
    if bytes_per_second <= f64::EPSILON {
        return "0 B/s".to_string();
    }

    format!("{}/s", format_bytes(bytes_per_second as u64))
}

fn format_duration(duration: Option<Duration>) -> String {
    let Some(duration) = duration else {
        return "--".to_string();
    };

    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds}s")
    }
}
