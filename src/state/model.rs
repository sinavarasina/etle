use super::prelude::*;

pub const LIBRARY_DIR_NAME: &str = "library";
pub const DESCRIPTOR_FILE_NAME: &str = "descriptor.etle";
pub const SECRET_FILE_NAME: &str = "secret.etlekey";
pub const PROGRESS_FILE_NAME: &str = "progress.bin";
pub const STATE_FILE_NAME: &str = "state.bin";
pub const CHUNKS_DIR_NAME: &str = "chunks";
pub const OUTPUT_DIR_NAME: &str = "output";
pub const CHUNK_EXTENSION: &str = "etle";
pub const ETLE_LIBRARY_ROOT_ENV: &str = "ETLE_LIBRARY_ROOT";
pub const DOWNLOADS_DIR_NAME: &str = "Downloads";
pub const ETLE_DOWNLOADS_DIR_NAME: &str = "ETLE";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareMode {
    Seeding,
    Downloading,
    Completed,
    Paused,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtleSecret {
    pub share_id: ShareId,
    pub file_key: SymmetricKey,
}

impl std::fmt::Debug for EtleSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EtleSecret")
            .field("share_id", &self.share_id)
            .field("file_key", &"<redacted>")
            .finish()
    }
}

impl EtleSecret {
    #[must_use]
    pub const fn new(share_id: ShareId, file_key: SymmetricKey) -> Self {
        Self { share_id, file_key }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
        bincode_next::serde::encode_to_vec(self, bincode_next::config::standard())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode_next::error::DecodeError> {
        let (secret, bytes_read): (Self, usize) =
            bincode_next::serde::decode_from_slice(bytes, bincode_next::config::standard())?;

        if bytes_read != bytes.len() {
            return Err(bincode_next::error::DecodeError::OtherString(format!(
                "secret has trailing bytes: decoded {bytes_read} of {} bytes",
                bytes.len()
            )));
        }

        Ok(secret)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub share_id: ShareId,
    pub completed_chunks: Vec<u32>,
}

impl DownloadProgress {
    #[must_use]
    pub fn new(share_id: ShareId, completed_chunks: impl Into<Vec<u32>>) -> Self {
        let mut completed_chunks = completed_chunks.into();
        normalize_chunk_list(&mut completed_chunks);

        Self {
            share_id,
            completed_chunks,
        }
    }

    #[must_use]
    pub fn empty(share_id: ShareId) -> Self {
        Self::new(share_id, Vec::new())
    }

    pub fn mark_completed(&mut self, index: u32) {
        match self.completed_chunks.binary_search(&index) {
            Ok(_) => {}
            Err(position) => self.completed_chunks.insert(position, index),
        }
    }

    #[must_use]
    pub fn has_chunk(&self, index: u32) -> bool {
        self.completed_chunks.binary_search(&index).is_ok()
    }

    #[must_use]
    pub fn is_complete(&self, total_chunks: usize) -> bool {
        self.completed_chunks.len() == total_chunks
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
        super::codec::encode_progress(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode_next::error::DecodeError> {
        super::codec::decode_progress(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareState {
    pub share_id: ShareId,
    pub mode: ShareMode,
    pub output_dir: Option<PathBuf>,
    pub completed_chunks: Vec<u32>,
}

impl ShareState {
    #[must_use]
    pub fn new(
        share_id: ShareId,
        mode: ShareMode,
        output_dir: Option<PathBuf>,
        completed_chunks: impl Into<Vec<u32>>,
    ) -> Self {
        let mut completed_chunks = completed_chunks.into();
        normalize_chunk_list(&mut completed_chunks);

        Self {
            share_id,
            mode,
            output_dir,
            completed_chunks,
        }
    }

    #[must_use]
    pub fn from_progress(
        mode: ShareMode,
        output_dir: Option<PathBuf>,
        progress: &DownloadProgress,
    ) -> Self {
        Self::new(
            progress.share_id,
            mode,
            output_dir,
            progress.completed_chunks.clone(),
        )
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode_next::error::EncodeError> {
        super::codec::encode_state(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode_next::error::DecodeError> {
        super::codec::decode_state(bytes)
    }
}

fn normalize_chunk_list(chunks: &mut Vec<u32>) {
    chunks.sort_unstable();
    chunks.dedup();
}
