use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::crypto::{
    aead::{Nonce, SymmetricKey, decrypt_chunk, encrypt_chunk, generate_nonce},
    error::CryptoError,
    hash::FileId,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedFileKey {
    pub nonce: Nonce,
    pub data: Vec<u8>,
}

#[must_use]
pub fn generate_file_key() -> SymmetricKey {
    let mut key = [0_u8; 32];
    OsRng.fill_bytes(&mut key);
    SymmetricKey(key)
}

pub fn wrap_file_key(
    session_key: &SymmetricKey,
    file_id: FileId,
    file_key: &SymmetricKey,
) -> Result<WrappedFileKey, CryptoError> {
    let nonce = generate_nonce();
    let aad = build_file_key_aad(file_id);
    let data = encrypt_chunk(session_key, nonce, file_key.as_bytes(), &aad)?;

    Ok(WrappedFileKey { nonce, data })
}

pub fn unwrap_file_key(
    session_key: &SymmetricKey,
    file_id: FileId,
    wrapped: &WrappedFileKey,
) -> Result<SymmetricKey, CryptoError> {
    let aad = build_file_key_aad(file_id);
    let bytes = decrypt_chunk(session_key, wrapped.nonce, &wrapped.data, &aad)?;

    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CryptoError::InvalidWrappedKeyLength)?;

    Ok(SymmetricKey(key_bytes))
}

#[must_use]
pub fn build_file_key_aad(file_id: FileId) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + 16);
    aad.extend_from_slice(b"etle-file-key-v1");
    aad.extend_from_slice(file_id.as_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session_key() -> SymmetricKey {
        SymmetricKey([3_u8; 32])
    }

    fn test_file_id() -> FileId {
        FileId([7_u8; 32])
    }

    #[test]
    fn generated_file_keys_are_not_constant() {
        let first = generate_file_key();
        let second = generate_file_key();

        assert_ne!(first, second);
    }

    #[test]
    fn wrap_then_unwrap_file_key() {
        let file_key = generate_file_key();
        let wrapped = wrap_file_key(&test_session_key(), test_file_id(), &file_key).unwrap();
        let unwrapped = unwrap_file_key(&test_session_key(), test_file_id(), &wrapped).unwrap();

        assert_eq!(unwrapped, file_key);
    }

    #[test]
    fn wrong_file_id_fails_to_unwrap() {
        let file_key = generate_file_key();
        let wrapped = wrap_file_key(&test_session_key(), test_file_id(), &file_key).unwrap();
        let wrong_file_id = FileId([8_u8; 32]);

        assert!(unwrap_file_key(&test_session_key(), wrong_file_id, &wrapped).is_err());
    }
}
