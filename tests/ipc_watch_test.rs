#![cfg(unix)]

use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use etle::ipc::{
    client as ipc_client, codec as ipc_codec,
    message::{IpcCommand, IpcEvent, IpcResponse},
    server as ipc_server,
};
use tokio::net::UnixStream;

#[tokio::test]
async fn ipc_event_subscription_does_not_block_other_commands() {
    let root = temp_dir("ipc-watch");
    let socket_path = root.join(".etle").join("etled.sock");
    fs::create_dir_all(socket_path.parent().unwrap()).unwrap();

    let server_root = root.clone();
    let server_socket = socket_path.clone();
    let server =
        tokio::spawn(
            async move { ipc_server::listener::forever(server_socket, server_root).await },
        );

    wait_for_ping(&socket_path).await;

    let mut watch_stream = UnixStream::connect(&socket_path).await.unwrap();
    ipc_codec::send_ipc_message(&mut watch_stream, &IpcCommand::SubscribeEvents)
        .await
        .unwrap();
    let ack: IpcResponse = ipc_codec::receive_ipc_message(&mut watch_stream)
        .await
        .unwrap();
    assert_eq!(
        ack,
        IpcResponse::Ack {
            message: "event subscription started".to_string(),
        }
    );

    let ping = ipc_client::send_ipc_command(&socket_path, IpcCommand::Ping)
        .await
        .unwrap();
    assert_eq!(ping, IpcResponse::Pong);

    let shares = ipc_client::send_ipc_command(&socket_path, IpcCommand::ListShares)
        .await
        .unwrap();
    assert_eq!(shares, IpcResponse::Shares { shares: vec![] });

    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7000);
    ipc_server::events::publish(IpcEvent::ServerStarted { listen });
    let event: IpcEvent = tokio::time::timeout(
        Duration::from_secs(1),
        ipc_codec::receive_ipc_message(&mut watch_stream),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(event, IpcEvent::ServerStarted { listen });

    let shutdown = ipc_client::send_ipc_command(&socket_path, IpcCommand::Shutdown)
        .await
        .unwrap();
    assert_eq!(
        shutdown,
        IpcResponse::Ack {
            message: "daemon shutdown requested".to_string(),
        }
    );

    server.await.unwrap().unwrap();
    let _ = fs::remove_dir_all(root);
}

async fn wait_for_ping(socket_path: &PathBuf) {
    for _ in 0..50 {
        if matches!(
            ipc_client::send_ipc_command(socket_path, IpcCommand::Ping).await,
            Ok(IpcResponse::Pong)
        ) {
            return;
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("IPC server did not become reachable");
}

fn temp_dir(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("etle-{name}-{}-{millis}", std::process::id()))
}
