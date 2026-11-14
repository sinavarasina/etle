use std::{
    cmp::Reverse,
    collections::{BTreeSet, VecDeque},
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};

use etle::{
    file::{chunker::DEFAULT_CHUNK_SIZE, descriptor::ShareId},
    ipc::message::{
        IpcActiveJob, IpcCommand, IpcDaemonStatus, IpcEvent, IpcJobKind, IpcResponse,
        IpcShareSummary,
    },
};

use super::{
    format::{compact_log_line, file_label, non_empty, short_time, trim_path_label},
    model::{
        BannerTone, DEFAULT_ACTIVITY_LIMIT, DEFAULT_REFRESH_INTERVAL_SECS, DaemonSettingsSnapshot,
        DownloadFormRequest, GuiInit, GuiTransfer, IpcRequestKind, SeedFileEntry, ShareRowData,
        TransferKind, TransferStatus, TransferView, UiBanner, parse_peers, share_is_complete,
    },
    progress::TaskProgressSnapshot,
};

pub struct GuiState {
    pub default_socket_path: String,
    pub active_socket_path: String,
    pub connected: bool,
    pub watching: bool,
    pub watch_connecting: bool,
    pub watch_generation: u64,
    pub watch_retry_ticks: u8,
    pub auto_refresh: bool,
    pub clear_activity_on_task: bool,
    pub refresh_interval_secs: u64,
    pub activity_limit: usize,

    pub daemon_status: Option<IpcDaemonStatus>,
    pub selected_share_id: Option<ShareId>,
    pub pending_delete_share: Option<ShareId>,
    pub pending_shutdown_confirm: bool,
    pub shares: Vec<IpcShareSummary>,
    pub transfers: Vec<GuiTransfer>,
    pub hidden_finished_transfers: BTreeSet<String>,
    pub activity: VecDeque<String>,
    pub activity_revision: u64,
    pub last_activity_message: Option<String>,
    pub banner: Option<UiBanner>,
    pub status: String,
    pub last_offline_logged: bool,
    pub next_transfer_seq: u64,
    pub last_auto_refresh: Instant,
    pub refresh_inflight: bool,

    pub seed_files: Vec<SeedFileEntry>,
    pub selected_seed_file: Option<usize>,
    pub seed_chunk_size: usize,
    pub auth_psk: String,
}

impl GuiState {
    pub fn new(init: GuiInit) -> Self {
        let mut state = Self {
            default_socket_path: init.default_socket_path,
            active_socket_path: init.socket_path,
            connected: false,
            watching: false,
            watch_connecting: false,
            watch_generation: 0,
            watch_retry_ticks: 0,
            auto_refresh: true,
            clear_activity_on_task: false,
            refresh_interval_secs: DEFAULT_REFRESH_INTERVAL_SECS,
            activity_limit: DEFAULT_ACTIVITY_LIMIT,
            daemon_status: None,
            selected_share_id: None,
            pending_delete_share: None,
            pending_shutdown_confirm: false,
            shares: Vec::new(),
            transfers: Vec::new(),
            hidden_finished_transfers: BTreeSet::new(),
            activity: VecDeque::new(),
            activity_revision: 0,
            last_activity_message: None,
            banner: None,
            status: "connecting".to_string(),
            last_offline_logged: false,
            next_transfer_seq: 1,
            last_auto_refresh: Instant::now(),
            refresh_inflight: false,
            seed_files: Vec::new(),
            selected_seed_file: None,
            seed_chunk_size: DEFAULT_CHUNK_SIZE,
            auth_psk: init.auth_psk,
        };

        if let Some(warning) = init.startup_warning {
            state.set_banner(BannerTone::Warning, warning.clone());
            state.push_log(warning);
        }

        state
    }

    pub fn refresh_due(&self) -> bool {
        self.auto_refresh
            && !self.refresh_inflight
            && self.last_auto_refresh.elapsed()
                >= Duration::from_secs(self.refresh_interval_secs.max(1))
    }

    pub fn set_banner(&mut self, tone: BannerTone, message: impl Into<String>) {
        self.banner = Some(UiBanner {
            tone,
            message: message.into(),
        });
    }

    pub fn bump_seq(&mut self) -> u64 {
        let seq = self.next_transfer_seq;
        self.next_transfer_seq = self.next_transfer_seq.saturating_add(1);
        seq
    }

    pub fn push_log(&mut self, message: impl Into<String>) {
        let compact = compact_log_line(message.into());
        if self.last_activity_message.as_deref() == Some(compact.as_str()) {
            return;
        }
        self.last_activity_message = Some(compact.clone());
        self.activity
            .push_back(format!("{}  {compact}", short_time()));
        self.trim_activity();
        self.activity_revision = self.activity_revision.saturating_add(1);
    }

    pub fn trim_activity(&mut self) {
        let before = self.activity.len();
        while self.activity.len() > self.activity_limit {
            let _ = self.activity.pop_front();
        }
        if self.activity.len() != before {
            self.activity_revision = self.activity_revision.saturating_add(1);
        }
    }

    pub fn clear_log(&mut self) {
        self.activity.clear();
        self.last_activity_message = None;
        self.activity_revision = self.activity_revision.saturating_add(1);
    }

    pub fn prepare_new_task_log(&mut self) {
        if self.clear_activity_on_task {
            self.clear_log();
        }
    }

