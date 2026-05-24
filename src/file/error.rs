use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::error::CryptoError),

    #[error("serialization encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),

    #[error("serialization decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),

    #[error("invalid chunk size: {0}")]
    InvalidChunkSize(usize),

    #[error("too many chunks for u32 chunk index space")]
    TooManyChunks,

    #[error("package is too large to represent safely")]
    PackageTooLarge,

    #[error("missing encrypted chunk with index {0}")]
    MissingChunk(u32),

    #[error("chunk hash mismatch at index {0}")]
    ChunkHashMismatch(u32),

    #[error("chunk size mismatch at index {index}: expected {expected} bytes, got {actual} bytes")]
    ChunkSizeMismatch {
        index: u32,
        expected: u64,
        actual: u64,
    },

    #[error("invalid package input path: {0:?}")]
    InvalidPackageInput(PathBuf),

    #[error("package path is outside root: path={path:?}, root={root:?}")]
    PathOutsideRoot { path: PathBuf, root: PathBuf },

    #[error("package contains no files")]
    EmptyPackage,

    #[error("share id mismatch: expected {expected}, got {actual}")]
    ShareIdMismatch {
        expected: crate::file::descriptor::ShareId,
        actual: crate::file::descriptor::ShareId,
    },

    #[error("final file hash mismatch")]
    FinalHashMismatch,
}
