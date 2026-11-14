use std::{
    io::ErrorKind,
    sync::atomic::{AtomicU64, Ordering},
};

use super::prelude::*;
use super::{
    model::{DownloadProgress, EtleSecret, LIBRARY_DIR_NAME, ShareMode, ShareState},
    paths::{LibraryPaths, LocalShareSummary},
    storage::{
        read_descriptor, read_progress, read_state, write_descriptor, write_progress, write_secret,
        write_state,
    },
};

static DELETE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn list(root: impl AsRef<Path>) -> Result<Vec<LocalShareSummary>, FileError> {
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
        if name != share_id.to_string() {
            continue;
        }

        let paths = LibraryPaths::for_share(root, share_id);
        let Some(summary) = read_share_summary(&paths) else {
            continue;
        };
        shares.push(summary);
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

fn read_share_summary(paths: &LibraryPaths) -> Option<LocalShareSummary> {
    let descriptor = match read_descriptor(paths) {
        Ok(descriptor) => descriptor,
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!(
                "[state] warning: skipping corrupted share descriptor {}: {error}",
                paths.share_id
            );
            return None;
        }
    };

    let progress = match read_progress(paths) {
        Ok(progress) => Some(progress),
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!(
                "[state] warning: skipping share {} due to unreadable progress: {error}",
                paths.share_id
            );
            return None;
        }
    };

    let state = match read_state(paths) {
        Ok(state) => Some(state),
        Err(FileError::Io(error)) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!(
                "[state] warning: skipping share {} due to unreadable state: {error}",
                paths.share_id
            );
            return None;
        }
    };

    Some(LocalShareSummary {
        has_secret: paths.secret_path().is_file(),
        paths: paths.clone(),
        descriptor,
        progress,
        state,
    })
}

pub fn init(
    root: impl AsRef<Path>,
    descriptor: &EtleDescriptor,
    file_key: SymmetricKey,
    mode: ShareMode,
    output_dir: Option<PathBuf>,
) -> Result<LibraryPaths, FileError> {
    if !descriptor.verify_share_id() {
        return Err(FileError::ShareIdMismatch {
            expected: descriptor.recompute_share_id(),
            actual: descriptor.share_id,
        });
    }

    let paths = LibraryPaths::for_share(root, descriptor.share_id);
    if paths.chunks_dir().is_dir() {
        clear_existing_chunk_files(&paths.chunks_dir())?;
    }

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

fn clear_existing_chunk_files(chunks_dir: &Path) -> Result<(), FileError> {
    for entry in fs::read_dir(chunks_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            match fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

pub fn delete(root: impl AsRef<Path>, share_id: ShareId) -> Result<bool, FileError> {
    let paths = LibraryPaths::for_share(root, share_id);
    let share_dir = paths.share_dir();
    let library_dir = paths.library_dir();
    let seq = DELETE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let tombstone = library_dir.join(format!(".deleting-{share_id}-{}-{seq}", std::process::id()));

    match fs::rename(&share_dir, &tombstone) {
        Ok(()) => {
            fs::remove_dir_all(&tombstone)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
