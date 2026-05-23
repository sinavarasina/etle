use std::{net::SocketAddr, path::PathBuf};

use etle::{
    file::descriptor::ShareId,
    ipc::{
        IpcCommand, IpcEvent, IpcResponse, IpcShareSummary, MAX_IPC_FRAME_SIZE,
        default_ipc_socket_path, default_windows_pipe_name, receive_ipc_message, send_ipc_message,
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

    send_ipc_message(&mut client, &command).await.unwrap();
    let decoded_command: IpcCommand = receive_ipc_message(&mut daemon).await.unwrap();
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
    send_ipc_message(&mut daemon, &response).await.unwrap();
    let decoded_response: IpcResponse = receive_ipc_message(&mut client).await.unwrap();
    assert_eq!(decoded_response, response);

    let event = IpcEvent::ServerStarted { listen };
    send_ipc_message(&mut daemon, &event).await.unwrap();
    let decoded_event: IpcEvent = receive_ipc_message(&mut client).await.unwrap();
    assert_eq!(decoded_event, event);
}

#[test]
fn ipc_paths_are_under_etle_state_directory() {
    let path = default_ipc_socket_path(PathBuf::from("/tmp/etle-root"));
    assert_eq!(path, PathBuf::from("/tmp/etle-root/.etle/etled.sock"));
    assert_eq!(default_windows_pipe_name(), r"\\.\pipe\etled");
}

#[test]
fn ipc_frame_size_is_smaller_than_transfer_frame_size() {
    assert_eq!(MAX_IPC_FRAME_SIZE, 4 * 1024 * 1024);
}
