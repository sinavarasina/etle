use super::prelude::*;

const STAGING_DIR_NAME: &str = "staging";

pub fn add(
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
    add_streaming(
        input_path,
        file_key,
        chunk_size,
        library_root.as_ref(),
        log_level,
    )
}

fn add_streaming(
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
    let descriptor = super::download::descriptor_from_manifest(&manifest);
    let paths = library::init(
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
            .join(crate::state::paths::ETLE_DIR_NAME)
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
        self.path.join(format!(
            "{index:06}.{}",
            crate::state::model::CHUNK_EXTENSION
        ))
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

pub(super) struct TemporaryLibraryRoot {
    pub(super) path: PathBuf,
}

impl TemporaryLibraryRoot {
    pub(super) fn create() -> Result<Self, std::io::Error> {
        let base = std::env::temp_dir().join("etle-transient-serve");
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

pub(super) fn staging_timestamp() -> u128 {
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

                super::progress::with_label(
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
            index = index.checked_add(1).ok_or(FileError::TooManyChunks)?;
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
        .clamp(1, 4)
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
    usize::try_from(total_size.div_ceil(chunk_size)).unwrap_or(usize::MAX)
}
