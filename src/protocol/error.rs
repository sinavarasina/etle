use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization encode error: {0}")]
    Encode(#[from] bincode_next::error::EncodeError),

    #[error("serialization decode error: {0}")]
    Decode(#[from] bincode_next::error::DecodeError),

    #[error("empty protocol frame")]
    EmptyFrame,

    #[error("protocol frame too large: {len} bytes > {max} bytes")]
    FrameTooLarge { len: usize, max: usize },

    #[error("protocol frame too small: {len} bytes < {min} bytes")]
    FrameTooSmall { len: usize, min: usize },

    #[error("raw chunk frame size mismatch: expected {expected} bytes, got {actual} bytes")]
    RawChunkSizeMismatch { expected: usize, actual: usize },

    #[error("protocol frame has trailing bytes: decoded {bytes_read} bytes from {frame_len} bytes")]
    TrailingBytes { bytes_read: usize, frame_len: usize },

    #[error("invalid raw chunk frame: {0}")]
    InvalidRawChunkFrame(&'static str),
}
