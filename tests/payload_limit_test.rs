//! Payload and chunk size limit coverage.

mod common;

use common::{print_banner, print_kv, print_step};
use std::{fs, path::PathBuf};

use etle::{
    file::chunker::{DEFAULT_CHUNK_SIZE, join_chunks, read_file_chunks},
    protocol::{
        codec::{MAX_FRAME_SIZE, receive, send},
        error::ProtocolError,
        message::WireMessage,
    },
};
use tokio::io::AsyncWriteExt;

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-payload-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn codec_accepts_frame_at_exact_max_size() {
    print_banner("codec_accepts_frame_at_exact_max_size");
    print_step(1, "execute scenario");
    print_kv("test", "codec_accepts_frame_at_exact_max_size");
    let raw_chunk_overhead = 1 + 4;
    let data_size = MAX_FRAME_SIZE - raw_chunk_overhead;
    let message = WireMessage::Chunk {
        index: 0,
        data: vec![0xab_u8; data_size],
    };

    let (mut writer, mut reader) = tokio::io::duplex(MAX_FRAME_SIZE + 128);
    send(&mut writer, &message)
        .await
        .expect("send should succeed at the frame limit");

    let received = receive(&mut reader)
        .await
        .expect("receive should succeed at the frame limit");

    assert_eq!(received, message);
}

#[tokio::test]
async fn codec_rejects_frame_one_byte_over_max() {
    print_banner("codec_rejects_frame_one_byte_over_max");
    print_step(1, "execute scenario");
    print_kv("test", "codec_rejects_frame_one_byte_over_max");
    let (mut writer, mut reader) = tokio::io::duplex(8);
    let over_limit = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();

    writer.write_all(&over_limit).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("oversized frame should fail");

    assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
}

#[tokio::test]
async fn codec_rejects_zero_length_frame() {
    print_banner("codec_rejects_zero_length_frame");
    print_step(1, "execute scenario");
    print_kv("test", "codec_rejects_zero_length_frame");
    let (mut writer, mut reader) = tokio::io::duplex(8);

    writer.write_all(&0_u32.to_be_bytes()).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("empty frame should fail");

    assert!(matches!(err, ProtocolError::EmptyFrame));
}

#[tokio::test]
async fn codec_rejects_u32_max_frame_length() {
    print_banner("codec_rejects_u32_max_frame_length");
    print_step(1, "execute scenario");
    print_kv("test", "codec_rejects_u32_max_frame_length");
    let (mut writer, mut reader) = tokio::io::duplex(8);

    writer.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    writer.flush().await.unwrap();

    let err = receive(&mut reader)
        .await
        .expect_err("u32::MAX frame length should fail");

    assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
}

#[tokio::test]
async fn codec_handles_multiple_large_frames_sequentially() {
    print_banner("codec_handles_multiple_large_frames_sequentially");
    print_step(1, "execute scenario");
    print_kv("test", "codec_handles_multiple_large_frames_sequentially");
    let chunk_size = 1024 * 1024;
    let num_chunks = 5;

    for index in 0..num_chunks {
        let message = WireMessage::Chunk {
            index,
            data: vec![index as u8; chunk_size],
        };
        let (mut writer, mut reader) = tokio::io::duplex(chunk_size + 128);

        send(&mut writer, &message).await.unwrap();
        let received = receive(&mut reader).await.unwrap();

        assert_eq!(received, message);
    }
}

#[test]
fn chunker_rejects_zero_chunk_size() {
    print_banner("chunker_rejects_zero_chunk_size");
    print_step(1, "execute scenario");
    print_kv("test", "chunker_rejects_zero_chunk_size");
    let path = temp_file("zero-chunk.bin");
    fs::write(&path, b"some data").unwrap();

    let result = read_file_chunks(&path, 0);

    assert!(result.is_err());
    fs::remove_file(path).unwrap();
}

#[test]
fn chunker_accepts_minimum_chunk_size_of_one() {
    print_banner("chunker_accepts_minimum_chunk_size_of_one");
    print_step(1, "execute scenario");
    print_kv("test", "chunker_accepts_minimum_chunk_size_of_one");
    let path = temp_file("min-chunk.bin");
    let data = b"abcde";
    fs::write(&path, data).unwrap();

    let chunks = read_file_chunks(&path, 1).unwrap();

    assert_eq!(chunks.len(), data.len());
    assert_eq!(join_chunks(&chunks), data);

    fs::remove_file(path).unwrap();
}

#[test]
fn chunker_single_chunk_when_file_smaller_than_chunk_size() {
    print_banner("chunker_single_chunk_when_file_smaller_than_chunk_size");
    print_step(1, "execute scenario");
    print_kv(
        "test",
        "chunker_single_chunk_when_file_smaller_than_chunk_size",
    );
    let path = temp_file("single-chunk.bin");
    let data = b"tiny";
    fs::write(&path, data).unwrap();

    let chunks = read_file_chunks(&path, 1024 * 1024).unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].data, data);

    fs::remove_file(path).unwrap();
}

#[test]
fn chunker_default_chunk_size_is_valid() {
    print_banner("chunker_default_chunk_size_is_valid");
    print_step(1, "execute scenario");
    print_kv("test", "chunker_default_chunk_size_is_valid");
    let path = temp_file("default-chunk.bin");
    let data: Vec<u8> = (0..DEFAULT_CHUNK_SIZE + 100)
        .map(|index| index as u8)
        .collect();
    fs::write(&path, &data).unwrap();

    let chunks = read_file_chunks(&path, DEFAULT_CHUNK_SIZE).unwrap();

    assert_eq!(chunks.len(), 2);
    assert_eq!(join_chunks(&chunks), data.as_slice());

    fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn codec_handles_long_error_message() {
    print_banner("codec_handles_long_error_message");
    print_step(1, "execute scenario");
    print_kv("test", "codec_handles_long_error_message");
    let long_message = "E".repeat(10_000);
    let message = WireMessage::Error {
        message: long_message,
    };

    let (mut writer, mut reader) = tokio::io::duplex(64 * 1024);
    send(&mut writer, &message).await.unwrap();
    let received = receive(&mut reader).await.unwrap();

    assert_eq!(received, message);
}
