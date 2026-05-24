use std::{fs, path::PathBuf};

use etle::{
    crypto::hash::hash_file,
    file::descriptor::{EtleDescriptor, FileEntry},
    network::{
        tcp::bind_listener,
        transfer::{
            download,
            options::{DownloadFileOptions, ServeFileOptions, TransferLogLevel},
            serve,
        },
    },
    state::{model, paths, storage},
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn transfer_persists_seed_and_download_library_state() {
    let input = temp_path("state-transfer-input.bin");
    let output = temp_path("state-transfer-output.bin");
    let seeder_root = temp_path("state-transfer-seeder-root");
    let peer_root = temp_path("state-transfer-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
    let _ = fs::remove_dir_all(&seeder_root);
    let _ = fs::remove_dir_all(&peer_root);

    fs::write(
        &input,
        b"persistent encrypted chunk state should survive after transfer",
    )
    .unwrap();

    let listener = bind_listener("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_input = input.clone();
    let server_root = seeder_root.clone();

    let server = tokio::spawn(async move {
        serve::file_once_with(
            listener,
            server_input,
            8,
            ServeFileOptions::new("seeder", TransferLogLevel::Quiet).with_library_root(server_root),
        )
        .await
        .unwrap();
    });

    let manifest = download::from_peer_with(
        addr,
        &output,
        DownloadFileOptions::new("peer", TransferLogLevel::Quiet).with_library_root(&peer_root),
    )
    .await
    .unwrap();
    server.await.unwrap();

    assert_eq!(hash_file(&input).unwrap(), hash_file(&output).unwrap());

    let descriptor = EtleDescriptor::new(
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
    );

    let seeder_paths = paths::LibraryPaths::for_share(&seeder_root, descriptor.share_id);
    let peer_paths = paths::LibraryPaths::for_share(&peer_root, descriptor.share_id);

    assert!(seeder_paths.descriptor_path().is_file());
    assert!(seeder_paths.secret_path().is_file());
    assert!(peer_paths.descriptor_path().is_file());
    assert!(peer_paths.secret_path().is_file());

    assert_eq!(
        storage::read_state(&seeder_paths).unwrap().mode,
        model::ShareMode::Seeding
    );
    assert_eq!(
        storage::read_state(&peer_paths).unwrap().mode,
        model::ShareMode::Completed
    );
    assert_eq!(
        storage::read_progress(&peer_paths)
            .unwrap()
            .completed_chunks
            .len(),
        manifest.chunks.len()
    );

    for meta in &manifest.chunks {
        assert!(seeder_paths.chunk_path(meta.index).is_file());
        assert!(peer_paths.chunk_path(meta.index).is_file());
    }

    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
    fs::remove_dir_all(seeder_root).unwrap();
    fs::remove_dir_all(peer_root).unwrap();
}
