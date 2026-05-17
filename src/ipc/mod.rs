pub mod codec;
pub mod error;
pub mod message;
pub mod path;

pub use codec::{MAX_IPC_FRAME_SIZE, receive_ipc_message, send_ipc_message};
pub use error::IpcError;
pub use message::{IpcCommand, IpcEvent, IpcResponse, IpcShareSummary};
pub use path::{DEFAULT_IPC_SOCKET_FILE_NAME, default_ipc_socket_path, default_windows_pipe_name};
