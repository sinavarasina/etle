use std::{cell::Cell, time::Duration};

use etle::ipc::message::IpcCommand;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    gtk::{self, gdk, glib, pango, prelude::*},
};
use tokio::sync::watch;

use super::{
    ipc::{fetch_daemon_snapshot, run_command_with_timeout, stream_daemon_events},
    model::{AppCmd, BannerTone, GuiInit, IpcRequestKind},
    pages::{
        activity::{ActivityInput, ActivityOutput, ActivityPage},
        download::{DownloadInput, DownloadOutput, DownloadPage},
        library::{LibraryInput, LibraryOutput, LibraryPage},
        seed::{SeedInput, SeedOutput, SeedPage},
        settings::{SettingsInput, SettingsOutput, SettingsPage},
    },
    state::GuiState,
    style,
};

#[derive(Debug)]
pub enum AppInput {
    Refresh,
    Reconnect,
    AutoRefreshTick,
    StartWatch,
    DismissBanner,
    OpenTab(&'static str),
    Library(LibraryOutput),
    Seed(SeedOutput),
    Download(DownloadOutput),
    Activity(ActivityOutput),
    Settings(SettingsOutput),
}

pub struct EtleGui {
    state: GuiState,
    pending_tab: Cell<Option<&'static str>>,
    watch_gen_tx: watch::Sender<u64>,
    library: Controller<LibraryPage>,
    seed: Controller<SeedPage>,
    download: Controller<DownloadPage>,
    activity: Controller<ActivityPage>,
    settings: Controller<SettingsPage>,
}

impl EtleGui {
    fn conn_pill_text(&self) -> &'static str {
        if self.state.connected {
            "● Online"
        } else if self.state.status == "checking" {
            "◌ Connecting…"
        } else {
            "○ Offline"
        }
    }

    fn header_summary_text(&self) -> String {
        let active_count = self.state.active_transfers().len();
        let watch_text = if self.state.watching {
            "Live watch on"
        } else if self.state.watch_connecting {
            "Subscribing…"
        } else {
            "Watch off"
        };
        let listen_part = self
            .state
            .daemon_status
            .as_ref()
            .and_then(|s| s.listen)
            .map(|addr| format!("P2P {addr} · "))
            .unwrap_or_default();
        format!(
            "{listen_part}{} share(s) · {} active · {watch_text}",
            self.state.shares.len(),
            active_count
        )
    }

    fn active_strip_visible(&self) -> bool {
        !self.state.active_transfers().is_empty()
    }

    fn active_strip_text(&self) -> String {
        let active = self.state.active_transfers();
        let Some(primary) = active.first() else {
            return String::new();
        };
        if active.len() == 1 {
            format!(
                "{} · {} · {}",
                primary.kind.display_label(),
                primary.label,
                primary.compact_line()
            )
        } else {
            format!(
                "{} active transfers · primary: {} ({:.1}%)",
                active.len(),
                primary.label,
                primary.fraction() * 100.0
            )
        }
    }

    fn primary_active_fraction(&self) -> f64 {
        self.state
            .active_transfers()
            .first()
            .map_or(0.0, |t| t.fraction())
    }

    fn should_pulse_active_bar(&self) -> bool {
        self.state
            .active_transfers()
            .first()
            .is_some_and(|t| t.total_bytes == 0 && t.total_chunks == 0)
    }

    fn banner_message(&self) -> String {
        self.state
            .banner
            .as_ref()
            .map(|b| b.message.clone())
            .unwrap_or_default()
    }

    fn banner_tone_is(&self, tone: BannerTone) -> bool {
        self.state.banner.as_ref().is_some_and(|b| b.tone == tone)
    }

    fn sync_all_pages(&self) {
        self.library.emit(LibraryInput::Sync {
            rows: self.state.share_row_snapshots(),
            selected_share_id: self.state.selected_share_id,
            pending_delete: self.state.pending_delete_share,
            connected: self.state.connected,
        });
        self.seed.emit(SeedInput::Sync {
            entries: self.state.seed_files.clone(),
            selected_index: self.state.selected_seed_file,
            connected: self.state.connected,
            chunk_size: self.state.seed_chunk_size,
        });
        self.activity.emit(ActivityInput::Sync {
            transfers: self.state.transfer_view_snapshots(),
            log_text: self.state.activity_text(),
        });
        self.settings
            .emit(SettingsInput::Sync(self.state.settings_snapshot()));
    }

