mod common;

use std::{fs, path::PathBuf};

use common::{hex_prefix, print_banner, print_kv, print_result, print_step};
use etle::{
    crypto::aead::SymmetricKey,
    file::storage::{
        debug_chunk_path, debug_manifest_path, decrypt_to_bytes, encrypt_file,
        read_debug_workspace, write_debug_workspace,
    },
};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "etle-debug-workspace-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn debug_workspace_writes_manifest_and_encrypted_chunks() {
    print_banner("debug_workspace_writes_manifest_and_encrypted_chunks");

    let input = temp_path("input.bin");
    let workspace = temp_path("workspace");
    let plaintext = b"persist encrypted chunks for debug demo";

    print_step(1, "prepare input and workspace paths");
    fs::write(&input, plaintext).unwrap();
    print_kv("input", input.display());
    print_kv("workspace", workspace.display());
    print_kv("plaintext_len", plaintext.len());

    print_step(2, "encrypt input and write debug workspace");
    let key = SymmetricKey([11_u8; 32]);
    let encrypted = encrypt_file(&input, &key, 8).unwrap();
    write_debug_workspace(&encrypted, &workspace).unwrap();
    print_kv("chunk_count", encrypted.manifest.chunks.len());
    print_kv("manifest", debug_manifest_path(&workspace).display());

    print_step(3, "verify persisted manifest and encrypted chunk file");
    let manifest_path = debug_manifest_path(&workspace);
    let chunk_path = debug_chunk_path(&workspace, 0);
    let first_chunk_bytes = fs::read(&chunk_path).unwrap();
    print_kv("manifest_exists", manifest_path.is_file());
    print_kv("chunk_0_exists", chunk_path.is_file());
    print_kv("chunk_0_prefix", hex_prefix(&first_chunk_bytes, 16));
    print_kv(
        "chunk_0_differs_from_plaintext",
        first_chunk_bytes.as_slice() != &plaintext[..8],
    );
    assert!(manifest_path.is_file());
    assert!(chunk_path.is_file());
    assert_ne!(first_chunk_bytes.as_slice(), &plaintext[..8]);

    print_step(4, "reload workspace and decrypt");
    let loaded = read_debug_workspace(&workspace).unwrap();
    let output = decrypt_to_bytes(&loaded, &key).unwrap();
    print_kv(
        "manifest_roundtrip_equal",
        loaded.manifest == encrypted.manifest,
    );
    print_kv("output_matches_plaintext", output == plaintext);

    assert_eq!(loaded.manifest, encrypted.manifest);
    assert_eq!(output, plaintext);

    fs::remove_file(input).unwrap();
    fs::remove_dir_all(workspace).unwrap();
    print_result("debug_workspace_writes_manifest_and_encrypted_chunks", "ok");
}
