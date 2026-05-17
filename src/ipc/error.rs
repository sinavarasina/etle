use thiserror::Error;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),

    #[error("serialization decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),

    #[error("empty IPC frame")]
    EmptyFrame,

    #[error("IPC frame too large: {len} bytes > {max} bytes")]
    FrameTooLarge { len: usize, max: usize },

    #[error("IPC frame has trailing bytes: decoded {bytes_read} bytes from {frame_len} bytes")]
    TrailingBytes { bytes_read: usize, frame_len: usize },

    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(&'static str),
}
