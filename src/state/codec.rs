use super::model::{DownloadProgress, ShareMode, ShareState};
use super::prelude::*;

const COMPACT_PROGRESS_MAGIC: &[u8; 8] = b"ETLEPRG2";
const COMPACT_STATE_MAGIC: &[u8; 8] = b"ETLESTA2";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CompactDownloadProgress {
    share_id: ShareId,
    completed_count: u64,
    completed_bitmap: Vec<u8>,
}

impl CompactDownloadProgress {
    fn from_progress(progress: &DownloadProgress) -> Self {
        Self {
            share_id: progress.share_id,
            completed_count: progress.completed_chunks.len() as u64,
            completed_bitmap: encode_completed_bitmap(&progress.completed_chunks),
        }
    }

    fn into_progress(self) -> DownloadProgress {
        let mut completed_chunks = decode_completed_bitmap(&self.completed_bitmap);
        completed_chunks.truncate(self.completed_count as usize);
        DownloadProgress::new(self.share_id, completed_chunks)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CompactShareState {
    share_id: ShareId,
    mode: ShareMode,
    output_dir: Option<PathBuf>,
    completed_count: u64,
    completed_bitmap: Vec<u8>,
}

impl CompactShareState {
    fn from_state(state: &ShareState) -> Self {
        Self {
            share_id: state.share_id,
            mode: state.mode,
            output_dir: state.output_dir.clone(),
            completed_count: state.completed_chunks.len() as u64,
            completed_bitmap: encode_completed_bitmap(&state.completed_chunks),
        }
    }

    fn into_state(self) -> ShareState {
        let mut completed_chunks = decode_completed_bitmap(&self.completed_bitmap);
        completed_chunks.truncate(self.completed_count as usize);
        ShareState::new(self.share_id, self.mode, self.output_dir, completed_chunks)
    }
}

pub(super) fn encode_progress(
    progress: &DownloadProgress,
) -> Result<Vec<u8>, bincode::error::EncodeError> {
    encode_with_magic(
        COMPACT_PROGRESS_MAGIC,
        &CompactDownloadProgress::from_progress(progress),
    )
}

pub(super) fn encode_state(state: &ShareState) -> Result<Vec<u8>, bincode::error::EncodeError> {
    encode_with_magic(COMPACT_STATE_MAGIC, &CompactShareState::from_state(state))
}

pub(super) fn decode_progress(
    bytes: &[u8],
) -> Result<DownloadProgress, bincode::error::DecodeError> {
    if let Some(payload) = bytes.strip_prefix(COMPACT_PROGRESS_MAGIC) {
        let (progress, _bytes_read): (CompactDownloadProgress, usize) =
            bincode::serde::decode_from_slice(payload, bincode::config::standard())?;
        return Ok(progress.into_progress());
    }

    let (progress, _bytes_read): (DownloadProgress, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

    Ok(progress)
}

pub(super) fn decode_state(bytes: &[u8]) -> Result<ShareState, bincode::error::DecodeError> {
    if let Some(payload) = bytes.strip_prefix(COMPACT_STATE_MAGIC) {
        let (state, _bytes_read): (CompactShareState, usize) =
            bincode::serde::decode_from_slice(payload, bincode::config::standard())?;
        return Ok(state.into_state());
    }

    let (state, _bytes_read): (ShareState, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

    Ok(state)
}

fn encode_with_magic<T: Serialize>(
    magic: &[u8; 8],
    value: &T,
) -> Result<Vec<u8>, bincode::error::EncodeError> {
    let payload = bincode::serde::encode_to_vec(value, bincode::config::standard())?;
    let mut output = Vec::with_capacity(magic.len() + payload.len());
    output.extend_from_slice(magic);
    output.extend_from_slice(&payload);
    Ok(output)
}

fn encode_completed_bitmap(completed_chunks: &[u32]) -> Vec<u8> {
    let Some(max_index) = completed_chunks.iter().copied().max() else {
        return Vec::new();
    };

    let mut bitmap = vec![0_u8; (max_index as usize / 8) + 1];
    for index in completed_chunks {
        let index = *index as usize;
        bitmap[index / 8] |= 1_u8 << (index % 8);
    }
    bitmap
}

fn decode_completed_bitmap(bitmap: &[u8]) -> Vec<u32> {
    let mut chunks = Vec::new();

    for (byte_index, byte) in bitmap.iter().copied().enumerate() {
        if byte == 0 {
            continue;
        }

        for bit in 0..8 {
            if byte & (1_u8 << bit) == 0 {
                continue;
            }
            chunks.push((byte_index * 8 + bit) as u32);
        }
    }

    chunks
}
