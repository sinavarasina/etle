use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

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
        request_window: usize,
        discovery_port: u16,
        discovery_timeout_ms: u64,
        discovery_multicast: Ipv4Addr,
        auth_psk: Option<String>,
    },
    DownloadFresh {
        share_id: ShareId,
        peers: Vec<SocketAddr>,
        output: Option<PathBuf>,
        parallelism: usize,
        request_window: usize,
        discovery_port: u16,
        discovery_timeout_ms: u64,
        discovery_multicast: Ipv4Addr,
        auth_psk: Option<String>,
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
        job_id: String,
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
    TransferProgress {
        job_id: Option<String>,
        share_id: ShareId,
        completed_chunks: usize,
        total_chunks: usize,
        bytes_done: u64,
        total_bytes: u64,
        bytes_per_second: u64,
    },
    TransferCompleted {
        job_id: Option<String>,
        share_id: ShareId,
        output: PathBuf,
    },
    Error {
        message: String,
    },
}
