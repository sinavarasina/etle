//! Pengujian Batas Payload (Payload Size Limit)
//!
//! Memverifikasi bahwa codec menolak frame di atas MAX_FRAME_SIZE,
//! menerima frame tepat di batas, dan chunker menangani edge case ukuran.

use etle::{
    file::chunker::{DEFAULT_CHUNK_SIZE, join_chunks, read_file_chunks},
    protocol::{
        codec::{MAX_FRAME_SIZE, receive, send},
        error::ProtocolError,
        message::WireMessage,
    },
};
use std::{fs, path::PathBuf};
use tokio::io::AsyncWriteExt;

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-payload-{name}-{}", std::process::id()))
}

// ── Codec frame size limits ────────────────────────────────────────────────

/// Frame tepat sebesar MAX_FRAME_SIZE harus DITERIMA
#[tokio::test]
async fn codec_accepts_frame_at_exact_max_size() {
    // Buat WireMessage::Chunk dengan data tepat memenuhi frame limit.
    // Frame = 4 (len header) + 1 (raw tag) + 4 (index) + data.len()
    // Jadi data.len() = MAX_FRAME_SIZE - RAW_CHUNK_HEADER_SIZE (5 bytes)
    let raw_chunk_overhead = 1 + 4; // tag byte + u32 index
    let data_size = MAX_FRAME_SIZE - raw_chunk_overhead;
    let message = WireMessage::Chunk {
        index: 0,
        data: vec![0xAB_u8; data_size],
    };

    let (mut writer, mut reader) = tokio::io::duplex(MAX_FRAME_SIZE + 128);
    send(&mut writer, &message)
        .await
        .expect("send harus sukses pada batas MAX");

    let received = receive(&mut reader)
        .await
        .expect("receive harus sukses pada batas MAX");
    assert_eq!(received, message, "Pesan harus identik setelah roundtrip");

    println!("[payload-limit] frame tepat MAX_FRAME_SIZE ({MAX_FRAME_SIZE} bytes): DITERIMA ✓");
}

/// Frame 1 byte di atas MAX_FRAME_SIZE harus DITOLAK
#[tokio::test]
async fn codec_rejects_frame_one_byte_over_max() {
    let (mut writer, mut reader) = tokio::io::duplex(8);
    let over_limit = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();

    writer.write_all(&over_limit).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("harus error untuk frame terlalu besar");

    assert!(
        matches!(err, ProtocolError::FrameTooLarge { .. }),
        "Error harus FrameTooLarge, dapat: {err:?}"
    );
    println!("[payload-limit] MAX+1 bytes: DITOLAK dengan FrameTooLarge ✓");
}

/// Frame dengan panjang 0 harus DITOLAK (EmptyFrame)
#[tokio::test]
async fn codec_rejects_zero_length_frame() {
    let (mut writer, mut reader) = tokio::io::duplex(8);

    writer.write_all(&0_u32.to_be_bytes()).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("harus error untuk frame kosong");

    assert!(
        matches!(err, ProtocolError::EmptyFrame),
        "Error harus EmptyFrame, dapat: {err:?}"
    );
    println!("[payload-limit] frame 0 bytes: DITOLAK dengan EmptyFrame ✓");
}

/// Nilai u32::MAX sebagai panjang frame harus DITOLAK
#[tokio::test]
async fn codec_rejects_u32_max_frame_length() {
    let (mut writer, mut reader) = tokio::io::duplex(8);

    writer.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("harus error untuk u32::MAX");

    assert!(
        matches!(err, ProtocolError::FrameTooLarge { .. }),
        "Error harus FrameTooLarge, dapat: {err:?}"
    );
    println!("[payload-limit] u32::MAX ({}) bytes: DITOLAK ✓", u32::MAX);
}

