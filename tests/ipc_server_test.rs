#![cfg(unix)]

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use etle::ipc::{
    IpcCommand, IpcResponse, receive_ipc_message, send_ipc_message, serve_ipc_forever,
};
use tokio::{net::UnixStream, time::sleep};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn unix_ipc_server_handles_ping_list_and_shutdown() {
    let root = temp_path("ipc-server-root");
    let socket = root.join(".etle").join("etled.sock");
    let _ = fs::remove_dir_all(&root);

    let server_root = root.clone();
    let server_socket = socket.clone();
    let server = tokio::spawn(async move { serve_ipc_forever(server_socket, server_root).await });

    let response = roundtrip(&socket, IpcCommand::Ping).await;
    assert_eq!(response, IpcResponse::Pong);

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

async fn roundtrip(socket: &Path, command: IpcCommand) -> IpcResponse {
    let mut stream = connect_with_retry(socket).await;
    send_ipc_message(&mut stream, &command).await.unwrap();
    receive_ipc_message(&mut stream).await.unwrap()
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