    fn spawn_sync(&mut self, sender: &ComponentSender<Self>) {
        if self.state.refresh_inflight || self.state.active_socket_path.trim().is_empty() {
            return;
        }
        self.state.refresh_inflight = true;
        self.state.last_auto_refresh = std::time::Instant::now();
        let socket = self.state.active_socket_path.clone();
        sender.oneshot_command(fetch_daemon_snapshot(socket));
    }

    fn spawn_ipc(&self, kind: IpcRequestKind, command: IpcCommand, sender: &ComponentSender<Self>) {
        let socket = self.state.active_socket_path.clone();
        sender.oneshot_command(run_command_with_timeout(socket, kind, command));
    }

    fn start_watch(&mut self, sender: &ComponentSender<Self>) {
        if self.state.active_socket_path.trim().is_empty() {
            self.state.connected = false;
            self.state.watching = false;
            self.state.watch_connecting = false;
            self.state.status = "offline".to_string();
            return;
        }
        if self.state.watching || self.state.watch_connecting {
            return;
        }
        self.state.watch_generation = self.state.watch_generation.saturating_add(1);
        let generation = self.state.watch_generation;
        let _ = self.watch_gen_tx.send(generation);
        let active_gen = self.watch_gen_tx.subscribe();
        self.state.watch_connecting = true;
        let socket = self.state.active_socket_path.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(stream_daemon_events(socket, generation, active_gen, out))
                .drop_on_shutdown()
        });
    }

    fn apply_socket(&mut self, raw: &str, sender: &ComponentSender<Self>) -> bool {
        let next = raw.trim().to_string();
        if next.is_empty() {
            self.state
                .set_banner(BannerTone::Warning, "IPC endpoint path cannot be empty.");
            self.state.push_log("socket: empty path ignored");
            return false;
        }
        if next != raw {
            self.settings
                .emit(SettingsInput::SetSocketText(next.clone()));
        }
        if next != self.state.active_socket_path {
            self.state.watch_generation = self.state.watch_generation.saturating_add(1);
            let _ = self.watch_gen_tx.send(self.state.watch_generation);
            self.state.active_socket_path = next.clone();
            self.state.connected = false;
            self.state.watching = false;
            self.state.watch_connecting = false;
            self.state.refresh_inflight = false;
            self.state.last_offline_logged = false;
            self.state.daemon_status = None;
            self.state.selected_share_id = None;
            self.state.pending_delete_share = None;
            self.state.pending_shutdown_confirm = false;
            self.state.shares.clear();
            self.state.push_log(format!("socket applied: {next}"));
        }
        self.spawn_sync(sender);
        self.start_watch(sender);
        true
    }

    fn handle_library(&mut self, output: LibraryOutput, sender: &ComponentSender<Self>) {
        match output {
            LibraryOutput::SelectShare(id) => {
                self.state.selected_share_id = Some(id);
                self.state.pending_delete_share = None;
            }
            LibraryOutput::CopyShareId(id) => {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&id.to_string());
                    let name = self
                        .state
                        .shares
                        .iter()
                        .find(|s| s.share_id == id)
                        .map(|s| s.name.as_str())
                        .unwrap_or("share");
                    self.state.set_banner(
                        BannerTone::Success,
                        format!("Copied Share ID for \"{name}\""),
                    );
                    self.state.push_log(format!("copied share id: {id}"));
                }
            }
            LibraryOutput::PrepareDownload(id) => {
                self.download.emit(DownloadInput::SetShareId(id));
                let name = self
                    .state
                    .shares
                    .iter()
                    .find(|s| s.share_id == id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("share");
                self.state.set_banner(
                    BannerTone::Info,
                    format!("Prepared download form for \"{name}\""),
                );
                self.pending_tab.set(Some("download"));
            }
            LibraryOutput::RequestDelete(id) => {
                self.state.pending_delete_share = Some(id);
                self.state
                    .push_log(format!("delete: confirmation requested for {id}"));
            }
            LibraryOutput::ConfirmDelete(id) => {
                self.state.pending_delete_share = None;
                self.state.push_log(format!("delete requested: {id}"));
                self.spawn_ipc(
                    IpcRequestKind::DeleteShare,
                    IpcCommand::DeleteShare { share_id: id },
                    sender,
                );
            }
            LibraryOutput::CancelDelete => {
                if self.state.pending_delete_share.take().is_some() {
                    self.state.push_log("delete cancelled");
                }
            }
            LibraryOutput::Refresh => self.spawn_sync(sender),
        }
    }

    fn handle_seed(&mut self, output: SeedOutput, sender: &ComponentSender<Self>) {
        match output {
            SeedOutput::AddFiles(paths) => {
                for path in paths {
                    self.state.add_seed_file(path);
                }
            }
            SeedOutput::SelectFile(idx) => {
                if idx < self.state.seed_files.len() {
                    self.state.selected_seed_file = Some(idx);
                }
            }
            SeedOutput::SetChunkSize(size) => {
                self.state.seed_chunk_size = size.max(1);
            }
            SeedOutput::SeedSelected => {
                if let Some(entry) = self.state.take_selected_seed_file() {
                    let cmd = self.state.queue_seed_entry(&entry);
                    self.spawn_ipc(IpcRequestKind::Seed, cmd, sender);
                }
            }
            SeedOutput::SeedAll => {
                if self.state.seed_files.is_empty() {
                    self.state
                        .set_banner(BannerTone::Warning, "Seed queue is empty.");
                    return;
                }
                let entries = std::mem::take(&mut self.state.seed_files);
                self.state.selected_seed_file = None;
                for cmd in self.state.queue_seed_entries(&entries) {
                    self.spawn_ipc(IpcRequestKind::Seed, cmd, sender);
                }
            }
            SeedOutput::RemoveSelected => self.state.remove_selected_seed_file(),
            SeedOutput::ClearQueue => {
                self.state.seed_files.clear();
                self.state.selected_seed_file = None;
            }
        }
    }

    fn handle_settings(&mut self, output: SettingsOutput, sender: &ComponentSender<Self>) {
        match output {
            SettingsOutput::ApplySettings {
                socket_path,
                auth_psk,
            } => {
                self.state.auth_psk = auth_psk;
                if self.apply_socket(&socket_path, sender) {
                    self.state
                        .set_banner(BannerTone::Success, "Settings saved and applied.");
                }
            }
            SettingsOutput::ResetSocketDefault => {
                let default = self.state.default_socket_path.clone();
                self.settings
                    .emit(SettingsInput::SetSocketText(default.clone()));
                self.apply_socket(&default, sender);
                self.state.set_banner(
                    BannerTone::Info,
                    format!("IPC endpoint reset to {}", self.state.active_socket_path),
                );
            }
            SettingsOutput::ReconnectWatch => self.start_watch(sender),
            SettingsOutput::RequestShutdown => {
                if self.state.connected {
                    self.state.pending_shutdown_confirm = true;
                }
            }
            SettingsOutput::ConfirmShutdown => {
                self.state.pending_shutdown_confirm = false;
                self.state.push_log("daemon: shutdown requested");
                self.spawn_ipc(IpcRequestKind::Shutdown, IpcCommand::Shutdown, sender);
            }
            SettingsOutput::CancelShutdown => {
                self.state.pending_shutdown_confirm = false;
            }
            SettingsOutput::SetAutoRefresh(val) => self.state.auto_refresh = val,
            SettingsOutput::SetClearActivityOnTask(val) => {
                self.state.clear_activity_on_task = val;
            }
            SettingsOutput::SetRefreshInterval(secs) => {
                self.state.refresh_interval_secs = secs.max(1);
            }
            SettingsOutput::SetActivityLimit(limit) => {
                self.state.activity_limit = limit.max(20);
                self.state.trim_activity();
            }
        }
    }
}

