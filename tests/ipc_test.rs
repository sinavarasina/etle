use std::{net::SocketAddr, path::PathBuf};

use etle::{
    file::descriptor::ShareId,
    ipc::{
        codec::{self as ipc_codec, MAX_IPC_FRAME_SIZE},
        message::{IpcCommand, IpcEvent, IpcResponse, IpcShareSummary},
        path as ipc_path,
    },
};
use tokio::io::duplex;

#[tokio::test]
async fn ipc_codec_roundtrips_command_response_and_event() {
    let share_id = ShareId([7_u8; 32]);
    let listen: SocketAddr = "127.0.0.1:7000".parse().unwrap();
    let peer: SocketAddr = "127.0.0.1:7001".parse().unwrap();
    let (mut client, mut daemon) = duplex(4096);

    let command = IpcCommand::Download {
        share_id,
        peers: vec![peer],
        output: Some(PathBuf::from("received.webm")),
        parallelism: 2,
        request_window: 16,
        discovery_port: 7003,
        discovery_timeout_ms: 3000,
        discovery_multicast: "239.255.0.86".parse().unwrap(),
    };

    ipc_codec::send_ipc_message(&mut client, &command)
        .await
        .unwrap();
    let decoded_command: IpcCommand = ipc_codec::receive_ipc_message(&mut daemon).await.unwrap();
    assert_eq!(decoded_command, command);

    let summary = IpcShareSummary {
        share_id,
        name: "sample.webm".to_string(),
        completed_chunks: 3,
        total_chunks: 5,
        has_secret: true,
        mode: Some("downloading".to_string()),
    };

    let response = IpcResponse::Shares {
        shares: vec![summary.clone()],
    };
    ipc_codec::send_ipc_message(&mut daemon, &response)
        .await
        .unwrap();
    let decoded_response: IpcResponse = ipc_codec::receive_ipc_message(&mut client).await.unwrap();
    assert_eq!(decoded_response, response);

    let event = IpcEvent::ServerStarted { listen };
    ipc_codec::send_ipc_message(&mut daemon, &event)
        .await
        .unwrap();
    let decoded_event: IpcEvent = ipc_codec::receive_ipc_message(&mut client).await.unwrap();
    assert_eq!(decoded_event, event);
}

#[cfg(unix)]
#[test]
fn ipc_paths_are_under_etle_state_directory() {
    let path = ipc_path::default_ipc_socket_path(PathBuf::from("/tmp/etle-root"));
    assert_eq!(path, PathBuf::from("/tmp/etle-root/.etle/etled.sock"));
    assert_eq!(ipc_path::default_windows_pipe_name(), r"\\.\pipe\etled");
}

#[cfg(windows)]
#[test]
fn ipc_paths_use_windows_named_pipe() {
    let path = ipc_path::default_ipc_socket_path(PathBuf::from(r"C:\Temp\etle-root"));
    assert_eq!(path, PathBuf::from(r"\\.\pipe\etled"));
    assert_eq!(ipc_path::default_windows_pipe_name(), r"\\.\pipe\etled");
}

#[test]
fn ipc_frame_size_is_smaller_than_transfer_frame_size() {
    assert_eq!(MAX_IPC_FRAME_SIZE, 4 * 1024 * 1024);
}
