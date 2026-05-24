pub(super) use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

pub(super) use tokio::{io::AsyncWrite, sync::broadcast};

#[cfg(unix)]
pub(super) use tokio::net::UnixListener;

#[cfg(windows)]
pub(super) use tokio::net::windows::named_pipe::ServerOptions;

pub(super) use crate::{
    discovery::{client, options::DiscoveryOptions},
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId, manifest::Manifest},
    ipc::{
        error::IpcError,
        message::{IpcCommand, IpcEvent, IpcResponse, IpcShareSummary},
    },
    network::transfer::{
        download, jobs,
        options::{DownloadFileOptions, TransferLogLevel},
        seed,
    },
    state::{
        library,
        model::{OUTPUT_DIR_NAME, ShareMode},
        paths::LocalShareSummary,
    },
};

#[cfg(any(unix, windows))]
pub(super) use crate::ipc::codec::{receive_ipc_message, send_ipc_message};