#[relm4::component(pub)]
impl Component for EtleGui {
    type Init = GuiInit;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = AppCmd;

    view! {
        #[root]
        gtk::Window {
            set_title: Some("ETLE"),
            set_default_size: (820, 860),
            set_resizable: true,
            add_css_class: "etle-window",

            #[wrap(Some)]
            set_child = &gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                add_css_class: "etle-main-scroll",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    set_margin_top: 10,
                    set_margin_bottom: 10,
                    set_margin_start: 12,
                    set_margin_end: 12,
                    add_css_class: "etle-root",

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        add_css_class: "etle-header",

                        gtk::Label {
                            add_css_class: "etle-conn-pill",
                            #[watch]
                            set_label: model.conn_pill_text(),
                            #[watch]
                            set_tooltip_text: Some(&model.state.active_socket_path),
                            #[watch]
                            set_class_active: ("etle-conn-online", model.state.connected),
                            #[watch]
                            set_class_active: ("etle-conn-offline", !model.state.connected && model.state.status != "checking"),
                        },

                        gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_width_chars: 1,
                            set_ellipsize: pango::EllipsizeMode::End,
                            add_css_class: "dim-label",
                            #[watch]
                            set_label: &model.header_summary_text(),
                        },

                        gtk::Button {
                            set_label: "Refresh",
                            connect_clicked => AppInput::Refresh,
                        },

                        gtk::Button {
                            set_label: "Reconnect",
                            #[watch]
                            set_visible: !model.state.connected || !model.state.watching,
                            connect_clicked => AppInput::Reconnect,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        add_css_class: "etle-banner",
                        #[watch]
                        set_visible: model.state.banner.is_some(),
                        #[watch]
                        set_class_active: ("etle-banner-info", model.banner_tone_is(BannerTone::Info)),
                        #[watch]
                        set_class_active: ("etle-banner-success", model.banner_tone_is(BannerTone::Success)),
                        #[watch]
                        set_class_active: ("etle-banner-warning", model.banner_tone_is(BannerTone::Warning)),
                        #[watch]
                        set_class_active: ("etle-banner-error", model.banner_tone_is(BannerTone::Error)),

                        gtk::Label {
                            set_xalign: 0.0,
                            set_wrap: true,
                            set_hexpand: true,
                            #[watch]
                            set_label: &model.banner_message(),
                        },

                        gtk::Button {
                            set_label: "Dismiss",
                            connect_clicked => AppInput::DismissBanner,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 10,
                        add_css_class: "etle-strip",
                        #[watch]
                        set_visible: model.active_strip_visible(),

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_hexpand: true,

                            gtk::Label {
                                set_xalign: 0.0,
                                set_width_chars: 1,
                                set_ellipsize: pango::EllipsizeMode::End,
                                #[watch]
                                set_label: &model.active_strip_text(),
                            },

                            #[name = "active_bar"]
                            gtk::ProgressBar {
                                #[watch]
                                set_fraction: model.primary_active_fraction(),
                            },
                        },

                        gtk::Button {
                            set_label: "Open Activity →",
                            connect_clicked => AppInput::OpenTab("activity"),
                        },
                    },

                    gtk::StackSwitcher {
                        set_halign: gtk::Align::Center,
                        set_stack: Some(&stack),
                    },

                    #[name = "stack"]
                    gtk::Stack {
                        set_hexpand: true,
                        set_vexpand: true,
                        set_transition_type: gtk::StackTransitionType::SlideLeftRight,
                        add_titled: (model.library.widget(), Some("library"), "Library"),
                        add_titled: (model.seed.widget(), Some("seed"), "Seed"),
                        add_titled: (model.download.widget(), Some("download"), "Download"),
                        add_titled: (model.activity.widget(), Some("activity"), "Activity"),
                        add_titled: (model.settings.widget(), Some("settings"), "Settings"),
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        style::install_platform_style();

        let state = GuiState::new(init.clone());
        let settings_snapshot = state.settings_snapshot();

        let library = LibraryPage::builder()
            .launch(())
            .forward(sender.input_sender(), AppInput::Library);
        let seed = SeedPage::builder()
            .launch(state.seed_chunk_size)
            .forward(sender.input_sender(), AppInput::Seed);
        let download = DownloadPage::builder()
            .launch(init.clone())
            .forward(sender.input_sender(), AppInput::Download);
        let activity = ActivityPage::builder()
            .launch(())
            .forward(sender.input_sender(), AppInput::Activity);
        let settings = SettingsPage::builder()
            .launch((init, settings_snapshot))
            .forward(sender.input_sender(), AppInput::Settings);

        let (watch_gen_tx, _) = watch::channel(state.watch_generation);

        let model = Self {
            state,
            pending_tab: Cell::new(None),
            watch_gen_tx,
            library,
            seed,
            download,
            activity,
            settings,
        };

        let widgets = view_output!();

        attach_auto_refresh_timer(sender.clone());
        model.sync_all_pages();
        sender.input(AppInput::Refresh);
        sender.input(AppInput::StartWatch);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            AppInput::Refresh => self.spawn_sync(&sender),
            AppInput::Reconnect => {
                if self.state.active_socket_path.trim().is_empty() {
                    self.state.connected = false;
                    self.state.status = "offline".to_string();
                    self.state
                        .set_banner(BannerTone::Warning, "IPC endpoint path is empty.");
                    self.state.push_log("ipc: empty socket path");
                } else {
                    self.state.status = "checking".to_string();
                    self.spawn_ipc(IpcRequestKind::Ping, IpcCommand::Ping, &sender);
                    self.spawn_sync(&sender);
                    self.start_watch(&sender);
                }
            }
            AppInput::AutoRefreshTick => {
                if !self.state.watching && !self.state.watch_connecting {
                    if self.state.connected {
                        self.start_watch(&sender);
                    } else if self.state.watch_retry_ticks >= 3 {
                        self.state.watch_retry_ticks = 0;
                        self.start_watch(&sender);
                    } else {
                        self.state.watch_retry_ticks =
                            self.state.watch_retry_ticks.saturating_add(1);
                    }
                }
                if self.state.refresh_due() {
                    self.spawn_sync(&sender);
                }
            }
            AppInput::StartWatch => self.start_watch(&sender),
            AppInput::DismissBanner => self.state.banner = None,
            AppInput::OpenTab(tab) => self.pending_tab.set(Some(tab)),
            AppInput::Library(out) => self.handle_library(out, &sender),
            AppInput::Seed(out) => self.handle_seed(out, &sender),
            AppInput::Download(DownloadOutput::StartDownload(req)) => {
                match self.state.prepare_download_request(&req) {
                    Ok(cmd) => self.spawn_ipc(IpcRequestKind::Download, cmd, &sender),
                    Err(error) => {
                        self.state.set_banner(BannerTone::Error, error.clone());
                        self.state.push_log(format!("download: {error}"));
                    }
                }
            }
            AppInput::Activity(out) => match out {
                ActivityOutput::ClearFinishedTransfers => self.state.clear_finished_transfers(),
                ActivityOutput::CopyActivityLog => {
                    let text = self.state.activity_text();
                    if let Some(display) = gdk::Display::default() {
                        display.clipboard().set_text(&text);
                        self.state
                            .set_banner(BannerTone::Success, "Copied activity log to clipboard");
                    }
                }
                ActivityOutput::ClearLog => self.state.clear_log(),
            },
            AppInput::Settings(out) => self.handle_settings(out, &sender),
        }

        self.sync_all_pages();
    }

