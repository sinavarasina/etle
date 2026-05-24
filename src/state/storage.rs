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

pub fn write_chunk(paths: &LibraryPaths, chunk: &EncryptedChunk) -> Result<PathBuf, FileError> {
    fs::create_dir_all(paths.chunks_dir())?;
    let path = paths.chunk_path(chunk.index);
    fs::write(&path, &chunk.data)?;
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

fn ensure_share_id(expected: ShareId, actual: ShareId) -> Result<(), FileError> {
    if expected == actual {
        Ok(())
    } else {
        Err(FileError::ShareIdMismatch { expected, actual })
    }
}
