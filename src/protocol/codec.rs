use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol::{error::ProtocolError, message::WireMessage};

/// Maximum serialized protocol frame size.
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;
/// Maximum control/handshake protocol frame size.
pub const MAX_CONTROL_FRAME_SIZE: usize = 256 * 1024;
const RAW_CHUNK_FRAME_TAG: u8 = 0xec;
const RAW_CHUNK_HEADER_SIZE: usize = 1 + 4;
const INCREMENTAL_READ_CHUNK_SIZE: usize = 64 * 1024;

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

pub async fn send<W>(writer: &mut W, message: &WireMessage) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    if let WireMessage::Chunk { index, data } = message {
        return send_raw_chunk_frame(writer, *index, data).await;
    }

    let payload = bincode_next::serde::encode_to_vec(message, bincode_next::config::standard())?;
    validate_frame_len(payload.len())?;

    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;

    Ok(())
}

pub async fn receive<R>(reader: &mut R) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let len = read_frame_len_with_max(reader, MAX_FRAME_SIZE).await?;
    let mut first = [0_u8; 1];
    reader.read_exact(&mut first).await?;

    if first[0] == RAW_CHUNK_FRAME_TAG {
        return receive_raw_chunk_frame_after_tag(reader, len).await;
    }

    receive_encoded_frame_after_first_byte(reader, len, first[0], MAX_FRAME_SIZE).await
}

pub async fn receive_control<R>(reader: &mut R) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let len = read_frame_len_with_max(reader, MAX_CONTROL_FRAME_SIZE).await?;
    let mut first = [0_u8; 1];
    reader.read_exact(&mut first).await?;

    if first[0] == RAW_CHUNK_FRAME_TAG {
        return Err(ProtocolError::InvalidRawChunkFrame(
            "raw chunk frame is not allowed on control channel",
        ));
    }

    receive_encoded_frame_after_first_byte(reader, len, first[0], MAX_CONTROL_FRAME_SIZE).await
}

pub async fn receive_chunk_to_file<R>(
    reader: &mut R,
    output_path: impl AsRef<Path>,
) -> Result<ReceivedChunkFrame, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let len = read_frame_len_with_max(reader, MAX_FRAME_SIZE).await?;
    let mut first = [0_u8; 1];
    reader.read_exact(&mut first).await?;

    if first[0] != RAW_CHUNK_FRAME_TAG {
        return receive_encoded_frame_after_first_byte(reader, len, first[0], MAX_FRAME_SIZE)
            .await
            .map(ReceivedChunkFrame::Message);
    }

    receive_raw_chunk_frame_to_file_after_tag(reader, len, output_path)
        .await
        .map(ReceivedChunkFrame::RawChunkFile)
}

pub async fn send_chunk_file<W>(
    writer: &mut W,
    index: u32,
    path: impl AsRef<Path>,
    expected_size: u64,
) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let file = File::open(path.as_ref())?;
    let actual_size = file.metadata()?.len();
    if actual_size != expected_size {
        return Err(ProtocolError::RawChunkSizeMismatch {
            expected: expected_size,
            actual: actual_size,
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
    validate_frame_len_with_max(frame_len, MAX_FRAME_SIZE)?;

    writer.write_all(&(frame_len as u32).to_be_bytes()).await?;
    writer.write_all(&[RAW_CHUNK_FRAME_TAG]).await?;
    writer.write_all(&index.to_be_bytes()).await?;

    let mut limited = file.take(actual_size);
    let mut sent = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = match limited.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        writer.write_all(&buffer[..read]).await?;
        sent += read as u64;
    }

    if sent != actual_size {
        return Err(ProtocolError::RawChunkSizeMismatch {
            expected: actual_size,
            actual: sent,
        });
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

struct PartialFileCleanup<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> PartialFileCleanup<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PartialFileCleanup<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.path);
        }
    }
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

    let mut cleanup = PartialFileCleanup::new(output_path);
    let mut output = File::create(output_path)?;
    let mut hasher = blake3::Hasher::new();
    let mut remaining = data_len;
    let mut buffer = vec![0_u8; 64 * 1024];

    while remaining > 0 {
        let take = remaining.min(buffer.len());
        reader.read_exact(&mut buffer[..take]).await?;
        output.write_all(&buffer[..take])?;
        hasher.update(&buffer[..take]);
        remaining -= take;
    }

    output.flush()?;
    cleanup.disarm();

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
    let data = read_exact_incremental(reader, data_len, &[]).await?;

    Ok(WireMessage::Chunk { index, data })
}

async fn receive_encoded_frame_after_first_byte<R>(
    reader: &mut R,
    frame_len: usize,
    first_byte: u8,
    max_len: usize,
) -> Result<WireMessage, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let payload = read_exact_incremental(reader, frame_len, &[first_byte]).await?;

    let (message, bytes_read): (WireMessage, usize) = match max_len {
        MAX_CONTROL_FRAME_SIZE => bincode_next::serde::decode_from_slice(
            &payload,
            bincode_next::config::standard().with_limit::<MAX_CONTROL_FRAME_SIZE>(),
        )?,
        _ => bincode_next::serde::decode_from_slice(
            &payload,
            bincode_next::config::standard().with_limit::<MAX_FRAME_SIZE>(),
        )?,
    };

    if bytes_read != payload.len() {
        return Err(ProtocolError::TrailingBytes {
            bytes_read,
            frame_len: payload.len(),
        });
    }

    Ok(message)
}

async fn read_exact_incremental<R>(
    reader: &mut R,
    total_len: usize,
    prefix: &[u8],
) -> Result<Vec<u8>, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    debug_assert!(prefix.len() <= total_len);
    let mut payload = Vec::with_capacity(total_len.min(INCREMENTAL_READ_CHUNK_SIZE));
    payload.extend_from_slice(prefix);

    while payload.len() < total_len {
        let remaining = total_len - payload.len();
        let take = remaining.min(INCREMENTAL_READ_CHUNK_SIZE);
        let old_len = payload.len();
        payload.resize(old_len + take, 0);
        let read_bytes = reader.read(&mut payload[old_len..]).await?;
        if read_bytes == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "early eof while reading protocol frame payload",
            )
            .into());
        }
        payload.truncate(old_len + read_bytes);
    }

    Ok(payload)
}

async fn read_frame_len_with_max<R>(reader: &mut R, max_len: usize) -> Result<usize, ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut len_bytes = [0_u8; 4];
    reader.read_exact(&mut len_bytes).await?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    validate_frame_len_with_max(len, max_len)?;
    Ok(len)
}

fn validate_frame_len(len: usize) -> Result<(), ProtocolError> {
    validate_frame_len_with_max(len, MAX_FRAME_SIZE)
}

fn validate_frame_len_with_max(len: usize, max: usize) -> Result<(), ProtocolError> {
    if len == 0 {
        return Err(ProtocolError::EmptyFrame);
    }

    if len > max {
        return Err(ProtocolError::FrameTooLarge { len, max });
    }

    Ok(())
}
