use serde::{Deserialize, Serialize};
use std::fmt;
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::crypto::{aead::SymmetricKey, error::CryptoError, hash::FileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyBytes(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SharedSecretBytes(pub [u8; 32]);

impl fmt::Debug for SharedSecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SharedSecretBytes(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthPsk([u8; 32]);

impl AuthPsk {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn from_passphrase(passphrase: impl AsRef<[u8]>) -> Self {
        Self(blake3::derive_key(
            "etle v1 psk authentication passphrase",
            passphrase.as_ref(),
        ))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for AuthPsk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthPsk(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthRole {
    Client,
    Server,
}

pub type AuthTag = [u8; 32];

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
        let shared = self.secret.diffie_hellman(&peer_public).to_bytes();

        if shared.iter().all(|byte| *byte == 0) {
            return Err(CryptoError::InvalidPublicKey);
        }

        Ok(SharedSecretBytes(shared))
    }
}

#[must_use]
pub fn derive_session_key(shared_secret: SharedSecretBytes) -> SymmetricKey {
    SymmetricKey(blake3::derive_key(
        "etle v1 x25519 session key",
        &shared_secret.0,
    ))
}

/// Derives a session key that is bound to the X25519 transcript.
///
/// This prevents swapped-role or public-key-order confusion. It still needs an
/// out-of-band authenticator, such as [`AuthPsk`], to stop active MITM attacks.
#[must_use]
pub fn derive_session_key_with_transcript(
    shared_secret: SharedSecretBytes,
    client_public_key: PublicKeyBytes,
    server_public_key: PublicKeyBytes,
) -> SymmetricKey {
    let mut material = Vec::with_capacity(32 + 32 + 32);
    material.extend_from_slice(&shared_secret.0);
    material.extend_from_slice(&client_public_key.0);
    material.extend_from_slice(&server_public_key.0);

    SymmetricKey(blake3::derive_key(
        "etle v1 x25519 transcript-bound session key",
        &material,
    ))
}

/// Computes a PSK proof for the X25519 transcript and derived session key.
///
/// The proof authenticates both the out-of-band PSK and key confirmation for the
/// ephemeral DH result. A MITM can relay a same-transcript proof, but then it
/// does not know the session key; if it changes either public key, the proof no
/// longer verifies without the PSK.
#[must_use]
pub fn derive_auth_tag(
    psk: &AuthPsk,
    session_key: &SymmetricKey,
    client_public_key: PublicKeyBytes,
    server_public_key: PublicKeyBytes,
    role: AuthRole,
) -> AuthTag {
    let mut hasher = blake3::Hasher::new_keyed(psk.as_bytes());
    hasher.update(b"etle v1 psk-authenticated x25519 transcript");
    match role {
        AuthRole::Client => hasher.update(b"client"),
        AuthRole::Server => hasher.update(b"server"),
    };
    hasher.update(session_key.as_bytes());
    hasher.update(&client_public_key.0);
    hasher.update(&server_public_key.0);

    *hasher.finalize().as_bytes()
}

#[must_use]
pub fn auth_tags_equal(left: &AuthTag, right: &AuthTag) -> bool {
    let diff = left
        .iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right));

    diff == 0
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
    fn rejects_all_zero_shared_secret() {
        let alice = EphemeralKeypair::generate();
        let result = alice.diffie_hellman(PublicKeyBytes([0_u8; 32]));

        assert!(matches!(result, Err(CryptoError::InvalidPublicKey)));
    }

    #[test]
    fn same_secret_derives_same_session_key() {
        let shared_secret = SharedSecretBytes([42_u8; 32]);

        let first = derive_session_key(shared_secret);
        let second = derive_session_key(shared_secret);

        assert_eq!(first, second);
    }

    #[test]
    fn psk_auth_tag_verifies_for_same_transcript() {
        let psk = AuthPsk::from_passphrase("correct horse battery staple");
        let shared_secret = SharedSecretBytes([42_u8; 32]);
        let client_public = PublicKeyBytes([1_u8; 32]);
        let server_public = PublicKeyBytes([2_u8; 32]);
        let session_key =
            derive_session_key_with_transcript(shared_secret, client_public, server_public);

        let tag = derive_auth_tag(
            &psk,
            &session_key,
            client_public,
            server_public,
            AuthRole::Client,
        );
        let expected = derive_auth_tag(
            &psk,
            &session_key,
            client_public,
            server_public,
            AuthRole::Client,
        );

        assert!(auth_tags_equal(&tag, &expected));
    }

    #[test]
    fn psk_auth_tag_changes_for_wrong_psk() {
        let good_psk = AuthPsk::from_passphrase("correct horse battery staple");
        let wrong_psk = AuthPsk::from_passphrase("hunter2");
        let shared_secret = SharedSecretBytes([42_u8; 32]);
        let client_public = PublicKeyBytes([1_u8; 32]);
        let server_public = PublicKeyBytes([2_u8; 32]);
        let session_key =
            derive_session_key_with_transcript(shared_secret, client_public, server_public);

        let tag = derive_auth_tag(
            &good_psk,
            &session_key,
            client_public,
            server_public,
            AuthRole::Client,
        );
        let expected = derive_auth_tag(
            &wrong_psk,
            &session_key,
            client_public,
            server_public,
            AuthRole::Client,
        );

        assert!(!auth_tags_equal(&tag, &expected));
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
