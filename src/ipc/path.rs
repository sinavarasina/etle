use std::path::{Path, PathBuf};

pub const DEFAULT_IPC_SOCKET_FILE_NAME: &str = "etled.sock";
pub const DEFAULT_WINDOWS_PIPE_NAME: &str = r"\\.\pipe\etled";

/// Returns the default local IPC endpoint.
///
/// Unix-like platforms use a filesystem Unix-domain socket under the ETLE
/// library root. Windows uses a named pipe so `etle-cli` can control the
/// local `etled` daemon without requiring Unix socket support.
#[must_use]
pub fn default_ipc_socket_path(library_root: impl AsRef<Path>) -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(DEFAULT_WINDOWS_PIPE_NAME);
    }

    library_root
        .as_ref()
        .join(crate::state::ETLE_DIR_NAME)
        .join(DEFAULT_IPC_SOCKET_FILE_NAME)
}

#[must_use]
pub const fn default_windows_pipe_name() -> &'static str {
    DEFAULT_WINDOWS_PIPE_NAME
}
