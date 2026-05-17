use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    crypto::aead::SymmetricKey,
    file::{
        descriptor::{EtleDescriptor, ShareId},
        error::FileError,
        storage::EncryptedChunk,
    },
};

pub const ETLE_DIR_NAME: &str = ".etle";
pub const LIBRARY_DIR_NAME: &str = "library";
pub const DESCRIPTOR_FILE_NAME: &str = "descriptor.etle";
pub const SECRET_FILE_NAME: &str = "secret.etlekey";
pub const PROGRESS_FILE_NAME: &str = "progress.bin";
pub const STATE_FILE_NAME: &str = "state.bin";
pub const CHUNKS_DIR_NAME: &str = "chunks";
pub const OUTPUT_DIR_NAME: &str = "output";
pub const CHUNK_EXTENSION: &str = "etle";
pub const ETLE_LIBRARY_ROOT_ENV: &str = "ETLE_LIBRARY_ROOT";
pub const DOWNLOADS_DIR_NAME: &str = "Downloads";
pub const ETLE_DOWNLOADS_DIR_NAME: &str = "ETLE";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareMode {
    Seeding,
    Downloading,
    Completed,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtleSecret {
    pub share_id: ShareId,
    pub file_key: SymmetricKey,
}

impl EtleSecret {
    #[must_use]
    pub const fn new(share_id: ShareId, file_key: SymmetricKey) -> Self {
        Self { share_id, file_key }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (secret, _bytes_read): (Self, usize) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

        Ok(secret)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub share_id: ShareId,
    pub completed_chunks: Vec<u32>,
}

impl DownloadProgress {
    #[must_use]
    pub fn new(share_id: ShareId, completed_chunks: impl Into<Vec<u32>>) -> Self {
        let mut completed_chunks = completed_chunks.into();
        normalize_chunk_list(&mut completed_chunks);

        Self {
            share_id,
            completed_chunks,
        }
    }

    #[must_use]
    pub fn empty(share_id: ShareId) -> Self {
        Self::new(share_id, Vec::new())
    }

    pub fn mark_completed(&mut self, index: u32) {
        self.completed_chunks.push(index);
        normalize_chunk_list(&mut self.completed_chunks);
    }

    #[must_use]
    pub fn has_chunk(&self, index: u32) -> bool {
        self.completed_chunks.binary_search(&index).is_ok()
    }

    #[must_use]
    pub fn is_complete(&self, total_chunks: usize) -> bool {
        self.completed_chunks.len() == total_chunks
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (progress, _bytes_read): (Self, usize) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

        Ok(progress)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareState {
    pub share_id: ShareId,
    pub mode: ShareMode,
    pub output_dir: Option<PathBuf>,
    pub completed_chunks: Vec<u32>,
}

impl ShareState {
    #[must_use]
    pub fn new(
        share_id: ShareId,
        mode: ShareMode,
        output_dir: Option<PathBuf>,
        completed_chunks: impl Into<Vec<u32>>,
    ) -> Self {
        let mut completed_chunks = completed_chunks.into();
        normalize_chunk_list(&mut completed_chunks);

        Self {
            share_id,
            mode,
            output_dir,
            completed_chunks,
        }
    }

    #[must_use]
    pub fn from_progress(
        mode: ShareMode,
        output_dir: Option<PathBuf>,
        progress: &DownloadProgress,
    ) -> Self {
        Self::new(
            progress.share_id,
            mode,
            output_dir,
            progress.completed_chunks.clone(),
        )
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (state, _bytes_read): (Self, usize) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;

        Ok(state)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPaths {
    pub root: PathBuf,
    pub share_id: ShareId,
}

impl LibraryPaths {
    #[must_use]
    pub fn for_share(root: impl AsRef<Path>, share_id: ShareId) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            share_id,
        }
    }

    #[must_use]
    pub fn etle_dir(&self) -> PathBuf {
        self.root.join(ETLE_DIR_NAME)
    }

    #[must_use]
    pub fn library_dir(&self) -> PathBuf {
        self.etle_dir().join(LIBRARY_DIR_NAME)
    }

    #[must_use]
    pub fn share_dir(&self) -> PathBuf {
        self.library_dir().join(self.share_id.to_string())
    }

    #[must_use]
    pub fn descriptor_path(&self) -> PathBuf {
        self.share_dir().join(DESCRIPTOR_FILE_NAME)
    }

    #[must_use]
    pub fn secret_path(&self) -> PathBuf {
        self.share_dir().join(SECRET_FILE_NAME)
    }

    #[must_use]
    pub fn progress_path(&self) -> PathBuf {
        self.share_dir().join(PROGRESS_FILE_NAME)
    }

    #[must_use]
    pub fn state_path(&self) -> PathBuf {
        self.share_dir().join(STATE_FILE_NAME)
    }

    #[must_use]
    pub fn chunks_dir(&self) -> PathBuf {
        self.share_dir().join(CHUNKS_DIR_NAME)
    }

    #[must_use]
    pub fn output_dir(&self) -> PathBuf {
        self.share_dir().join(OUTPUT_DIR_NAME)
    }

    #[must_use]
    pub fn chunk_path(&self, index: u32) -> PathBuf {
        self.chunks_dir()
            .join(format!("{index:06}.{CHUNK_EXTENSION}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalShareSummary {
    pub paths: LibraryPaths,
    pub descriptor: EtleDescriptor,
    pub progress: Option<DownloadProgress>,
    pub state: Option<ShareState>,
    pub has_secret: bool,
}

impl LocalShareSummary {
    #[must_use]
    pub fn completed_chunks(&self) -> usize {
        self.progress
            .as_ref()
            .map_or(0, |progress| progress.completed_chunks.len())
    }

    #[must_use]
    pub fn total_chunks(&self) -> usize {
        self.descriptor.chunks.len()
    }

    #[must_use]
    pub fn mode(&self) -> Option<ShareMode> {
        self.state.as_ref().map(|state| state.mode)
    }
}

/// Returns ETLE's platform-friendly default library root.
///
/// Precedence:
///
/// 1. `ETLE_LIBRARY_ROOT`
/// 2. Windows: `%USERPROFILE%\\Downloads\\ETLE`
/// 3. Unix-like: `$HOME/Downloads/ETLE`
/// 4. Fallback: `./Downloads/ETLE`
#[must_use]
pub fn default_library_root() -> PathBuf {
    if let Some(root) = env::var_os(ETLE_LIBRARY_ROOT_ENV) {
        return PathBuf::from(root);
    }

    home_dir_from_env()
        .map(default_library_root_from_home)
        .unwrap_or_else(|| default_library_root_from_home(Path::new(".")))
}

#[must_use]
pub fn default_library_root_from_home(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref()
        .join(DOWNLOADS_DIR_NAME)
        .join(ETLE_DOWNLOADS_DIR_NAME)
}

#[must_use]
pub fn home_dir_from_env() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(windows_home_from_drive_and_path)
    } else {
        env::var_os("HOME").map(PathBuf::from)
    }
}

fn windows_home_from_drive_and_path() -> Option<PathBuf> {
    let drive = env::var_os("HOMEDRIVE")?;
    let path = env::var_os("HOMEPATH")?;
    Some(PathBuf::from(format!(
        "{}{}",
        drive.to_string_lossy(),
        path.to_string_lossy()
    )))
}

pub fn list_library_shares(root: impl AsRef<Path>) -> Result<Vec<LocalShareSummary>, FileError> {
    let root = root.as_ref();
    let library_dir = root.join(ETLE_DIR_NAME).join(LIBRARY_DIR_NAME);

    if !library_dir.exists() {
        return Ok(Vec::new());
    }

    let mut shares = Vec::new();
    for entry in fs::read_dir(library_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };

        let Ok(share_id) = name.parse::<ShareId>() else {
            continue;
        };

        let paths = LibraryPaths::for_share(root, share_id);
        if !paths.descriptor_path().is_file() {
            continue;
        }

        let descriptor = read_descriptor(&paths)?;
        let progress = if paths.progress_path().is_file() {
            Some(read_progress(&paths)?)
        } else {
            None
        };
        let state = if paths.state_path().is_file() {
            Some(read_state(&paths)?)
        } else {
            None
        };

        shares.push(LocalShareSummary {
            has_secret: paths.secret_path().is_file(),
            paths,
            descriptor,
            progress,
            state,
        });
    }

    shares.sort_by(|left, right| {
        left.descriptor
            .name
            .cmp(&right.descriptor.name)
            .then_with(|| {
                left.descriptor
                    .share_id
                    .to_string()
                    .cmp(&right.descriptor.share_id.to_string())
            })
    });

    Ok(shares)
}

pub fn initialize_share_library(
    root: impl AsRef<Path>,
    descriptor: &EtleDescriptor,
    file_key: SymmetricKey,
    mode: ShareMode,
    output_dir: Option<PathBuf>,
) -> Result<LibraryPaths, FileError> {
    let paths = LibraryPaths::for_share(root, descriptor.share_id);

    fs::create_dir_all(paths.chunks_dir())?;
    fs::create_dir_all(paths.output_dir())?;

    write_descriptor(&paths, descriptor)?;
    write_secret(&paths, &EtleSecret::new(descriptor.share_id, file_key))?;

    let progress = match mode {
        ShareMode::Seeding | ShareMode::Completed => {
            let completed: Vec<u32> = descriptor.chunks.iter().map(|chunk| chunk.index).collect();
            DownloadProgress::new(descriptor.share_id, completed)
        }
        ShareMode::Downloading | ShareMode::Paused => DownloadProgress::empty(descriptor.share_id),
    };

    write_progress(&paths, &progress)?;
    write_state(
        &paths,
        &ShareState::from_progress(mode, output_dir, &progress),
    )?;

    Ok(paths)
}

pub fn write_descriptor(
    paths: &LibraryPaths,
    descriptor: &EtleDescriptor,
) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, descriptor.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    fs::write(paths.descriptor_path(), descriptor.to_bytes()?)?;
    Ok(())
}

pub fn read_descriptor(paths: &LibraryPaths) -> Result<EtleDescriptor, FileError> {
    let descriptor = EtleDescriptor::from_bytes(&fs::read(paths.descriptor_path())?)?;
    ensure_share_id(paths.share_id, descriptor.share_id)?;
    Ok(descriptor)
}

pub fn write_secret(paths: &LibraryPaths, secret: &EtleSecret) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, secret.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    fs::write(paths.secret_path(), secret.to_bytes()?)?;
    Ok(())
}

pub fn read_secret(paths: &LibraryPaths) -> Result<EtleSecret, FileError> {
    let secret = EtleSecret::from_bytes(&fs::read(paths.secret_path())?)?;
    ensure_share_id(paths.share_id, secret.share_id)?;
    Ok(secret)
}

pub fn write_progress(paths: &LibraryPaths, progress: &DownloadProgress) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, progress.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    fs::write(paths.progress_path(), progress.to_bytes()?)?;
    Ok(())
}

pub fn read_progress(paths: &LibraryPaths) -> Result<DownloadProgress, FileError> {
    let progress = DownloadProgress::from_bytes(&fs::read(paths.progress_path())?)?;
    ensure_share_id(paths.share_id, progress.share_id)?;
    Ok(progress)
}

pub fn write_state(paths: &LibraryPaths, state: &ShareState) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, state.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    fs::write(paths.state_path(), state.to_bytes()?)?;
    Ok(())
}

