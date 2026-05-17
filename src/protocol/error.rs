use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),

    #[error("serialization decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),

    #[error("empty protocol frame")]
    EmptyFrame,

    #[error("protocol frame too large: {len} bytes > {max} bytes")]
    FrameTooLarge { len: usize, max: usize },

    #[error("protocol frame has trailing bytes: decoded {bytes_read} bytes from {frame_len} bytes")]
    TrailingBytes { bytes_read: usize, frame_len: usize },
}
