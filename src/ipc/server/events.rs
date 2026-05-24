use super::prelude::*;

const IPC_EVENT_CHANNEL_CAPACITY: usize = 512;

pub fn publish(event: IpcEvent) {
    let _ = ipc_event_bus().send(event);
}

fn subscribe() -> broadcast::Receiver<IpcEvent> {
    ipc_event_bus().subscribe()
}

fn ipc_event_bus() -> &'static broadcast::Sender<IpcEvent> {
    static EVENT_BUS: OnceLock<broadcast::Sender<IpcEvent>> = OnceLock::new();
    EVENT_BUS.get_or_init(|| {
        let (sender, _receiver) = broadcast::channel(IPC_EVENT_CHANNEL_CAPACITY);
        sender
    })
}

pub(super) async fn serve_subscription<W>(stream: &mut W) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
{
    let mut events = subscribe();

    if let Err(error) = send_ipc_message(
        stream,
        &IpcResponse::Ack {
            message: "event subscription started".to_string(),
        },
    )
    .await
    {
        if is_disconnected_client(&error) {
            return Ok(());
        }

        return Err(error);
    }

    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(skipped)) => IpcEvent::Error {
                message: format!("event subscriber lagged; skipped {skipped} event(s)"),
            },
            Err(broadcast::error::RecvError::Closed) => break,
        };

        if let Err(error) = send_ipc_message(stream, &event).await {
            if is_disconnected_client(&error) {
                break;
            }

            return Err(error);
        }
    }

    Ok(())
}

pub(super) fn is_disconnected_client(error: &IpcError) -> bool {
    match error {
        IpcError::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::UnexpectedEof
        ),
        _ => false,
    }
}
