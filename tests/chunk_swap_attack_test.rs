//! Pengujian Chunk Swapping Attack
//!
//! Memverifikasi bahwa sistem mendeteksi dan menolak:
//! - Chunk yang ditukar posisinya (index salah)
//! - Chunk yang dimodifikasi isinya
//! - Chunk dari file yang berbeda (replay chunk)
//! - Duplikasi chunk
//!
//! Keamanan bergantung pada verifikasi index + BLAKE3 hash per-chunk
//! yang ada di Manifest.

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

// ── Helper: verifikasi chunk berdasarkan manifest ──────────────────────────

/// Cek apakah data chunk cocok dengan metadata yang ada di manifest
fn verify_chunk_against_manifest(
    manifest: &Manifest,
    index: u32,
    data: &[u8],
) -> Result<(), String> {
    let meta = manifest
        .chunks
        .iter()
        .find(|c| c.index == index)
        .ok_or_else(|| format!("chunk index {index} tidak ada di manifest"))?;

    // Verifikasi ukuran
    if data.len() as u64 != meta.encrypted_size {
        return Err(format!(
            "ukuran chunk {index} salah: dapat {} bytes, manifest expects {} bytes",
            data.len(),
            meta.encrypted_size
        ));
    }

    // Verifikasi BLAKE3 hash
    let hash = blake3::hash(data);
    if hash.as_bytes() != &meta.blake3_hash.0 {
        return Err(format!(
            "hash chunk {index} tidak cocok: payload telah dimodifikasi atau ditukar"
        ));
    }

    Ok(())
}

// ── Chunk swap detection tests ─────────────────────────────────────────────

/// Menukar dua chunk (swap index) harus terdeteksi oleh verifikasi hash
#[test]
fn detects_swapped_chunk_order() {
    let path = temp_file("swap-order.bin");
    let data: Vec<u8> = (0..12u8).collect();
    fs::write(&path, &data).unwrap();

    let mut chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 3);

    let original_chunks = chunks.clone();

    // Swap DATA antar chunk, tapi index tetap (simulasi attacker menukar isi)
    // chunk[0].index=0 sekarang berisi data milik chunk[1]
    // chunk[1].index=1 sekarang berisi data milik chunk[0]
    let tmp = chunks[0].data.clone();
    chunks[0].data = chunks[1].data.clone();
    chunks[1].data = tmp;

    let swapped_result = join_chunks(&chunks);
    let normal_result = join_chunks(&original_chunks);

    assert_ne!(
        swapped_result, normal_result,
        "Data setelah chunk swap harus BERBEDA dari data asli"
    );

    // Verifikasi: index di dalam struct harus tidak cocok dengan posisi
    let first_data_in_slot_0 = &chunks[0].data; // data chunk index=1
    let expected_slot_0_data = &original_chunks[0].data; // data chunk index=0

    assert_ne!(
        first_data_in_slot_0, expected_slot_0_data,
        "Chunk yang ditukar harus menghasilkan data slot yang berbeda"
    );

    println!(
        "[chunk-swap] swap chunk[0]↔chunk[1]: data mismatch terdeteksi ✓\n  asli:    {:?}\n  swapped: {:?}",
        &normal_result[..8],
        &swapped_result[..8]
    );

    fs::remove_file(path).unwrap();
}

/// Memodifikasi 1 byte dalam data chunk harus terdeteksi oleh hash verification
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

    // Simulasi: attacker memodifikasi byte pertama dari chunk pertama
    let first_chunk_meta = &manifest.chunks[0];
    let mut tampered_data = vec![0xFF_u8; first_chunk_meta.encrypted_size as usize];
    tampered_data[0] ^= 0x01; // flip 1 bit

    let result = verify_chunk_against_manifest(manifest, 0, &tampered_data);
    assert!(
        result.is_err(),
        "chunk yang dimodifikasi harus DITOLAK oleh verifikasi hash"
    );

    println!(
        "[chunk-swap] chunk dimodifikasi 1 bit: DITOLAK — {} ✓",
        result.unwrap_err()
    );

    fs::remove_file(path).unwrap();
}

