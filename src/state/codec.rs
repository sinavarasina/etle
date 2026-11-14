use serde::de::DeserializeOwned;

use crate::file::manifest::MAX_METADATA_DECODE_BYTES;

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
        let normalized =
            DownloadProgress::new(progress.share_id, progress.completed_chunks.clone());
        Self {
            share_id: normalized.share_id,
            completed_count: normalized.completed_chunks.len() as u64,
            completed_bitmap: encode_completed_bitmap(&normalized.completed_chunks),
        }
    }

    fn into_progress(self) -> Result<DownloadProgress, bincode_next::error::DecodeError> {
        let completed_chunks =
            decode_completed_bitmap(&self.completed_bitmap, self.completed_count)?;
        Ok(DownloadProgress::new(self.share_id, completed_chunks))
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
        let normalized = ShareState::new(
            state.share_id,
            state.mode,
            state.output_dir.clone(),
            state.completed_chunks.clone(),
        );
        Self {
            share_id: normalized.share_id,
            mode: normalized.mode,
            output_dir: normalized.output_dir,
            completed_count: normalized.completed_chunks.len() as u64,
            completed_bitmap: encode_completed_bitmap(&normalized.completed_chunks),
        }
    }

    fn into_state(self) -> Result<ShareState, bincode_next::error::DecodeError> {
        let completed_chunks =
            decode_completed_bitmap(&self.completed_bitmap, self.completed_count)?;
        Ok(ShareState::new(
            self.share_id,
            self.mode,
            self.output_dir,
            completed_chunks,
        ))
    }
}

pub(super) fn encode_progress(
    progress: &DownloadProgress,
) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
    encode_with_magic(
        COMPACT_PROGRESS_MAGIC,
        &CompactDownloadProgress::from_progress(progress),
    )
}

pub(super) fn encode_state(
    state: &ShareState,
) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
    encode_with_magic(COMPACT_STATE_MAGIC, &CompactShareState::from_state(state))
}

pub(super) fn decode_progress(
    bytes: &[u8],
) -> Result<DownloadProgress, bincode_next::error::DecodeError> {
    if let Some(payload) = bytes.strip_prefix(COMPACT_PROGRESS_MAGIC) {
        let progress: CompactDownloadProgress = decode_exact(payload, "compact progress")?;
        return progress.into_progress();
    }

    let progress: DownloadProgress = decode_exact(bytes, "legacy progress")?;
    Ok(DownloadProgress::new(
        progress.share_id,
        progress.completed_chunks,
    ))
}

pub(super) fn decode_state(bytes: &[u8]) -> Result<ShareState, bincode_next::error::DecodeError> {
    if let Some(payload) = bytes.strip_prefix(COMPACT_STATE_MAGIC) {
        let state: CompactShareState = decode_exact(payload, "compact state")?;
        return state.into_state();
    }

    let state: ShareState = decode_exact(bytes, "legacy state")?;
    Ok(ShareState::new(
        state.share_id,
        state.mode,
        state.output_dir,
        state.completed_chunks,
    ))
}

fn decode_exact<T>(bytes: &[u8], label: &str) -> Result<T, bincode_next::error::DecodeError>
where
    T: DeserializeOwned,
{
    if bytes.len() > MAX_METADATA_DECODE_BYTES {
        return Err(bincode_next::error::DecodeError::LimitExceeded);
    }

    let (value, bytes_read): (T, usize) = bincode_next::serde::decode_from_slice(
        bytes,
        bincode_next::config::standard().with_limit::<MAX_METADATA_DECODE_BYTES>(),
    )?;

    if bytes_read != bytes.len() {
        return Err(bincode_next::error::DecodeError::OtherString(format!(
            "{label} has trailing bytes: decoded {bytes_read} of {} bytes",
            bytes.len()
        )));
    }

    Ok(value)
}

fn encode_with_magic<T: Serialize>(
    magic: &[u8; 8],
    value: &T,
) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
    let payload = bincode_next::serde::encode_to_vec(value, bincode_next::config::standard())?;
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

fn decode_completed_bitmap(
    bitmap: &[u8],
    expected_count: u64,
) -> Result<Vec<u32>, bincode_next::error::DecodeError> {
    if bitmap.last() == Some(&0) {
        return Err(bincode_next::error::DecodeError::OtherString(
            "compact completed bitmap contains trailing zero bytes".to_string(),
        ));
    }

    let mut chunks = Vec::new();

    for (byte_index, byte) in bitmap.iter().copied().enumerate() {
        if byte == 0 {
            continue;
        }

        for bit in 0..8 {
            if byte & (1_u8 << bit) == 0 {
                continue;
            }
            let chunk_index = byte_index
                .checked_mul(8)
                .and_then(|base| base.checked_add(bit))
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    bincode_next::error::DecodeError::OtherString(
                        "compact completed bitmap chunk index overflows u32".to_string(),
                    )
                })?;
            chunks.push(chunk_index);
        }
    }

    if chunks.len() as u64 != expected_count {
        return Err(bincode_next::error::DecodeError::OtherString(format!(
            "compact completed count mismatch: header claims {expected_count}, bitmap contains {}",
            chunks.len()
        )));
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupt_compact_bitmap_count_mismatch() {
        let compact = CompactDownloadProgress {
            share_id: ShareId([7_u8; 32]),
            completed_count: 1,
            completed_bitmap: vec![0b0000_0011],
        };
        let payload =
            bincode_next::serde::encode_to_vec(&compact, bincode_next::config::standard()).unwrap();
        let mut encoded = Vec::from(COMPACT_PROGRESS_MAGIC.as_slice());
        encoded.extend_from_slice(&payload);

        assert!(decode_progress(&encoded).is_err());
    }

    #[test]
    fn rejects_trailing_zero_bytes_in_compact_bitmap() {
        let compact = CompactDownloadProgress {
            share_id: ShareId([8_u8; 32]),
            completed_count: 1,
            completed_bitmap: vec![0x01, 0x00],
        };
        let payload =
            bincode_next::serde::encode_to_vec(&compact, bincode_next::config::standard()).unwrap();
        let mut encoded = Vec::from(COMPACT_PROGRESS_MAGIC.as_slice());
        encoded.extend_from_slice(&payload);

        assert!(decode_progress(&encoded).is_err());
    }
}
