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
        chunker::DEFAULT_CHUNK_SIZE,
        storage::{decrypt_to_file, default_output_path, encrypt_file},
    },
};

fn main() -> anyhow::Result<()> {
    let input = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: cargo run --example local_roundtrip -- <file>"))?;

    let psk = example_psk();
    let seeder = EphemeralKeypair::generate();
    let peer = EphemeralKeypair::generate();
    let seeder_public = seeder.public_key();
    let peer_public = peer.public_key();

    let seeder_shared = seeder.diffie_hellman(peer_public)?;
    let peer_shared = peer.diffie_hellman(seeder_public)?;

    let seeder_session_key =
        derive_session_key_with_transcript(seeder_shared, peer_public, seeder_public);
    let peer_session_key =
        derive_session_key_with_transcript(peer_shared, peer_public, seeder_public);
    anyhow::ensure!(
        seeder_session_key == peer_session_key,
        "session keys do not match"
    );

    verify_psk_proofs(
        &psk,
        &seeder_session_key,
        &peer_session_key,
        peer_public,
        seeder_public,
    )?;

    let file_id = hash_file(&input)?;
    let file_key = generate_file_key();
    let wrapped_file_key = wrap_file_key(&seeder_session_key, file_id, &file_key)?;
    let peer_file_key = unwrap_file_key(&peer_session_key, file_id, &wrapped_file_key)?;
    anyhow::ensure!(
        peer_file_key == file_key,
        "unwrapped file key does not match"
    );

    let encrypted = encrypt_file(&input, &file_key, DEFAULT_CHUNK_SIZE)?;
    let output_path = default_output_path(&input);
    decrypt_to_file(&encrypted, &peer_file_key, &output_path)?;

    let output_file_id = hash_file(&output_path)?;
    anyhow::ensure!(
        output_file_id == file_id,
        "reconstructed file hash does not match original"
    );

    println!("file_id: {}", encrypted.manifest.file_id);
    println!("chunks: {}", encrypted.manifest.chunks.len());
    println!("output: {}", output_path.display());
    println!("local authenticated encrypted roundtrip OK");

    Ok(())
}

fn example_psk() -> AuthPsk {
    AuthPsk::from_passphrase(
        std::env::var("ETLE_EXAMPLE_PSK").unwrap_or_else(|_| "etle-example-psk".to_string()),
    )
}

fn verify_psk_proofs(
    psk: &AuthPsk,
    seeder_session_key: &etle::crypto::aead::SymmetricKey,
    peer_session_key: &etle::crypto::aead::SymmetricKey,
    peer_public: etle::crypto::key_exchange::PublicKeyBytes,
    seeder_public: etle::crypto::key_exchange::PublicKeyBytes,
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
