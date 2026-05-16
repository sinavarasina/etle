use serde::{Deserialize, Serialize};

use crate::crypto::{
    aead::Nonce,
    hash::{ChunkHash, FileId},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkMeta {
    pub index: u32,
    pub plain_size: u64,
    pub encrypted_size: u64,
    pub nonce: Nonce,
    pub blake3_hash: ChunkHash,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub file_id: FileId,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u64,
    pub chunks: Vec<ChunkMeta>,
}

impl Manifest {
    pub fn to_bytes(&self) -> Result<Vec<u8>, Box<bincode::ErrorKind>> {
        bincode::serialize(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<bincode::ErrorKind>> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_serialization() {
        let manifest = Manifest {
            file_id: FileId([1_u8; 32]),
            file_name: "sample.bin".to_string(),
            file_size: 123,
            chunk_size: 64,
            chunks: vec![ChunkMeta {
                index: 0,
                plain_size: 64,
                encrypted_size: 80,
                nonce: Nonce([2_u8; 24]),
                blake3_hash: ChunkHash([3_u8; 32]),
            }],
        };

        let encoded = manifest.to_bytes().unwrap();
        let decoded = Manifest::from_bytes(&encoded).unwrap();

        assert_eq!(decoded, manifest);
    }
}