    pub fn activity_text(&self) -> String {
        self.activity.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    pub fn sync_selected_share(&mut self) {
        if let Some(pending) = self.pending_delete_share
            && !self.shares.iter().any(|share| share.share_id == pending)
        {
            self.pending_delete_share = None;
        }
        if let Some(target) = self.selected_share_id
            && self.shares.iter().any(|share| share.share_id == target)
        {
            return;
        }
        self.selected_share_id = self.shares.first().map(|share| share.share_id);
    }

    pub fn active_download_share_ids(&self) -> Vec<ShareId> {
        let mut ids = Vec::new();
        if let Some(status) = &self.daemon_status {
            for job in &status.active_jobs {
                if job.kind == IpcJobKind::Download
                    && let Some(share_id) = job.share_id
                    && !ids.contains(&share_id)
                {
                    ids.push(share_id);
                }
            }
        }
        for transfer in &self.transfers {
            if transfer.kind == TransferKind::Download
                && transfer.status.is_active()
                && let Some(share_id) = transfer.share_id
                && !ids.contains(&share_id)
            {
                ids.push(share_id);
            }
        }
        ids
    }

    pub fn share_row_snapshots(&self) -> Vec<ShareRowData> {
        let downloading = self.active_download_share_ids();
        self.shares
            .iter()
            .cloned()
            .map(|summary| {
                let is_downloading = downloading.contains(&summary.share_id);
                ShareRowData {
                    summary,
                    is_downloading,
                }
            })
            .collect()
    }

    pub fn transfer_view_snapshots(&self) -> Vec<TransferView> {
        self.transfers.iter().map(GuiTransfer::to_view).collect()
    }

    pub fn active_transfers(&self) -> Vec<&GuiTransfer> {
        self.transfers
            .iter()
            .filter(|transfer| transfer.status.is_active())
            .collect()
    }

    pub fn settings_snapshot(&self) -> DaemonSettingsSnapshot {
        DaemonSettingsSnapshot {
            connected: self.connected,
            watching: self.watching,
            watch_connecting: self.watch_connecting,
            active_socket_path: self.active_socket_path.clone(),
            daemon_status: self.daemon_status.clone(),
            pending_shutdown_confirm: self.pending_shutdown_confirm,
            auto_refresh: self.auto_refresh,
            clear_activity_on_task: self.clear_activity_on_task,
            refresh_interval_secs: self.refresh_interval_secs,
            activity_limit: self.activity_limit,
        }
    }

    pub fn add_seed_file(&mut self, path: PathBuf) -> bool {
        if path.as_os_str().is_empty() {
            return false;
        }
        if self.seed_files.iter().any(|existing| existing.path == path) {
            self.set_banner(
                BannerTone::Info,
                format!("Already queued: {}", file_label(&path)),
            );
            return false;
        }
        self.push_log(format!("seed queue: added {}", path.display()));
        self.seed_files.push(SeedFileEntry::from_path(path));
        if self.selected_seed_file.is_none() {
            self.selected_seed_file = Some(0);
        }
        true
    }

    pub fn remove_selected_seed_file(&mut self) {
        let Some(index) = self.selected_seed_file else {
            return;
        };
        if index < self.seed_files.len() {
            let removed = self.seed_files.remove(index);
            self.push_log(format!("seed queue: removed {}", removed.path.display()));
            self.selected_seed_file = if self.seed_files.is_empty() {
                None
            } else {
                Some(index.min(self.seed_files.len() - 1))
            };
        }
    }

    pub fn take_selected_seed_file(&mut self) -> Option<SeedFileEntry> {
        let index = self.selected_seed_file.unwrap_or(0);
        if index >= self.seed_files.len() {
            self.set_banner(BannerTone::Warning, "Add or select a file to seed first.");
            return None;
        }
        let entry = self.seed_files.remove(index);
        self.selected_seed_file = if self.seed_files.is_empty() {
            None
        } else {
            Some(index.min(self.seed_files.len() - 1))
        };
        Some(entry)
    }

    pub fn queue_seed_entry(&mut self, entry: &SeedFileEntry) -> IpcCommand {
        self.prepare_new_task_log();
        let (label, command) = self.enqueue_seed_transfer(entry);
        self.set_banner(BannerTone::Info, format!("Queued seed job for \"{label}\""));
        command
    }

    pub fn queue_seed_entries(&mut self, entries: &[SeedFileEntry]) -> Vec<IpcCommand> {
        if entries.is_empty() {
            return Vec::new();
        }
        self.prepare_new_task_log();
        let mut commands = Vec::with_capacity(entries.len());
        let mut last_label = String::new();
        for entry in entries {
            let (label, command) = self.enqueue_seed_transfer(entry);
            last_label = label;
            commands.push(command);
        }
        if entries.len() == 1 {
            self.set_banner(
                BannerTone::Info,
                format!("Queued seed job for \"{last_label}\""),
            );
        } else {
            self.set_banner(
                BannerTone::Info,
                format!("Queued {} seed jobs", entries.len()),
            );
        }
        commands
    }

    fn enqueue_seed_transfer(&mut self, entry: &SeedFileEntry) -> (String, IpcCommand) {
        let path_text = entry.path.display().to_string();
        let label = file_label(&entry.path);
        let transfer_seq = self.bump_seq();
        self.upsert_transfer(GuiTransfer {
            id: format!("seed:{label}:{transfer_seq}"),
            kind: TransferKind::Seed,
            label: label.clone(),
            share_id: None,
            status: TransferStatus::Queued,
            completed_chunks: 0,
            total_chunks: 0,
            bytes_done: 0,
            total_bytes: entry.size_bytes,
            bytes_per_second: 0,
            detail: format!("queued · {path_text}"),
            started_at: Instant::now(),
            updated_seq: transfer_seq,
        });
        self.push_log(format!("seed queued: {path_text}"));
        let command = IpcCommand::SeedFile {
            input: entry.path.clone(),
            chunk_size: self.seed_chunk_size.max(1),
        };
        (label, command)
    }

    pub fn build_download_command_from(
        &self,
        req: &DownloadFormRequest,
    ) -> Result<(IpcCommand, ShareId), String> {
        let share_id = ShareId::from_str(req.share_id.trim())
            .map_err(|_| "Share ID must be 64 hexadecimal characters".to_string())?;
        let peers = parse_peers(&req.peers)?;
        let output = non_empty(req.output.trim()).map(PathBuf::from);
        let auth_psk = non_empty(req.auth_psk.trim())
            .or_else(|| non_empty(&self.auth_psk))
            .map(ToOwned::to_owned);
        let discovery_multicast = req
            .discovery_multicast
            .trim()
            .parse()
            .map_err(|error| format!("invalid multicast address: {error}"))?;

        let common_args = (
            share_id,
            peers,
            output,
            req.parallelism,
            req.request_window.max(1),
            req.discovery_port,
            req.discovery_timeout_ms.max(1),
            discovery_multicast,
            auth_psk,
        );

        let command = if req.resume {
            IpcCommand::Download {
                share_id: common_args.0,
                peers: common_args.1,
                output: common_args.2,
                parallelism: common_args.3,
                request_window: common_args.4,
                discovery_port: common_args.5,
                discovery_timeout_ms: common_args.6,
                discovery_multicast: common_args.7,
                auth_psk: common_args.8,
            }
        } else {
            IpcCommand::DownloadFresh {
                share_id: common_args.0,
                peers: common_args.1,
                output: common_args.2,
                parallelism: common_args.3,
                request_window: common_args.4,
                discovery_port: common_args.5,
                discovery_timeout_ms: common_args.6,
                discovery_multicast: common_args.7,
                auth_psk: common_args.8,
            }
        };

        Ok((command, share_id))
    }

    pub fn prepare_download_request(
        &mut self,
        req: &DownloadFormRequest,
    ) -> Result<IpcCommand, String> {
        let (command, share_id) = self.build_download_command_from(req)?;
        self.prepare_new_task_log();
        let transfer_seq = self.bump_seq();
        let known_share = self.shares.iter().find(|s| s.share_id == share_id).cloned();
        let label = known_share
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| share_id.to_string());
        self.upsert_transfer(GuiTransfer {
            id: format!("download:{share_id}"),
            kind: TransferKind::Download,
            label: label.clone(),
            share_id: Some(share_id),
            status: TransferStatus::Queued,
            completed_chunks: known_share.as_ref().map_or(0, |s| s.completed_chunks),
            total_chunks: known_share.as_ref().map_or(0, |s| s.total_chunks),
            bytes_done: 0,
            total_bytes: known_share.as_ref().map_or(0, |s| s.total_bytes),
            bytes_per_second: 0,
            detail: "waiting for daemon".to_string(),
            started_at: Instant::now(),
            updated_seq: transfer_seq,
        });
        self.set_banner(
            BannerTone::Info,
            format!("Download requested for \"{label}\""),
        );
        self.push_log(format!("download queued: {share_id}"));
        Ok(command)
    }

