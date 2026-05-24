use super::model::{DownloadProgress, ShareMode, ShareState};
use super::prelude::*;

pub const ETLE_DIR_NAME: &str = ".etle";

pub use super::model::{
    CHUNK_EXTENSION, CHUNKS_DIR_NAME, DESCRIPTOR_FILE_NAME, DOWNLOADS_DIR_NAME,
    ETLE_DOWNLOADS_DIR_NAME, ETLE_LIBRARY_ROOT_ENV, LIBRARY_DIR_NAME, OUTPUT_DIR_NAME,
    PROGRESS_FILE_NAME, SECRET_FILE_NAME, STATE_FILE_NAME,
};

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
