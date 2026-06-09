mod common;

use common::{print_banner, print_kv, print_result, print_step};
use etle::{
    crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId},
    },
    file::manifest::{ChunkMeta, Manifest},
};

#[test]
fn manifest_serialization_roundtrip() {
    print_banner("manifest_serialization_roundtrip");

    print_step(1, "build manifest");
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
    print_kv("file_name", &manifest.file_name);
    print_kv("file_size", manifest.file_size);
    print_kv("chunk_size", manifest.chunk_size);
    print_kv("chunk_count", manifest.chunks.len());

    print_step(2, "encode manifest");
    let encoded = manifest.to_bytes().unwrap();
    print_kv("encoded_len", encoded.len());

    print_step(3, "decode and verify equality");
    let decoded = Manifest::from_bytes(&encoded).unwrap();
    print_kv("decoded_equal", decoded == manifest);

    assert_eq!(decoded, manifest);
    print_result("manifest_serialization_roundtrip", "ok");
}
