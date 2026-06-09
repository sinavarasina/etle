use super::prelude::*;
use super::{
    model::{DownloadProgress, EtleSecret, LIBRARY_DIR_NAME, ShareMode, ShareState},
    paths::{LibraryPaths, LocalShareSummary},
    storage::{
        read_descriptor, read_progress, read_state, write_descriptor, write_progress, write_secret,
        write_state,
    },
};

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

pub fn init(
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

pub fn delete(root: impl AsRef<Path>, share_id: ShareId) -> Result<bool, FileError> {
    let paths = LibraryPaths::for_share(root, share_id);
    let share_dir = paths.share_dir();

    if !share_dir.exists() {
        return Ok(false);
    }

    fs::remove_dir_all(share_dir)?;
    Ok(true)
}