    pub fn clear_finished_transfers(&mut self) {
        let hidden = self
            .transfers
            .iter()
            .filter(|transfer| transfer.status.is_finished())
            .flat_map(transfer_hidden_keys)
            .collect::<Vec<_>>();
        self.hidden_finished_transfers.extend(hidden);
        self.transfers
            .retain(|transfer| !transfer.status.is_finished());
    }

    pub fn apply_sync_snapshot(
        &mut self,
        status_result: Result<IpcResponse, String>,
        shares_result: Result<IpcResponse, String>,
    ) {
        match status_result {
            Ok(IpcResponse::Status(status)) => {
                self.connected = true;
                self.last_offline_logged = false;
                self.daemon_status = Some(status);
            }
            Ok(IpcResponse::Error { message }) => {
                self.push_log(format!("daemon status error: {message}"));
            }
            Ok(_) => {}
            Err(error) => {
                self.daemon_status = None;
                self.push_log(format!("daemon status failed: {error}"));
            }
        }

        match shares_result {
            Ok(IpcResponse::Shares { mut shares }) => {
                sort_shares(&mut shares);
                self.connected = true;
                self.status = "online".to_string();
                self.last_offline_logged = false;
                self.shares = shares;
                self.sync_selected_share();
            }
            Ok(IpcResponse::Error { message }) => {
                self.connected = true;
                self.push_log(format!("library refresh error: {message}"));
            }
            Ok(_) => {}
            Err(error) => {
                self.connected = false;
                self.daemon_status = None;
                self.status = "offline".to_string();
                if !self.last_offline_logged {
                    self.last_offline_logged = true;
                    self.push_log(format!("daemon unreachable: {error}"));
                }
            }
        }

        if self.connected {
            self.reconcile_transfers_from_daemon();
        }
    }

    pub fn apply_ipc_response(
        &mut self,
        kind: IpcRequestKind,
        result: Result<IpcResponse, String>,
    ) {
        match result {
            Ok(IpcResponse::Pong) => {
                self.connected = true;
                self.last_offline_logged = false;
                self.status = "online".to_string();
                self.set_banner(BannerTone::Success, "Daemon reachable (Pong).");
                self.push_log("daemon: pong");
            }
            Ok(IpcResponse::Status(status)) => {
                self.connected = true;
                self.last_offline_logged = false;
                self.daemon_status = Some(status);
                self.reconcile_transfers_from_daemon();
            }
            Ok(IpcResponse::Ack { message }) => {
                self.connected = true;
                self.last_offline_logged = false;
                if kind == IpcRequestKind::Shutdown {
                    self.set_banner(BannerTone::Info, message.clone());
                }
                self.push_log(format!("daemon: {message}"));
            }
            Ok(IpcResponse::Shares { mut shares }) => {
                sort_shares(&mut shares);
                self.connected = true;
                self.last_offline_logged = false;
                self.status = "online".to_string();
                self.shares = shares;
                self.sync_selected_share();
                self.reconcile_transfers_from_daemon();
            }
            Ok(IpcResponse::ShareAdded { share }) => {
                self.connected = true;
                self.upsert_share(share.clone());
                self.sync_share_update_into_transfers(&share);
                self.set_banner(
                    BannerTone::Success,
                    format!("Added share \"{}\"", share.name),
                );
                self.push_log(format!("share added: {} {}", share.share_id, share.name));
            }
            Ok(IpcResponse::ShareDeleted { share_id, name }) => {
                self.connected = true;
                self.remove_share(share_id);
                self.set_banner(BannerTone::Success, format!("Deleted share \"{name}\""));
                self.push_log(format!("share deleted: {share_id} {name}"));
            }
            Ok(IpcResponse::SeedQueued { job_id, label }) => {
                self.connected = true;
                let seq = self.bump_seq();
                let target_idx = self
                    .transfers
                    .iter()
                    .position(|t| t.id == job_id)
                    .or_else(|| {
                        self.transfers.iter().position(|t| {
                            t.kind == TransferKind::Seed
                                && t.status == TransferStatus::Queued
                                && t.id.starts_with("seed:")
                                && t.label == label
                        })
                    });
                if let Some(idx) = target_idx {
                    let transfer = &mut self.transfers[idx];
                    transfer.id = job_id.clone();
                    transfer.status = TransferStatus::Running;
                    transfer.detail = "hashing input file".to_string();
                    transfer.updated_seq = seq;
                    let snapshot = transfer.clone();
                    self.unhide_transfer_keys(&snapshot);
                } else {
                    self.upsert_transfer(GuiTransfer {
                        id: job_id.clone(),
                        kind: TransferKind::Seed,
                        label: label.clone(),
                        share_id: None,
                        status: TransferStatus::Running,
                        completed_chunks: 0,
                        total_chunks: 0,
                        bytes_done: 0,
                        total_bytes: 0,
                        bytes_per_second: 0,
                        detail: "hashing input file".to_string(),
                        started_at: Instant::now(),
                        updated_seq: seq,
                    });
                }
                self.sort_transfers();
                self.push_log(format!("seed running: job={job_id} label=\"{label}\""));
            }
            Ok(IpcResponse::TransferQueued { share_id, job_id }) => {
                self.connected = true;
                let seq = self.bump_seq();
                if let Some(transfer) = self.transfers.iter_mut().find(|t| {
                    t.kind == TransferKind::Download
                        && t.share_id == Some(share_id)
                        && !t.status.is_finished()
                }) {
                    transfer.id = job_id.clone();
                    transfer.status = TransferStatus::Running;
                    transfer.detail = "starting download".to_string();
                    transfer.updated_seq = seq;
                    let snapshot = transfer.clone();
                    self.unhide_transfer_keys(&snapshot);
                } else {
                    self.upsert_transfer(GuiTransfer {
                        id: job_id.clone(),
                        kind: TransferKind::Download,
                        label: self.share_name_or_id(share_id),
                        share_id: Some(share_id),
                        status: TransferStatus::Running,
                        completed_chunks: 0,
                        total_chunks: 0,
                        bytes_done: 0,
                        total_bytes: 0,
                        bytes_per_second: 0,
                        detail: "starting download".to_string(),
                        started_at: Instant::now(),
                        updated_seq: seq,
                    });
                }
                self.sort_transfers();
                self.push_log(format!("queued: job={job_id} share={share_id}"));
            }
            Ok(IpcResponse::TransferCompleted {
                share_id,
                output,
                file_name,
                file_size,
                chunks,
            }) => {
                self.connected = true;
                let seq = self.bump_seq();
                if let Some(transfer) = self.transfers.iter_mut().find(|t| {
                    t.kind == TransferKind::Download
                        && t.share_id == Some(share_id)
                        && !t.status.is_finished()
                }) {
                    transfer.status = TransferStatus::Done;
                    transfer.label = file_name;
                    transfer.completed_chunks = chunks;
                    transfer.total_chunks = chunks;
                    transfer.bytes_done = file_size;
                    transfer.total_bytes = file_size;
                    transfer.bytes_per_second = 0;
                    transfer.detail = format!("saved · {}", output.display());
                    transfer.updated_seq = seq;
                }
                self.push_log(format!(
                    "completed: share={share_id} output={}",
                    output.display()
                ));
                self.sort_transfers();
            }
            Ok(IpcResponse::Error { message }) => {
                if matches!(kind, IpcRequestKind::Seed | IpcRequestKind::Download) {
                    self.mark_recent_running_failed(kind, &message);
                }
                self.set_banner(BannerTone::Error, message.clone());
                self.push_log(format!("daemon error: {message}"));
            }
            Err(error) => match kind {
                IpcRequestKind::Ping => {
                    self.connected = false;
                    self.daemon_status = None;
                    self.status = "offline".to_string();
                    self.set_banner(BannerTone::Error, format!("Daemon unreachable: {error}"));
                    self.push_log(format!("ipc: {error}"));
                }
                IpcRequestKind::Seed | IpcRequestKind::Download => {
                    self.connected = false;
                    self.status = "offline".to_string();
                    self.mark_recent_running_failed(kind, &error);
                    self.set_banner(
                        BannerTone::Error,
                        format!("{} request failed: {error}", kind.label()),
                    );
                    self.push_log(format!("{} request failed: {error}", kind.label()));
                }
                IpcRequestKind::DeleteShare | IpcRequestKind::Shutdown => {
                    self.set_banner(
                        BannerTone::Error,
                        format!("{} request failed: {error}", kind.label()),
                    );
                    self.push_log(format!("{} request failed: {error}", kind.label()));
                }
            },
        }
    }

