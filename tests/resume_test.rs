use std::{fs, path::PathBuf};

use etle::{
    crypto::hash::hash_file,
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        download_file_from_peer_with_options, serve_file_to_one_peer_with_options,
        serve_library_share_to_one_peer,
    },
    state::{
        DownloadProgress, LibraryPaths, ShareMode, ShareState, list_library_shares, read_progress,
        write_progress, write_state,
    },
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn download_can_resume_from_existing_verified_chunks() {
    let input = temp_path("resume-input.bin");
    let first_output = temp_path("resume-first-output.bin");
    let resumed_output = temp_path("resume-output.bin");
    let seeder_root = temp_path("resume-seeder-root");
    let peer_root = temp_path("resume-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&first_output);
    let _ = fs::remove_file(&resumed_output);
    let _ = fs::remove_dir_all(&seeder_root);
    let _ = fs::remove_dir_all(&peer_root);

    fs::write(
        &input,
        b"resume should reuse verified encrypted chunks and fetch only missing pieces",
    )
    .unwrap();

    let original_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let original_addr = original_listener.local_addr().unwrap();
    let server_input = input.clone();
    let server_root = seeder_root.clone();

    let original_server = tokio::spawn(async move {
        serve_file_to_one_peer_with_options(
            original_listener,
            server_input,
            8,
            ServeFileOptions::new("original-seeder", TransferLogLevel::Quiet)
                .with_library_root(server_root),
        )
        .await
        .unwrap();
    });

    let manifest = download_file_from_peer_with_options(
        original_addr,
        &first_output,
        DownloadFileOptions::new("first-peer", TransferLogLevel::Quiet)
            .with_library_root(peer_root.clone()),
    )
    .await
    .unwrap();
    original_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&first_output).unwrap()
    );
    assert!(manifest.chunks.len() > 1);

    let shares = list_library_shares(&peer_root).unwrap();
    assert_eq!(shares.len(), 1);
    let share_id = shares[0].descriptor.share_id;
    let peer_paths = LibraryPaths::for_share(&peer_root, share_id);

    let first_chunk = manifest.chunks[0].index;
    let partial_progress = DownloadProgress::new(share_id, vec![first_chunk]);
    write_progress(&peer_paths, &partial_progress).unwrap();
    write_state(
        &peer_paths,
        &ShareState::from_progress(ShareMode::Downloading, None, &partial_progress),
    )
    .unwrap();

    for meta in manifest.chunks.iter().skip(1) {
        fs::remove_file(peer_paths.chunk_path(meta.index)).unwrap();
    }

    let state_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let state_addr = state_listener.local_addr().unwrap();
    let seeder_root_task = seeder_root.clone();

    let state_server = tokio::spawn(async move {
        serve_library_share_to_one_peer(
            state_listener,
            seeder_root_task,
            share_id,
            ServeFileOptions::new("state-seeder", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    download_file_from_peer_with_options(
        state_addr,
        &resumed_output,
        DownloadFileOptions::new("resume-peer", TransferLogLevel::Quiet)
            .with_library_root(peer_root.clone())
            .with_resume(true),
    )
    .await
    .unwrap();
    state_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&resumed_output).unwrap()
    );
    assert_eq!(
        read_progress(&peer_paths).unwrap().completed_chunks.len(),
        manifest.chunks.len()
    );

    fs::remove_file(input).unwrap();
    fs::remove_file(first_output).unwrap();
    fs::remove_file(resumed_output).unwrap();
    fs::remove_dir_all(seeder_root).unwrap();
    fs::remove_dir_all(peer_root).unwrap();
}
