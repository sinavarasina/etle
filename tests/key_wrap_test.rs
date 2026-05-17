use etle::crypto::{
    aead::SymmetricKey,
    hash::FileId,
    key_exchange::{EphemeralKeypair, derive_session_key},
    key_wrap::{generate_file_key, unwrap_file_key, wrap_file_key},
};

#[test]
fn peers_can_wrap_and_unwrap_reusable_file_key() {
    let alice = EphemeralKeypair::generate();
    let bob = EphemeralKeypair::generate();

    let alice_public = alice.public_key();
    let bob_public = bob.public_key();

    let alice_session_key = derive_session_key(alice.diffie_hellman(bob_public).unwrap());
    let bob_session_key = derive_session_key(bob.diffie_hellman(alice_public).unwrap());

    assert_eq!(alice_session_key, bob_session_key);

    let file_id = FileId([11_u8; 32]);
    let file_key = generate_file_key();
    let wrapped = wrap_file_key(&alice_session_key, file_id, &file_key).unwrap();
    let unwrapped = unwrap_file_key(&bob_session_key, file_id, &wrapped).unwrap();

    assert_eq!(unwrapped, file_key);
}

#[test]
fn wrong_session_key_cannot_unwrap_file_key() {
    let file_id = FileId([12_u8; 32]);
    let file_key = generate_file_key();
    let session_key = SymmetricKey([1_u8; 32]);
    let wrong_session_key = SymmetricKey([2_u8; 32]);

    let wrapped = wrap_file_key(&session_key, file_id, &file_key).unwrap();

    assert!(unwrap_file_key(&wrong_session_key, file_id, &wrapped).is_err());
}
