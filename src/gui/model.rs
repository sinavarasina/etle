use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::{Duration, Instant},
};

use etle::{
    config::load,
    file::descriptor::ShareId,
    ipc::{
        message::{IpcDaemonStatus, IpcEvent, IpcJobKind, IpcResponse, IpcShareSummary},
        path::default_ipc_socket_path,
    },
    state::paths::default_library_root,
};

use super::format::{eta_text, fraction, human_bytes};

pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 2;
pub const DEFAULT_ACTIVITY_LIMIT: usize = 200;

#[derive(Debug, Clone)]
pub struct GuiInit {
    pub default_socket_path: String,
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
            .filter(|psk| !psk.trim().is_empty())
            .or_else(|| config.auth_psk().filter(|psk| !psk.trim().is_empty()))
            .unwrap_or_default();
        let socket_text = socket_path.display().to_string();

        Self {
            default_socket_path: socket_text.clone(),
            socket_path: socket_text,
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
pub enum BannerTone {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiBanner {
    pub tone: BannerTone,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferKind {
    Seed,
    Download,
    Upload,
}

impl TransferKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Download => "download",
            Self::Upload => "upload",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Seed => "Seed",
            Self::Download => "Download",
            Self::Upload => "Upload",
        }
    }

    pub const fn from_ipc(kind: IpcJobKind) -> Self {
        match kind {
            IpcJobKind::Seed => Self::Seed,
            IpcJobKind::Download => Self::Download,
            IpcJobKind::Upload => Self::Upload,
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

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferView {
    pub id: String,
    pub kind: TransferKind,
    pub label: String,
    pub status: TransferStatus,
    pub progress_visible: bool,
    pub fraction_milli: u16,
    pub metrics_line: String,
    pub detail: String,
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
    pub started_at: Instant,
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

    pub fn metrics_line(&self) -> String {
        let percent = self.fraction() * 100.0;
        let chunks = if self.total_chunks == 0 {
            "chunks --/--".to_string()
        } else {
            format!("{}/{} chunks", self.completed_chunks, self.total_chunks)
        };
        let bytes = if self.total_bytes == 0 {
            "--/--".to_string()
        } else {
            format!(
                "{} / {}",
                human_bytes(self.bytes_done),
                human_bytes(self.total_bytes)
            )
        };
        let speed = if self.status.is_finished() {
            "0 B/s".to_string()
        } else if self.bytes_per_second == 0 {
            "--/s".to_string()
        } else {
            format!("{}/s", human_bytes(self.bytes_per_second))
        };
        let timing = if self.status.is_active() {
            if let Some(eta) = eta_text(self.bytes_done, self.total_bytes, self.bytes_per_second) {
                format!("ETA {eta}")
            } else {
                format!("{}s", self.started_at.elapsed().as_secs())
            }
        } else {
            format!("{}s", self.started_at.elapsed().as_secs())
        };
        format!("{percent:5.1}% · {bytes} · {chunks} · {speed} · {timing}")
    }

    pub fn compact_line(&self) -> String {
        let percent = self.fraction() * 100.0;
        if self.bytes_per_second > 0 && self.status.is_active() {
            format!("{percent:.1}% · {}/s", human_bytes(self.bytes_per_second))
        } else if self.total_bytes > 0 {
            format!(
                "{percent:.1}% · {} / {}",
                human_bytes(self.bytes_done),
                human_bytes(self.total_bytes)
            )
        } else if self.total_chunks > 0 {
            format!(
                "{percent:.1}% · {}/{} chunks",
                self.completed_chunks, self.total_chunks
            )
        } else {
            self.detail.clone()
        }
    }

    pub fn to_view(&self) -> TransferView {
        let progress_visible =
            self.status.is_active() || self.total_chunks > 0 || self.total_bytes > 0;
        let fraction_milli = (self.fraction().clamp(0.0, 1.0) * 1000.0).round() as u16;
        TransferView {
            id: self.id.clone(),
            kind: self.kind,
            label: self.label.clone(),
            status: self.status,
            progress_visible,
            fraction_milli,
            metrics_line: self.metrics_line(),
            detail: self.detail.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareRowData {
    pub summary: IpcShareSummary,
    pub is_downloading: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedFileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
}

impl SeedFileEntry {
    pub fn from_path(path: PathBuf) -> Self {
        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self { path, size_bytes }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadFormRequest {
    pub share_id: String,
    pub peers: String,
    pub output: String,
    pub auth_psk: String,
    pub discovery_multicast: String,
    pub parallelism: usize,
    pub request_window: usize,
    pub discovery_port: u16,
    pub discovery_timeout_ms: u64,
    pub resume: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaemonSettingsSnapshot {
    pub connected: bool,
    pub watching: bool,
    pub watch_connecting: bool,
    pub active_socket_path: String,
    pub daemon_status: Option<IpcDaemonStatus>,
    pub pending_shutdown_confirm: bool,
    pub auto_refresh: bool,
    pub clear_activity_on_task: bool,
    pub refresh_interval_secs: u64,
    pub activity_limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcRequestKind {
    Ping,
    Seed,
    Download,
    DeleteShare,
    Shutdown,
}

impl IpcRequestKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::Seed => "seed",
            Self::Download => "download",
            Self::DeleteShare => "delete",
            Self::Shutdown => "shutdown",
        }
    }

    pub const fn timeout(self) -> Duration {
        match self {
            Self::Ping | Self::Shutdown => Duration::from_secs(5),
            Self::Seed | Self::Download | Self::DeleteShare => Duration::from_secs(30),
        }
    }
}

#[derive(Debug)]
pub enum AppCmd {
    SyncSnapshot {
        socket_path: String,
        status_result: Result<IpcResponse, String>,
        shares_result: Result<IpcResponse, String>,
    },
    CommandResponse {
        socket_path: String,
        kind: IpcRequestKind,
        result: Result<IpcResponse, String>,
    },
    WatchConnected {
        generation: u64,
    },
    WatchEvent {
        generation: u64,
        event: IpcEvent,
    },
    WatchStopped {
        generation: u64,
        error: String,
    },
}

pub fn parse_peers(raw: &str) -> Result<Vec<SocketAddr>, String> {
    raw.split([',', '\n'])
        .flat_map(str::split_whitespace)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<SocketAddr>()
                .map_err(|_| format!("invalid peer socket address: {entry}"))
        })
        .collect()
}

pub fn share_is_complete(share: &IpcShareSummary) -> bool {
    if share.total_chunks == 0 {
        return matches!(share.mode.as_deref(), Some("seeding" | "completed"));
    }

    share.completed_chunks >= share.total_chunks
}
