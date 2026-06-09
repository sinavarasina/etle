use std::{
    collections::{BTreeSet, VecDeque},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};

use etle::{
    config::load,
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    ipc::{
        message::{IpcCommand, IpcEvent, IpcResponse, IpcShareSummary},
        path::default_ipc_socket_path,
    },
    state::paths::default_library_root,
};

use super::format::{fraction, human_bytes, non_empty};

pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 2;
pub const DEFAULT_ACTIVITY_LIMIT: usize = 160;

#[derive(Debug, Clone)]
pub struct GuiInit {
    pub socket_path: String,
    pub auth_psk: String,
    pub download_parallelism: usize,
    pub request_window: usize,
    pub discovery_port: u16,
    pub discovery_timeout_ms: u64,
    pub discovery_multicast: Ipv4Addr,
    pub startup_warning: Option<String>,
}

impl Default for GuiInit {
    fn default() -> Self {
        let mut startup_warning = None;
        let config = match load::load() {
            Ok(config) => config,
            Err(error) => {
                startup_warning = Some(format!(
                    "config: failed to load config.toml; using built-in defaults ({error})"
                ));
                Default::default()
            }
        };

        let library_root = config.library_root().unwrap_or_else(default_library_root);
        let socket_path = config
            .ipc_socket()
            .unwrap_or_else(|| default_ipc_socket_path(&library_root));
        let auth_psk = std::env::var("ETLE_AUTH_PSK")
            .ok()
            .or_else(|| config.auth_psk())
            .unwrap_or_default();

        Self {
            socket_path: socket_path.display().to_string(),
            auth_psk,
            download_parallelism: config.parallel(),
            request_window: config.request_window(),
            discovery_port: config.discovery_port(),
            discovery_timeout_ms: config.discovery_timeout_ms(),
            discovery_multicast: config.discovery_multicast(),
            startup_warning,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferKind {
    Seed,
    Download,
}

impl TransferKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Download => "download",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferStatus {
    Queued,
    Running,
    Done,
    Failed,
}

impl TransferStatus {
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Queued => "…",
            Self::Running => "↻",
            Self::Done => "✓",
            Self::Failed => "✕",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }
}

#[derive(Clone, Debug)]
pub struct GuiTransfer {
    pub id: String,
    pub kind: TransferKind,
    pub label: String,
    pub share_id: Option<ShareId>,
    pub status: TransferStatus,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub bytes_done: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
    pub detail: String,
    pub updated_seq: u64,
}

impl GuiTransfer {
    pub fn fraction(&self) -> f64 {
        if self.total_bytes > 0 {
            fraction(self.bytes_done, self.total_bytes)
        } else if self.total_chunks > 0 {
            fraction(self.completed_chunks as u64, self.total_chunks as u64)
        } else if self.status == TransferStatus::Done {
            1.0
        } else {
            0.0
        }
    }

    pub fn compact_line(&self) -> String {
        let percent = self.fraction() * 100.0;
        let chunks = if self.total_chunks == 0 {
            "chunks --/--".to_string()
        } else {
            format!("chunks {}/{}", self.completed_chunks, self.total_chunks)
        };
        let bytes = if self.total_bytes == 0 {
            "bytes --/--".to_string()
        } else {
            format!(
                "bytes {}/{}",
                human_bytes(self.bytes_done),
                human_bytes(self.total_bytes)
            )
        };
        let speed = if self.bytes_per_second == 0 {
            "--/s".to_string()
        } else {
            format!("{}/s", human_bytes(self.bytes_per_second))
        };

        let phase = if self.detail.trim().is_empty() {
            self.kind.label().to_string()
        } else {
            self.detail.clone()
        };

        format!(
            "{} {:<7} {:<24} {:>6.1}% · {chunks} · {bytes} · {speed}",
            self.status.icon(),
            self.status.label(),
            phase,
            percent,
        )
    }

    pub fn hide_key(&self) -> String {
        if let Some(share_id) = self.share_id {
            format!("{}:{}:{}", self.kind.label(), self.status.label(), share_id)
        } else {
            format!("{}:{}:{}", self.kind.label(), self.status.label(), self.id)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcRequestKind {
    Ping,
    ListShares,
    Seed,
    Download,
}

impl IpcRequestKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::ListShares => "refresh",
            Self::Seed => "seed",
            Self::Download => "download",
        }
    }
}

pub struct EtleGui {
    pub active_socket_path: String,
    pub socket_draft: String,
    pub connected: bool,
    pub watching: bool,
    pub watch_generation: u64,
    pub auto_refresh: bool,
    pub clear_activity_on_task: bool,
    pub refresh_interval_secs: u64,
    pub activity_limit: usize,
    pub selected_share: Option<usize>,
    pub shares: Vec<IpcShareSummary>,
    pub transfers: Vec<GuiTransfer>,
    pub hidden_finished_transfers: BTreeSet<String>,
    pub activity: VecDeque<String>,
    pub last_activity_message: Option<String>,
    pub status: String,
    pub latest_progress: String,
    pub progress_fraction: f64,
    pub next_transfer_seq: u64,
    pub last_auto_refresh: Instant,
    pub refresh_inflight: bool,

    pub seed_path: String,
    pub seed_files: Vec<PathBuf>,
    pub selected_seed_file: Option<usize>,
    pub seed_chunk_size: usize,

    pub download_share_id: String,
    pub download_peers: String,
    pub output_path: String,
    pub download_parallelism: usize,
    pub download_request_window: usize,
    pub discovery_port: u16,
    pub discovery_timeout_ms: u64,
    pub discovery_multicast: Ipv4Addr,
    pub resume: bool,
    pub auth_psk: String,
}

impl EtleGui {
    pub fn new(init: GuiInit) -> Self {
        let mut model = Self {
            active_socket_path: init.socket_path.clone(),
            socket_draft: init.socket_path,
            connected: false,
            watching: false,
            watch_generation: 0,
            auto_refresh: true,
            clear_activity_on_task: true,
            refresh_interval_secs: DEFAULT_REFRESH_INTERVAL_SECS,
            activity_limit: DEFAULT_ACTIVITY_LIMIT,
            selected_share: None,
            shares: Vec::new(),
            transfers: Vec::new(),
            hidden_finished_transfers: BTreeSet::new(),
            activity: VecDeque::new(),
            last_activity_message: None,
            status: "offline".to_string(),
            latest_progress: "idle".to_string(),
            progress_fraction: 0.0,
            next_transfer_seq: 1,
            last_auto_refresh: Instant::now(),
            refresh_inflight: false,

            seed_path: String::new(),
            seed_files: Vec::new(),
            selected_seed_file: None,
            seed_chunk_size: DEFAULT_CHUNK_SIZE,

            download_share_id: String::new(),
            download_peers: String::new(),
            output_path: String::new(),
            download_parallelism: init.download_parallelism,
            download_request_window: init.request_window,
            discovery_port: init.discovery_port,
            discovery_timeout_ms: init.discovery_timeout_ms,
            discovery_multicast: init.discovery_multicast,
            resume: true,
            auth_psk: init.auth_psk,
        };

        if let Some(warning) = init.startup_warning {
            model.push_log(warning);
        }

        model
    }

    pub fn refresh_due(&self) -> bool {
        self.auto_refresh
            && !self.refresh_inflight
            && self.last_auto_refresh.elapsed()
                >= Duration::from_secs(self.refresh_interval_secs.max(1))
    }

    pub fn build_download_command_from(
        &self,
        share_id_text: &str,
        peers_text: &str,
        output_text: &str,
        auth_psk_text: &str,
        discovery_multicast_text: &str,
    ) -> Result<(IpcCommand, ShareId), String> {
        let share_id =
            ShareId::from_str(share_id_text.trim()).map_err(|_| "invalid share id".to_string())?;
        let peers = parse_peers(peers_text)?;
        let output = non_empty(output_text.trim()).map(PathBuf::from);
        let auth_psk = non_empty(auth_psk_text.trim())
            .or_else(|| non_empty(&self.auth_psk))
            .map(ToOwned::to_owned);
        let discovery_multicast = discovery_multicast_text
            .trim()
            .parse()
            .map_err(|error| format!("invalid multicast address: {error}"))?;

        let command = if self.resume {
            IpcCommand::Download {
                share_id,
                peers,
                output,
                parallelism: self.download_parallelism,
                request_window: self.download_request_window.max(1),
                discovery_port: self.discovery_port,
                discovery_timeout_ms: self.discovery_timeout_ms.max(1),
                discovery_multicast,
                auth_psk,
            }
        } else {
            IpcCommand::DownloadFresh {
                share_id,
                peers,
                output,
                parallelism: self.download_parallelism,
                request_window: self.download_request_window.max(1),
                discovery_port: self.discovery_port,
                discovery_timeout_ms: self.discovery_timeout_ms.max(1),
                discovery_multicast,
                auth_psk,
            }
        };

        Ok((command, share_id))
    }
}

#[derive(Debug)]
pub enum AppInput {
    ApplySocketText(String),
    Connect,
    Refresh,
    AutoRefreshTick,
    StartWatch,
    IpcResponse {
        socket_path: String,
        kind: IpcRequestKind,
        result: Result<IpcResponse, String>,
    },
    IpcEvent {
        generation: u64,
        event: IpcEvent,
    },
    IpcWatchStopped {
        generation: u64,
        error: String,
    },

    SelectShare(usize),
    CopySelectedShareId,
    ClearLog,
    ClearFinishedTransfers,

    AddSeedPathText(String),
    AddSeedFile(PathBuf),
    BrowseSeedFiles,
    SelectSeedFile(usize),
    RemoveSelectedSeedFile,
    ClearSeedFiles,
    SetSeedChunkSize(usize),
    StartSeedSelected,

    SetParallelism(usize),
    SetRequestWindow(usize),
    SetDiscoveryPort(u16),
    SetDiscoveryTimeout(u64),
    SetResume(bool),
    StartDownloadFromForm {
        share_id: String,
        peers: String,
        output: String,
        auth_psk: String,
        discovery_multicast: String,
    },
    ApplySettings {
        socket_path: String,
        auth_psk: String,
    },

    SetAutoRefresh(bool),
    SetClearActivityOnTask(bool),
    SetRefreshInterval(u64),
    SetActivityLimit(usize),
}

pub fn parse_peers(input: &str) -> Result<Vec<SocketAddr>, String> {
    input
        .split(|character: char| character == ',' || character == '\n' || character.is_whitespace())
        .filter_map(non_empty)
        .map(|peer| {
            peer.parse::<SocketAddr>()
                .map_err(|error| format!("invalid peer `{peer}`: {error}"))
        })
        .collect()
}
