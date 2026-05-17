use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol::{error::ProtocolError, message::WireMessage};

/// Maximum serialized protocol frame size.
///
/// Current chunks are expected to be around 1 MiB, so 64 MiB is generous
/// enough for Sprint 2 while still protecting the receiver from absurd frames.
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

pub async fn send_message<W>(writer: &mut W, message: &WireMessage) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
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

pub async fn receive_message<R>(reader: &mut R) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut len_bytes = [0_u8; 4];
    reader.read_exact(&mut len_bytes).await?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    validate_frame_len(len)?;

    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload).await?;

    let (message, bytes_read): (WireMessage, usize) =
        bincode::serde::decode_from_slice(&payload, bincode::config::standard())?;

    if bytes_read != payload.len() {
        return Err(ProtocolError::TrailingBytes {
            bytes_read,
            frame_len: payload.len(),
        });
    }

    Ok(message)
}

fn validate_frame_len(len: usize) -> Result<(), ProtocolError> {
    if len == 0 {
        return Err(ProtocolError::EmptyFrame);
    }

    if len > MAX_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge {
            len,
            max: MAX_FRAME_SIZE,
        });
    }

    Ok(())
}
