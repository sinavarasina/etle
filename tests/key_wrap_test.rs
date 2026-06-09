mod common;

use common::{print_banner, print_kv, print_result, print_step};
use etle::crypto::{
    aead::SymmetricKey,
    hash::FileId,
    key_exchange::{EphemeralKeypair, derive_session_key},
    key_wrap::{generate_file_key, unwrap_file_key, wrap_file_key},
};

#[test]
fn peers_can_wrap_and_unwrap_reusable_file_key() {
    print_banner("peers_can_wrap_and_unwrap_reusable_file_key");

    print_step(1, "derive matching session keys");
    let alice = EphemeralKeypair::generate();
    let bob = EphemeralKeypair::generate();
    let alice_public = alice.public_key();
    let bob_public = bob.public_key();
    let alice_session_key = derive_session_key(alice.diffie_hellman(bob_public).unwrap());
    let bob_session_key = derive_session_key(bob.diffie_hellman(alice_public).unwrap());
    print_kv("session_keys_equal", alice_session_key == bob_session_key);
    assert_eq!(alice_session_key, bob_session_key);

    print_step(2, "wrap file key with session key");
    let file_id = FileId([11_u8; 32]);
    let file_key = generate_file_key();
    let wrapped = wrap_file_key(&alice_session_key, file_id, &file_key).unwrap();
    print_kv("file_id", file_id);
    print_kv("wrapped_key_len", wrapped.data.len());

    print_step(3, "unwrap from peer side");
    let unwrapped = unwrap_file_key(&bob_session_key, file_id, &wrapped).unwrap();
    print_kv("unwrapped_matches", unwrapped == file_key);
    assert_eq!(unwrapped, file_key);

    print_result("peers_can_wrap_and_unwrap_reusable_file_key", "ok");
}

#[test]
fn wrong_session_key_cannot_unwrap_file_key() {
    print_banner("wrong_session_key_cannot_unwrap_file_key");

    print_step(1, "wrap using correct session key");
    let file_id = FileId([12_u8; 32]);
    let file_key = generate_file_key();
    let session_key = SymmetricKey([1_u8; 32]);
    let wrong_session_key = SymmetricKey([2_u8; 32]);
    let wrapped = wrap_file_key(&session_key, file_id, &file_key).unwrap();
    print_kv("wrapped_key_len", wrapped.data.len());

    print_step(2, "attempt unwrap using wrong session key");
    let result = unwrap_file_key(&wrong_session_key, file_id, &wrapped);
    print_kv("wrong_key_rejected", result.is_err());
    assert!(result.is_err());

    print_result("wrong_session_key_cannot_unwrap_file_key", "ok");
}
