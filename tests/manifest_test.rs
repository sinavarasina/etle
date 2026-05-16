use etle::{
    crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId},
    },
    file::manifest::{ChunkMeta, Manifest},
};

#[test]
fn manifest_serialization_roundtrip() {
    let manifest = Manifest {
        file_id: FileId([1_u8; 32]),
        file_name: "miku.mp4".into(),
        file_size: 4096,
        chunk_size: 1024,
        chunks: vec![ChunkMeta {
            index: 0,
            plain_size: 1024,
            encrypted_size: 1040,
            nonce: Nonce([2_u8; 24]),
            blake3_hash: ChunkHash([3_u8; 32]),
        }],
    };

    let encoded = manifest.to_bytes().unwrap();
    let decoded = Manifest::from_bytes(&encoded).unwrap();

    assert_eq!(decoded, manifest);
}
