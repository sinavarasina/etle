use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::ipc::error::IpcError;

pub const MAX_IPC_FRAME_SIZE: usize = 4 * 1024 * 1024;

pub async fn send_ipc_message<W, T>(writer: &mut W, message: &T) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = bincode::serde::encode_to_vec(message, bincode::config::standard())?;
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

    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload).await?;

    let (message, bytes_read): (T, usize) =
        bincode::serde::decode_from_slice(&payload, bincode::config::standard())?;

    if bytes_read != payload.len() {
        return Err(IpcError::TrailingBytes {
            bytes_read,
            frame_len: payload.len(),
        });
    }

    Ok(message)
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
