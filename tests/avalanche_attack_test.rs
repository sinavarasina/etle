//! Avalanche effect coverage for hash and encrypted metadata changes.

mod common;

use common::{print_banner, print_kv, print_step};
use etle::crypto::{
    aead::Nonce,
    hash::{ChunkHash, FileId, hash_file},
    key_exchange::{EphemeralKeypair, derive_file_key},
};
use etle::file::{
    manifest::{ChunkMeta, Manifest},
    storage::encrypt_file,
};
use std::{fs, path::PathBuf};

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-avalanche-{name}-{}", std::process::id()))
}

fn count_differing_bits(a: &[u8], b: &[u8]) -> usize {
    let len = a.len().min(b.len());
    let mut count = 0;
    for i in 0..len {
        count += (a[i] ^ b[i]).count_ones() as usize;
    }
    count
}

fn flip_bit(data: &[u8], bit_position: usize) -> Vec<u8> {
    let mut result = data.to_vec();
    let byte_idx = bit_position / 8;
    let bit_idx = bit_position % 8;
    if byte_idx < result.len() {
        result[byte_idx] ^= 1 << bit_idx;
    }
    result
}

#[test]
fn single_bit_flip_causes_large_ciphertext_change() {
    print_banner("single_bit_flip_causes_large_ciphertext_change");
    print_step(1, "execute scenario");
    print_kv("test", "single_bit_flip_causes_large_ciphertext_change");
    let path_a = temp_file("plain-a.bin");
    let path_b = temp_file("plain-b.bin");

    let original = b"Hello, this is a test payload for avalanche attack!";
    let mut modified = original.to_vec();
    modified[0] ^= 0b0000_0001;

    fs::write(&path_a, original).unwrap();
    fs::write(&path_b, &modified).unwrap();

    let keypair_a = EphemeralKeypair::generate();
    let keypair_b = EphemeralKeypair::generate();

    let shared_secret = keypair_a.diffie_hellman(keypair_b.public_key()).unwrap();

    let file_key_a = derive_file_key(shared_secret, hash_file(&path_a).unwrap());

    let file_key_b = derive_file_key(shared_secret, hash_file(&path_b).unwrap());

    let enc_a = encrypt_file(&path_a, &file_key_a, original.len()).unwrap();
    let enc_b = encrypt_file(&path_b, &file_key_b, original.len()).unwrap();

    let bytes_a = enc_a.manifest.to_bytes().unwrap();
    let bytes_b = enc_b.manifest.to_bytes().unwrap();

    let differing = count_differing_bits(&bytes_a, &bytes_b);
    let total_bits = bytes_a.len() * 8;
    let diff_ratio = differing as f64 / total_bits as f64;

    assert!(
        diff_ratio > 0.20,
        "weak avalanche effect: only {:.1}% bits changed, expected > 20%",
        diff_ratio * 100.0
    );

    fs::remove_file(path_a).unwrap();
    fs::remove_file(path_b).unwrap();
}

#[test]
fn file_id_avalanche_on_single_bit_flip() {
    print_banner("file_id_avalanche_on_single_bit_flip");
    print_step(1, "execute scenario");
    print_kv("test", "file_id_avalanche_on_single_bit_flip");
    let path_original = temp_file("fileid-original.bin");
    let path_flipped = temp_file("fileid-flipped.bin");

    let data: Vec<u8> = (0..256).map(|i| i as u8).collect();
    fs::write(&path_original, &data).unwrap();

    let original_id = hash_file(&path_original).unwrap();
    let mut total_diff_bits = 0usize;
    let sample_bits = 16;

    for bit in 0..sample_bits {
        let flipped = flip_bit(&data, bit);
        fs::write(&path_flipped, &flipped).unwrap();
        let flipped_id = hash_file(&path_flipped).unwrap();

        let diff = count_differing_bits(&original_id.0, &flipped_id.0);
        total_diff_bits += diff;
    }

    let avg_diff = total_diff_bits as f64 / (sample_bits * 256) as f64;

    assert!(
        avg_diff > 0.30,
        "weak FileId avalanche effect: average changed bits only {:.1}%",
        avg_diff * 100.0
    );

    fs::remove_file(path_original).unwrap();
    fs::remove_file(path_flipped).unwrap();
}

#[test]
fn manifest_hash_field_avalanche() {
    print_banner("manifest_hash_field_avalanche");
    print_step(1, "execute scenario");
    print_kv("test", "manifest_hash_field_avalanche");
    let base_manifest = Manifest {
        file_id: FileId([0x42_u8; 32]),
        file_name: "test.bin".to_string(),
        file_size: 1024,
        chunk_size: 512,
        chunks: vec![ChunkMeta {
            index: 0,
            plain_size: 512,
            encrypted_size: 528,
            nonce: Nonce([0xAB_u8; 24]),
            blake3_hash: ChunkHash([0xFF_u8; 32]),
        }],
    };

    let mut modified = base_manifest.clone();
    modified.file_id.0[0] ^= 0x01;

    let bytes_base = base_manifest.to_bytes().unwrap();
    let bytes_modified = modified.to_bytes().unwrap();

    let diff = count_differing_bits(&bytes_base, &bytes_modified);
    assert!(diff > 0, "manifest did not change after modification");
}