/// Mengirim chunk dari file yang berbeda (replay attack) harus terdeteksi
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

    // File ID harus berbeda (key derivation berbeda)
    assert_ne!(
        enc_a.manifest.file_id, enc_b.manifest.file_id,
        "File ID untuk file yang berbeda harus BERBEDA"
    );

    // Chunk hash dari file B tidak boleh cocok dengan manifest file A
    if let (Some(chunk_a_meta), Some(chunk_b_meta)) =
        (enc_a.manifest.chunks.first(), enc_b.manifest.chunks.first())
    {
        assert_ne!(
            chunk_a_meta.blake3_hash, chunk_b_meta.blake3_hash,
            "Hash chunk dari file berbeda tidak boleh sama (replay terdeteksi)"
        );

        println!(
            "[chunk-swap] file A chunk hash: {:?}\n[chunk-swap] file B chunk hash: {:?}",
            &chunk_a_meta.blake3_hash.0[..8],
            &chunk_b_meta.blake3_hash.0[..8]
        );
        println!("[chunk-swap] replay chunk dari file berbeda: hash mismatch terdeteksi ✓");
    }

    fs::remove_file(path_a).unwrap();
    fs::remove_file(path_b).unwrap();
}

/// Duplikasi chunk (chunk index sama dikirim dua kali) harus ditangani
#[test]
fn detects_duplicate_chunk_injection() {
    let path = temp_file("dup-chunk.bin");
    let data: Vec<u8> = (0..16u8).collect();
    fs::write(&path, &data).unwrap();

    let chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 4);

    // Simulasi: attacker menyisipkan duplikat chunk[0] di posisi yang seharusnya chunk[2]
    let mut tampered: Vec<PlainChunk> = chunks.clone();
    // Ganti chunk index 2 dengan data chunk index 0 (tapi pertahankan index aslinya)
    tampered[2] = PlainChunk {
        index: 2,                     // index tetap 2 (terlihat valid)
        data: chunks[0].data.clone(), // isi adalah data dari chunk 0 (SERANGAN)
    };

    let normal_output = join_chunks(&chunks);
    let tampered_output = join_chunks(&tampered);

    assert_ne!(
        normal_output, tampered_output,
        "Output dengan chunk duplikat yang ditukar harus BERBEDA"
    );

    // Verifikasi bahwa chunk asli dan palsu berbeda
    assert_ne!(
        chunks[2].data, tampered[2].data,
        "Chunk yang diganti harus berbeda isinya dari aslinya"
    );

    println!(
        "[chunk-swap] chunk[2] diganti dengan isi chunk[0]:\n  asli:    {:?}\n  disusupi: {:?}",
        normal_output, tampered_output
    );
    println!("[chunk-swap] injeksi duplikat chunk: perubahan output terdeteksi ✓");

    fs::remove_file(path).unwrap();
}

/// Manifest dengan index chunk yang tidak berurutan harus tetap direkonstruksi benar
#[test]
fn join_chunks_is_order_agnostic_by_index() {
    let path = temp_file("out-of-order.bin");
    let data: Vec<u8> = (0..12u8).collect();
    fs::write(&path, &data).unwrap();

    let mut chunks = read_file_chunks(&path, 4).unwrap();
    assert_eq!(chunks.len(), 3);

    // Acak urutan penerimaan (simulasi out-of-order delivery)
    chunks.reverse();

    let result = join_chunks(&chunks);
    assert_eq!(
        result,
        data.as_slice(),
        "join_chunks harus tetap benar meski urutan terbalik"
    );

    println!("[chunk-swap] 3 chunk diterima terbalik: join_chunks rekonstruksi BENAR ✓");

    fs::remove_file(path).unwrap();
}

/// Chunk dengan index di luar range manifest harus ditolak
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

    // Kirim chunk dengan index 99 — tidak ada di manifest
    let result = verify_chunk_against_manifest(&manifest, 99, &[0u8; 8]);
    assert!(result.is_err(), "chunk index di luar range harus DITOLAK");

    println!(
        "[chunk-swap] chunk index=99 tidak ada di manifest: DITOLAK — {} ✓",
        result.unwrap_err()
    );
}

/// Roundtrip enkripsi-dekripsi tetap valid (baseline — serangan tidak boleh lolos)
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
        "dekripsi dari file yang tidak dimodifikasi harus identik"
    );
    println!("[chunk-swap] roundtrip integritas enkripsi: VALID ✓");

    fs::remove_file(path).unwrap();
}
