mod common;

use common::{print_banner, print_kv, print_step};
use std::{fs, path::Path};

use etle::{
    crypto::hash::hash_file,
    network::{
        tcp::bind_listener,
        transfer::{
            download,
            options::{DownloadFileOptions, ServeFileOptions, TransferLogLevel},
            serve,
        },
    },
    state::{library, paths, storage},
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
async fn multi_peer_download_falls_back_and_reuses_persisted_chunks() {
    print_banner("multi_peer_download_falls_back_and_reuses_persisted_chunks");
    print_step(1, "execute scenario");
    print_kv(
        "test",
        "multi_peer_download_falls_back_and_reuses_persisted_chunks",
    );
    let input = temp_path("multi-peer-input.bin");
    let bootstrap_output = temp_path("multi-peer-bootstrap-output.bin");
    let final_output = temp_path("multi-peer-output.bin");
    let seeder_root = temp_path("multi-peer-seeder-root");
    let partial_root = temp_path("multi-peer-partial-root");
    let bootstrap_peer_root = temp_path("multi-peer-bootstrap-peer-root");
    let final_peer_root = temp_path("multi-peer-final-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&bootstrap_output);
    let _ = fs::remove_file(&final_output);
    let _ = fs::remove_dir_all(&seeder_root);
    let _ = fs::remove_dir_all(&partial_root);
    let _ = fs::remove_dir_all(&bootstrap_peer_root);
    let _ = fs::remove_dir_all(&final_peer_root);

    fs::write(
        &input,
        b"multi peer fallback should keep chunks from a partial peer and finish from another peer",
    )
    .unwrap();

    let bootstrap_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let bootstrap_addr = bootstrap_listener.local_addr().unwrap();
    let bootstrap_input = input.clone();
    let bootstrap_seeder_root = seeder_root.clone();

    let bootstrap_server = tokio::spawn(async move {
        serve::file_once_with(
            bootstrap_listener,
            bootstrap_input,
            8,
            ServeFileOptions::new("bootstrap-seeder", TransferLogLevel::Quiet)
                .with_library_root(bootstrap_seeder_root),
        )
        .await
        .unwrap();
    });

    let bootstrap_manifest = download::from_peer_with(
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

    let shares = library::list(&seeder_root).unwrap();
    assert_eq!(shares.len(), 1);
    let share_id = shares[0].descriptor.share_id;

    copy_dir_all(&seeder_root, &partial_root);
    let partial_paths = paths::LibraryPaths::for_share(&partial_root, share_id);
    for meta in bootstrap_manifest.chunks.iter().skip(1) {
        fs::remove_file(partial_paths.chunk_path(meta.index)).unwrap();
    }

    let partial_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let partial_addr = partial_listener.local_addr().unwrap();
    let partial_root_task = partial_root.clone();

    let partial_server = tokio::spawn(async move {
        let _ = serve::share_once(
            partial_listener,
            partial_root_task,
            share_id,
            ServeFileOptions::new("partial-seeder", TransferLogLevel::Quiet),
        )
        .await;
    });

    let full_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let full_addr = full_listener.local_addr().unwrap();
    let seeder_root_task = seeder_root.clone();

    let full_server = tokio::spawn(async move {
        serve::share_once(
            full_listener,
            seeder_root_task,
            share_id,
            ServeFileOptions::new("full-seeder", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    let final_manifest = download::from_peers(
        vec![partial_addr, full_addr],
        &final_output,
        DownloadFileOptions::new("fallback-peer", TransferLogLevel::Quiet)
            .with_library_root(final_peer_root.clone()),
    )
    .await
    .unwrap();

    partial_server.await.unwrap();
    full_server.await.unwrap();

    assert_eq!(final_manifest.file_id, bootstrap_manifest.file_id);
    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&final_output).unwrap()
    );

    let final_paths = paths::LibraryPaths::for_share(&final_peer_root, share_id);
    assert_eq!(
        storage::read_progress(&final_paths)
            .unwrap()
            .completed_chunks
            .len(),
        bootstrap_manifest.chunks.len()
    );

    fs::remove_file(input).unwrap();
    fs::remove_file(bootstrap_output).unwrap();
    fs::remove_file(final_output).unwrap();
    fs::remove_dir_all(seeder_root).unwrap();
    fs::remove_dir_all(partial_root).unwrap();
    fs::remove_dir_all(bootstrap_peer_root).unwrap();
    fs::remove_dir_all(final_peer_root).unwrap();
}
