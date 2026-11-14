#![cfg(unix)]

mod common;

use common::{print_banner, print_kv, print_step};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use etle::{
    file::descriptor::ShareId,
    ipc::{
        client as ipc_client, codec as ipc_codec,
        error::IpcError,
        message::{IpcActiveJob, IpcCommand, IpcEvent, IpcJobKind, IpcResponse},
        server as ipc_server,
    },
    network::transfer::jobs,
};
use tokio::{io::AsyncWriteExt, net::UnixStream, time::sleep};

static SERVER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn unix_ipc_server_handles_ping_list_and_shutdown() {
    let _guard = SERVER_LOCK.lock().await;
    print_banner("unix_ipc_server_handles_ping_list_and_shutdown");
    print_step(1, "execute scenario");
    print_kv("test", "unix_ipc_server_handles_ping_list_and_shutdown");
    let root = temp_path("ipc-server-root");
    let socket = root.join(".etle").join("etled.sock");
    let _ = fs::remove_dir_all(&root);

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    let response = roundtrip(&socket, IpcCommand::Ping).await;
    assert_eq!(response, IpcResponse::Pong);

    let status = roundtrip(&socket, IpcCommand::GetStatus).await;
    match status {
        IpcResponse::Status(status) => {
            assert_eq!(status.library_root, root);
            assert_eq!(status.share_count, 0);
            assert!(status.active_jobs.is_empty());
        }
        other => panic!("expected Status response, got {other:?}"),
    }

    let response = roundtrip(&socket, IpcCommand::ListShares).await;
    assert_eq!(response, IpcResponse::Shares { shares: Vec::new() });

    let response = roundtrip(&socket, IpcCommand::Shutdown).await;
    assert_eq!(
        response,
        IpcResponse::Ack {
            message: "daemon shutdown requested".to_string(),
        }
    );

    server.await.unwrap().unwrap();
    assert!(!socket.exists());
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn unix_ipc_server_seed_status_and_delete_lifecycle() {
    let _guard = SERVER_LOCK.lock().await;
    print_banner("unix_ipc_server_seed_status_and_delete_lifecycle");
    print_step(1, "execute scenario");
    print_kv("test", "unix_ipc_server_seed_status_and_delete_lifecycle");
    let root = temp_path("ipc-server-seed-root");
    let socket = root.join(".etle").join("etled.sock");
    let sample_file = root.join("sample-payload.bin");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let payload = vec![0x5A_u8; 4096];
    fs::write(&sample_file, &payload).unwrap();

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    assert_eq!(
        roundtrip(&socket, IpcCommand::Ping).await,
        IpcResponse::Pong
    );

    let mut events = ipc_client::EventSubscription::connect(socket.clone())
        .await
        .unwrap();

    let job_id = match roundtrip(
        &socket,
        IpcCommand::SeedFile {
            input: sample_file.clone(),
            chunk_size: 1024,
        },
    )
    .await
    {
        IpcResponse::SeedQueued { job_id, label } => {
            assert_eq!(label, "sample-payload.bin");
            job_id
        }
        other => panic!("expected SeedQueued response, got {other:?}"),
    };

    let mut saw_started = false;
    let mut saw_updated = false;
    let mut completed_share_id = None;

    for _ in 0..32 {
        let event = tokio::time::timeout(Duration::from_secs(5), events.next_event())
            .await
            .unwrap()
            .unwrap();
        match event {
            IpcEvent::JobStarted { job } if job.job_id == job_id => {
                assert_eq!(job.kind, IpcJobKind::Seed);
                saw_started = true;
            }
            IpcEvent::ShareUpdated { share } if share.name == "sample-payload.bin" => {
                assert_eq!(share.total_bytes, payload.len() as u64);
                assert_eq!(share.chunk_size, 1024);
                assert_eq!(share.total_chunks, 4);
                assert_eq!(share.completed_chunks, 4);
                saw_updated = true;
            }
            IpcEvent::JobCompleted {
                job_id: done_id,
                kind,
                share_id,
                ..
            } if done_id == job_id => {
                assert_eq!(kind, IpcJobKind::Seed);
                completed_share_id = share_id;
                if saw_started && saw_updated {
                    break;
                }
            }
            _ => {}
        }
        if saw_started && saw_updated && completed_share_id.is_some() {
            break;
        }
    }

    assert!(saw_started, "expected JobStarted event for seed job");
    assert!(saw_updated, "expected ShareUpdated event for seeded file");
    let share_id = completed_share_id.expect("expected JobCompleted event with ShareId");

    match roundtrip(&socket, IpcCommand::GetStatus).await {
        IpcResponse::Status(status) => {
            assert_eq!(status.share_count, 1);
            assert!(status.active_jobs.is_empty());
        }
        other => panic!("expected Status response, got {other:?}"),
    }

    match roundtrip(&socket, IpcCommand::DeleteShare { share_id }).await {
        IpcResponse::ShareDeleted {
            share_id: deleted_id,
            ..
        } => assert_eq!(deleted_id, share_id),
        other => panic!("expected ShareDeleted response, got {other:?}"),
    }

    match roundtrip(&socket, IpcCommand::GetStatus).await {
        IpcResponse::Status(status) => {
            assert_eq!(status.share_count, 0);
        }
        other => panic!("expected Status response, got {other:?}"),
    }

    let _ = roundtrip(&socket, IpcCommand::Shutdown).await;
    server.await.unwrap().unwrap();
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn unix_ipc_server_survives_malformed_or_abrupt_client() {
    let _guard = SERVER_LOCK.lock().await;
    print_banner("unix_ipc_server_survives_malformed_or_abrupt_client");
    let root = temp_path("ipc-server-resilient-root");
    let socket = root.join(".etle").join("etled.sock");
    let _ = fs::remove_dir_all(&root);

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    assert_eq!(
        roundtrip(&socket, IpcCommand::Ping).await,
        IpcResponse::Pong
    );

    // Client connects and drops immediately without sending anything.
    drop(connect_with_retry(&socket).await);

    // Client writes invalid frame header and disconnects.
    let mut malformed = connect_with_retry(&socket).await;
    let _ = malformed.write_all(&[0xff_u8; 16]).await;
    drop(malformed);

    assert_eq!(
        roundtrip(&socket, IpcCommand::Ping).await,
        IpcResponse::Pong
    );

    let _ = roundtrip(&socket, IpcCommand::Shutdown).await;
    server.await.unwrap().unwrap();
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn unix_ipc_server_avoids_hol_blocking_and_refuses_duplicate_daemon() {
    let _guard = SERVER_LOCK.lock().await;
    print_banner("unix_ipc_server_avoids_hol_blocking_and_refuses_duplicate_daemon");
    let root = temp_path("ipc-server-hol-root");
    let socket = root.join(".etle").join("etled.sock");
    let _ = fs::remove_dir_all(&root);

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    assert_eq!(
        roundtrip(&socket, IpcCommand::Ping).await,
        IpcResponse::Pong
    );

    // A second daemon on the same socket path must fail instead of unlinking the live socket.
    let duplicate_err = ipc_server::listener::forever(socket.clone(), root.clone())
        .await
        .expect_err("second daemon on live socket must be rejected");
    assert!(
        matches!(duplicate_err, IpcError::AlreadyRunning(_)),
        "expected AlreadyRunning error, got {duplicate_err:?}"
    );

    // Keep a silent client connected without sending a frame header.
    let _stalled = connect_with_retry(&socket).await;

    let ping_res =
        tokio::time::timeout(Duration::from_secs(2), roundtrip(&socket, IpcCommand::Ping))
            .await
            .expect("concurrent client must not be blocked by stalled peer");
    assert_eq!(ping_res, IpcResponse::Pong);

    let shutdown_res = tokio::time::timeout(
        Duration::from_secs(2),
        roundtrip(&socket, IpcCommand::Shutdown),
    )
    .await
    .expect("shutdown must not be blocked by stalled peer");
    assert!(matches!(shutdown_res, IpcResponse::Ack { .. }));

    server.await.unwrap().unwrap();
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn unix_ipc_server_rejects_zero_chunk_size_and_guards_active_upload_delete() {
    let _guard = SERVER_LOCK.lock().await;
    print_banner("unix_ipc_server_rejects_zero_chunk_size_and_guards_active_upload_delete");
    let root = temp_path("ipc-server-guards-root");
    let socket = root.join(".etle").join("etled.sock");
    let sample_file = root.join("payload.bin");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(&sample_file, vec![0x77_u8; 512]).unwrap();

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    let zero_res = roundtrip(
        &socket,
        IpcCommand::SeedFile {
            input: sample_file,
            chunk_size: 0,
        },
    )
    .await;
    assert!(
        matches!(zero_res, IpcResponse::Error { .. }),
        "chunk_size=0 must return Error, got {zero_res:?}"
    );

    let share_id = ShareId([0x42; 32]);
    let job_id = format!("test-upload-guard-{}", std::process::id());
    jobs::start_job(IpcActiveJob {
        job_id: job_id.clone(),
        kind: IpcJobKind::Upload,
        share_id: Some(share_id),
        label: "payload.bin".to_string(),
        phase: "seeder:sending".to_string(),
        completed_chunks: 0,
        total_chunks: 1,
        bytes_done: 0,
        total_bytes: 512,
        bytes_per_second: 0,
    });

    let delete_res = roundtrip(&socket, IpcCommand::DeleteShare { share_id }).await;
    jobs::complete_job(&job_id, "test cleanup");

    match delete_res {
        IpcResponse::Error { message } => {
            assert!(
                message.contains("active"),
                "expected active job error message, got: {message}"
            );
        }
        other => panic!("expected Error blocking delete during Upload job, got {other:?}"),
    }

    let _ = roundtrip(&socket, IpcCommand::Shutdown).await;
    server.await.unwrap().unwrap();
    let _ = fs::remove_dir_all(&root);
}

async fn roundtrip(socket: &Path, command: IpcCommand) -> IpcResponse {
    let mut stream = connect_with_retry(socket).await;
    ipc_codec::send_ipc_message(&mut stream, &command)
        .await
        .unwrap();
    ipc_codec::receive_ipc_message(&mut stream).await.unwrap()
}

async fn connect_with_retry(socket: &Path) -> UnixStream {
    for _ in 0..50 {
        match UnixStream::connect(socket).await {
            Ok(stream) => return stream,
            Err(_) => sleep(Duration::from_millis(10)).await,
        }
    }

    UnixStream::connect(socket).await.unwrap()
}
