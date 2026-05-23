use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol::{error::ProtocolError, message::WireMessage};

/// Maximum serialized protocol frame size.
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;
const RAW_CHUNK_MAGIC: &[u8; 8] = b"ETLECHK1";
const RAW_CHUNK_HEADER_LEN: usize = RAW_CHUNK_MAGIC.len() + 4 + 8;

pub async fn send_message<W>(writer: &mut W, message: &WireMessage) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    if let WireMessage::Chunk { index, data } = message {
        return send_raw_chunk_message(writer, *index, data).await;
    }

    let payload = bincode::serde::encode_to_vec(message, bincode::config::standard())?;
    validate_frame_len(payload.len())?;

    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;

    Ok(())
}

async fn send_raw_chunk_message<W>(
    writer: &mut W,
    index: u32,
    data: &[u8],
) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let frame_len =
        RAW_CHUNK_HEADER_LEN
            .checked_add(data.len())
            .ok_or(ProtocolError::FrameTooLarge {
                len: usize::MAX,
                max: MAX_FRAME_SIZE,
            })?;
    validate_frame_len(frame_len)?;

    writer.write_all(&(frame_len as u32).to_be_bytes()).await?;
    writer.write_all(RAW_CHUNK_MAGIC).await?;
    writer.write_all(&index.to_le_bytes()).await?;
    writer.write_all(&(data.len() as u64).to_le_bytes()).await?;
    writer.write_all(data).await?;
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

    let prefix_len = len.min(RAW_CHUNK_MAGIC.len());
    let mut prefix = vec![0_u8; prefix_len];
    reader.read_exact(&mut prefix).await?;

    if prefix.as_slice() == RAW_CHUNK_MAGIC {
        return receive_raw_chunk_message(reader, len).await;
    }

    let mut payload = prefix;
    payload.resize(len, 0);
    reader.read_exact(&mut payload[prefix_len..]).await?;
    decode_bincode_message(&payload)
}

async fn receive_raw_chunk_message<R>(
    reader: &mut R,
    frame_len: usize,
) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    if frame_len < RAW_CHUNK_HEADER_LEN {
        return Err(ProtocolError::FrameTooSmall {
            len: frame_len,
            min: RAW_CHUNK_HEADER_LEN,
        });
    }

    let mut index_bytes = [0_u8; 4];
    reader.read_exact(&mut index_bytes).await?;
    let index = u32::from_le_bytes(index_bytes);

    let mut data_len_bytes = [0_u8; 8];
    reader.read_exact(&mut data_len_bytes).await?;
    let data_len = u64::from_le_bytes(data_len_bytes) as usize;

    let expected_data_len = frame_len - RAW_CHUNK_HEADER_LEN;
    if data_len != expected_data_len {
        return Err(ProtocolError::RawChunkSizeMismatch {
            expected: expected_data_len,
            actual: data_len,
        });
    }

    let mut data = vec![0_u8; data_len];
    reader.read_exact(&mut data).await?;

    Ok(WireMessage::Chunk { index, data })
}

fn decode_bincode_message(payload: &[u8]) -> Result<WireMessage, ProtocolError> {
    let (message, bytes_read): (WireMessage, usize) =
        bincode::serde::decode_from_slice(payload, bincode::config::standard())?;

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
