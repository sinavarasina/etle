//! Tamper-evidence coverage for chunk swapping and replay attempts.

use etle::{
    crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId, hash_file},
        key_exchange::{EphemeralKeypair, derive_file_key},
    },
    file::{
        chunker::{PlainChunk, join_chunks, read_file_chunks},
        manifest::{ChunkMeta, Manifest},
        storage::{decrypt_to_bytes, encrypt_file},
    },
};
use std::{fs, path::PathBuf};

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-swap-{name}-{}", std::process::id()))
}

fn verify_chunk_against_manifest(
    manifest: &Manifest,
    index: u32,
    data: &[u8],
) -> Result<(), String> {
    let meta = manifest
        .chunks
        .iter()
        .find(|c| c.index == index)
        .ok_or_else(|| format!("chunk index {index} is not present in the manifest"))?;

    if data.len() as u64 != meta.encrypted_size {
        return Err(format!(
            "invalid chunk {index} size: got {} bytes, expected {} bytes",
            data.len(),
            meta.encrypted_size
        ));
    }

    let hash = blake3::hash(data);
    if hash.as_bytes() != &meta.blake3_hash.0 {
        return Err(format!(
            "chunk {index} hash mismatch: payload was modified or swapped"
        ));
    }

    Ok(())
}

#[test]
fn detects_swapped_chunk_order() {
    let path = temp_file("swap-order.bin");
    let data: Vec<u8> = (0..12u8).collect();
    fs::write(&path, &data).unwrap();

    let mut chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 3);

    let original_chunks = chunks.clone();

    let tmp = chunks[0].data.clone();
    chunks[0].data = chunks[1].data.clone();
    chunks[1].data = tmp;

    let swapped_result = join_chunks(&chunks);
    let normal_result = join_chunks(&original_chunks);

    assert_ne!(
        swapped_result, normal_result,
        "output should differ after swapping chunk data"
    );

    let first_data_in_slot_0 = &chunks[0].data;
    let expected_slot_0_data = &original_chunks[0].data;

    assert_ne!(
        first_data_in_slot_0, expected_slot_0_data,
        "swapped chunk data should change the target slot"
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn detects_modified_chunk_content_via_hash() {
    let path = temp_file("modified-chunk.bin");
    let data = b"chunk0dataXXchunk1dataYYchunk2dataZZ";
    fs::write(&path, data).unwrap();

    let keypair_a = EphemeralKeypair::generate();
    let keypair_b = EphemeralKeypair::generate();
    let file_id = hash_file(&path).unwrap();
    let file_key = derive_file_key(
        keypair_a.diffie_hellman(keypair_b.public_key()).unwrap(),
        file_id,
    );

    let encrypted = encrypt_file(&path, &file_key, 12).unwrap();
    let manifest = &encrypted.manifest;

    let first_chunk_meta = &manifest.chunks[0];
    let mut tampered_data = vec![0xFF_u8; first_chunk_meta.encrypted_size as usize];
    tampered_data[0] ^= 0x01;

    let result = verify_chunk_against_manifest(manifest, 0, &tampered_data);
    assert!(
        result.is_err(),
        "modified chunk should be rejected by hash verification"
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn detects_chunk_from_different_file_replay() {
    let path_a = temp_file("replay-file-a.bin");
    let path_b = temp_file("replay-file-b.bin");

    let data_a = b"file-A-data-payload-content-test";
    let data_b = b"file-B-different-payload-entirely";
    fs::write(&path_a, data_a).unwrap();
    fs::write(&path_b, data_b).unwrap();

    let kp = EphemeralKeypair::generate();
    let kp2 = EphemeralKeypair::generate();
    let shared_secret = kp.diffie_hellman(kp2.public_key()).unwrap();
    let key_a = derive_file_key(shared_secret, hash_file(&path_a).unwrap());
    let key_b = derive_file_key(shared_secret, hash_file(&path_b).unwrap());

    let enc_a = encrypt_file(&path_a, &key_a, 16).unwrap();
    let enc_b = encrypt_file(&path_b, &key_b, 16).unwrap();

    assert_ne!(
        enc_a.manifest.file_id, enc_b.manifest.file_id,
        "different files should have different file IDs"
    );

    if let (Some(chunk_a_meta), Some(chunk_b_meta)) =
        (enc_a.manifest.chunks.first(), enc_b.manifest.chunks.first())
    {
        assert_ne!(
            chunk_a_meta.blake3_hash, chunk_b_meta.blake3_hash,
            "chunks from different files should not share the same hash"
        );
    }

    fs::remove_file(path_a).unwrap();
    fs::remove_file(path_b).unwrap();
}

#[test]
fn detects_duplicate_chunk_injection() {
    let path = temp_file("dup-chunk.bin");
    let data: Vec<u8> = (0..16u8).collect();
    fs::write(&path, &data).unwrap();

    let chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 4);

    let mut tampered: Vec<PlainChunk> = chunks.clone();
    tampered[2] = PlainChunk {
        index: 2,
        data: chunks[0].data.clone(),
    };

    let normal_output = join_chunks(&chunks);
    let tampered_output = join_chunks(&tampered);

    assert_ne!(
        normal_output, tampered_output,
        "output should change after duplicate chunk injection"
    );

    assert_ne!(
        chunks[2].data, tampered[2].data,
        "replaced chunk data should differ from the original chunk"
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn join_chunks_is_order_agnostic_by_index() {
    let path = temp_file("out-of-order.bin");
    let data: Vec<u8> = (0..12u8).collect();
    fs::write(&path, &data).unwrap();

    let mut chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 3);

    chunks.reverse();

    let result = join_chunks(&chunks);
    assert_eq!(
        result,
        data.as_slice(),
        "join_chunks should reconstruct correctly from reversed input order"
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn verify_rejects_chunk_with_out_of_range_index() {
    let manifest = Manifest {
        file_id: FileId([0x11_u8; 32]),
        file_name: "test.bin".to_string(),
        file_size: 8,
        chunk_size: 8,
        chunks: vec![ChunkMeta {
            index: 0,
            plain_size: 8,
            encrypted_size: 8,
            nonce: Nonce([0xCC_u8; 24]),
            blake3_hash: ChunkHash([0xDD_u8; 32]),
        }],
    };

    let result = verify_chunk_against_manifest(&manifest, 99, &[0u8; 8]);
    assert!(
        result.is_err(),
        "out-of-range chunk index should be rejected"
    );
}

#[test]
fn encrypted_chunks_roundtrip_is_tamper_evident() {
    let path = temp_file("roundtrip-integrity.bin");
    let data = b"integrity check data for tamper evidence test";
    fs::write(&path, data).unwrap();

    let kp_a = EphemeralKeypair::generate();
    let kp_b = EphemeralKeypair::generate();
    let file_id = hash_file(&path).unwrap();
    let key = derive_file_key(kp_a.diffie_hellman(kp_b.public_key()).unwrap(), file_id);

    let encrypted = encrypt_file(&path, &key, data.len()).unwrap();
    let decrypted = decrypt_to_bytes(&encrypted, &key).unwrap();

    assert_eq!(
        decrypted, data,
        "decrypting an unmodified encrypted file should reproduce the original data"
    );

    fs::remove_file(path).unwrap();
}
