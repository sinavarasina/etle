#[cfg(unix)]
use super::prelude::*;

#[cfg(unix)]
pub(super) struct IpcSocketCleanup {
    socket_path: PathBuf,
    _lock_file: fs::File,
}

#[cfg(unix)]
impl IpcSocketCleanup {
    pub(super) fn new(socket_path: PathBuf, lock_file: fs::File) -> Self {
        Self {
            socket_path,
            _lock_file: lock_file,
        }
    }
}

#[cfg(unix)]
impl Drop for IpcSocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}
