use std::{fs, path::PathBuf};

use etle::{
    crypto::hash::hash_file,
    network::{
        DownloadFileOptions, ServeFileOptions, TransferLogLevel, bind_listener,
        download_file_from_peer_with_options, serve_file_to_one_peer_with_options,
        serve_library_to_one_peer,
    },
    state::list_library_shares,
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn library_server_serves_requested_share_id() {
    let input = temp_path("library-server-input.bin");
    let bootstrap_output = temp_path("library-server-bootstrap-output.bin");
    let requested_output = temp_path("library-server-requested-output.bin");
    let seed_root = temp_path("library-server-seed-root");
    let bootstrap_peer_root = temp_path("library-server-bootstrap-peer-root");
    let requested_peer_root = temp_path("library-server-requested-peer-root");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&bootstrap_output);
    let _ = fs::remove_file(&requested_output);
    let _ = fs::remove_dir_all(&seed_root);
    let _ = fs::remove_dir_all(&bootstrap_peer_root);
    let _ = fs::remove_dir_all(&requested_peer_root);

    fs::write(
        &input,
        b"multi-share library server should serve requested share",
    )
    .unwrap();

    let bootstrap_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let bootstrap_addr = bootstrap_listener.local_addr().unwrap();
    let bootstrap_input = input.clone();
    let seed_root_task = seed_root.clone();

    let bootstrap_server = tokio::spawn(async move {
        serve_file_to_one_peer_with_options(
            bootstrap_listener,
            bootstrap_input,
            8,
            ServeFileOptions::new("bootstrap-seeder", TransferLogLevel::Quiet)
                .with_library_root(seed_root_task),
        )
        .await
        .unwrap();
    });

    download_file_from_peer_with_options(
        bootstrap_addr,
        &bootstrap_output,
        DownloadFileOptions::new("bootstrap-peer", TransferLogLevel::Quiet)
            .with_library_root(bootstrap_peer_root.clone()),
    )
    .await
    .unwrap();
    bootstrap_server.await.unwrap();

    let shares = list_library_shares(&seed_root).unwrap();
    assert_eq!(shares.len(), 1);
    let share_id = shares[0].descriptor.share_id;

    let library_listener = bind_listener("127.0.0.1:0").await.unwrap();
    let library_addr = library_listener.local_addr().unwrap();
    let library_root_task = seed_root.clone();

    let library_server = tokio::spawn(async move {
        serve_library_to_one_peer(
            library_listener,
            library_root_task,
            ServeFileOptions::new("library-seeder", TransferLogLevel::Quiet),
        )
        .await
        .unwrap();
    });

    download_file_from_peer_with_options(
        library_addr,
        &requested_output,
        DownloadFileOptions::new("requesting-peer", TransferLogLevel::Quiet)
            .with_library_root(requested_peer_root.clone())
            .with_requested_share_id(Some(share_id)),
    )
    .await
    .unwrap();
    library_server.await.unwrap();

    assert_eq!(
        hash_file(&input).unwrap(),
        hash_file(&requested_output).unwrap()
    );

    fs::remove_file(input).unwrap();
    fs::remove_file(bootstrap_output).unwrap();
    fs::remove_file(requested_output).unwrap();
    fs::remove_dir_all(seed_root).unwrap();
    fs::remove_dir_all(bootstrap_peer_root).unwrap();
    fs::remove_dir_all(requested_peer_root).unwrap();
}
