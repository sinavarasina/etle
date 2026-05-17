use std::{fs, path::Path};

use etle::{
    crypto::hash::hash_file,
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        download_file_from_peer_with_options, download_file_from_peers_parallel_with_options,
        serve_file_to_one_peer_with_options, serve_library_share_to_one_peer,
    },
    state::{LibraryPaths, list_library_shares, read_progress},
};

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

fn copy_dir_all(from: impl AsRef<Path>, to: impl AsRef<Path>) {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::create_dir_all(to).unwrap();

    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        let target = to.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(entry.path(), target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn parallel_download_fetches_chunks_from_multiple_seeders() {
    let input = temp_path("parallel-input.bin");
    let bootstrap_output = temp_path("parallel-bootstrap-output.bin");
    let final_output = temp_path("parallel-output.bin");
    let seeder_a_root = temp_path("parallel-seeder-a-root");
    let seeder_b_root = temp_path("parallel-seeder-b-root");
    let bootstrap_peer_root = temp_path("parallel-bootstrap-peer-root");
    let final_peer_root = temp_path("parallel-final-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&bootstrap_output);
    let _ = fs::remove_file(&final_output);
    let _ = fs::remove_dir_all(&seeder_a_root);
    let _ = fs::remove_dir_all(&seeder_b_root);
    let _ = fs::remove_dir_all(&bootstrap_peer_root);
    let _ = fs::remove_dir_all(&final_peer_root);

    fs::write(
        &input,
        b"parallel swarm download should request different encrypted chunks from multiple seeders",
    )
    .unwrap();

    let bootstrap_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let bootstrap_addr = bootstrap_listener.local_addr().unwrap();
    let bootstrap_input = input.clone();
    let bootstrap_seeder_root = seeder_a_root.clone();

    let bootstrap_server = tokio::spawn(async move {
        serve_file_to_one_peer_with_options(
            bootstrap_listener,
            bootstrap_input,
            8,
            ServeFileOptions::new("bootstrap-seeder", TransferLogLevel::Quiet)
                .with_library_root(bootstrap_seeder_root),
        )
        .await
        .unwrap();
    });

    let bootstrap_manifest = download_file_from_peer_with_options(
        bootstrap_addr,
        &bootstrap_output,
        DownloadFileOptions::new("bootstrap-peer", TransferLogLevel::Quiet)
            .with_library_root(bootstrap_peer_root.clone()),
    )
    .await
    .unwrap();
    bootstrap_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&bootstrap_output).unwrap()
    );
    assert!(bootstrap_manifest.chunks.len() > 2);

    let shares = list_library_shares(&seeder_a_root).unwrap();
    assert_eq!(shares.len(), 1);
    let share_id = shares[0].descriptor.share_id;

    copy_dir_all(&seeder_a_root, &seeder_b_root);

    // Simulate partial seeders: each peer has only part of the encrypted
    // chunk set, but their availability union is complete.
    let seeder_a_paths = LibraryPaths::for_share(&seeder_a_root, share_id);
    let seeder_b_paths = LibraryPaths::for_share(&seeder_b_root, share_id);
    for meta in &bootstrap_manifest.chunks {
        if meta.index % 2 == 0 {
            fs::remove_file(seeder_b_paths.chunk_path(meta.index)).unwrap();
        } else {
            fs::remove_file(seeder_a_paths.chunk_path(meta.index)).unwrap();
        }
    }

    let seeder_a_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let seeder_a_addr = seeder_a_listener.local_addr().unwrap();
    let seeder_a_root_task = seeder_a_root.clone();

    let seeder_a = tokio::spawn(async move {
        serve_library_share_to_one_peer(
            seeder_a_listener,
            seeder_a_root_task,
            share_id,
            ServeFileOptions::new("seeder-a", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    let seeder_b_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let seeder_b_addr = seeder_b_listener.local_addr().unwrap();
    let seeder_b_root_task = seeder_b_root.clone();

    let seeder_b = tokio::spawn(async move {
        serve_library_share_to_one_peer(
            seeder_b_listener,
            seeder_b_root_task,
            share_id,
            ServeFileOptions::new("seeder-b", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    let final_manifest = download_file_from_peers_parallel_with_options(
        vec![seeder_a_addr, seeder_b_addr],
        &final_output,
        DownloadFileOptions::new("parallel-peer", TransferLogLevel::Quiet)
            .with_library_root(final_peer_root.clone()),
        2,
    )
    .await
    .unwrap();

    seeder_a.await.unwrap();
    seeder_b.await.unwrap();

    assert_eq!(final_manifest.file_id, bootstrap_manifest.file_id);
    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&final_output).unwrap()
    );

    let final_paths = LibraryPaths::for_share(&final_peer_root, share_id);
    assert_eq!(
        read_progress(&final_paths).unwrap().completed_chunks.len(),
        bootstrap_manifest.chunks.len()
    );

    fs::remove_file(input).unwrap();
    fs::remove_file(bootstrap_output).unwrap();
    fs::remove_file(final_output).unwrap();
    fs::remove_dir_all(seeder_a_root).unwrap();
    fs::remove_dir_all(seeder_b_root).unwrap();
    fs::remove_dir_all(bootstrap_peer_root).unwrap();
    fs::remove_dir_all(final_peer_root).unwrap();
}
