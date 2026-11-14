use std::{path::PathBuf, time::Duration};

use etle::ipc::{
    client::{EventSubscription, send_ipc_command},
    message::{IpcCommand, IpcResponse},
};
use relm4::Sender;
use tokio::sync::watch;

use super::model::{AppCmd, IpcRequestKind};

pub async fn run_command_with_timeout(
    socket_path: String,
    kind: IpcRequestKind,
    command: IpcCommand,
) -> AppCmd {
    let label = kind.label();
    let result =
        match tokio::time::timeout(kind.timeout(), send_ipc_command(&socket_path, command)).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err(format!("{label} request timed out")),
        };

    AppCmd::CommandResponse {
        socket_path,
        kind,
        result,
    }
}

pub async fn fetch_daemon_snapshot(socket_path: String) -> AppCmd {
    let status_fut = tokio::time::timeout(
        Duration::from_secs(15),
        send_ipc_command(&socket_path, IpcCommand::GetStatus),
    );
    let shares_fut = tokio::time::timeout(
        Duration::from_secs(15),
        send_ipc_command(&socket_path, IpcCommand::ListShares),
    );
    let (status_raw, shares_raw) = tokio::join!(status_fut, shares_fut);

    AppCmd::SyncSnapshot {
        socket_path,
        status_result: flatten_timeout(status_raw, "status"),
        shares_result: flatten_timeout(shares_raw, "refresh"),
    }
}

pub async fn stream_daemon_events(
    socket_path: String,
    generation: u64,
    mut active_gen: watch::Receiver<u64>,
    out: Sender<AppCmd>,
) {
    let connect_fut = EventSubscription::connect(PathBuf::from(socket_path));
    let mut subscription = tokio::select! {
        result = connect_fut => match result {
            Ok(sub) => sub,
            Err(error) => {
                if *active_gen.borrow() == generation {
                    let _ = out.send(AppCmd::WatchStopped {
                        generation,
                        error: error.to_string(),
                    });
                }
                return;
            }
        },
        _ = wait_for_superseded(&mut active_gen, generation) => return,
    };

    if *active_gen.borrow() != generation
        || out.send(AppCmd::WatchConnected { generation }).is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            _ = wait_for_superseded(&mut active_gen, generation) => return,
            event_res = subscription.next_event() => match event_res {
                Ok(event) => {
                    if *active_gen.borrow() != generation
                        || out.send(AppCmd::WatchEvent { generation, event }).is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    if *active_gen.borrow() == generation {
                        let _ = out.send(AppCmd::WatchStopped {
                            generation,
                            error: error.to_string(),
                        });
                    }
                    return;
                }
            }
        }
    }
}

async fn wait_for_superseded(active_gen: &mut watch::Receiver<u64>, generation: u64) {
    while *active_gen.borrow() == generation {
        if active_gen.changed().await.is_err() {
            return;
        }
    }
}

fn flatten_timeout(
    raw: Result<Result<IpcResponse, etle::ipc::error::IpcError>, tokio::time::error::Elapsed>,
    label: &str,
) -> Result<IpcResponse, String> {
    match raw {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err(format!("{label} request timed out")),
    }
}
