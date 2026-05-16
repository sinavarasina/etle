use std::{fs, path::PathBuf};

use etle::{
    crypto::{
        hash::hash_file,
        key_exchange::{derive_file_key, EphemeralKeypair},
    },
    file::storage::{decrypt_to_bytes, encrypt_file},
};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-test-{name}-{}", std::process::id()))
}

#[test]
fn local_encrypted_transfer_roundtrip() {
    let input = temp_file_name("local-transfer.bin");
    let bytes = b"local p2p transfer simulation before tcp exists";
    fs::write(&input, bytes).unwrap();

    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public = seeder.public_key();
    let peer_public = peer.public_key();

    let file_id = hash_file(&input).unwrap();
    let seeder_key = derive_file_key(seeder.diffie_hellman(peer_public).unwrap(), file_id);
    let peer_key = derive_file_key(peer.diffie_hellman(seeder_public).unwrap(), file_id);

    let encrypted = encrypt_file(&input, &seeder_key, 8).unwrap();
    let output = decrypt_to_bytes(&encrypted, &peer_key).unwrap();

    assert_eq!(output, bytes);
    assert_eq!(encrypted.manifest.file_id, file_id);

    fs::remove_file(input).unwrap();
}
