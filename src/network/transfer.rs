use std::{collections::BTreeMap, net::SocketAddr, path::Path};

use tokio::net::TcpListener;

use crate::{
    crypto::{
        hash::hash_chunk,
        key_exchange::derive_session_key,
        key_wrap::{WrappedFileKey, generate_file_key, unwrap_file_key, wrap_file_key},
    },
    file::{
        error::FileError,
        manifest::Manifest,
        storage::{EncryptedChunk, EncryptedFile, decrypt_to_file, encrypt_file},
    },
    network::{
        NetworkError, accept_peer, client_hello_handshake, client_shared_secret_exchange,
        connect_peer, server_hello_handshake, server_shared_secret_exchange,
    },
    protocol::{WireMessage, receive_message, send_message},
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
}

impl ServeFileOptions {
    #[must_use]
    pub fn new(seeder_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            seeder_id: seeder_id.into(),
            log_level,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadFileOptions {
    pub peer_id: String,
    pub log_level: TransferLogLevel,
}

impl DownloadFileOptions {
    #[must_use]
    pub fn new(peer_id: impl Into<String>, log_level: TransferLogLevel) -> Self {
        Self {
            peer_id: peer_id.into(),
            log_level,
        }
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

    let mut served = std::collections::BTreeSet::new();

    while served.len() < total_chunks {
        match receive_message(&mut stream).await? {
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

                served.insert(index);
                let served_count = served.len();
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
                    expected: "RequestChunk",
                    actual,
                });
            }
        }
    }

    Ok(())
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

pub async fn download_file_from_peer_with_options(
    peer_addr: SocketAddr,
    output_path: impl AsRef<Path>,
    options: DownloadFileOptions,
) -> Result<Manifest, NetworkError> {
    let DownloadFileOptions { peer_id, log_level } = options;
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

    let mut chunks = BTreeMap::new();

    for (position, meta) in manifest.chunks.iter().enumerate() {
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

                chunks.insert(index, EncryptedChunk { index, data });
                log_chunk_progress(
                    "peer",
                    "received+verified",
                    log_level,
                    position + 1,
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

    if log_level.is_normal() {
        println!("[peer] final hash verified: {}", manifest.file_id);
        println!("[peer] output written: {}", output_path.display());
    }

    Ok(manifest)
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
