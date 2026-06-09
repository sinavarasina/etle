use std::path::PathBuf;

use etle::{
    crypto::{
        aead::SymmetricKey,
        hash::hash_file,
        key_exchange::{
            AuthPsk, AuthRole, EphemeralKeypair, PublicKeyBytes, auth_tags_equal, derive_auth_tag,
            derive_session_key_with_transcript,
        },
        key_wrap::{generate_file_key, unwrap_file_key, wrap_file_key},
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
    let input = PathBuf::from(input);

    print_banner("ETLE example: local authenticated encrypted roundtrip");

    print_step(1, "load input and derive file identity");
    let file_id = hash_file(&input)?;
    print_kv("input", input.display());
    print_kv("file_id", file_id);
    print_kv("chunk_size", DEFAULT_CHUNK_SIZE);

    print_step(2, "generate ephemeral X25519 peers");
    let psk = example_psk();
    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public = seeder.public_key();
    let peer_public = peer.public_key();
    print_kv("seeder_public", hex::encode(seeder_public.0));
    print_kv("peer_public", hex::encode(peer_public.0));

    print_step(3, "derive transcript-bound session keys");
    let seeder_shared = seeder.diffie_hellman(peer_public)?;
    let peer_shared = peer.diffie_hellman(seeder_public)?;
    let seeder_session_key =
        derive_session_key_with_transcript(seeder_shared, peer_public, seeder_public);
    let peer_session_key =
        derive_session_key_with_transcript(peer_shared, peer_public, seeder_public);
    print_kv("session_keys_equal", seeder_session_key == peer_session_key);
    anyhow::ensure!(
        seeder_session_key == peer_session_key,
        "session keys do not match"
    );

    print_step(4, "verify PSK proofs for both roles");
    verify_psk_proofs(
        &psk,
        &seeder_session_key,
        &peer_session_key,
        peer_public,
        seeder_public,
    )?;
    print_kv("client_psk_proof", "ok");
    print_kv("server_psk_proof", "ok");

    print_step(5, "generate and wrap reusable file key");
    let file_key = generate_file_key();
    let wrapped_file_key = wrap_file_key(&seeder_session_key, file_id, &file_key)?;
    let peer_file_key = unwrap_file_key(&peer_session_key, file_id, &wrapped_file_key)?;
    print_kv("wrapped_file_key_len", wrapped_file_key.data.len());
    print_kv("unwrapped_key_matches", peer_file_key == file_key);
    anyhow::ensure!(
        peer_file_key == file_key,
        "unwrapped file key does not match"
    );

    print_step(6, "encrypt chunks and decrypt to output file");
    let encrypted = encrypt_file(&input, &file_key, DEFAULT_CHUNK_SIZE)?;
    let output_path = default_output_path(&input);
    decrypt_to_file(&encrypted, &peer_file_key, &output_path)?;
    print_kv("chunks", encrypted.manifest.chunks.len());
    print_kv("output", output_path.display());

    print_step(7, "verify reconstructed output hash");
    let output_file_id = hash_file(&output_path)?;
    let verified = output_file_id == file_id;
    print_kv("output_file_id", output_file_id);
    print_kv("verified", verified);
    anyhow::ensure!(verified, "reconstructed file hash does not match original");

    print_result("local_roundtrip", "ok");
    Ok(())
}

fn example_psk() -> AuthPsk {
    AuthPsk::from_passphrase(
        std::env::var("ETLE_EXAMPLE_PSK").unwrap_or_else(|_| "etle-example-psk".to_string()),
    )
}

fn verify_psk_proofs(
    psk: &AuthPsk,
    seeder_session_key: &SymmetricKey,
    peer_session_key: &SymmetricKey,
    peer_public: PublicKeyBytes,
    seeder_public: PublicKeyBytes,
) -> anyhow::Result<()> {
    let peer_proof = derive_auth_tag(
        psk,
        peer_session_key,
        peer_public,
        seeder_public,
        AuthRole::Client,
    );
    let expected_peer_proof = derive_auth_tag(
        psk,
        seeder_session_key,
        peer_public,
        seeder_public,
        AuthRole::Client,
    );
    anyhow::ensure!(
        auth_tags_equal(&peer_proof, &expected_peer_proof),
        "client PSK proof failed"
    );

    let seeder_proof = derive_auth_tag(
        psk,
        seeder_session_key,
        peer_public,
        seeder_public,
        AuthRole::Server,
    );
    let expected_seeder_proof = derive_auth_tag(
        psk,
        peer_session_key,
        peer_public,
        seeder_public,
        AuthRole::Server,
    );
    anyhow::ensure!(
        auth_tags_equal(&seeder_proof, &expected_seeder_proof),
        "server PSK proof failed"
    );

    Ok(())
}

fn print_banner(title: &str) {
    println!();
    println!("============================================");
    println!("{title}");
    println!("============================================");
}

fn print_step(index: usize, title: &str) {
    println!();
    println!("[ step={index} title=\"{title}\" ]");
}

fn print_kv(key: &str, value: impl std::fmt::Display) {
    println!("{key}={value}");
}

fn print_result(example: &str, status: &str) {
    println!();
    println!("result={status}");
    println!("example={example}");
}