/// Beberapa frame besar berturut-turut harus diterima selama di dalam batas
#[tokio::test]
async fn codec_handles_multiple_large_frames_sequentially() {
    let chunk_size = 1024 * 1024; // 1 MB per chunk
    let num_chunks = 5;

    for i in 0..num_chunks {
        let msg = WireMessage::Chunk {
            index: i,
            data: vec![i as u8; chunk_size],
        };
        let (mut w, mut r) = tokio::io::duplex(chunk_size + 128);
        send(&mut w, &msg).await.unwrap();
        let recv = receive(&mut r).await.unwrap();
        assert_eq!(recv, msg);
    }

    println!("[payload-limit] {num_chunks}x 1MB chunk berturut-turut: semua DITERIMA ✓");
}

// ── Chunker size limits ────────────────────────────────────────────────────

/// Chunker menolak chunk_size = 0
#[test]
fn chunker_rejects_zero_chunk_size() {
    let path = temp_file("zero-chunk.bin");
    fs::write(&path, b"some data").unwrap();

    let err = read_file_chunks(&path, 0);
    assert!(err.is_err(), "chunk_size=0 harus menghasilkan error");

    println!("[payload-limit] chunk_size=0: DITOLAK ✓");
    fs::remove_file(path).unwrap();
}

/// Chunker menerima chunk_size = 1 (batas minimum valid)
#[test]
fn chunker_accepts_minimum_chunk_size_of_one() {
    let path = temp_file("min-chunk.bin");
    let data = b"abcde";
    fs::write(&path, data).unwrap();

    let chunks = read_file_chunks(&path, 1).unwrap();
    assert_eq!(chunks.len(), data.len(), "setiap byte menjadi 1 chunk");
    assert_eq!(join_chunks(&chunks), data);

    println!(
        "[payload-limit] chunk_size=1: DITERIMA, {} chunks ✓",
        chunks.len()
    );
    fs::remove_file(path).unwrap();
}

/// Chunk tunggal: file lebih kecil dari chunk_size
#[test]
fn chunker_single_chunk_when_file_smaller_than_chunk_size() {
    let path = temp_file("single-chunk.bin");
    let data = b"tiny";
    fs::write(&path, data).unwrap();

    let chunks = read_file_chunks(&path, 1024 * 1024).unwrap();
    assert_eq!(chunks.len(), 1, "file kecil harus jadi 1 chunk");
    assert_eq!(chunks[0].data, data);

    println!("[payload-limit] file 4 bytes dengan chunk_size 1MB: 1 chunk ✓");
    fs::remove_file(path).unwrap();
}

/// DEFAULT_CHUNK_SIZE (1 MB) harus valid dan bisa digunakan
#[test]
fn chunker_default_chunk_size_is_valid() {
    let path = temp_file("default-chunk.bin");
    let data: Vec<u8> = (0..DEFAULT_CHUNK_SIZE + 100).map(|i| i as u8).collect();
    fs::write(&path, &data).unwrap();

    let chunks = read_file_chunks(&path, DEFAULT_CHUNK_SIZE).unwrap();
    assert_eq!(
        chunks.len(),
        2,
        "data sedikit di atas 1MB harus menjadi 2 chunk"
    );
    assert_eq!(join_chunks(&chunks), data.as_slice());

    println!(
        "[payload-limit] DEFAULT_CHUNK_SIZE ({DEFAULT_CHUNK_SIZE} bytes): {} chunk ✓",
        chunks.len()
    );
    fs::remove_file(path).unwrap();
}

/// Error field message pada WireMessage harus tetap bisa dikirim walaupun panjang
#[tokio::test]
async fn codec_handles_long_error_message() {
    let long_msg = "E".repeat(10_000);
    let message = WireMessage::Error {
        message: long_msg.clone(),
    };

    let (mut w, mut r) = tokio::io::duplex(64 * 1024);
    send(&mut w, &message).await.unwrap();
    let received = receive(&mut r).await.unwrap();

    assert_eq!(received, message);
    println!("[payload-limit] WireMessage::Error dengan 10.000 karakter: DITERIMA ✓");
}