    fn update_cmd(
        &mut self,
        cmd: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match cmd {
            AppCmd::SyncSnapshot {
                socket_path,
                status_result,
                shares_result,
            } => {
                if socket_path == self.state.active_socket_path {
                    let was_connected = self.state.connected;
                    self.state.refresh_inflight = false;
                    self.state.apply_sync_snapshot(status_result, shares_result);
                    if self.state.connected
                        && !was_connected
                        && !self.state.watching
                        && !self.state.watch_connecting
                    {
                        self.start_watch(&sender);
                    }
                }
            }
            AppCmd::CommandResponse {
                socket_path,
                kind,
                result,
            } => {
                if socket_path == self.state.active_socket_path {
                    self.state.apply_ipc_response(kind, result);
                }
            }
            AppCmd::WatchConnected { generation } => {
                if generation == self.state.watch_generation {
                    self.state.watch_connecting = false;
                    self.state.watching = true;
                    self.state.connected = true;
                    self.state.last_offline_logged = false;
                    self.state.push_log("watch: daemon event stream active");
                }
            }
            AppCmd::WatchEvent { generation, event } => {
                if generation == self.state.watch_generation {
                    self.state.apply_ipc_event(event);
                }
            }
            AppCmd::WatchStopped { generation, error } => {
                if generation == self.state.watch_generation {
                    let was_watching = self.state.watching;
                    self.state.watching = false;
                    self.state.watch_connecting = false;
                    self.state.watch_retry_ticks = 0;
                    self.state.connected = false;
                    self.state.daemon_status = None;
                    self.state.status = "offline".to_string();
                    if was_watching {
                        self.state.push_log(format!("watch stopped: {error}"));
                    }
                }
            }
        }

        self.sync_all_pages();
    }

    fn post_view() {
        if model.should_pulse_active_bar() {
            widgets.active_bar.pulse();
        }
        if let Some(tab) = model.pending_tab.take() {
            widgets.stack.set_visible_child_name(tab);
        }
    }
}

fn attach_auto_refresh_timer(sender: ComponentSender<EtleGui>) {
    glib::timeout_add_local(Duration::from_secs(1), move || {
        if sender
            .input_sender()
            .send(AppInput::AutoRefreshTick)
            .is_err()
        {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}
