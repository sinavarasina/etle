use thiserror::Error;

use crate::{
    crypto::error::CryptoError,
    file::error::FileError,
    protocol::{error::ProtocolError, message::WireMessage},
};

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("file error: {0}")]
    File(#[from] FileError),

    #[error("unexpected wire message: expected {expected}, got {actual:?}")]
    UnexpectedMessage {
        expected: &'static str,
        actual: WireMessage,
    },

    #[error("missing encrypted chunk with index {0}")]
    MissingEncryptedChunk(u32),

    #[error("unexpected chunk index: expected {expected}, got {actual}")]
    UnexpectedChunkIndex { expected: u32, actual: u32 },

    #[error("state seeding currently supports exactly one file entry, got {0}")]
    UnsupportedMultiFileDescriptor(usize),

    #[error("at least one peer address is required")]
    NoPeersProvided,

    #[error(
        "all peer download attempts failed after {attempts} attempt(s); last error: {last_error}"
    )]
    AllPeersFailed { attempts: usize, last_error: String },

    #[error("unsupported peer protocol version: peer={peer}, supported={supported}")]
    UnsupportedProtocolVersion { peer: u16, supported: u16 },

    #[error("peer is missing required capability: {0}")]
    MissingPeerCapability(String),

    #[error("peer error: {0}")]
    PeerError(String),
}
