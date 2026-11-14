use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::ipc::error::IpcError;

pub const MAX_IPC_FRAME_SIZE: usize = 4 * 1024 * 1024;
const IPC_READ_CHUNK_BYTES: usize = 64 * 1024;

pub async fn send_ipc_message<W, T>(writer: &mut W, message: &T) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = bincode_next::serde::encode_to_vec(message, bincode_next::config::standard())?;
    validate_frame_len(payload.len())?;

    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;

    Ok(())
}

pub async fn receive_ipc_message<R, T>(reader: &mut R) -> Result<T, IpcError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_bytes = [0_u8; 4];
    reader.read_exact(&mut len_bytes).await?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    validate_frame_len(len)?;

    let payload = read_ipc_payload_incremental(reader, len).await?;

    let config = bincode_next::config::standard().with_limit::<MAX_IPC_FRAME_SIZE>();
    let (message, bytes_read): (T, usize) =
        bincode_next::serde::decode_from_slice(&payload, config)?;

    if bytes_read != payload.len() {
        return Err(IpcError::TrailingBytes {
            bytes_read,
            frame_len: payload.len(),
        });
    }

    Ok(message)
}

async fn read_ipc_payload_incremental<R>(reader: &mut R, len: usize) -> Result<Vec<u8>, IpcError>
where
    R: AsyncRead + Unpin,
{
    let mut payload = Vec::with_capacity(len.min(IPC_READ_CHUNK_BYTES));
    while payload.len() < len {
        let remaining = len - payload.len();
        let to_read = remaining.min(IPC_READ_CHUNK_BYTES);
        let old_len = payload.len();
        payload.resize(old_len + to_read, 0);
        let read_bytes = reader.read(&mut payload[old_len..]).await?;
        if read_bytes == 0 {
            return Err(IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "early eof while reading IPC frame payload",
            )));
        }
        payload.truncate(old_len + read_bytes);
    }
    Ok(payload)
}

fn validate_frame_len(len: usize) -> Result<(), IpcError> {
    if len == 0 {
        return Err(IpcError::EmptyFrame);
    }

    if len > MAX_IPC_FRAME_SIZE {
        return Err(IpcError::FrameTooLarge {
            len,
            max: MAX_IPC_FRAME_SIZE,
        });
    }

    Ok(())
}
