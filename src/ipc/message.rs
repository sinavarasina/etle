use std::{net::SocketAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::file::descriptor::ShareId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcCommand {
    Ping,
    ListShares,
    SeedFile {
        input: PathBuf,
        chunk_size: usize,
    },
    StartServing {
        listen: SocketAddr,
    },
    StopServing,
    Download {
        share_id: ShareId,
        peers: Vec<SocketAddr>,
        output: Option<PathBuf>,
        parallelism: usize,
    },
    DownloadFresh {
        share_id: ShareId,
        peers: Vec<SocketAddr>,
        output: Option<PathBuf>,
        parallelism: usize,
    },
    Pause {
        share_id: ShareId,
    },
    Resume {
        share_id: ShareId,
    },
    SubscribeEvents,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcResponse {
    Pong,
    Ack {
        message: String,
    },
    Shares {
        shares: Vec<IpcShareSummary>,
    },
    ShareAdded {
        share: IpcShareSummary,
    },
    TransferQueued {
        share_id: ShareId,
    },
    TransferCompleted {
        share_id: ShareId,
        output: PathBuf,
        file_name: String,
        file_size: u64,
        chunks: usize,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcShareSummary {
    pub share_id: ShareId,
    pub name: String,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub has_secret: bool,
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcEvent {
    ServerStarted {
        listen: SocketAddr,
    },
    ServerStopped,
    ShareUpdated {
        share: IpcShareSummary,
    },
    PeerConnected {
        peer_id: String,
    },
    ChunkCompleted {
        share_id: ShareId,
        completed_chunks: usize,
        total_chunks: usize,
    },
    TransferCompleted {
        share_id: ShareId,
        output: PathBuf,
    },
    Error {
        message: String,
    },
}
