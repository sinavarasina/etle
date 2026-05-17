use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("AEAD encryption failed")]
    Encrypt,

    #[error("AEAD decryption failed")]
    Decrypt,

    #[error("invalid public key")]
    InvalidPublicKey,

    #[error("invalid wrapped file key length")]
    InvalidWrappedKeyLength,
}
