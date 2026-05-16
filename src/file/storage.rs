use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    crypto::{
        aead::{build_chunk_aad, decrypt_chunk, encrypt_chunk, generate_nonce, SymmetricKey},
        hash::{hash_chunk, hash_file, FileId},
    },
    file::{
        chunker::{join_chunks, read_file_chunks, PlainChunk},
        error::FileError,
        manifest::{ChunkMeta, Manifest},
    },
};

pub const DEBUG_MANIFEST_FILE_NAME: &str = "manifest.bin";
pub const DEBUG_CHUNKS_DIR_NAME: &str = "chunks";
pub const DEBUG_CHUNK_EXTENSION: &str = "etle";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedChunk {
    pub index: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedFile {
    pub manifest: Manifest,
    pub chunks: BTreeMap<u32, EncryptedChunk>,
}

pub fn encrypt_file(
    path: impl AsRef<Path>,
    key: &SymmetricKey,
    chunk_size: usize,
) -> Result<EncryptedFile, FileError> {
    let path = path.as_ref();
    let file_id = hash_file(path)?;
    let file_size = fs::metadata(path)?.len();
    let file_name = file_name(path);
    let plain_chunks = read_file_chunks(path, chunk_size)?;

    let mut chunks = BTreeMap::new();
    let mut chunk_metas = Vec::with_capacity(plain_chunks.len());

    for chunk in plain_chunks {
        let nonce = generate_nonce();
        let aad = build_chunk_aad(file_id, chunk.index, chunk.data.len() as u64);
        let ciphertext = encrypt_chunk(key, nonce, &chunk.data, &aad)?;
        let encrypted_hash = hash_chunk(&ciphertext);

        chunk_metas.push(ChunkMeta {
            index: chunk.index,
            plain_size: chunk.data.len() as u64,
            encrypted_size: ciphertext.len() as u64,
            nonce,
            blake3_hash: encrypted_hash,
        });

        chunks.insert(
            chunk.index,
            EncryptedChunk {
                index: chunk.index,
                data: ciphertext,
            },
        );
    }

    Ok(EncryptedFile {
        manifest: Manifest {
            file_id,
            file_name,
            file_size,
            chunk_size: chunk_size as u64,
            chunks: chunk_metas,
        },
        chunks,
    })
}

pub fn decrypt_to_bytes(encrypted: &EncryptedFile, key: &SymmetricKey) -> Result<Vec<u8>, FileError> {
    let mut plain_chunks = Vec::with_capacity(encrypted.manifest.chunks.len());

    for meta in &encrypted.manifest.chunks {
        let encrypted_chunk = encrypted
            .chunks
            .get(&meta.index)
            .ok_or(FileError::MissingChunk(meta.index))?;

        let actual_hash = hash_chunk(&encrypted_chunk.data);
        if actual_hash != meta.blake3_hash {
            return Err(FileError::ChunkHashMismatch(meta.index));
        }

        let aad = build_chunk_aad(encrypted.manifest.file_id, meta.index, meta.plain_size);
        let plaintext = decrypt_chunk(key, meta.nonce, &encrypted_chunk.data, &aad)?;

        plain_chunks.push(PlainChunk {
            index: meta.index,
            data: plaintext,
        });
    }

    let output = join_chunks(&plain_chunks);
    let output_hash = FileId(crate::crypto::hash::hash_bytes(&output));

    if output_hash != encrypted.manifest.file_id {
        return Err(FileError::FinalHashMismatch);
    }

    Ok(output)
}

pub fn decrypt_to_file(
    encrypted: &EncryptedFile,
    key: &SymmetricKey,
    output_path: impl AsRef<Path>,
) -> Result<(), FileError> {
    let output = decrypt_to_bytes(encrypted, key)?;
    fs::write(output_path, output)?;
    Ok(())
}

pub fn write_debug_workspace(
    encrypted: &EncryptedFile,
    root: impl AsRef<Path>,
) -> Result<(), FileError> {
    let root = root.as_ref();
    let chunks_dir = debug_chunks_dir(root);

    fs::create_dir_all(&chunks_dir)?;
    fs::write(debug_manifest_path(root), encrypted.manifest.to_bytes()?)?;

    for meta in &encrypted.manifest.chunks {
        let chunk = encrypted
            .chunks
            .get(&meta.index)
            .ok_or(FileError::MissingChunk(meta.index))?;

        fs::write(debug_chunk_path(root, meta.index), &chunk.data)?;
    }

    Ok(())
}

pub fn read_debug_workspace(root: impl AsRef<Path>) -> Result<EncryptedFile, FileError> {
    let root = root.as_ref();
    let manifest_bytes = fs::read(debug_manifest_path(root))?;
    let manifest = Manifest::from_bytes(&manifest_bytes)?;
    let mut chunks = BTreeMap::new();

    for meta in &manifest.chunks {
        let data = fs::read(debug_chunk_path(root, meta.index))?;
        let actual = data.len() as u64;

        if actual != meta.encrypted_size {
            return Err(FileError::ChunkSizeMismatch {
                index: meta.index,
                expected: meta.encrypted_size,
                actual,
            });
        }

        chunks.insert(
            meta.index,
            EncryptedChunk {
                index: meta.index,
                data,
            },
        );
    }

    Ok(EncryptedFile { manifest, chunks })
}

#[must_use]
pub fn debug_manifest_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(DEBUG_MANIFEST_FILE_NAME)
}

#[must_use]
pub fn debug_chunks_dir(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(DEBUG_CHUNKS_DIR_NAME)
}

#[must_use]
pub fn debug_chunk_path(root: impl AsRef<Path>, index: u32) -> PathBuf {
    debug_chunks_dir(root).join(format!("{index:06}.{DEBUG_CHUNK_EXTENSION}"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string())
}

#[must_use]
pub fn default_output_path(input: impl AsRef<Path>) -> PathBuf {
    let input = input.as_ref();
    let file_name = file_name(input);
    input.with_file_name(format!("reconstructed-{file_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::aead::SymmetricKey;

    fn temp_file_name(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
    }

    #[test]
    fn encrypt_decrypt_reconstructs_same_bytes() {
        let input = temp_file_name("roundtrip-input.bin");
        fs::write(&input, b"chunk me, encrypt me, restore me").unwrap();

        let key = SymmetricKey([5_u8; 32]);
        let encrypted = encrypt_file(&input, &key, 8).unwrap();
        let output = decrypt_to_bytes(&encrypted, &key).unwrap();

        assert_eq!(output, b"chunk me, encrypt me, restore me");

        fs::remove_file(input).unwrap();
    }

    #[test]
    fn tampered_encrypted_chunk_is_rejected() {
        let input = temp_file_name("tamper-input.bin");
        fs::write(&input, b"tamper check").unwrap();

        let key = SymmetricKey([5_u8; 32]);
        let mut encrypted = encrypt_file(&input, &key, 8).unwrap();
        encrypted.chunks.get_mut(&0).unwrap().data[0] ^= 0xff;

        assert!(matches!(
            decrypt_to_bytes(&encrypted, &key),
            Err(FileError::ChunkHashMismatch(0))
        ));

        fs::remove_file(input).unwrap();
    }
}
