use std::{
    fs::OpenOptions,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use super::prelude::*;
use super::{
    model::{DownloadProgress, EtleSecret, ShareState},
    paths::LibraryPaths,
};

pub fn write_descriptor(
    paths: &LibraryPaths,
    descriptor: &EtleDescriptor,
) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, descriptor.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    write_file_atomic(&paths.descriptor_path(), &descriptor.to_bytes()?, false)?;
    Ok(())
}

pub fn read_descriptor(paths: &LibraryPaths) -> Result<EtleDescriptor, FileError> {
    let descriptor = EtleDescriptor::from_bytes(&fs::read(paths.descriptor_path())?)?;
    ensure_share_id(paths.share_id, descriptor.share_id)?;
    if !descriptor.verify_share_id() {
        return Err(FileError::ShareIdMismatch {
            expected: descriptor.recompute_share_id(),
            actual: descriptor.share_id,
        });
    }
    Ok(descriptor)
}

pub fn write_secret(paths: &LibraryPaths, secret: &EtleSecret) -> Result<(), FileError> {
    ensure_share_id(paths.share_id, secret.share_id)?;
    fs::create_dir_all(paths.share_dir())?;
    write_file_atomic(&paths.secret_path(), &secret.to_bytes()?, true)?;
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
    write_file_atomic(&paths.progress_path(), &progress.to_bytes()?, false)?;
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
    write_file_atomic(&paths.state_path(), &state.to_bytes()?, false)?;
    Ok(())
}

pub fn read_state(paths: &LibraryPaths) -> Result<ShareState, FileError> {
    let state = ShareState::from_bytes(&fs::read(paths.state_path())?)?;
    ensure_share_id(paths.share_id, state.share_id)?;
    Ok(state)
}

pub fn write_chunk(paths: &LibraryPaths, chunk: &EncryptedChunk) -> Result<PathBuf, FileError> {
    fs::create_dir_all(paths.chunks_dir())?;
    let path = paths.chunk_path(chunk.index);
    write_file_atomic(&path, &chunk.data, false)?;
    Ok(path)
}

pub fn read_chunk(
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
pub fn has_chunk(paths: &LibraryPaths, index: u32) -> bool {
    paths.chunk_path(index).is_file()
}

#[allow(unused_variables)]
fn write_file_atomic(path: &Path, bytes: &[u8], secret: bool) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp_path = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("etle-write"),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        if secret {
            options.mode(0o600);
        }
    }

    let mut file = options.open(&tmp_path)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)?;
    }

    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    Ok(())
}

fn ensure_share_id(expected: ShareId, actual: ShareId) -> Result<(), FileError> {
    if expected == actual {
        Ok(())
    } else {
        Err(FileError::ShareIdMismatch { expected, actual })
    }
}
