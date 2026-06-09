mod common;

use common::{hex_prefix, print_banner, print_kv, print_result, print_step};
use etle::crypto::{
    aead::{SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk, generate_nonce},
    hash::{FileId, hash_bytes},
    key_exchange::{EphemeralKeypair, derive_file_key},
};

#[test]
fn blake3_hash_is_deterministic() {
    print_banner("blake3_hash_is_deterministic");

    print_step(1, "hash identical inputs");
    let first = hash_bytes(b"same");
    let second = hash_bytes(b"same");
    print_kv("first", hex::encode(first));
    print_kv("second", hex::encode(second));
    print_kv("same_input_equal", first == second);
    assert_eq!(first, second);

    print_step(2, "hash different input");
    let different = hash_bytes(b"different");
    print_kv("different", hex::encode(different));
    print_kv("different_input_not_equal", first != different);
    assert_ne!(first, different);

    print_result("blake3_hash_is_deterministic", "ok");
}

#[test]
fn aead_roundtrip_and_tamper_detection() {
    print_banner("aead_roundtrip_and_tamper_detection");

    let key = SymmetricKey([1_u8; 32]);
    let nonce = generate_nonce();
    let file_id = FileId([2_u8; 32]);
    let plaintext = b"hello encrypted torrent-like chunk";

    print_step(1, "build authenticated metadata");
    let aad = build_chunk_aad(file_id, 7, plaintext.len() as u64);
    print_kv("file_id", file_id);
    print_kv("chunk_index", 7);
    print_kv("plain_size", plaintext.len());
    print_kv("aad", hex::encode(&aad));

    print_step(2, "encrypt and decrypt normally");
    let mut ciphertext = encrypt_chunk(&key, nonce, plaintext, &aad).unwrap();
    let decrypted = decrypt_chunk(&key, nonce, &ciphertext, &aad).unwrap();
    print_kv("ciphertext_len", ciphertext.len());
    print_kv("ciphertext_prefix", hex_prefix(&ciphertext, 16));
    print_kv("roundtrip_matches", decrypted == plaintext);
    assert_eq!(decrypted, plaintext);

    print_step(3, "tamper with ciphertext");
    ciphertext[0] ^= 0xff;
    let tamper_result = decrypt_chunk(&key, nonce, &ciphertext, &aad);
    print_kv("tampered_decrypt_rejected", tamper_result.is_err());
    assert!(tamper_result.is_err());

    print_result("aead_roundtrip_and_tamper_detection", "ok");
}

#[test]
fn x25519_peers_derive_same_file_key() {
    print_banner("x25519_peers_derive_same_file_key");

    print_step(1, "generate ephemeral peers");
    let alice = EphemeralKeypair::generate();
    let bob = EphemeralKeypair::generate();
    let alice_public = alice.public_key();
    let bob_public = bob.public_key();
    print_kv("alice_public", hex::encode(alice_public.0));
    print_kv("bob_public", hex::encode(bob_public.0));

    print_step(2, "derive shared secrets from both sides");
    let alice_secret = alice.diffie_hellman(bob_public).unwrap();
    let bob_secret = bob.diffie_hellman(alice_public).unwrap();
    print_kv("shared_secret_equal", alice_secret == bob_secret);

    print_step(3, "derive file-bound keys");
    let file_id = FileId([9_u8; 32]);
    let alice_key = derive_file_key(alice_secret, file_id);
    let bob_key = derive_file_key(bob_secret, file_id);
    print_kv("file_id", file_id);
    print_kv("file_key_equal", alice_key == bob_key);

    assert_eq!(alice_key, bob_key);
    print_result("x25519_peers_derive_same_file_key", "ok");
}