pub fn read_state(paths: &LibraryPaths) -> Result<ShareState, FileError> {
    let state = ShareState::from_bytes(&fs::read(paths.state_path())?)?;
    ensure_share_id(paths.share_id, state.share_id)?;
    Ok(state)
}

pub fn write_encrypted_chunk(
    paths: &LibraryPaths,
    chunk: &EncryptedChunk,
) -> Result<PathBuf, FileError> {
    fs::create_dir_all(paths.chunks_dir())?;
    let path = paths.chunk_path(chunk.index);
    fs::write(&path, &chunk.data)?;
    Ok(path)
}

pub fn read_encrypted_chunk(
    paths: &LibraryPaths,
    index: u32,
    expected_size: u64,
) -> Result<EncryptedChunk, FileError> {
    let data = fs::read(paths.chunk_path(index))?;
    let actual = data.len() as u64;

    if actual != expected_size {
        return Err(FileError::ChunkSizeMismatch {
            index,
            expected: expected_size,
            actual,
        });
    }

    Ok(EncryptedChunk { index, data })
}

#[must_use]
pub fn has_encrypted_chunk(paths: &LibraryPaths, index: u32) -> bool {
    paths.chunk_path(index).is_file()
}

fn ensure_share_id(expected: ShareId, actual: ShareId) -> Result<(), FileError> {
    if expected == actual {
        Ok(())
    } else {
        Err(FileError::ShareIdMismatch { expected, actual })
    }
}

