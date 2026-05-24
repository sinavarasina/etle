use super::prelude::*;

pub async fn from_peer(
    peer_addr: SocketAddr,
    output_path: impl AsRef<Path>,
    peer_id: impl Into<String>,
) -> Result<Manifest, NetworkError> {
    from_peer_with(
        peer_addr,
        output_path,
        DownloadFileOptions::new(peer_id, TransferLogLevel::Quiet),
    )
    .await
}

pub async fn from_peers(
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

        match from_peer_with(peer_addr, output_path, attempt_options).await {
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

pub async fn from_peers_parallel(
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
        return from_peers(peer_addrs, output_path, options.with_resume(true)).await;
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

    if connected_peers.len() == 1 {
        let peer_addr = connected_peers[0].peer_addr;
        if options.log_level.is_normal() {
            println!(
                "[peer] only one compatible peer prepared; using sequential windowed download from {peer_addr}"
            );
        }

        drop(connected_peers);
        return from_peers([peer_addr], output_path, options.with_resume(true)).await;
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
    let queue = Arc::new(Mutex::new(missing_chunks));
    let completed_chunks = Arc::new(Mutex::new(completed_chunks));
    let state = Arc::new(Mutex::new(library_state));
    let share_id = descriptor.share_id;
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
                share_id,
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

            return from_peers(peer_addrs, output_path, options.with_resume(true)).await;
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

pub async fn from_peer_with(
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

    let manifest = match receive(&mut stream).await? {
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
    let wrapped_file_key = match receive(&mut stream).await? {
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
        send(
            &mut stream,
            &WireMessage::Have {
                chunks: have_chunks,
            },
        )
        .await?;
    }

    download_missing_chunks_windowed(
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
        descriptor.share_id,
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

    send(stream, &WireMessage::RequestShare { share_id }).await?;

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

    let manifest = match receive(&mut stream).await? {
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
    let wrapped_file_key = match receive(&mut stream).await? {
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

#[allow(clippy::too_many_arguments)]
async fn parallel_download_worker(
    mut peer: ConnectedDownloadPeer,
    manifest: Arc<Manifest>,
    queue: Arc<Mutex<VecDeque<u32>>>,
    chunks: Arc<Mutex<BTreeSet<u32>>>,
    state: Arc<Mutex<Option<ActiveDownloadLibraryState>>>,
    output_dir: Option<PathBuf>,
    log_level: TransferLogLevel,
    share_id: ShareId,
) -> Result<(), NetworkError> {
    let initial_have = {
        let chunks = chunks.expect_lock("parallel chunk mutex poisoned");
        chunks.iter().copied().collect::<Vec<_>>()
    };

    if !initial_have.is_empty() {
        send(
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

        match request_chunk_from_peer_to_library(&mut peer.stream, meta, &state, output_dir.clone())
            .await
        {
            Ok(chunk_len) => {
                let done = {
                    let mut chunks = chunks.expect_lock("parallel chunk mutex poisoned");
                    chunks.insert(index);
                    chunks.len()
                };

                super::progress::for_share(
                    share_id,
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
        let _ = send(&mut peer.stream, &WireMessage::Have { chunks: final_have }).await;
    }

    if log_level.is_normal() {
        println!("[peer] parallel worker finished: {}", peer.peer_addr);
    }

    Ok(())
}

async fn request_chunk_from_peer_to_library(
    stream: &mut TcpStream,
    meta: &ChunkMeta,
    state: &Arc<Mutex<Option<ActiveDownloadLibraryState>>>,
    output_dir: Option<PathBuf>,
) -> Result<usize, NetworkError> {
    send(stream, &WireMessage::RequestChunk { index: meta.index }).await?;

    let temp_path = {
        let state = state.expect_lock("parallel state mutex poisoned");
        download_chunk_temp_path(&state, meta.index)?
    };

    match receive_chunk_to_file(stream, &temp_path).await? {
        ReceivedChunkFrame::RawChunkFile(chunk) => {
            if chunk.index != meta.index {
                remove_file_if_exists(&temp_path);
                return Err(NetworkError::UnexpectedChunkIndex {
                    expected: meta.index,
                    actual: chunk.index,
                });
            }

            if chunk.blake3_hash != *meta.blake3_hash.as_bytes() {
                remove_file_if_exists(&temp_path);
                return Err(FileError::ChunkHashMismatch(meta.index).into());
            }

            let mut state = state.expect_lock("parallel state mutex poisoned");
            persist_downloaded_chunk_file(&mut state, meta.index, &temp_path, output_dir)?;
            Ok(chunk.data_len)
        }
        ReceivedChunkFrame::Message(WireMessage::Chunk { index, data }) => {
            remove_file_if_exists(&temp_path);
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
            let encrypted_chunk = EncryptedChunk { index, data };
            let mut state = state.expect_lock("parallel state mutex poisoned");
            persist_downloaded_chunk(&mut state, &encrypted_chunk, output_dir)?;
            Ok(chunk_len)
        }
        ReceivedChunkFrame::Message(WireMessage::Error { message }) => {
            remove_file_if_exists(&temp_path);
            Err(NetworkError::PeerError(message))
        }
        ReceivedChunkFrame::Message(actual) => {
            remove_file_if_exists(&temp_path);
            Err(NetworkError::UnexpectedMessage {
                expected: "Chunk",
                actual,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn download_missing_chunks_windowed(
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
    share_id: ShareId,
) -> Result<(), NetworkError> {
    let max_request_window = request_window.max(1);
    let mut active_request_window = initial_request_window(max_request_window);
    let mut chunks_since_window_growth = 0_usize;
    let total_chunks = manifest.chunks.len();
    let mut next_meta = 0_usize;
    let mut in_flight = BTreeMap::<u32, ChunkMeta>::new();

    if log_level.is_normal() && max_request_window > 1 {
        println!(
            "[peer] adaptive request window enabled: start={}, max={}",
            active_request_window, max_request_window
        );
    }

    for meta in &manifest.chunks {
        if completed_chunks.contains(&meta.index) {
            super::progress::for_share(
                share_id,
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
        while in_flight.len() < active_request_window && next_meta < manifest.chunks.len() {
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

            send(stream, &WireMessage::RequestChunk { index: meta.index }).await?;
            in_flight.insert(meta.index, meta.clone());
        }

        if in_flight.is_empty() {
            break;
        }

        let received = if streaming_to_library {
            let temp_path = download_chunk_temp_path(library_state, 0)?;
            receive_chunk_to_file(stream, &temp_path)
                .await
                .map(|frame| (frame, Some(temp_path)))?
        } else {
            (ReceivedChunkFrame::Message(receive(stream).await?), None)
        };

        match received {
            (ReceivedChunkFrame::RawChunkFile(chunk), Some(temp_path)) => {
                let index = chunk.index;
                let expected = in_flight.keys().next().copied().unwrap_or(index);
                let meta = in_flight
                    .remove(&index)
                    .ok_or(NetworkError::UnexpectedChunkIndex {
                        expected,
                        actual: index,
                    })?;

                if chunk.blake3_hash != *meta.blake3_hash.as_bytes() {
                    remove_file_if_exists(&temp_path);
                    return Err(FileError::ChunkHashMismatch(meta.index).into());
                }

                let chunk_len = chunk.data_len;
                persist_downloaded_chunk_file(
                    library_state,
                    index,
                    &temp_path,
                    output_state_dir.clone(),
                )?;

                completed_chunks.insert(index);

                super::progress::for_share(
                    share_id,
                    "peer",
                    "received+verified",
                    log_level,
                    completed_chunks.len(),
                    total_chunks,
                    index,
                    chunk_len,
                );

                chunks_since_window_growth = chunks_since_window_growth.saturating_add(1);
                maybe_grow_request_window(
                    &mut active_request_window,
                    &mut chunks_since_window_growth,
                    max_request_window,
                    log_level,
                );
            }
            (ReceivedChunkFrame::Message(WireMessage::Chunk { index, data }), temp_path) => {
                if let Some(temp_path) = temp_path {
                    remove_file_if_exists(&temp_path);
                }

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

                super::progress::for_share(
                    share_id,
                    "peer",
                    "received+verified",
                    log_level,
                    completed_chunks.len(),
                    total_chunks,
                    index,
                    chunk_len,
                );

                chunks_since_window_growth = chunks_since_window_growth.saturating_add(1);
                maybe_grow_request_window(
                    &mut active_request_window,
                    &mut chunks_since_window_growth,
                    max_request_window,
                    log_level,
                );
            }
            (ReceivedChunkFrame::Message(WireMessage::Error { message }), temp_path) => {
                if let Some(temp_path) = temp_path {
                    remove_file_if_exists(&temp_path);
                }
                return Err(NetworkError::PeerError(message));
            }
            (ReceivedChunkFrame::Message(actual), temp_path) => {
                if let Some(temp_path) = temp_path {
                    remove_file_if_exists(&temp_path);
                }
                return Err(NetworkError::UnexpectedMessage {
                    expected: "Chunk",
                    actual,
                });
            }
            (ReceivedChunkFrame::RawChunkFile(_), None) => {
                unreachable!("raw chunk file requires a temporary path")
            }
        }
    }

    Ok(())
}

fn initial_request_window(max_request_window: usize) -> usize {
    max_request_window.clamp(1, 4)
}

fn maybe_grow_request_window(
    active_request_window: &mut usize,
    chunks_since_window_growth: &mut usize,
    max_request_window: usize,
    log_level: TransferLogLevel,
) {
    if *active_request_window >= max_request_window {
        return;
    }

    if *chunks_since_window_growth < *active_request_window {
        return;
    }

    *chunks_since_window_growth = 0;
    *active_request_window = (*active_request_window * 2).min(max_request_window);

    if log_level.is_verbose() {
        println!("[peer] adaptive request window increased to {active_request_window}");
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

async fn receive_peer_availability(
    stream: &mut TcpStream,
    log_level: TransferLogLevel,
    peer_addr: SocketAddr,
) -> Result<BTreeSet<u32>, NetworkError> {
    match receive(stream).await? {
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

pub(super) fn available_chunk_indexes(
    paths: &LibraryPaths,
    descriptor: &EtleDescriptor,
) -> Result<Vec<u32>, NetworkError> {
    let mut available = Vec::new();

    for meta in &descriptor.chunks {
        if !has_chunk(paths, meta.index) {
            continue;
        }

        let Ok(chunk) = read_chunk(paths, meta.index, meta.encrypted_size) else {
            continue;
        };

        if hash_chunk(&chunk.data) == meta.blake3_hash {
            available.push(meta.index);
        }
    }

    Ok(available)
}

pub(super) fn descriptor_from_manifest(manifest: &Manifest) -> EtleDescriptor {
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

pub(super) fn manifest_from_descriptor(
    descriptor: &EtleDescriptor,
) -> Result<Manifest, NetworkError> {
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

    let paths = library::init(
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
        if !state.progress.has_chunk(meta.index) || !has_chunk(&state.paths, meta.index) {
            continue;
        }

        let chunk = read_chunk(&state.paths, meta.index, meta.encrypted_size)?;
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
        if !state.progress.has_chunk(meta.index) || !has_chunk(&state.paths, meta.index) {
            continue;
        }

        let chunk = read_chunk(&state.paths, meta.index, meta.encrypted_size)?;
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
        super::progress::with_label(
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
                        super::progress::with_label(
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
    let chunk = read_chunk(paths, meta.index, meta.encrypted_size)?;
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
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
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

fn download_chunk_temp_path(
    state: &Option<ActiveDownloadLibraryState>,
    index: u32,
) -> Result<PathBuf, NetworkError> {
    let state = active_download_state(state)?;
    let suffix = super::seed::staging_timestamp();
    Ok(state.paths.chunks_dir().join(format!(
        "{index:06}.{}.{}.part",
        crate::state::model::CHUNK_EXTENSION,
        suffix
    )))
}

fn persist_downloaded_chunk_file(
    state: &mut Option<ActiveDownloadLibraryState>,
    index: u32,
    temp_path: &Path,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    let Some(state) = state else {
        remove_file_if_exists(temp_path);
        return Ok(());
    };

    fs::create_dir_all(state.paths.chunks_dir())?;
    let final_path = state.paths.chunk_path(index);
    if final_path.exists() {
        fs::remove_file(&final_path)?;
    }
    fs::rename(temp_path, final_path)?;

    state.progress.mark_completed(index);
    state.dirty_chunks = state.dirty_chunks.saturating_add(1);
    maybe_flush_download_progress(state, output_dir)?;

    Ok(())
}

fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn persist_downloaded_chunk(
    state: &mut Option<ActiveDownloadLibraryState>,
    chunk: &EncryptedChunk,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
    let Some(state) = state else {
        return Ok(());
    };

    write_chunk(&state.paths, chunk)?;
    state.progress.mark_completed(chunk.index);
    state.dirty_chunks = state.dirty_chunks.saturating_add(1);

    if should_flush_download_progress(state) {
        flush_download_progress(state, ShareMode::Downloading, output_dir)?;
    }

    Ok(())
}

fn maybe_flush_download_progress(
    state: &mut ActiveDownloadLibraryState,
    output_dir: Option<PathBuf>,
) -> Result<(), NetworkError> {
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

pub(super) fn is_peer_closed_protocol_error(error: &ProtocolError) -> bool {
    matches!(
        error,
        ProtocolError::Io(io_error)
            if matches!(
                io_error.kind(),
                ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            )
    )
}
