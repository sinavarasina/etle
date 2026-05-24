use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid config line {line}: {message}")]
    InvalidLine { line: usize, message: String },

    #[error("invalid value for config key `{key}` on line {line}: {value}")]
    InvalidValue {
        line: usize,
        key: String,
        value: String,
    },

    #[error("unknown config key `{key}` on line {line}")]
    UnknownKey { line: usize, key: String },
}