    pub fn apply_ipc_event(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::ServerStarted { listen } => {
                self.connected = true;
                self.status = "online".to_string();
                self.push_log(format!("server started: {listen}"));
            }
            IpcEvent::ServerStopped => {
                self.connected = false;
                self.watching = false;
                self.watch_connecting = false;
                self.daemon_status = None;
                self.pending_shutdown_confirm = false;
                self.pending_delete_share = None;
                self.status = "offline".to_string();
                self.set_banner(BannerTone::Warning, "Daemon stopped.");
                self.push_log("daemon stopped");
            }
            IpcEvent::ShareUpdated { share } => {
                self.connected = true;
                self.upsert_share(share.clone());
                self.sync_share_update_into_transfers(&share);
            }
            IpcEvent::ShareDeleted { share_id } => {
                self.remove_share(share_id);
                self.push_log(format!("share deleted: {share_id}"));
            }
            IpcEvent::PeerConnected { peer_id } => {
                self.push_log(format!("peer connected: {peer_id}"));
            }
            IpcEvent::JobStarted { job } => {
                self.upsert_from_active_job(&job);
                self.push_log(format!(
                    "job started: [{}] {} ({})",
                    TransferKind::from_ipc(job.kind).label(),
                    job.label,
                    job.job_id
                ));
            }
            IpcEvent::JobCompleted {
                job_id,
                kind,
                share_id,
                label,
                summary,
            } => {
                let target_kind = TransferKind::from_ipc(kind);
                let seq = self.bump_seq();
                let found_name = share_id.and_then(|sid| {
                    self.shares
                        .iter()
                        .find(|s| s.share_id == sid)
                        .map(|s| s.name.clone())
                });
                if let Some(transfer) =
                    self.find_transfer_for_job_mut(&job_id, target_kind, share_id)
                {
                    transfer.status = TransferStatus::Done;
                    if transfer.kind != TransferKind::Upload {
                        if let Some(share_id) = share_id {
                            transfer.share_id = Some(share_id);
                            if let Some(name) = found_name {
                                transfer.label = name;
                            } else if !label.is_empty() {
                                transfer.label = label.clone();
                            }
                        } else if !label.is_empty() {
                            transfer.label = label.clone();
                        }
                    }
                    if transfer.total_chunks > 0 {
                        transfer.completed_chunks = transfer.total_chunks;
                    }
                    if transfer.total_bytes > 0 {
                        transfer.bytes_done = transfer.total_bytes;
                    }
                    transfer.bytes_per_second = 0;
                    transfer.detail = summary.clone();
                    transfer.updated_seq = seq;
                } else {
                    self.upsert_transfer(GuiTransfer {
                        id: job_id,
                        kind: target_kind,
                        label: label.clone(),
                        share_id,
                        status: TransferStatus::Done,
                        completed_chunks: 1,
                        total_chunks: 1,
                        bytes_done: 0,
                        total_bytes: 0,
                        bytes_per_second: 0,
                        detail: summary.clone(),
                        started_at: Instant::now(),
                        updated_seq: seq,
                    });
                }
                if target_kind != TransferKind::Upload {
                    self.set_banner(BannerTone::Success, summary.clone());
                }
                self.push_log(format!("job completed: [{label}] {summary}"));
                self.sort_transfers();
            }
            IpcEvent::JobFailed {
                job_id,
                kind,
                share_id,
                label,
                error,
            } => {
                let target_kind = TransferKind::from_ipc(kind);
                let seq = self.bump_seq();
                if let Some(transfer) =
                    self.find_transfer_for_job_mut(&job_id, target_kind, share_id)
                {
                    transfer.status = TransferStatus::Failed;
                    transfer.bytes_per_second = 0;
                    transfer.detail = error.clone();
                    transfer.updated_seq = seq;
                } else {
                    self.upsert_transfer(GuiTransfer {
                        id: job_id,
                        kind: target_kind,
                        label: label.clone(),
                        share_id,
                        status: TransferStatus::Failed,
                        completed_chunks: 0,
                        total_chunks: 0,
                        bytes_done: 0,
                        total_bytes: 0,
                        bytes_per_second: 0,
                        detail: error.clone(),
                        started_at: Instant::now(),
                        updated_seq: seq,
                    });
                }
                self.set_banner(BannerTone::Error, format!("{label}: {error}"));
                self.push_log(format!("job failed: [{label}] {error}"));
                self.sort_transfers();
            }
            IpcEvent::ChunkCompleted {
                share_id,
                completed_chunks,
                total_chunks,
            } => {
                let seq = self.bump_seq();
                if let Some(transfer) = self.transfers.iter_mut().find(|t| {
                    t.kind == TransferKind::Download
                        && t.share_id == Some(share_id)
                        && !t.status.is_finished()
                }) {
                    transfer.status = TransferStatus::Running;
                    transfer.completed_chunks = completed_chunks;
                    transfer.total_chunks = total_chunks;
                    transfer.detail = "chunk verified".to_string();
                    transfer.updated_seq = seq;
                }
                self.sort_transfers();
            }
            IpcEvent::TaskProgress {
                job_id,
                task,
                label,
                completed_chunks,
                total_chunks,
                bytes_done,
                total_bytes,
                bytes_per_second,
            } => {
                self.apply_task_progress(TaskProgressSnapshot {
                    job_id,
                    task,
                    label,
                    completed_chunks,
                    total_chunks,
                    bytes_done,
                    total_bytes,
                    bytes_per_second,
                });
            }
            IpcEvent::TransferProgress {
                job_id,
                share_id,
                completed_chunks,
                total_chunks,
                bytes_done,
                total_bytes,
                bytes_per_second,
            } => {
                let id = job_id.unwrap_or_else(|| format!("download:{share_id}"));
                let seq = self.bump_seq();
                let label = self.share_name_or_id(share_id);
                let existing_detail = self
                    .transfers
                    .iter()
                    .find(|t| {
                        t.id == id
                            || (t.share_id == Some(share_id)
                                && t.kind == TransferKind::Download
                                && !t.status.is_finished())
                    })
                    .map(|t| t.detail.clone())
                    .unwrap_or_default();
                let detail = if is_specific_progress_detail(&existing_detail) {
                    existing_detail
                } else {
                    "receiving + verifying".to_string()
                };
                self.upsert_transfer(GuiTransfer {
                    id,
                    kind: TransferKind::Download,
                    label,
                    share_id: Some(share_id),
                    status: TransferStatus::Running,
                    completed_chunks,
                    total_chunks,
                    bytes_done,
                    total_bytes,
                    bytes_per_second,
                    detail,
                    started_at: Instant::now(),
                    updated_seq: seq,
                });
            }
            IpcEvent::TransferCompleted {
                job_id,
                share_id,
                output,
            } => {
                let seq = self.bump_seq();
                let transfer_index = job_id
                    .as_deref()
                    .and_then(|id| self.transfers.iter().position(|t| t.id == id))
                    .or_else(|| {
                        self.transfers.iter().position(|t| {
                            t.share_id == Some(share_id) && t.kind == TransferKind::Download
                        })
                    });
                if let Some(index) = transfer_index {
                    let share_snapshot =
                        self.shares.iter().find(|s| s.share_id == share_id).cloned();
                    let transfer = &mut self.transfers[index];
                    transfer.status = TransferStatus::Done;
                    transfer.share_id = Some(share_id);
                    if let Some(share) = share_snapshot {
                        transfer.label = share.name;
                        transfer.completed_chunks = share.completed_chunks;
                        transfer.total_chunks = share.total_chunks;
                    }
                    if transfer.total_bytes > 0 {
                        transfer.bytes_done = transfer.total_bytes;
                    }
                    transfer.bytes_per_second = 0;
                    transfer.detail = format!("saved · {}", output.display());
                    transfer.updated_seq = seq;
                }
                self.push_log(format!(
                    "completed: share={share_id} output={}",
                    output.display()
                ));
                self.sort_transfers();
            }
            IpcEvent::Error { message } => {
                self.set_banner(BannerTone::Error, message.clone());
                self.push_log(format!("event error: {message}"));
            }
        }
    }

    pub fn apply_task_progress(&mut self, progress: TaskProgressSnapshot) {
        let kind = classify_task_kind(&progress.task);
        let share_id: Option<ShareId> = progress.label.parse().ok();
        let existing_label = progress
            .job_id
            .as_deref()
            .and_then(|id| self.transfers.iter().find(|t| t.id == id))
            .map(|t| t.label.clone());
        let label = existing_label.unwrap_or_else(|| {
            share_id
                .map(|id| self.share_name_or_id(id))
                .unwrap_or_else(|| trim_path_label(&progress.label))
        });
        let id = progress
            .job_id
            .clone()
            .unwrap_or_else(|| format!("{}:{label}", kind.label()));
        let status = if progress.task == "peer:completed" {
            TransferStatus::Done
        } else {
            TransferStatus::Running
        };
        let speed = if status.is_finished() {
            0
        } else {
            progress.bytes_per_second
        };
        let detail = friendly_task_detail(&progress.task);
        let seq = self.bump_seq();
        self.upsert_transfer(GuiTransfer {
            id,
            kind,
            label,
            share_id,
            status,
            completed_chunks: progress.completed_chunks,
            total_chunks: progress.total_chunks,
            bytes_done: progress.bytes_done,
            total_bytes: progress.total_bytes,
            bytes_per_second: speed,
            detail,
            started_at: Instant::now(),
            updated_seq: seq,
        });
    }

    fn upsert_from_active_job(&mut self, job: &IpcActiveJob) {
        let kind = TransferKind::from_ipc(job.kind);
        let label = if kind == TransferKind::Upload {
            job.label.clone()
        } else if let Some(share_id) = job.share_id {
            if job.label == share_id.to_string() {
                self.share_name_or_id(share_id)
            } else {
                job.label.clone()
            }
        } else {
            job.label.clone()
        };
        let seq = self.bump_seq();
        self.upsert_transfer(GuiTransfer {
            id: job.job_id.clone(),
            kind,
            label,
            share_id: job.share_id,
            status: TransferStatus::Running,
            completed_chunks: job.completed_chunks,
            total_chunks: job.total_chunks,
            bytes_done: job.bytes_done,
            total_bytes: job.total_bytes,
            bytes_per_second: job.bytes_per_second,
            detail: friendly_task_detail(&job.phase),
            started_at: Instant::now(),
            updated_seq: seq,
        });
    }

    pub fn reconcile_transfers_from_daemon(&mut self) {
        let Some(status) = self.daemon_status.clone() else {
            return;
        };

        for job in &status.active_jobs {
            self.upsert_from_active_job(job);
        }

        let seq = self.bump_seq();
        let shares = self.shares.clone();
        for transfer in &mut self.transfers {
            if !transfer.status.is_active() {
                continue;
            }
            let still_active = status.active_jobs.iter().any(|job| {
                job.job_id == transfer.id
                    || (transfer.kind != TransferKind::Upload
                        && transfer.share_id.is_some()
                        && TransferKind::from_ipc(job.kind) == transfer.kind
                        && job.share_id == transfer.share_id)
            });
            if still_active || transfer.started_at.elapsed() < Duration::from_secs(2) {
                continue;
            }

            let matching_share = transfer
                .share_id
                .and_then(|id| shares.iter().find(|s| s.share_id == id));

            match transfer.kind {
                TransferKind::Upload => {
                    transfer.status = TransferStatus::Done;
                    transfer.bytes_per_second = 0;
                    transfer.detail = "upload session ended".to_string();
                    transfer.updated_seq = seq;
                }
                TransferKind::Seed => {
                    if let Some(share) = matching_share
                        && share_is_complete(share)
                    {
                        transfer.status = TransferStatus::Done;
                        transfer.completed_chunks = share.total_chunks;
                        transfer.total_chunks = share.total_chunks;
                        if share.total_bytes > 0 {
                            transfer.bytes_done = share.total_bytes;
                            transfer.total_bytes = share.total_bytes;
                        }
                        transfer.bytes_per_second = 0;
                        transfer.detail = "encrypted and stored".to_string();
                        transfer.updated_seq = seq;
                    } else {
                        transfer.status = TransferStatus::Failed;
                        transfer.bytes_per_second = 0;
                        transfer.detail = "seed job no longer active in daemon".to_string();
                        transfer.updated_seq = seq;
                    }
                }
                TransferKind::Download => {
                    if let Some(share) = matching_share
                        && share_is_complete(share)
                        && matches!(share.mode.as_deref(), Some("completed" | "seeding"))
                    {
                        transfer.status = TransferStatus::Done;
                        transfer.label = share.name.clone();
                        transfer.completed_chunks = share.total_chunks;
                        transfer.total_chunks = share.total_chunks;
                        if transfer.total_bytes == 0 && share.total_bytes > 0 {
                            transfer.bytes_done = share.total_bytes;
                            transfer.total_bytes = share.total_bytes;
                        } else if transfer.total_bytes > 0 {
                            transfer.bytes_done = transfer.total_bytes;
                        }
                        transfer.bytes_per_second = 0;
                        if !transfer.detail.contains("saved") {
                            transfer.detail = "completed in library".to_string();
                        }
                        transfer.updated_seq = seq;
                    } else {
                        transfer.status = TransferStatus::Failed;
                        transfer.bytes_per_second = 0;
                        transfer.detail =
                            "stopped before completing (resumable in Library)".to_string();
                        transfer.updated_seq = seq;
                    }
                }
            }
        }
        self.sort_transfers();
    }

    fn sync_share_update_into_transfers(&mut self, share: &IpcShareSummary) {
        let mode = share.mode.as_deref().unwrap_or("unknown");
        let done = share_is_complete(share);
        let seq = self.bump_seq();

        let seed_index = self.transfers.iter().position(|t| {
            t.kind == TransferKind::Seed
                && (t.share_id == Some(share.share_id)
                    || (!t.status.is_finished() && t.share_id.is_none() && t.label == share.name))
        });

        if let Some(index) = seed_index
            && done
            && mode == "seeding"
        {
            let transfer = &mut self.transfers[index];
            transfer.share_id = Some(share.share_id);
            transfer.label = share.name.clone();
            transfer.status = TransferStatus::Done;
            transfer.completed_chunks = share.completed_chunks;
            transfer.total_chunks = share.total_chunks;
            if share.total_bytes > 0 {
                transfer.bytes_done = share.total_bytes;
                transfer.total_bytes = share.total_bytes;
            }
            transfer.bytes_per_second = 0;
            transfer.detail = "encrypted and stored".to_string();
            transfer.updated_seq = seq;
        }

        if let Some(index) = self
            .transfers
            .iter()
            .position(|t| t.kind == TransferKind::Download && t.share_id == Some(share.share_id))
        {
            let transfer = &mut self.transfers[index];
            transfer.label = share.name.clone();
            transfer.completed_chunks = share.completed_chunks;
            transfer.total_chunks = share.total_chunks;
            if transfer.total_bytes == 0 && share.total_bytes > 0 {
                transfer.total_bytes = share.total_bytes;
            }
            if done && matches!(mode, "completed" | "seeding") {
                transfer.status = TransferStatus::Done;
                if transfer.total_bytes > 0 {
                    transfer.bytes_done = transfer.total_bytes;
                }
                transfer.bytes_per_second = 0;
                if !transfer.detail.contains("saved") {
                    transfer.detail = "completed in library".to_string();
                }
            }
            transfer.updated_seq = seq;
        }

        self.sort_transfers();
    }

    fn unhide_transfer_keys(&mut self, transfer: &GuiTransfer) {
        for key in transfer_hidden_keys(transfer) {
            self.hidden_finished_transfers.remove(&key);
        }
    }

    fn upsert_transfer(&mut self, transfer: GuiTransfer) {
        if !transfer.status.is_finished() {
            self.unhide_transfer_keys(&transfer);
        } else if self.is_transfer_hidden(&transfer) {
            return;
        }
        if let Some(existing) = self.find_matching_transfer_mut(&transfer) {
            let started_at = existing.started_at;
            *existing = GuiTransfer {
                started_at,
                ..transfer
            };
        } else {
            self.transfers.push(transfer);
        }
        self.sort_transfers();
    }

    fn find_matching_transfer_mut(&mut self, transfer: &GuiTransfer) -> Option<&mut GuiTransfer> {
        if let Some(pos) = self.transfers.iter().position(|c| c.id == transfer.id) {
            return Some(&mut self.transfers[pos]);
        }
        if transfer.kind != TransferKind::Upload {
            if let Some(share_id) = transfer.share_id
                && let Some(pos) = self.transfers.iter().position(|c| {
                    c.share_id == Some(share_id)
                        && c.kind == transfer.kind
                        && !c.status.is_finished()
                })
            {
                return Some(&mut self.transfers[pos]);
            }
            if transfer.kind == TransferKind::Seed
                && !transfer.id.starts_with("seed:")
                && let Some(pos) = self.transfers.iter().position(|c| {
                    c.kind == TransferKind::Seed
                        && c.share_id.is_none()
                        && c.id.starts_with("seed:")
                        && !c.status.is_finished()
                        && c.label == transfer.label
                })
            {
                return Some(&mut self.transfers[pos]);
            }
        }
        None
    }

    fn find_transfer_for_job_mut(
        &mut self,
        job_id: &str,
        kind: TransferKind,
        share_id: Option<ShareId>,
    ) -> Option<&mut GuiTransfer> {
        if let Some(pos) = self.transfers.iter().position(|t| t.id == job_id) {
            return Some(&mut self.transfers[pos]);
        }
        if kind != TransferKind::Upload
            && let Some(share_id) = share_id
            && let Some(pos) = self.transfers.iter().position(|t| {
                t.kind == kind && t.share_id == Some(share_id) && !t.status.is_finished()
            })
        {
            return Some(&mut self.transfers[pos]);
        }
        None
    }

    fn sort_transfers(&mut self) {
        self.transfers.sort_by_key(|transfer| {
            let status_rank = match transfer.status {
                TransferStatus::Running => 0_u8,
                TransferStatus::Queued => 1,
                TransferStatus::Failed => 2,
                TransferStatus::Done => 3,
            };
            (status_rank, Reverse(transfer.updated_seq))
        });
    }

    fn is_transfer_hidden(&self, transfer: &GuiTransfer) -> bool {
        transfer.status.is_finished()
            && transfer_hidden_keys(transfer)
                .into_iter()
                .any(|key| self.hidden_finished_transfers.contains(&key))
    }

    fn mark_recent_running_failed(&mut self, kind: IpcRequestKind, reason: &str) {
        let target_kind = match kind {
            IpcRequestKind::Seed => TransferKind::Seed,
            IpcRequestKind::Download => TransferKind::Download,
            _ => return,
        };
        if let Some(transfer) = self
            .transfers
            .iter_mut()
            .filter(|t| t.kind == target_kind && t.status.is_active())
            .max_by_key(|t| t.updated_seq)
        {
            transfer.status = TransferStatus::Failed;
            transfer.bytes_per_second = 0;
            transfer.detail = compact_log_line(reason);
        }
        self.sort_transfers();
    }

    fn remove_share(&mut self, share_id: ShareId) {
        if self.pending_delete_share == Some(share_id) {
            self.pending_delete_share = None;
        }
        self.shares.retain(|share| share.share_id != share_id);
        self.sync_selected_share();
        self.transfers
            .retain(|transfer| transfer.share_id != Some(share_id));
    }

    fn upsert_share(&mut self, share: IpcShareSummary) {
        if let Some(existing) = self
            .shares
            .iter_mut()
            .find(|candidate| candidate.share_id == share.share_id)
        {
            *existing = share;
        } else {
            self.shares.push(share);
        }
        sort_shares(&mut self.shares);
        self.sync_selected_share();
    }

    fn share_name_or_id(&self, share_id: ShareId) -> String {
        self.shares
            .iter()
            .find(|share| share.share_id == share_id)
            .map(|share| share.name.clone())
            .unwrap_or_else(|| share_id.to_string())
    }
}