fn normalize_chunk_list(chunks: &mut Vec<u32>) {
    chunks.sort_unstable();
    chunks.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{
            aead::{Nonce, SymmetricKey},
            hash::{ChunkHash, FileId},
        },
        file::{descriptor::FileEntry, manifest::ChunkMeta},
    };

    fn temp_dir_name(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
    }

    fn sample_descriptor() -> EtleDescriptor {
        EtleDescriptor::new(
            "sample-package",
            4,
            4,
            vec![FileEntry {
                path: "sample.txt".to_string(),
                size: 4,
                offset: 0,
                blake3_hash: FileId([1_u8; 32]),
            }],
            vec![ChunkMeta {
                index: 0,
                plain_size: 4,
                encrypted_size: 6,
                nonce: Nonce([2_u8; 24]),
                blake3_hash: ChunkHash([3_u8; 32]),
            }],
        )
    }

    #[test]
    fn initializes_share_library_layout() {
        let root = temp_dir_name("state-init");
        let _ = fs::remove_dir_all(&root);
        let descriptor = sample_descriptor();
        let key = SymmetricKey([9_u8; 32]);

        let paths = initialize_share_library(
            &root,
            &descriptor,
            key,
            ShareMode::Downloading,
            Some(root.join("out")),
        )
        .unwrap();

        assert!(paths.descriptor_path().is_file());
        assert!(paths.secret_path().is_file());
        assert!(paths.progress_path().is_file());
        assert!(paths.state_path().is_file());
        assert!(paths.chunks_dir().is_dir());
        assert!(paths.output_dir().is_dir());

        assert_eq!(read_descriptor(&paths).unwrap(), descriptor);
        assert_eq!(read_secret(&paths).unwrap().file_key, key);
        assert_eq!(
            read_progress(&paths).unwrap().completed_chunks,
            Vec::<u32>::new()
        );
        assert_eq!(read_state(&paths).unwrap().mode, ShareMode::Downloading);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lists_local_library_shares() {
        let root = temp_dir_name("state-list");
        let _ = fs::remove_dir_all(&root);
        let descriptor = sample_descriptor();
        let key = SymmetricKey([9_u8; 32]);

        let paths =
            initialize_share_library(&root, &descriptor, key, ShareMode::Seeding, None).unwrap();

        let shares = list_library_shares(&root).unwrap();

        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].descriptor, descriptor);
        assert_eq!(shares[0].paths, paths);
        assert_eq!(shares[0].mode(), Some(ShareMode::Seeding));
        assert_eq!(shares[0].completed_chunks(), 1);
        assert_eq!(shares[0].total_chunks(), 1);
        assert!(shares[0].has_secret);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn progress_sorts_and_deduplicates_completed_chunks() {
        let descriptor = sample_descriptor();
        let mut progress = DownloadProgress::new(descriptor.share_id, vec![2, 1, 2, 0]);

        assert_eq!(progress.completed_chunks, vec![0, 1, 2]);
        assert!(progress.has_chunk(1));
        assert!(!progress.has_chunk(3));

        progress.mark_completed(3);
        assert_eq!(progress.completed_chunks, vec![0, 1, 2, 3]);
    }

    #[test]
    fn encrypted_chunk_storage_roundtrip() {
        let root = temp_dir_name("state-chunk");
        let _ = fs::remove_dir_all(&root);
        let descriptor = sample_descriptor();
        let paths = LibraryPaths::for_share(&root, descriptor.share_id);
        let chunk = EncryptedChunk {
            index: 0,
            data: b"abcdef".to_vec(),
        };

        write_encrypted_chunk(&paths, &chunk).unwrap();

        assert!(has_encrypted_chunk(&paths, 0));
        assert_eq!(read_encrypted_chunk(&paths, 0, 6).unwrap(), chunk);
        assert!(matches!(
            read_encrypted_chunk(&paths, 0, 5),
            Err(FileError::ChunkSizeMismatch { index: 0, .. })
        ));

        fs::remove_dir_all(root).unwrap();
    }
}
