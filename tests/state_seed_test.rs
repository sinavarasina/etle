use std::{fs, path::PathBuf};

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
    state::library,
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn completed_download_state_can_seed_same_file_again() {
    let input = temp_path("state-seed-input.bin");
    let first_output = temp_path("state-seed-first-output.bin");
    let second_output = temp_path("state-seed-second-output.bin");
    let original_seed_root = temp_path("state-seed-original-root");
    let downloaded_seed_root = temp_path("state-seed-downloaded-root");
    let second_peer_root = temp_path("state-seed-second-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&first_output);
    let _ = fs::remove_file(&second_output);
    let _ = fs::remove_dir_all(&original_seed_root);
    let _ = fs::remove_dir_all(&downloaded_seed_root);
    let _ = fs::remove_dir_all(&second_peer_root);

    fs::write(
        &input,
        b"downloaded ETLE state should become a reusable seeding source",
    )
    .unwrap();

    let original_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let original_addr = original_listener.local_addr().unwrap();
    let original_input = input.clone();
    let original_seed_root_task = original_seed_root.clone();

    let original_server = tokio::spawn(async move {
        serve::file_once_with(
            original_listener,
            original_input,
            8,
            ServeFileOptions::new("original-seeder", TransferLogLevel::Quiet)
                .with_library_root(original_seed_root_task),
        )
        .await
        .unwrap();
    });

    let first_manifest = download::from_peer_with(
        original_addr,
        &first_output,
        DownloadFileOptions::new("first-peer", TransferLogLevel::Quiet)
            .with_library_root(downloaded_seed_root.clone()),
    )
    .await
    .unwrap();
    original_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&first_output).unwrap()
    );

    let shares = library::list(&downloaded_seed_root).unwrap();
    assert_eq!(shares.len(), 1);
    let share_id = shares[0].descriptor.share_id;
    assert_eq!(
        shares[0].descriptor.chunks.len(),
        first_manifest.chunks.len()
    );

    let state_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let state_addr = state_listener.local_addr().unwrap();
    let downloaded_seed_root_task = downloaded_seed_root.clone();

    let state_server = tokio::spawn(async move {
        serve::share_once(
            state_listener,
            downloaded_seed_root_task,
            share_id,
            ServeFileOptions::new("state-seeder", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    download::from_peer_with(
        state_addr,
        &second_output,
        DownloadFileOptions::new("second-peer", TransferLogLevel::Quiet)
            .with_library_root(second_peer_root.clone()),
    )
    .await
    .unwrap();
    state_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&second_output).unwrap()
    );

    fs::remove_file(input).unwrap();
    fs::remove_file(first_output).unwrap();
    fs::remove_file(second_output).unwrap();
    fs::remove_dir_all(original_seed_root).unwrap();
    fs::remove_dir_all(downloaded_seed_root).unwrap();
    fs::remove_dir_all(second_peer_root).unwrap();
}
