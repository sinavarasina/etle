#[cfg(unix)]
use super::prelude::*;

#[cfg(unix)]
pub(super) struct IpcSocketCleanup {
    socket_path: PathBuf,
}

#[cfg(unix)]
impl IpcSocketCleanup {
    pub(super) fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }
}

#[cfg(unix)]
impl Drop for IpcSocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}
