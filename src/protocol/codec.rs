use std::{fs::{self, File}, io::{Read, Write}, path::Path};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol::{error::ProtocolError, message::WireMessage};

/// Maximum serialized protocol frame size.
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;
const RAW_CHUNK_FRAME_TAG: u8 = 0xec;
const RAW_CHUNK_HEADER_SIZE: usize = 1 + 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedRawChunkFile {
    pub index: u32,
    pub data_len: usize,
    pub blake3_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceivedChunkFrame {
    RawChunkFile(ReceivedRawChunkFile),
    Message(WireMessage),
}

pub async fn send_message<W>(writer: &mut W, message: &WireMessage) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    if let WireMessage::Chunk { index, data } = message {
        return send_raw_chunk_frame(writer, *index, data).await;
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

pub async fn receive_message<R>(reader: &mut R) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let len = read_frame_len(reader).await?;
    let mut first = [0_u8; 1];
    reader.read_exact(&mut first).await?;

    if first[0] == RAW_CHUNK_FRAME_TAG {
        return receive_raw_chunk_frame_after_tag(reader, len).await;
    }

    receive_bincode_frame_after_first_byte(reader, len, first[0]).await
}

pub async fn receive_chunk_frame_to_file<R>(
    reader: &mut R,
    output_path: impl AsRef<Path>,
) -> Result<ReceivedChunkFrame, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let len = read_frame_len(reader).await?;
    let mut first = [0_u8; 1];
    reader.read_exact(&mut first).await?;

    if first[0] != RAW_CHUNK_FRAME_TAG {
        return receive_bincode_frame_after_first_byte(reader, len, first[0])
            .await
            .map(ReceivedChunkFrame::Message);
    }

    receive_raw_chunk_frame_to_file_after_tag(reader, len, output_path)
        .await
        .map(ReceivedChunkFrame::RawChunkFile)
}

pub async fn send_raw_chunk_file<W>(
    writer: &mut W,
    index: u32,
    path: impl AsRef<Path>,
    expected_size: u64,
) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let mut file = File::open(path.as_ref())?;
    let actual_size = file.metadata()?.len();
    if actual_size != expected_size {
        return Err(ProtocolError::RawChunkSizeMismatch {
            expected: expected_size as usize,
            actual: actual_size as usize,
        });
    }

    let data_len = usize::try_from(actual_size).map_err(|_| ProtocolError::FrameTooLarge {
        len: usize::MAX,
        max: MAX_FRAME_SIZE,
    })?;
    let frame_len =
        RAW_CHUNK_HEADER_SIZE
            .checked_add(data_len)
            .ok_or(ProtocolError::FrameTooLarge {
                len: usize::MAX,
                max: MAX_FRAME_SIZE,
            })?;
    validate_frame_len(frame_len)?;

    writer.write_all(&(frame_len as u32).to_be_bytes()).await?;
    writer.write_all(&[RAW_CHUNK_FRAME_TAG]).await?;
    writer.write_all(&index.to_be_bytes()).await?;

    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read]).await?;
    }

    writer.flush().await?;
    Ok(())
}

async fn send_raw_chunk_frame<W>(
    writer: &mut W,
    index: u32,
    data: &[u8],
) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let frame_len =
        RAW_CHUNK_HEADER_SIZE
            .checked_add(data.len())
            .ok_or(ProtocolError::FrameTooLarge {
                len: usize::MAX,
                max: MAX_FRAME_SIZE,
            })?;
    validate_frame_len(frame_len)?;

    writer.write_all(&(frame_len as u32).to_be_bytes()).await?;
    writer.write_all(&[RAW_CHUNK_FRAME_TAG]).await?;
    writer.write_all(&index.to_be_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await?;

    Ok(())
}

async fn receive_raw_chunk_frame_to_file_after_tag<R>(
    reader: &mut R,
    frame_len: usize,
    output_path: impl AsRef<Path>,
) -> Result<ReceivedRawChunkFile, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    if frame_len < RAW_CHUNK_HEADER_SIZE {
        return Err(ProtocolError::InvalidRawChunkFrame(
            "frame is smaller than raw chunk header",
        ));
    }

    let mut index_bytes = [0_u8; 4];
    reader.read_exact(&mut index_bytes).await?;
    let index = u32::from_be_bytes(index_bytes);
    let data_len = frame_len - RAW_CHUNK_HEADER_SIZE;

    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut output = File::create(output_path)?;
    let mut hasher = blake3::Hasher::new();
    let mut remaining = data_len;
    let mut buffer = [0_u8; 64 * 1024];

    while remaining > 0 {
        let take = remaining.min(buffer.len());
        reader.read_exact(&mut buffer[..take]).await?;
        output.write_all(&buffer[..take])?;
        hasher.update(&buffer[..take]);
        remaining -= take;
    }

    output.flush()?;

    Ok(ReceivedRawChunkFile {
        index,
        data_len,
        blake3_hash: *hasher.finalize().as_bytes(),
    })
}

async fn receive_raw_chunk_frame_after_tag<R>(
    reader: &mut R,
    frame_len: usize,
) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    if frame_len < RAW_CHUNK_HEADER_SIZE {
        return Err(ProtocolError::InvalidRawChunkFrame(
            "frame is smaller than raw chunk header",
        ));
    }

    let mut index_bytes = [0_u8; 4];
    reader.read_exact(&mut index_bytes).await?;
    let index = u32::from_be_bytes(index_bytes);

    let data_len = frame_len - RAW_CHUNK_HEADER_SIZE;
    let mut data = vec![0_u8; data_len];
    if data_len > 0 {
        reader.read_exact(&mut data).await?;
    }

    Ok(WireMessage::Chunk { index, data })
}

async fn receive_bincode_frame_after_first_byte<R>(
    reader: &mut R,
    frame_len: usize,
    first_byte: u8,
) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut payload = vec![0_u8; frame_len];
    payload[0] = first_byte;

    if frame_len > 1 {
        reader.read_exact(&mut payload[1..]).await?;
    }

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

async fn read_frame_len<R>(reader: &mut R) -> Result<usize, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut len_bytes = [0_u8; 4];
    reader.read_exact(&mut len_bytes).await?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    validate_frame_len(len)?;
    Ok(len)
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
