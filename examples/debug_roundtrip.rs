use std::{
    fs,
    path::{Path, PathBuf},
};

use etle::{
    crypto::{
        hash::hash_file,
        key_exchange::{
            AuthPsk, AuthRole, EphemeralKeypair, auth_tags_equal, derive_auth_tag,
            derive_session_key_with_transcript,
        },
        key_wrap::{generate_file_key, unwrap_file_key, wrap_file_key},
    },
    file::{
        chunker::{DEFAULT_CHUNK_SIZE, read_file_chunks},
        storage::{
            debug_chunk_path, debug_chunks_dir, debug_manifest_path, decrypt_to_bytes,
            encrypt_file, read_debug_workspace, write_debug_workspace,
        },
    },
};

fn main() -> anyhow::Result<()> {
    let input = std::env::args().nth(1).ok_or_else(|| {
        anyhow::anyhow!("usage: cargo run --example debug_roundtrip -- <file> [workspace]")
    })?;
    let input = PathBuf::from(input);

    let workspace = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_workspace_path(&input));

    prepare_workspace(&workspace)?;

    let file_id = hash_file(&input)?;
    let file_size = fs::metadata(&input)?.len();
    let plain_chunks = read_file_chunks(&input, DEFAULT_CHUNK_SIZE)?;

    let psk = example_psk();
    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public_key = seeder.public_key();
    let peer_public_key = peer.public_key();

    let seeder_shared = seeder.diffie_hellman(peer_public_key)?;
    let peer_shared = peer.diffie_hellman(seeder_public_key)?;
    let seeder_session_key =
        derive_session_key_with_transcript(seeder_shared, peer_public_key, seeder_public_key);
    let peer_session_key =
        derive_session_key_with_transcript(peer_shared, peer_public_key, seeder_public_key);
    anyhow::ensure!(
        seeder_session_key == peer_session_key,
        "session keys do not match"
    );

    let peer_proof = derive_auth_tag(
        &psk,
        &peer_session_key,
        peer_public_key,
        seeder_public_key,
        AuthRole::Client,
    );
    let expected_peer_proof = derive_auth_tag(
        &psk,
        &seeder_session_key,
        peer_public_key,
        seeder_public_key,
        AuthRole::Client,
    );
    anyhow::ensure!(
        auth_tags_equal(&peer_proof, &expected_peer_proof),
        "client PSK proof failed"
    );

    let file_key = generate_file_key();
    let wrapped = wrap_file_key(&seeder_session_key, file_id, &file_key)?;
    let peer_file_key = unwrap_file_key(&peer_session_key, file_id, &wrapped)?;
    anyhow::ensure!(
        peer_file_key == file_key,
        "unwrapped file key does not match"
    );

    let encrypted = encrypt_file(&input, &file_key, DEFAULT_CHUNK_SIZE)?;
    write_debug_workspace(&encrypted, &workspace)?;

    let loaded = read_debug_workspace(&workspace)?;
    let plaintext = decrypt_to_bytes(&loaded, &peer_file_key)?;
    let output_path = workspace.join(format!("reconstructed-{}", file_name(&input)));
    fs::write(&output_path, plaintext)?;
    let output_hash = hash_file(&output_path)?;
    anyhow::ensure!(
        output_hash == file_id,
        "reconstructed file hash does not match original"
    );

    print_kv("example", "debug_roundtrip");
    print_kv("status", "ok");
    print_kv("input", input.display());
    print_kv("input_size", format_args!("{file_size} bytes"));
    print_kv("file_id", file_id);
    print_kv("chunk_size", format_args!("{DEFAULT_CHUNK_SIZE} bytes"));
    print_kv("chunks", plain_chunks.len());
    print_kv("workspace", workspace.display());
    print_kv("manifest", debug_manifest_path(&workspace).display());
    print_kv("chunks_dir", debug_chunks_dir(&workspace).display());
    print_kv("output", output_path.display());
    print_kv("output_file_id", output_hash);
    print_kv(
        "wrapped_file_key_nonce",
        format_args!("{:?}", wrapped.nonce),
    );
    print_kv(
        "wrapped_file_key_size",
        format_args!("{} bytes", wrapped.data.len()),
    );

    if let Some(first) = encrypted.chunks.get(&0) {
        print_kv(
            "first_chunk_path",
            debug_chunk_path(&workspace, 0).display(),
        );
        print_kv(
            "first_chunk_size",
            format_args!("{} bytes", first.data.len()),
        );
        print_kv("first_chunk_prefix", hex_prefix(&first.data, 16));
    }

    Ok(())
}

fn example_psk() -> AuthPsk {
    AuthPsk::from_passphrase(
        std::env::var("ETLE_EXAMPLE_PSK").unwrap_or_else(|_| "etle-example-psk".to_string()),
    )
}

fn prepare_workspace(workspace: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(workspace)?;

    let chunks_dir = debug_chunks_dir(workspace);
    if chunks_dir.exists() {
        fs::remove_dir_all(&chunks_dir)?;
    }

    Ok(())
}

fn default_workspace_path(input: &Path) -> PathBuf {
    PathBuf::from(".etle-work").join(file_name(input))
}

fn file_name(input: &Path) -> String {
    input
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string())
}

fn hex_prefix(bytes: &[u8], max_len: usize) -> String {
    let shown = bytes.len().min(max_len);
    let mut output = String::with_capacity(shown * 3);

    for (index, byte) in bytes.iter().take(shown).enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&format!("{byte:02x}"));
    }

    if bytes.len() > shown {
        output.push_str(" ...");
    }

    output
}

fn print_kv(key: &str, value: impl std::fmt::Display) {
    println!("{key}={value}");
}
