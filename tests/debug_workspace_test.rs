use std::{fs, path::PathBuf};

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
    let input = temp_path("input.bin");
    let workspace = temp_path("workspace");
    let plaintext = b"persist encrypted chunks for debug demo";
    fs::write(&input, plaintext).unwrap();

    let key = SymmetricKey([11_u8; 32]);
    let encrypted = encrypt_file(&input, &key, 8).unwrap();
    write_debug_workspace(&encrypted, &workspace).unwrap();

    let manifest_path = debug_manifest_path(&workspace);
    let chunk_path = debug_chunk_path(&workspace, 0);
    let first_chunk_bytes = fs::read(&chunk_path).unwrap();

    assert!(manifest_path.is_file());
    assert!(chunk_path.is_file());
    assert_ne!(first_chunk_bytes.as_slice(), &plaintext[..8]);

    let loaded = read_debug_workspace(&workspace).unwrap();
    let output = decrypt_to_bytes(&loaded, &key).unwrap();

    assert_eq!(loaded.manifest, encrypted.manifest);
    assert_eq!(output, plaintext);

    fs::remove_file(input).unwrap();
    fs::remove_dir_all(workspace).unwrap();
}
