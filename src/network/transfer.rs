use std::{collections::BTreeMap, net::SocketAddr, path::Path};

use tokio::net::TcpListener;

use crate::{
    crypto::{hash::hash_chunk, key_exchange::derive_file_key},
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

pub async fn serve_file_to_one_peer(
    listener: TcpListener,
    input_path: impl AsRef<Path>,
    chunk_size: usize,
    seeder_id: impl Into<String>,
) -> Result<(), NetworkError> {
    let (mut stream, _) = accept_peer(&listener).await?;

    server_hello_handshake(&mut stream, seeder_id).await?;
    let shared_secret = server_shared_secret_exchange(&mut stream).await?;

    // Encrypt after key exchange so every peer can receive chunks protected by
    // the session-derived file key.
    let file_id = crate::crypto::hash::hash_file(input_path.as_ref())?;
    let file_key = derive_file_key(shared_secret, file_id);
    let encrypted = encrypt_file(input_path, &file_key, chunk_size)?;

    send_message(
        &mut stream,
        &WireMessage::Manifest {
            manifest: encrypted.manifest.clone(),
        },
    )
    .await?;

    let mut served = std::collections::BTreeSet::new();
    let total_chunks = encrypted.manifest.chunks.len();

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
    let mut stream = connect_peer(peer_addr).await?;

    client_hello_handshake(&mut stream, peer_id).await?;
    let shared_secret = client_shared_secret_exchange(&mut stream).await?;

    let manifest = match receive_message(&mut stream).await? {
        WireMessage::Manifest { manifest } => manifest,
        actual => {
            return Err(NetworkError::UnexpectedMessage {
                expected: "Manifest",
                actual,
            });
        }
    };

    let file_key = derive_file_key(shared_secret, manifest.file_id);
    let mut chunks = BTreeMap::new();

    for meta in &manifest.chunks {
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

                chunks.insert(index, EncryptedChunk { index, data });
            }
            actual => {
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Chunk",
                    actual,
                });
            }
        }
    }

    let encrypted = EncryptedFile {
        manifest: manifest.clone(),
        chunks,
    };

    decrypt_to_file(&encrypted, &file_key, output_path)?;

    Ok(manifest)
}
