use serde::{Deserialize, Serialize};
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::crypto::{aead::SymmetricKey, error::CryptoError, hash::FileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyBytes(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedSecretBytes(pub [u8; 32]);

pub struct EphemeralKeypair {
    secret: EphemeralSecret,
    public: PublicKeyBytes,
}

impl EphemeralKeypair {
    #[must_use]
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(rand_core::OsRng);
        let public = PublicKey::from(&secret);

        Self {
            secret,
            public: PublicKeyBytes(public.to_bytes()),
        }
    }

    #[must_use]
    pub fn public_key(&self) -> PublicKeyBytes {
        self.public
    }

    pub fn diffie_hellman(
        self,
        peer_public_key: PublicKeyBytes,
    ) -> Result<SharedSecretBytes, CryptoError> {
        let peer_public = PublicKey::from(peer_public_key.0);
        let shared = self.secret.diffie_hellman(&peer_public);

        Ok(SharedSecretBytes(shared.to_bytes()))
    }
}

#[must_use]
pub fn derive_file_key(shared_secret: SharedSecretBytes, file_id: FileId) -> SymmetricKey {
    let mut material = Vec::with_capacity(64);
    material.extend_from_slice(&shared_secret.0);
    material.extend_from_slice(file_id.as_bytes());

    SymmetricKey(blake3::derive_key("etle v1 x25519 file key", &material))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_derive_same_shared_secret() {
        let alice = EphemeralKeypair::generate();
        let bob = EphemeralKeypair::generate();

        let alice_public = alice.public_key();
        let bob_public = bob.public_key();

        let alice_secret = alice.diffie_hellman(bob_public).unwrap();
        let bob_secret = bob.diffie_hellman(alice_public).unwrap();

        assert_eq!(alice_secret, bob_secret);
    }

    #[test]
    fn same_secret_and_file_id_derives_same_file_key() {
        let shared_secret = SharedSecretBytes([42_u8; 32]);
        let file_id = FileId([11_u8; 32]);

        let first = derive_file_key(shared_secret, file_id);
        let second = derive_file_key(shared_secret, file_id);

        assert_eq!(first, second);
    }

    #[test]
    fn different_file_id_derives_different_file_key() {
        let shared_secret = SharedSecretBytes([42_u8; 32]);
        let first = derive_file_key(shared_secret, FileId([1_u8; 32]));
        let second = derive_file_key(shared_secret, FileId([2_u8; 32]));

        assert_ne!(first, second);
    }
}
