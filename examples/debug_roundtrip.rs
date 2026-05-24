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

    println!("== step 1: original ==");
    let file_id = hash_file(&input)?;
    let file_size = fs::metadata(&input)?.len();
    println!("input: {}", input.display());
    println!("size: {file_size} bytes");
    println!("file_id: {file_id}");
    println!();

    println!("== step 2: chunk ==");
    let plain_chunks = read_file_chunks(&input, DEFAULT_CHUNK_SIZE)?;
    println!("chunk_size: {} bytes", DEFAULT_CHUNK_SIZE);
    println!("chunks: {}", plain_chunks.len());
    for chunk in plain_chunks.iter().take(5) {
        println!(
            "chunk {:06}: {} bytes plaintext",
            chunk.index,
            chunk.data.len()
        );
    }
    if plain_chunks.len() > 5 {
        println!("... {} more chunk(s)", plain_chunks.len() - 5);
    }
    println!();

    println!("== step 3: authenticated session ==");
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
    println!("psk auth: OK");
    println!();

    println!("== step 4: encrypt + persist debug workspace ==");
    let file_key = generate_file_key();
    let wrapped = wrap_file_key(&seeder_session_key, file_id, &file_key)?;
    let peer_file_key = unwrap_file_key(&peer_session_key, file_id, &wrapped)?;
    anyhow::ensure!(peer_file_key == file_key, "unwrapped file key does not match");

    let encrypted = encrypt_file(&input, &file_key, DEFAULT_CHUNK_SIZE)?;
    write_debug_workspace(&encrypted, &workspace)?;
    println!("manifest: {}", debug_manifest_path(&workspace).display());
    println!("chunks_dir: {}", debug_chunks_dir(&workspace).display());
    println!("wrapped_file_key_nonce: {:?}", wrapped.nonce);
    println!("wrapped_file_key_size: {} bytes", wrapped.data.len());

    if let Some(first) = encrypted.chunks.get(&0) {
        println!(
            "first encrypted chunk: {}",
            debug_chunk_path(&workspace, 0).display()
        );
        println!("first encrypted chunk size: {} bytes", first.data.len());
        println!("first encrypted bytes: {}", hex_prefix(&first.data, 16));
    }
    println!();

    println!("== step 5: decrypt ==");
    let loaded = read_debug_workspace(&workspace)?;
    let plaintext = decrypt_to_bytes(&loaded, &peer_file_key)?;
    println!("loaded manifest and encrypted chunks from workspace");
    println!("decrypted bytes: {}", plaintext.len());
    println!();

    println!("== step 6: reconstruct ==");
    let output_path = workspace.join(format!("reconstructed-{}", file_name(&input)));
    fs::write(&output_path, plaintext)?;
    let output_hash = hash_file(&output_path)?;
    println!("output: {}", output_path.display());
    println!("output_file_id: {output_hash}");

    if output_hash == file_id {
        println!("status: OK, reconstructed file matches original");
    } else {
        anyhow::bail!("reconstructed file hash does not match original");
    }

    println!();
    println!("workspace layout:");
    println!("{}", workspace.display());
    println!("├── manifest.bin");
    println!("├── chunks/");
    println!("│   ├── 000000.etle");
    println!("│   └── ...");
    println!("└── reconstructed-{}", file_name(&input));

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

    for (i, byte) in bytes.iter().take(shown).enumerate() {
        if i > 0 {
            output.push(' ');
        }
        output.push_str(&format!("{byte:02x}"));
    }

    if bytes.len() > shown {
        output.push_str(" ...");
    }

    output
}
