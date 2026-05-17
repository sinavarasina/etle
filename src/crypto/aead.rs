use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::crypto::{error::CryptoError, hash::FileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetricKey(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nonce(pub [u8; 24]);

impl SymmetricKey {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Nonce {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 24] {
        &self.0
    }
}

#[must_use]
pub fn generate_nonce() -> Nonce {
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);
    Nonce(nonce)
}

#[must_use]
pub fn build_chunk_aad(file_id: FileId, chunk_index: u32, plain_size: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + 4 + 8);
    aad.extend_from_slice(file_id.as_bytes());
    aad.extend_from_slice(&chunk_index.to_le_bytes());
    aad.extend_from_slice(&plain_size.to_le_bytes());
    aad
}

pub fn encrypt_chunk(
    key: &SymmetricKey,
    nonce: Nonce,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let nonce = XNonce::from_slice(nonce.as_bytes());

    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encrypt)
}

pub fn decrypt_chunk(
    key: &SymmetricKey,
    nonce: Nonce,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let nonce = XNonce::from_slice(nonce.as_bytes());

    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> SymmetricKey {
        SymmetricKey([7_u8; 32])
    }

    fn test_file_id() -> FileId {
        FileId([9_u8; 32])
    }

    #[test]
    fn encrypt_then_decrypt_chunk() {
        let key = test_key();
        let nonce = generate_nonce();
        let plaintext = b"encrypted negi";
        let aad = build_chunk_aad(test_file_id(), 0, plaintext.len() as u64);

        let ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();
        let decrypted = decrypt_chunk(&key, nonce, &ciphertext, &aad).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = test_key();
        let nonce = generate_nonce();
        let plaintext = b"do not tamper";
        let aad = build_chunk_aad(test_file_id(), 0, plaintext.len() as u64);

        let mut ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();
        ciphertext[0] ^= 0xff;

        assert!(decrypt_chunk(&key, nonce, &ciphertext, &aad).is_err());
    }

    #[test]
    fn wrong_aad_fails() {
        let key = test_key();
        let nonce = generate_nonce();
        let plaintext = b"aad binds chunk metadata";
        let aad = build_chunk_aad(test_file_id(), 0, plaintext.len() as u64);
        let wrong_aad = build_chunk_aad(test_file_id(), 1, plaintext.len() as u64);

        let ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();

        assert!(decrypt_chunk(&key, nonce, &ciphertext, &wrong_aad).is_err());
    }

    #[test]
    fn wrong_nonce_fails() {
        let key = test_key();
        let nonce = generate_nonce();
        let wrong_nonce = generate_nonce();
        let plaintext = b"nonce must be unique and correct";
        let aad = build_chunk_aad(test_file_id(), 0, plaintext.len() as u64);

        let ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();

        assert!(decrypt_chunk(&key, wrong_nonce, &ciphertext, &aad).is_err());
    }
}
