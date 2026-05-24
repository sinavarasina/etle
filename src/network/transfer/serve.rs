use super::prelude::*;

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
    let options =
        ServeFileOptions::new(seeder_id, log_level).with_library_root(library_root.clone());

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
        let (stream, peer_addr) = accept_peer(&listener).await?;
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
        let (stream, peer_addr) = accept_peer(&listener).await?;
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
                super::progress::with_context(
                    Some(share_id),
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

    let remote_peer_id = server_protocol_handshake(&mut stream, seeder_id).await?;
    if log_level.is_normal() {
        println!("[seeder] hello handshake completed with {remote_peer_id}");
    }

    let shared_secret = server_shared_secret_exchange(&mut stream).await?;
    let session_key = derive_session_key(shared_secret);
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
                super::progress::with_context(
                    Some(share_id),
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
                super::progress::with_context(
                    Some(share_id),
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
