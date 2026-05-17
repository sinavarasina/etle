use tokio::io::AsyncWriteExt;

use etle::{
    crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId},
        key_exchange::PublicKeyBytes,
    },
    file::manifest::{ChunkMeta, Manifest},
    protocol::{MAX_FRAME_SIZE, ProtocolError, WireMessage, receive_message, send_message},
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

fn assert_bincode_roundtrip(message: WireMessage) {
    let encoded = bincode::serialize(&message).unwrap();
    let decoded: WireMessage = bincode::deserialize(&encoded).unwrap();

    assert_eq!(decoded, message);
}

#[test]
fn wire_messages_serialize_roundtrip() {
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
        assert_bincode_roundtrip(message);
    }
}

#[tokio::test]
async fn codec_sends_and_receives_message() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    let sent = WireMessage::Hello {
        peer_id: "peer-a".to_string(),
    };

    send_message(&mut client, &sent).await.unwrap();
    let received = receive_message(&mut server).await.unwrap();

    assert_eq!(received, sent);
}

#[tokio::test]
async fn codec_rejects_empty_frame() {
    let (mut client, mut server) = tokio::io::duplex(4);

    client.write_all(&0_u32.to_be_bytes()).await.unwrap();

    assert!(matches!(
        receive_message(&mut server).await,
        Err(ProtocolError::EmptyFrame)
    ));
}

#[tokio::test]
async fn codec_rejects_too_large_frame() {
    let (mut client, mut server) = tokio::io::duplex(4);
    let too_large = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();

    client.write_all(&too_large).await.unwrap();

    assert!(matches!(
        receive_message(&mut server).await,
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}
