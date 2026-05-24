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

    send_ipc_message(
        stream,
        &IpcResponse::Ack {
            message: "event subscription started".to_string(),
        },
    )
    .await?;

    loop {
        match events.recv().await {
            Ok(event) => send_ipc_message(stream, &event).await?,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                send_ipc_message(
                    stream,
                    &IpcEvent::Error {
                        message: format!("event subscriber lagged; skipped {skipped} event(s)"),
                    },
                )
                .await?;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}
