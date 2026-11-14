use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::file::descriptor::ShareId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcCommand {
    Ping,
    GetStatus,
    ListShares,
    SeedFile {
        input: PathBuf,
        chunk_size: usize,
    },
    StartServing {
        listen: SocketAddr,
    },
    StopServing,
    DeleteShare {
        share_id: ShareId,
    },
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
    Status(IpcDaemonStatus),
    Ack {
        message: String,
    },
    Shares {
        shares: Vec<IpcShareSummary>,
    },
    ShareAdded {
        share: IpcShareSummary,
    },
    ShareDeleted {
        share_id: ShareId,
        name: String,
    },
    SeedQueued {
        job_id: String,
        label: String,
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
    pub total_bytes: u64,
    pub chunk_size: u64,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub has_secret: bool,
    pub mode: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcJobKind {
    Seed,
    Download,
    Upload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcActiveJob {
    pub job_id: String,
    pub kind: IpcJobKind,
    pub share_id: Option<ShareId>,
    pub label: String,
    pub phase: String,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub bytes_done: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcDaemonStatus {
    pub peer_id: String,
    pub listen: Option<SocketAddr>,
    pub library_root: PathBuf,
    pub discovery_port: u16,
    pub auto_discovery: bool,
    pub auth_required: bool,
    pub share_count: usize,
    pub active_jobs: Vec<IpcActiveJob>,
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
    ShareDeleted {
        share_id: ShareId,
    },
    PeerConnected {
        peer_id: String,
    },
    JobStarted {
        job: IpcActiveJob,
    },
    JobCompleted {
        job_id: String,
        kind: IpcJobKind,
        share_id: Option<ShareId>,
        label: String,
        summary: String,
    },
    JobFailed {
        job_id: String,
        kind: IpcJobKind,
        share_id: Option<ShareId>,
        label: String,
        error: String,
    },
    ChunkCompleted {
        share_id: ShareId,
        completed_chunks: usize,
        total_chunks: usize,
    },
    TaskProgress {
        job_id: Option<String>,
        task: String,
        label: String,
        completed_chunks: usize,
        total_chunks: usize,
        bytes_done: u64,
        total_bytes: u64,
        bytes_per_second: u64,
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
