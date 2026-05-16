use etle::{
    crypto::{
        hash::hash_file,
        key_exchange::{derive_file_key, EphemeralKeypair},
    },
    file::{
        chunker::DEFAULT_CHUNK_SIZE,
        storage::{decrypt_to_file, default_output_path, encrypt_file},
    },
};

fn main() -> anyhow::Result<()> {
    let input = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: cargo run --example local_roundtrip -- <file>"))?;

    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public = seeder.public_key();
    let peer_public = peer.public_key();

    let file_id = hash_file(&input)?;
    let seeder_key = derive_file_key(seeder.diffie_hellman(peer_public)?, file_id);
    let peer_key = derive_file_key(peer.diffie_hellman(seeder_public)?, file_id);

    let encrypted = encrypt_file(&input, &seeder_key, DEFAULT_CHUNK_SIZE)?;
    let output_path = default_output_path(&input);
    decrypt_to_file(&encrypted, &peer_key, &output_path)?;

    println!("file_id: {}", encrypted.manifest.file_id);
    println!("chunks: {}", encrypted.manifest.chunks.len());
    println!("output: {}", output_path.display());
    println!("local encrypted roundtrip OK");

    Ok(())
}
