use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialize(#[from] Box<bincode::ErrorKind>),

    #[error("empty protocol frame")]
    EmptyFrame,

    #[error("protocol frame too large: {len} bytes > {max} bytes")]
    FrameTooLarge { len: usize, max: usize },
}
