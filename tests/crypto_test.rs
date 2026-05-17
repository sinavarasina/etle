use etle::crypto::{
    aead::{SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk, generate_nonce},
    hash::{FileId, hash_bytes},
    key_exchange::{EphemeralKeypair, derive_file_key},
};

#[test]
fn blake3_hash_is_deterministic() {
    assert_eq!(hash_bytes(b"same"), hash_bytes(b"same"));
    assert_ne!(hash_bytes(b"same"), hash_bytes(b"different"));
}

#[test]
fn aead_roundtrip_and_tamper_detection() {
    let key = SymmetricKey([1_u8; 32]);
    let nonce = generate_nonce();
    let file_id = FileId([2_u8; 32]);
    let plaintext = b"hello encrypted torrent-like chunk";
    let aad = build_chunk_aad(file_id, 7, plaintext.len() as u64);

    let mut ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();
    let decrypted = decrypt_chunk(&key, nonce, &ciphertext, &aad).unwrap();
    assert_eq!(decrypted, plaintext);

    ciphertext[0] ^= 0xff;
    assert!(decrypt_chunk(&key, nonce, &ciphertext, &aad).is_err());
}

#[test]
fn x25519_peers_derive_same_file_key() {
    let alice = EphemeralKeypair::generate();
    let bob = EphemeralKeypair::generate();

    let alice_public = alice.public_key();
    let bob_public = bob.public_key();

    let alice_secret = alice.diffie_hellman(bob_public).unwrap();
    let bob_secret = bob.diffie_hellman(alice_public).unwrap();

    let file_id = FileId([9_u8; 32]);
    let alice_key = derive_file_key(alice_secret, file_id);
    let bob_key = derive_file_key(bob_secret, file_id);

    assert_eq!(alice_key, bob_key);
}
