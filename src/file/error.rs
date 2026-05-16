use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::error::CryptoError),

    #[error("serialization error: {0}")]
    Serialize(#[from] Box<bincode::ErrorKind>),

    #[error("invalid chunk size: {0}")]
    InvalidChunkSize(usize),

    #[error("missing encrypted chunk with index {0}")]
    MissingChunk(u32),

    #[error("chunk hash mismatch at index {0}")]
    ChunkHashMismatch(u32),

    #[error("chunk size mismatch at index {index}: expected {expected} bytes, got {actual} bytes")]
    ChunkSizeMismatch { index: u32, expected: u64, actual: u64 },

    #[error("final file hash mismatch")]
    FinalHashMismatch,
}
