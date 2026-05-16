use serde::{Deserialize, Serialize};
use std::{fmt, fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkHash(pub [u8; 32]);

impl FileId {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl ChunkHash {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Display for ChunkHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }

    Ok(())
}

#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

#[must_use]
pub fn hash_file_id(bytes: &[u8]) -> FileId {
    FileId(hash_bytes(bytes))
}

#[must_use]
pub fn hash_chunk(bytes: &[u8]) -> ChunkHash {
    ChunkHash(hash_bytes(bytes))
}

pub fn hash_file(path: impl AsRef<Path>) -> std::io::Result<FileId> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    Ok(FileId(*hasher.finalize().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_has_same_hash() {
        assert_eq!(hash_bytes(b"miku"), hash_bytes(b"miku"));
    }

    #[test]
    fn different_input_has_different_hash() {
        assert_ne!(hash_bytes(b"miku"), hash_bytes(b"teto"));
    }
}
