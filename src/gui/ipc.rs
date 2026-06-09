use std::{path::PathBuf, thread, time::Duration};

use etle::ipc::{
    client::{send_ipc_command, subscribe_ipc_events},
    message::IpcCommand,
};
use relm4::ComponentSender;

use super::{
    app::EtleGui,
    model::{AppInput, IpcRequestKind},
};

pub fn spawn_ipc_command(
    socket_path: String,
    kind: IpcRequestKind,
    command: IpcCommand,
    sender: ComponentSender<EtleGui>,
) {
    thread::spawn(move || {
        let response_socket = socket_path.clone();
        let result = run_async(async move {
            match tokio::time::timeout(
                request_timeout(kind),
                send_ipc_command(PathBuf::from(socket_path), command),
            )
            .await
            {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err(format!("{} request timed out", kind.label())),
            }
        });
        sender.input(AppInput::IpcResponse {
            socket_path: response_socket,
            kind,
            result,
        });
    });
}

fn request_timeout(kind: IpcRequestKind) -> Duration {
    match kind {
        IpcRequestKind::Ping => Duration::from_secs(5),
        IpcRequestKind::ListShares => Duration::from_secs(30),
        IpcRequestKind::Seed | IpcRequestKind::Download | IpcRequestKind::DeleteShare => {
            Duration::from_secs(30)
        }
    }
}

pub fn spawn_ipc_watch(socket_path: String, generation: u64, sender: ComponentSender<EtleGui>) {
    thread::spawn(move || {
        let sender_for_events = sender.clone();
        let result = run_async(async move {
            subscribe_ipc_events(PathBuf::from(socket_path), move |event| {
                sender_for_events.input(AppInput::IpcEvent { generation, event });
            })
            .await
            .map_err(|error| error.to_string())
        });

        if let Err(error) = result {
            sender.input(AppInput::IpcWatchStopped { generation, error });
        }
    });
}

pub fn spawn_auto_refresh_loop(sender: ComponentSender<EtleGui>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            sender.input(AppInput::AutoRefreshTick);
        }
    });
}

fn run_async<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create GUI IPC runtime")
        .block_on(future)
}
