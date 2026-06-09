mod common;

use std::{fs, path::PathBuf};

use common::{print_banner, print_kv, print_result, print_step};
use etle::{
    crypto::{
        hash::hash_file,
        key_exchange::{EphemeralKeypair, derive_file_key},
    },
    file::storage::{decrypt_to_bytes, encrypt_file},
};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-test-{name}-{}", std::process::id()))
}

#[test]
fn local_encrypted_transfer_roundtrip() {
    print_banner("local_encrypted_transfer_roundtrip");

    let input = temp_file_name("local-transfer.bin");
    let bytes = b"local p2p transfer simulation before tcp exists";

    print_step(1, "write source file");
    fs::write(&input, bytes).unwrap();
    print_kv("input", input.display());
    print_kv("input_len", bytes.len());

    print_step(2, "derive seeder and peer file keys");
    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public = seeder.public_key();
    let peer_public = peer.public_key();
    let file_id = hash_file(&input).unwrap();
    let seeder_key = derive_file_key(seeder.diffie_hellman(peer_public).unwrap(), file_id);
    let peer_key = derive_file_key(peer.diffie_hellman(seeder_public).unwrap(), file_id);
    print_kv("file_id", file_id);
    print_kv("file_key_equal", seeder_key == peer_key);

    print_step(3, "encrypt and decrypt local package");
    let encrypted = encrypt_file(&input, &seeder_key, 8).unwrap();
    let output = decrypt_to_bytes(&encrypted, &peer_key).unwrap();
    print_kv("chunk_count", encrypted.manifest.chunks.len());
    print_kv("output_len", output.len());
    print_kv("output_matches_input", output == bytes);
    print_kv(
        "manifest_file_id_matches",
        encrypted.manifest.file_id == file_id,
    );

    assert_eq!(output, bytes);
    assert_eq!(encrypted.manifest.file_id, file_id);

    fs::remove_file(input).unwrap();
    print_result("local_encrypted_transfer_roundtrip", "ok");
}
