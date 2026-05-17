use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(#[from] crate::protocol::ProtocolError),

    #[error("crypto error: {0}")]
    Crypto(#[from] crate::crypto::error::CryptoError),

    #[error("expected hello message during handshake")]
    ExpectedHello,

    #[error("expected key exchange message during handshake")]
    ExpectedKeyExchange,
}