fn sort_shares(shares: &mut [IpcShareSummary]) {
    shares.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.share_id.cmp(&b.share_id))
    });
}

fn classify_task_kind(task: &str) -> TransferKind {
    if task.starts_with("seeder:") || task.contains("served") {
        TransferKind::Upload
    } else if task.starts_with("daemon:") || task.contains("staged") || task.contains("encrypt") {
        TransferKind::Seed
    } else {
        TransferKind::Download
    }
}

fn friendly_task_detail(task: &str) -> String {
    if task.contains("completed") {
        "completed".to_string()
    } else if task.contains("decrypted+written") {
        "decrypting and writing output file".to_string()
    } else if task.contains("received+verified") {
        "downloading and verifying chunks".to_string()
    } else if task.contains("reused") {
        "reusing verified local chunks".to_string()
    } else if task.contains("served-from-library") || task.contains("served-from-state") {
        "uploading encrypted chunks to peer".to_string()
    } else if task.contains("staged+encrypted") {
        "encrypting and staging chunks".to_string()
    } else if task.contains("hashing") {
        "hashing input file".to_string()
    } else if task.contains("discovering") {
        "discovering LAN peers".to_string()
    } else if task.contains("connecting") {
        "connecting to peer".to_string()
    } else {
        task.replace(':', " · ")
    }
}

