mod common;

use common::{print_banner, print_kv, print_step};
use tokio::io::AsyncWriteExt;

use etle::{
    crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId},
        key_exchange::PublicKeyBytes,
    },
    file::manifest::{ChunkMeta, Manifest},
    protocol::{
        codec::{MAX_FRAME_SIZE, receive, send},
        error::ProtocolError,
        message::WireMessage,
    },
};

fn sample_manifest() -> Manifest {
    Manifest {
        file_id: FileId([1_u8; 32]),
        file_name: "sample.bin".to_string(),
        file_size: 12,
        chunk_size: 4,
        chunks: vec![ChunkMeta {
            index: 0,
            plain_size: 4,
            encrypted_size: 20,
            nonce: Nonce([2_u8; 24]),
            blake3_hash: ChunkHash([3_u8; 32]),
        }],
    }
}

fn assert_binary_codec_roundtrip(message: WireMessage) {
    let config = bincode_next::config::standard();
    let encoded = bincode_next::serde::encode_to_vec(&message, config).unwrap();
    let (decoded, bytes_read): (WireMessage, usize) =
        bincode_next::serde::decode_from_slice(&encoded, config).unwrap();

    assert_eq!(bytes_read, encoded.len());
    assert_eq!(decoded, message);
}

#[test]
fn wire_messages_serialize_roundtrip() {
    print_banner("wire_messages_serialize_roundtrip");
    print_step(1, "execute scenario");
    print_kv("test", "wire_messages_serialize_roundtrip");
    let messages = vec![
        WireMessage::Hello {
            peer_id: "peer-a".to_string(),
        },
        WireMessage::KeyExchange {
            public_key: PublicKeyBytes([7_u8; 32]),
        },
        WireMessage::RequestManifest,
        WireMessage::Manifest {
            manifest: sample_manifest(),
        },
        WireMessage::Have { chunks: vec![0, 2] },
        WireMessage::RequestChunk { index: 1 },
        WireMessage::Chunk {
            index: 0,
            data: b"encrypted-ish bytes".to_vec(),
        },
        WireMessage::Error {
            message: "no such chunk".to_string(),
        },
    ];

    for message in messages {
        assert_binary_codec_roundtrip(message);
    }
}

#[tokio::test]
async fn codec_sends_and_receives_message() {
    print_banner("codec_sends_and_receives_message");
    print_step(1, "execute scenario");
    print_kv("test", "codec_sends_and_receives_message");
    let (mut client, mut server) = tokio::io::duplex(1024);
    let sent = WireMessage::Hello {
        peer_id: "peer-a".to_string(),
    };

    send(&mut client, &sent).await.unwrap();
    let received = receive(&mut server).await.unwrap();

    assert_eq!(received, sent);
}

#[tokio::test]
async fn codec_rejects_empty_frame() {
    print_banner("codec_rejects_empty_frame");
    print_step(1, "execute scenario");
    print_kv("test", "codec_rejects_empty_frame");
    let (mut client, mut server) = tokio::io::duplex(4);

    client.write_all(&0_u32.to_be_bytes()).await.unwrap();

    assert!(matches!(
        receive(&mut server).await,
        Err(ProtocolError::EmptyFrame)
    ));
}

#[tokio::test]
async fn codec_rejects_too_large_frame() {
    print_banner("codec_rejects_too_large_frame");
    print_step(1, "execute scenario");
    print_kv("test", "codec_rejects_too_large_frame");
    let (mut client, mut server) = tokio::io::duplex(4);
    let too_large = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();

    client.write_all(&too_large).await.unwrap();

    assert!(matches!(
        receive(&mut server).await,
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}
