use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{crypto::hash::FileId, file::manifest::ChunkMeta};

pub const ETLE_DESCRIPTOR_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ShareId(pub [u8; 32]);

impl ShareId {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ShareId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseShareIdError;

impl fmt::Display for ParseShareIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "share id must be 64 lowercase/uppercase hexadecimal characters"
        )
    }
}

impl std::error::Error for ParseShareIdError {}

impl FromStr for ShareId {
    type Err = ParseShareIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(ParseShareIdError);
        }

        let value = value.as_bytes();
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            *byte = decode_hex_pair(value[offset], value[offset + 1])?;
        }

        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CryptoSuite {
    #[default]
    XChaCha20Poly1305Blake3X25519V1,
}

impl CryptoSuite {
    #[must_use]
    pub const fn as_domain_tag(self) -> &'static [u8] {
        match self {
            Self::XChaCha20Poly1305Blake3X25519V1 => b"xchacha20poly1305+blake3+x25519:v1",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub offset: u64,
    pub blake3_hash: FileId,
}

impl FileEntry {
    #[must_use]
    pub const fn end_offset(&self) -> u64 {
        self.offset + self.size
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtleDescriptor {
    pub version: u16,
    pub name: String,
    pub share_id: ShareId,
    pub total_size: u64,
    pub chunk_size: u64,
    pub crypto: CryptoSuite,
    pub files: Vec<FileEntry>,
    pub chunks: Vec<ChunkMeta>,
}

impl EtleDescriptor {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        total_size: u64,
        chunk_size: u64,
        files: Vec<FileEntry>,
        chunks: Vec<ChunkMeta>,
    ) -> Self {
        let name = name.into();
        let crypto = CryptoSuite::default();
        let share_id = compute_share_id(
            ETLE_DESCRIPTOR_VERSION,
            &name,
            total_size,
            chunk_size,
            crypto,
            &files,
            &chunks,
        );

        Self {
            version: ETLE_DESCRIPTOR_VERSION,
            name,
            share_id,
            total_size,
            chunk_size,
            crypto,
            files,
            chunks,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (descriptor, _bytes_read): (Self, usize) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

        Ok(descriptor)
    }

    #[must_use]
    pub fn recompute_share_id(&self) -> ShareId {
        compute_share_id(
            self.version,
            &self.name,
            self.total_size,
            self.chunk_size,
            self.crypto,
            &self.files,
            &self.chunks,
        )
    }

    #[must_use]
    pub fn verify_share_id(&self) -> bool {
        self.share_id == self.recompute_share_id()
    }
}

fn compute_share_id(
    version: u16,
    name: &str,
    total_size: u64,
    chunk_size: u64,
    crypto: CryptoSuite,
    files: &[FileEntry],
    chunks: &[ChunkMeta],
) -> ShareId {
    let mut hasher = blake3::Hasher::new();

    hasher.update(b"etle descriptor share id v1");
    hasher.update(&version.to_le_bytes());
    hash_string(&mut hasher, name);
    hasher.update(&total_size.to_le_bytes());
    hasher.update(&chunk_size.to_le_bytes());
    hasher.update(crypto.as_domain_tag());

    hasher.update(&(files.len() as u64).to_le_bytes());
    for file in files {
        hash_string(&mut hasher, &file.path);
        hasher.update(&file.size.to_le_bytes());
        hasher.update(&file.offset.to_le_bytes());
        hasher.update(file.blake3_hash.as_bytes());
    }

    hasher.update(&(chunks.len() as u64).to_le_bytes());
    for chunk in chunks {
        hasher.update(&chunk.index.to_le_bytes());
        hasher.update(&chunk.plain_size.to_le_bytes());
        hasher.update(&chunk.encrypted_size.to_le_bytes());
        hasher.update(chunk.nonce.as_bytes());
        hasher.update(chunk.blake3_hash.as_bytes());
    }

    ShareId(*hasher.finalize().as_bytes())
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    let bytes = value.as_bytes();
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn decode_hex_pair(high: u8, low: u8) -> Result<u8, ParseShareIdError> {
    Ok((decode_hex_nibble(high)? << 4) | decode_hex_nibble(low)?)
}

fn decode_hex_nibble(value: u8) -> Result<u8, ParseShareIdError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ParseShareIdError),
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{
        aead::Nonce,
        hash::{ChunkHash, FileId},
    };

    fn sample_chunk(index: u32) -> ChunkMeta {
        ChunkMeta {
            index,
            plain_size: 4,
            encrypted_size: 20,
            nonce: Nonce([index as u8; 24]),
            blake3_hash: ChunkHash([index as u8; 32]),
        }
    }

    #[test]
    fn share_id_parses_from_display_hex() {
        let share_id = ShareId([0xab_u8; 32]);
        let parsed = share_id.to_string().parse::<ShareId>().unwrap();

        assert_eq!(parsed, share_id);
        assert!("not-a-share-id".parse::<ShareId>().is_err());
    }

    #[test]
    fn descriptor_roundtrip_serialization() {
        let descriptor = EtleDescriptor::new(
            "sample-package",
            4,
            4,
            vec![FileEntry {
                path: "sample.txt".to_string(),
                size: 4,
                offset: 0,
                blake3_hash: FileId([1_u8; 32]),
            }],
            vec![sample_chunk(0)],
        );

        let encoded = descriptor.to_bytes().unwrap();
        let decoded = EtleDescriptor::from_bytes(&encoded).unwrap();

        assert_eq!(decoded, descriptor);
        assert!(decoded.verify_share_id());
    }

    #[test]
    fn share_id_changes_when_chunk_metadata_changes() {
        let first = EtleDescriptor::new(
            "sample-package",
            4,
            4,
            vec![FileEntry {
                path: "sample.txt".to_string(),
                size: 4,
                offset: 0,
                blake3_hash: FileId([1_u8; 32]),
            }],
            vec![sample_chunk(0)],
        );

        let second = EtleDescriptor::new(
            "sample-package",
            4,
            4,
            first.files.clone(),
            vec![sample_chunk(1)],
        );

        assert_ne!(first.share_id, second.share_id);
    }
}