fn is_specific_progress_detail(detail: &str) -> bool {
    detail.contains("downloading")
        || detail.contains("reusing")
        || detail.contains("decrypting")
        || detail.contains("encrypting")
        || detail.contains("uploading")
        || detail.contains("writing")
        || detail.contains("saved")
}

fn transfer_hidden_keys(transfer: &GuiTransfer) -> Vec<String> {
    if transfer.kind == TransferKind::Upload {
        return vec![transfer.id.clone(), transfer.detail.clone()];
    }
    let mut keys = vec![
        transfer.id.clone(),
        format!("{}:label:{}", transfer.kind.label(), transfer.label),
    ];
    if let Some(share_id) = transfer.share_id {
        keys.push(format!("{}:share:{share_id}", transfer.kind.label()));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> GuiState {
        GuiState::new(GuiInit {
            default_socket_path: "/tmp/etled.sock".to_string(),
            socket_path: "/tmp/etled.sock".to_string(),
            auth_psk: "default-psk".to_string(),
            download_parallelism: 0,
            request_window: 16,
            discovery_port: 37037,
            discovery_timeout_ms: 3000,
            discovery_multicast: "239.255.0.86".parse().unwrap(),
            startup_warning: None,
        })
    }

    fn make_share(
        byte: u8,
        name: &str,
        total_bytes: u64,
        completed_chunks: usize,
        total_chunks: usize,
        mode: &str,
    ) -> IpcShareSummary {
        IpcShareSummary {
            share_id: ShareId([byte; 32]),
            name: name.to_string(),
            total_bytes,
            chunk_size: 1024 * 1024,
            completed_chunks,
            total_chunks,
            has_secret: true,
            mode: Some(mode.to_string()),
        }
    }

    fn make_status(shares: usize, active_jobs: Vec<IpcActiveJob>) -> IpcDaemonStatus {
        IpcDaemonStatus {
            peer_id: "peer-1".to_string(),
            listen: None,
            library_root: PathBuf::from("/tmp/etle"),
            discovery_port: 37037,
            auto_discovery: true,
            auth_required: false,
            share_count: shares,
            active_jobs,
        }
    }

    #[test]
    fn upload_task_progress_stays_upload_even_when_share_is_seeding() {
        let mut state = test_state();
        let share = make_share(1, "movie.mkv", 7_670 * 1024 * 1024, 7855, 7855, "seeding");
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(1, vec![]))),
            Ok(IpcResponse::Shares {
                shares: vec![share.clone()],
            }),
        );

        state.apply_ipc_event(IpcEvent::TaskProgress {
            job_id: Some("upload-1".to_string()),
            task: "seeder:served-from-library".to_string(),
            label: share.share_id.to_string(),
            completed_chunks: 685,
            total_chunks: 7855,
            bytes_done: 685 * 1024 * 1024,
            total_bytes: 7855 * 1024 * 1024,
            bytes_per_second: 2_500_000,
        });

        assert_eq!(state.transfers.len(), 1);
        assert_eq!(state.transfers[0].kind, TransferKind::Upload);
        assert_eq!(state.transfers[0].status, TransferStatus::Running);
        assert_eq!(state.transfers[0].completed_chunks, 685);

        state.apply_ipc_event(IpcEvent::ShareUpdated { share });
        assert_eq!(state.transfers[0].kind, TransferKind::Upload);
        assert_eq!(state.transfers[0].completed_chunks, 685);
        assert_eq!(state.transfers[0].status, TransferStatus::Running);
    }

    #[test]
    fn interrupted_downloading_share_without_active_job_does_not_spin_in_transfers() {
        let mut state = test_state();
        let partial = make_share(2, "archive.tar", 100 * 1024 * 1024, 42, 100, "downloading");
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(1, vec![]))),
            Ok(IpcResponse::Shares {
                shares: vec![partial.clone()],
            }),
        );

        assert!(state.transfers.is_empty());
        assert!(!state.share_row_snapshots()[0].is_downloading);
    }

    #[test]
    fn active_download_in_status_is_synced_and_completed_zeroes_speed() {
        let mut state = test_state();
        let share = make_share(3, "iso.img", 10_000_000, 10, 10, "completed");
        let job = IpcActiveJob {
            job_id: "download-3".to_string(),
            kind: IpcJobKind::Download,
            share_id: Some(share.share_id),
            label: "iso.img".to_string(),
            phase: "peer:received+verified".to_string(),
            completed_chunks: 5,
            total_chunks: 10,
            bytes_done: 5_000_000,
            total_bytes: 10_000_000,
            bytes_per_second: 1_250_000,
        };

        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(1, vec![job]))),
            Ok(IpcResponse::Shares {
                shares: vec![make_share(3, "iso.img", 10_000_000, 5, 10, "downloading")],
            }),
        );
        assert_eq!(state.transfers.len(), 1);
        assert_eq!(state.transfers[0].status, TransferStatus::Running);
        assert!(state.share_row_snapshots()[0].is_downloading);

        state.transfers[0].started_at = Instant::now() - Duration::from_secs(5);
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(1, vec![]))),
            Ok(IpcResponse::Shares {
                shares: vec![share],
            }),
        );

        assert_eq!(state.transfers[0].status, TransferStatus::Done);
        assert_eq!(state.transfers[0].bytes_per_second, 0);
        assert_eq!(
            state.transfers[0].bytes_done,
            state.transfers[0].total_bytes
        );
    }

    #[test]
    fn selection_persists_by_share_id_across_reordering() {
        let mut state = test_state();
        let b = make_share(2, "bravo.bin", 1024, 1, 1, "seeding");
        let c = make_share(3, "charlie.bin", 1024, 1, 1, "seeding");
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(2, vec![]))),
            Ok(IpcResponse::Shares {
                shares: vec![b.clone(), c.clone()],
            }),
        );
        state.selected_share_id = Some(c.share_id);
        assert_eq!(
            state.shares.iter().position(|s| s.share_id == c.share_id),
            Some(1)
        );

        let a = make_share(1, "alpha.bin", 1024, 1, 1, "seeding");
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(3, vec![]))),
            Ok(IpcResponse::Shares {
                shares: vec![b, c.clone(), a],
            }),
        );
        assert_eq!(state.selected_share_id, Some(c.share_id));
        assert_eq!(
            state.shares.iter().position(|s| s.share_id == c.share_id),
            Some(2)
        );
    }

    #[test]
    fn sync_failure_marks_offline_and_download_command_keeps_default_psk() {
        let mut state = test_state();
        state.connected = true;
        state.apply_sync_snapshot(
            Err("connection refused".to_string()),
            Err("connection refused".to_string()),
        );
        assert!(!state.connected);
        assert!(state.daemon_status.is_none());

        let req = DownloadFormRequest {
            share_id: ShareId([9_u8; 32]).to_string(),
            peers: String::new(),
            output: String::new(),
            auth_psk: "one-time-override".to_string(),
            discovery_multicast: "239.255.0.86".to_string(),
            parallelism: 0,
            request_window: 16,
            discovery_port: 37037,
            discovery_timeout_ms: 3000,
            resume: true,
        };
        let (cmd, _) = state.build_download_command_from(&req).unwrap();
        match cmd {
            IpcCommand::Download { auth_psk, .. } => {
                assert_eq!(auth_psk.as_deref(), Some("one-time-override"));
            }
            other => panic!("expected Download command, got {other:?}"),
        }
        assert_eq!(state.auth_psk, "default-psk");
    }

    #[test]
    fn seed_queue_deduplicates_paths_and_maintains_selection() {
        let mut state = test_state();
        assert!(state.add_seed_file(PathBuf::from("/tmp/a.bin")));
        assert!(!state.add_seed_file(PathBuf::from("/tmp/a.bin")));
        assert!(state.add_seed_file(PathBuf::from("/tmp/b.bin")));
        assert_eq!(state.seed_files.len(), 2);
        assert_eq!(state.selected_seed_file, Some(0));

        state.remove_selected_seed_file();
        assert_eq!(state.seed_files.len(), 1);
        assert_eq!(state.seed_files[0].path, PathBuf::from("/tmp/b.bin"));
        assert_eq!(state.selected_seed_file, Some(0));
    }

    #[test]
    fn clear_finished_transfers_hides_done_rows_until_cleared_on_restart() {
        let mut state = test_state();
        state.apply_ipc_event(IpcEvent::JobCompleted {
            job_id: "job-7".to_string(),
            kind: IpcJobKind::Seed,
            share_id: Some(ShareId([7_u8; 32])),
            label: "seven.bin".to_string(),
            summary: "seeded".to_string(),
        });
        assert_eq!(state.transfers.len(), 1);

        state.clear_finished_transfers();
        assert!(state.transfers.is_empty());

        // Replaying same finished summary does not resurrect hidden card
        state.apply_ipc_event(IpcEvent::JobCompleted {
            job_id: "job-7".to_string(),
            kind: IpcJobKind::Seed,
            share_id: Some(ShareId([7_u8; 32])),
            label: "seven.bin".to_string(),
            summary: "seeded".to_string(),
        });
        assert!(state.transfers.is_empty());
    }

    #[test]
    fn cleared_share_redownload_transitions_to_done_when_restarted() {
        let mut state = test_state();
        let share_id = ShareId([11_u8; 32]);
        state.shares = vec![make_share(11, "archive.zip", 4096, 4, 4, "completed")];
        state.apply_ipc_event(IpcEvent::JobCompleted {
            job_id: "dl-1".to_string(),
            kind: IpcJobKind::Download,
            share_id: Some(share_id),
            label: "archive.zip".to_string(),
            summary: "downloaded".to_string(),
        });
        state.clear_finished_transfers();
        assert!(state.transfers.is_empty());

        let req = DownloadFormRequest {
            share_id: share_id.to_string(),
            peers: String::new(),
            output: String::new(),
            auth_psk: String::new(),
            discovery_multicast: "239.255.0.86".to_string(),
            parallelism: 0,
            request_window: 16,
            discovery_port: 37037,
            discovery_timeout_ms: 3000,
            resume: true,
        };
        assert!(state.prepare_download_request(&req).is_ok());
        state.apply_ipc_response(
            IpcRequestKind::Download,
            Ok(IpcResponse::TransferQueued {
                share_id,
                job_id: "dl-2".to_string(),
            }),
        );
        state.apply_task_progress(TaskProgressSnapshot {
            job_id: Some("dl-2".to_string()),
            task: "peer:completed".to_string(),
            label: share_id.to_string(),
            completed_chunks: 4,
            total_chunks: 4,
            bytes_done: 4096,
            total_bytes: 4096,
            bytes_per_second: 0,
        });

        assert_eq!(state.transfers.len(), 1);
        assert_eq!(state.transfers[0].id, "dl-2");
        assert_eq!(state.transfers[0].status, TransferStatus::Done);
    }

    #[test]
    fn duplicate_basename_seed_jobs_do_not_collide() {
        let mut state = test_state();
        let entries = vec![
            SeedFileEntry {
                path: PathBuf::from("/tmp/dir1/data.bin"),
                size_bytes: 100,
            },
            SeedFileEntry {
                path: PathBuf::from("/tmp/dir2/data.bin"),
                size_bytes: 200,
            },
        ];
        let cmds = state.queue_seed_entries(&entries);
        assert_eq!(cmds.len(), 2);
        assert_eq!(state.transfers.len(), 2);

        state.apply_ipc_response(
            IpcRequestKind::Seed,
            Ok(IpcResponse::SeedQueued {
                job_id: "seed-job-1".to_string(),
                label: "data.bin".to_string(),
            }),
        );
        state.apply_ipc_response(
            IpcRequestKind::Seed,
            Ok(IpcResponse::SeedQueued {
                job_id: "seed-job-2".to_string(),
                label: "data.bin".to_string(),
            }),
        );

        let ids: BTreeSet<&str> = state.transfers.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains("seed-job-1"));
        assert!(ids.contains("seed-job-2"));
        assert!(
            state
                .transfers
                .iter()
                .all(|t| t.status == TransferStatus::Running)
        );
    }

    #[test]
    fn status_timeout_clears_stale_daemon_status_and_removed_share_clears_pending_delete() {
        let mut state = test_state();
        let share = make_share(15, "gone.bin", 1024, 1, 1, "seeding");
        let stale_job = IpcActiveJob {
            job_id: "dl-stale".to_string(),
            kind: IpcJobKind::Download,
            share_id: Some(share.share_id),
            label: "gone.bin".to_string(),
            phase: "peer:connecting".to_string(),
            completed_chunks: 0,
            total_chunks: 1,
            bytes_done: 0,
            total_bytes: 1024,
            bytes_per_second: 0,
        };
        state.apply_sync_snapshot(
            Ok(IpcResponse::Status(make_status(1, vec![stale_job]))),
            Ok(IpcResponse::Shares {
                shares: vec![share.clone()],
            }),
        );
        state.pending_delete_share = Some(share.share_id);

        state.apply_sync_snapshot(
            Err("status request timed out".to_string()),
            Ok(IpcResponse::Shares { shares: vec![] }),
        );
        assert!(state.daemon_status.is_none());
        assert!(state.pending_delete_share.is_none());
    }
}
