use std::{cmp::Reverse, path::PathBuf, time::Instant};

use etle::{
    file::chunker::DEFAULT_CHUNK_SIZE,
    ipc::message::{IpcCommand, IpcEvent, IpcResponse, IpcShareSummary},
};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{
        self, Button, CheckButton, Entry, Label, ListBox, Orientation, PasswordEntry, ProgressBar,
        SpinButton, Stack, StackSwitcher, TextBuffer, gdk, prelude::*,
    },
};

pub use super::model::EtleGui;
use super::{
    format::{compact_log_line, file_label, fraction, human_bytes, short_time, trim_path_label},
    ipc::{spawn_auto_refresh_loop, spawn_ipc_command, spawn_ipc_watch},
    model::{AppInput, GuiInit, GuiTransfer, IpcRequestKind, TransferKind, TransferStatus},
    progress::{TaskProgressSnapshot, parse_task_progress_debug},
    widgets::{
        GuiWidgets, build_activity_page, build_download_page, build_library_page, build_seed_page,
        build_settings_page, library_signature, refill_library_list, refill_seed_list,
        refill_transfer_list, seed_signature, set_spin_if_changed, transfer_signature,
    },
};

impl SimpleComponent for EtleGui {
    type Input = AppInput;
    type Output = ();
    type Init = GuiInit;
    type Root = gtk::Window;
    type Widgets = GuiWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title("ETLE")
            .default_width(760)
            .default_height(820)
            .resizable(true)
            .build()
    }

    fn init(
        init: Self::Init,
        window: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = EtleGui::new(init);

        let outer_scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        window.set_child(Some(&outer_scroll));

        let root = gtk::Box::new(Orientation::Vertical, 8);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);
        outer_scroll.set_child(Some(&root));

        let header = gtk::Box::new(Orientation::Vertical, 6);
        root.append(&header);

        let header_main = gtk::Box::new(Orientation::Horizontal, 6);
        header.append(&header_main);

        let status_label = Label::new(None);
        status_label.set_xalign(0.0);
        status_label.set_width_chars(32);
        status_label.set_max_width_chars(36);
        status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        header_main.append(&status_label);

        let socket_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("IPC socket / pipe")
            .build();
        socket_entry.set_text(&model.socket_draft);
        header_main.append(&socket_entry);

        let header_actions = gtk::Box::new(Orientation::Horizontal, 6);
        header_actions.set_halign(gtk::Align::End);
        header.append(&header_actions);

        let apply_socket_button = Button::with_label("Apply");
        header_actions.append(&apply_socket_button);

        let connect_button = Button::with_label("Ping");
        header_actions.append(&connect_button);

        let refresh_button = Button::with_label("Refresh");
        header_actions.append(&refresh_button);

        let watch_button = Button::with_label("Watch");
        header_actions.append(&watch_button);

        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk::StackTransitionType::SlideLeftRight)
            .build();
        let switcher = StackSwitcher::builder().stack(&stack).build();
        root.append(&switcher);
        root.append(&stack);

        let library_list = ListBox::new();
        library_list.set_vexpand(true);
        let detail_buffer = TextBuffer::new(None);
        let library_page = build_library_page(&library_list, &detail_buffer, &sender);
        stack.add_titled(&library_page, Some("library"), "Library");

        let seed_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("file path; press Enter or Add")
            .build();
        let seed_list = ListBox::new();
        seed_list.set_vexpand(true);
        let seed_chunk_spin = SpinButton::with_range(1.0, 1_073_741_824.0, 1.0);
        seed_chunk_spin.set_value(DEFAULT_CHUNK_SIZE as f64);
        let seed_page = build_seed_page(&seed_entry, &seed_list, &seed_chunk_spin, &sender);
        stack.add_titled(&seed_page, Some("seed"), "Seed");

        let share_id_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("64 hex chars")
            .build();
        let peers_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("127.0.0.1:7000, 127.0.0.1:7001")
            .build();
        let output_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("optional output path")
            .build();
        let parallel_spin = SpinButton::with_range(0.0, 128.0, 1.0);
        parallel_spin.set_value(model.download_parallelism as f64);
        let request_window_spin = SpinButton::with_range(1.0, 2048.0, 1.0);
        request_window_spin.set_value(model.download_request_window as f64);
        let discovery_port_spin = SpinButton::with_range(1.0, 65535.0, 1.0);
        discovery_port_spin.set_value(model.discovery_port as f64);
        let discovery_timeout_spin = SpinButton::with_range(1.0, 120_000.0, 100.0);
        discovery_timeout_spin.set_value(model.discovery_timeout_ms as f64);
        let discovery_multicast_entry = Entry::builder().hexpand(true).build();
        discovery_multicast_entry.set_text(&model.discovery_multicast.to_string());
        let resume_check = CheckButton::with_label("Resume existing chunks");
        resume_check.set_active(model.resume);
        let psk_entry = PasswordEntry::builder().hexpand(true).build();
        psk_entry.set_placeholder_text(Some("Override; empty uses the default PSK"));
        let download_page = build_download_page(
            &share_id_entry,
            &peers_entry,
            &output_entry,
            &parallel_spin,
            &request_window_spin,
            &discovery_port_spin,
            &discovery_timeout_spin,
            &discovery_multicast_entry,
            &resume_check,
            &psk_entry,
            &sender,
        );
        stack.add_titled(&download_page, Some("download"), "Download");

        let transfer_list = ListBox::new();
        transfer_list.set_vexpand(true);
        let progress_bar = ProgressBar::new();
        let progress_label = Label::new(Some("idle"));
        progress_label.set_xalign(0.0);
        let activity_buffer = TextBuffer::new(None);
        let activity_page = build_activity_page(
            &transfer_list,
            &progress_bar,
            &progress_label,
            &activity_buffer,
            &sender,
        );
        stack.add_titled(&activity_page, Some("activity"), "Activity");

        let settings_socket_entry = Entry::builder().hexpand(true).build();
        settings_socket_entry.set_text(&model.socket_draft);
        let settings_psk_entry = PasswordEntry::builder().hexpand(true).build();
        settings_psk_entry.set_text(&model.auth_psk);
        let auto_refresh_check = CheckButton::with_label("Auto-refresh library");
        auto_refresh_check.set_active(model.auto_refresh);
        let clear_activity_check =
            CheckButton::with_label("Clear activity when starting a new task");
        clear_activity_check.set_active(model.clear_activity_on_task);
        let refresh_interval_spin = SpinButton::with_range(1.0, 60.0, 1.0);
        refresh_interval_spin.set_value(model.refresh_interval_secs as f64);
        let activity_limit_spin = SpinButton::with_range(20.0, 2000.0, 10.0);
        activity_limit_spin.set_value(model.activity_limit as f64);
        let settings_page = build_settings_page(
            &settings_socket_entry,
            &settings_psk_entry,
            &auto_refresh_check,
            &clear_activity_check,
            &refresh_interval_spin,
            &activity_limit_spin,
            &sender,
        );
        stack.add_titled(&settings_page, Some("settings"), "Settings");

        {
            let sender = sender.clone();
            let socket_entry = socket_entry.clone();
            apply_socket_button.connect_clicked(move |_| {
                sender.input(AppInput::ApplySocketText(socket_entry.text().to_string()));
            });
        }
        {
            let sender = sender.clone();
            let socket_entry_signal = socket_entry.clone();
            let socket_entry_value = socket_entry.clone();
            socket_entry_signal.connect_activate(move |_| {
                sender.input(AppInput::ApplySocketText(
                    socket_entry_value.text().to_string(),
                ));
            });
        }
        {
            let sender = sender.clone();
            connect_button.connect_clicked(move |_| sender.input(AppInput::Connect));
        }
        {
            let sender = sender.clone();
            refresh_button.connect_clicked(move |_| sender.input(AppInput::Refresh));
        }
        {
            let sender = sender.clone();
            watch_button.connect_clicked(move |_| sender.input(AppInput::StartWatch));
        }
        {
            let sender = sender.clone();
            library_list.connect_row_activated(move |_, row| {
                let index = row.index();
                if index >= 0 {
                    sender.input(AppInput::SelectShare(index as usize));
                }
            });
        }
        {
            let sender = sender.clone();
            seed_list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let index = row.index();
                    if index >= 0 {
                        sender.input(AppInput::SelectSeedFile(index as usize));
                    }
                }
            });
        }
        {
            let sender = sender.clone();
            seed_chunk_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetSeedChunkSize(
                    spin.value_as_int().max(1) as usize
                ));
            });
        }
        {
            let sender = sender.clone();
            parallel_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetParallelism(spin.value_as_int().max(0) as usize));
            });
        }
        {
            let sender = sender.clone();
            request_window_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetRequestWindow(
                    spin.value_as_int().max(1) as usize
                ));
            });
        }
        {
            let sender = sender.clone();
            discovery_port_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetDiscoveryPort(
                    spin.value_as_int().clamp(1, 65535) as u16,
                ));
            });
        }
        {
            let sender = sender.clone();
            discovery_timeout_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetDiscoveryTimeout(
                    spin.value_as_int().max(1) as u64
                ));
            });
        }
        {
            let sender = sender.clone();
            resume_check.connect_toggled(move |check| {
                sender.input(AppInput::SetResume(check.is_active()));
            });
        }
        {
            let sender = sender.clone();
            auto_refresh_check.connect_toggled(move |check| {
                sender.input(AppInput::SetAutoRefresh(check.is_active()));
            });
        }
        {
            let sender = sender.clone();
            clear_activity_check.connect_toggled(move |check| {
                sender.input(AppInput::SetClearActivityOnTask(check.is_active()));
            });
        }
        {
            let sender = sender.clone();
            refresh_interval_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetRefreshInterval(
                    spin.value_as_int().max(1) as u64
                ));
            });
        }
        {
            let sender = sender.clone();
            activity_limit_spin.connect_value_changed(move |spin| {
                sender.input(AppInput::SetActivityLimit(
                    spin.value_as_int().max(20) as usize
                ));
            });
        }

        let widgets = GuiWidgets {
            status_label,
            library_list,
            detail_buffer,
            seed_list,
            seed_chunk_spin,
            parallel_spin,
            request_window_spin,
            discovery_port_spin,
            discovery_timeout_spin,
            resume_check,
            transfer_list,
            progress_bar,
            progress_label,
            activity_buffer,
            auto_refresh_check,
            clear_activity_check,
            refresh_interval_spin,
            activity_limit_spin,
            library_signature: std::cell::RefCell::new(String::new()),
            seed_signature: std::cell::RefCell::new(String::new()),
            transfer_signature: std::cell::RefCell::new(String::new()),
            detail_text: std::cell::RefCell::new(String::new()),
            activity_text: std::cell::RefCell::new(String::new()),
        };

        spawn_auto_refresh_loop(sender.clone());
        sender.input(AppInput::Connect);
        sender.input(AppInput::Refresh);
        sender.input(AppInput::StartWatch);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            AppInput::ApplySocketText(value) => {
                self.socket_draft = value;
                self.apply_socket(sender);
            }
            AppInput::Connect => {
                if self.active_socket_path.trim().is_empty() {
                    self.connected = false;
                    self.status = "offline".to_string();
                    self.push_log("ipc: empty socket path");
                    return;
                }
                self.connected = false;
                self.status = "checking".to_string();
                spawn_ipc_command(
                    self.active_socket_path.clone(),
                    IpcRequestKind::Ping,
                    IpcCommand::Ping,
                    sender,
                );
            }
            AppInput::Refresh => self.spawn_refresh(sender),
            AppInput::AutoRefreshTick => {
                if self.refresh_due() {
                    self.spawn_refresh(sender);
                }
            }
            AppInput::StartWatch => self.start_watch(sender),
            AppInput::IpcResponse {
                socket_path,
                kind,
                result,
            } => {
                if socket_path == self.active_socket_path {
                    if kind == IpcRequestKind::ListShares {
                        self.refresh_inflight = false;
                    }
                    self.apply_ipc_response(kind, result);
                }
            }
            AppInput::IpcEvent { generation, event } => {
                if generation == self.watch_generation {
                    self.apply_ipc_event(event);
                }
            }
            AppInput::IpcWatchStopped { generation, error } => {
                if generation == self.watch_generation {
                    self.watching = false;
                    self.connected = false;
                    self.status = "offline".to_string();
                    self.push_log(format!("watch stopped: {error}"));
                }
            }
            AppInput::SelectShare(index) => {
                if index < self.shares.len() {
                    self.selected_share = Some(index);
                }
            }
            AppInput::CopySelectedShareId => {
                if let Some(share) = self.selected_share_summary()
                    && let Some(display) = gdk::Display::default()
                {
                    display.clipboard().set_text(&share.share_id.to_string());
                    self.push_log("share id copied");
                }
            }
            AppInput::ClearLog => {
                self.activity.clear();
                self.last_activity_message = None;
            }
            AppInput::ClearFinishedTransfers => self.clear_finished_transfers(),
            AppInput::AddSeedPathText(value) => self.add_seed_path_text(value),
            AppInput::AddSeedFile(path) => self.add_seed_file(path),
            AppInput::BrowseSeedFiles => {
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("Add seed files")
                    .pick_files()
                {
                    for path in paths {
                        sender.input(AppInput::AddSeedFile(path));
                    }
                }
            }
            AppInput::SelectSeedFile(index) => {
                if index < self.seed_files.len() {
                    self.selected_seed_file = Some(index);
                }
            }
            AppInput::RemoveSelectedSeedFile => self.remove_selected_seed_file(),
            AppInput::ClearSeedFiles => {
                self.seed_files.clear();
                self.selected_seed_file = None;
            }
            AppInput::SetSeedChunkSize(value) => self.seed_chunk_size = value.max(1),
            AppInput::StartSeedSelected => self.start_seed_selected(sender),
            AppInput::SetParallelism(value) => self.download_parallelism = value,
            AppInput::SetRequestWindow(value) => self.download_request_window = value.max(1),
            AppInput::SetDiscoveryPort(value) => self.discovery_port = value.max(1),
            AppInput::SetDiscoveryTimeout(value) => self.discovery_timeout_ms = value.max(1),
            AppInput::SetResume(value) => self.resume = value,
            AppInput::StartDownloadFromForm {
                share_id,
                peers,
                output,
                auth_psk,
                discovery_multicast,
            } => self.start_download_from_form(
                share_id,
                peers,
                output,
                auth_psk,
                discovery_multicast,
                sender,
            ),
            AppInput::ApplySettings {
                socket_path,
                auth_psk,
            } => {
                self.auth_psk = auth_psk;
                self.socket_draft = socket_path;
                self.apply_socket(sender);
            }
            AppInput::SetAutoRefresh(value) => self.auto_refresh = value,
            AppInput::SetClearActivityOnTask(value) => self.clear_activity_on_task = value,
            AppInput::SetRefreshInterval(value) => self.refresh_interval_secs = value.max(1),
            AppInput::SetActivityLimit(value) => {
                self.activity_limit = value.max(20);
                self.trim_activity();
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let status_detail = if self.status.is_empty()
            || matches!(self.status.as_str(), "online" | "offline" | "idle")
            || self.status.ends_with(" share(s)")
        {
            String::new()
        } else {
            format!(" · {}", self.status)
        };

        widgets.status_label.set_text(&format!(
            "{} · {} share(s){}{}",
            if self.connected { "online" } else { "offline" },
            self.shares.len(),
            status_detail,
            if self.watching { " · events on" } else { "" },
        ));

        // Keep text entries local while typing. They are read only when the user
        // presses Enter / Add / Download / Apply, so auto-refresh cannot rewrite
        // user input or make GTK churn on each keypress.
        set_spin_if_changed(&widgets.seed_chunk_spin, self.seed_chunk_size as f64);
        set_spin_if_changed(&widgets.parallel_spin, self.download_parallelism as f64);
        set_spin_if_changed(
            &widgets.request_window_spin,
            self.download_request_window as f64,
        );
        set_spin_if_changed(&widgets.discovery_port_spin, self.discovery_port as f64);
        set_spin_if_changed(
            &widgets.discovery_timeout_spin,
            self.discovery_timeout_ms as f64,
        );
        if widgets.resume_check.is_active() != self.resume {
            widgets.resume_check.set_active(self.resume);
        }
        if widgets.auto_refresh_check.is_active() != self.auto_refresh {
            widgets.auto_refresh_check.set_active(self.auto_refresh);
        }
        if widgets.clear_activity_check.is_active() != self.clear_activity_on_task {
            widgets
                .clear_activity_check
                .set_active(self.clear_activity_on_task);
        }
        set_spin_if_changed(
            &widgets.refresh_interval_spin,
            self.refresh_interval_secs as f64,
        );
        set_spin_if_changed(&widgets.activity_limit_spin, self.activity_limit as f64);

        let seed_sig = seed_signature(&self.seed_files);
        if *widgets.seed_signature.borrow() != seed_sig {
            refill_seed_list(&widgets.seed_list, &self.seed_files);
            *widgets.seed_signature.borrow_mut() = seed_sig;
        }

        let library_sig = library_signature(&self.shares);
        if *widgets.library_signature.borrow() != library_sig {
            refill_library_list(&widgets.library_list, &self.shares);
            *widgets.library_signature.borrow_mut() = library_sig;
        }

        let detail_text = self.selected_share_detail();
        if *widgets.detail_text.borrow() != detail_text {
            widgets.detail_buffer.set_text(&detail_text);
            *widgets.detail_text.borrow_mut() = detail_text;
        }

        let transfer_sig = transfer_signature(&self.transfers);
        if *widgets.transfer_signature.borrow() != transfer_sig {
            refill_transfer_list(&widgets.transfer_list, &self.transfers);
            *widgets.transfer_signature.borrow_mut() = transfer_sig;
        }

        widgets.progress_bar.set_fraction(self.progress_fraction);
        if self.progress_fraction == 0.0
            && self.transfers.iter().any(|transfer| {
                transfer.status == TransferStatus::Running && transfer.total_bytes == 0
            })
        {
            widgets.progress_bar.pulse();
        }
        widgets.progress_label.set_text(&self.latest_progress);

        let activity_text = self.activity.iter().cloned().collect::<Vec<_>>().join("\n");
        if *widgets.activity_text.borrow() != activity_text {
            widgets.activity_buffer.set_text(&activity_text);
            *widgets.activity_text.borrow_mut() = activity_text;
        }
    }
}

impl EtleGui {
    fn apply_socket(&mut self, sender: ComponentSender<Self>) {
        let next = self.socket_draft.trim().to_string();
        if next.is_empty() {
            self.push_log("socket: empty path ignored");
            return;
        }

        if next != self.active_socket_path {
            self.watch_generation = self.watch_generation.saturating_add(1);
            self.active_socket_path = next.clone();
            self.connected = false;
            self.watching = false;
            self.refresh_inflight = false;
            self.selected_share = None;
            self.shares.clear();
            self.push_log(format!("socket applied: {next}"));
        }

        sender.input(AppInput::Connect);
        sender.input(AppInput::Refresh);
        sender.input(AppInput::StartWatch);
    }

    fn spawn_refresh(&mut self, sender: ComponentSender<Self>) {
        if self.refresh_inflight {
            return;
        }
        self.status = "refreshing".to_string();
        self.refresh_inflight = true;
        self.last_auto_refresh = Instant::now();
        spawn_ipc_command(
            self.active_socket_path.clone(),
            IpcRequestKind::ListShares,
            IpcCommand::ListShares,
            sender,
        );
    }

    fn start_watch(&mut self, sender: ComponentSender<Self>) {
        if self.active_socket_path.trim().is_empty() {
            self.connected = false;
            self.status = "offline".to_string();
            self.push_log("event watch: empty IPC endpoint");
            return;
        }

        if self.watching {
            self.push_log("watch already active");
            return;
        }

        self.watch_generation = self.watch_generation.saturating_add(1);
        let generation = self.watch_generation;
        self.watching = true;
        self.push_log("watch: subscribing to daemon events");
        spawn_ipc_watch(self.active_socket_path.clone(), generation, sender);
    }

    fn add_seed_path_text(&mut self, value: String) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        self.add_seed_file(PathBuf::from(trimmed));
        self.seed_path.clear();
    }

    fn add_seed_file(&mut self, path: PathBuf) {
        if path.as_os_str().is_empty() {
            return;
        }
        if self.seed_files.iter().any(|existing| existing == &path) {
            self.push_log(format!("seed queue: already added {}", path.display()));
            return;
        }
        self.push_log(format!("seed queue: added {}", path.display()));
        self.seed_files.push(path);
        if self.selected_seed_file.is_none() {
            self.selected_seed_file = Some(0);
        }
    }

    fn remove_selected_seed_file(&mut self) {
        let Some(index) = self.selected_seed_file else {
            return;
        };
        if index < self.seed_files.len() {
            let removed = self.seed_files.remove(index);
            self.push_log(format!("seed queue: removed {}", removed.display()));
            self.selected_seed_file = if self.seed_files.is_empty() {
                None
            } else {
                Some(index.min(self.seed_files.len() - 1))
            };
        }
    }

    fn start_seed_selected(&mut self, sender: ComponentSender<Self>) {
        let index = self.selected_seed_file.unwrap_or(0);
        if self.seed_files.is_empty() {
            self.push_log("seed: add a file first");
            return;
        }
        if index >= self.seed_files.len() {
            self.push_log("seed: selected file no longer exists");
            self.selected_seed_file = None;
            return;
        }
        let path = self.seed_files.remove(index);
        self.selected_seed_file = if self.seed_files.is_empty() {
            None
        } else {
            Some(index.min(self.seed_files.len() - 1))
        };
        self.start_seed_path(path, sender);
    }

    fn start_seed_path(&mut self, path: PathBuf, sender: ComponentSender<Self>) {
        self.prepare_new_task_log();
        let path_text = path.display().to_string();
        let label = file_label(&path);
        let transfer_seq = self.bump_seq();
        self.upsert_transfer(GuiTransfer {
            id: format!("seed:{label}:{transfer_seq}"),
            kind: TransferKind::Seed,
            label: label.clone(),
            share_id: None,
            status: TransferStatus::Running,
            completed_chunks: 0,
            total_chunks: 0,
            bytes_done: 0,
            total_bytes: 0,
            bytes_per_second: 0,
            detail: format!("encrypting · {path_text}"),
            updated_seq: transfer_seq,
        });

        let command = IpcCommand::SeedFile {
            input: path,
            chunk_size: self.seed_chunk_size.max(1),
        };

        self.latest_progress = format!("encrypting · {path_text}");
        self.progress_fraction = 0.0;
        self.push_log(format!("seed queued: {path_text}"));
        spawn_ipc_command(
            self.active_socket_path.clone(),
            IpcRequestKind::Seed,
            command,
            sender,
        );
    }

    fn start_download_from_form(
        &mut self,
        share_id_text: String,
        peers_text: String,
        output_text: String,
        auth_psk_text: String,
        discovery_multicast_text: String,
        sender: ComponentSender<Self>,
    ) {
        match self.build_download_command_from(
            &share_id_text,
            &peers_text,
            &output_text,
            &auth_psk_text,
            &discovery_multicast_text,
        ) {
            Ok((command, share_id)) => {
                self.download_share_id = share_id_text;
                self.download_peers = peers_text;
                self.output_path = output_text;
                self.auth_psk = auth_psk_text;
                if let Ok(multicast) = discovery_multicast_text.parse() {
                    self.discovery_multicast = multicast;
                }

                self.prepare_new_task_log();
                let transfer_seq = self.bump_seq();
                self.upsert_transfer(GuiTransfer {
                    id: format!("download:{share_id}"),
                    kind: TransferKind::Download,
                    label: share_id.to_string(),
                    share_id: Some(share_id),
                    status: TransferStatus::Queued,
                    completed_chunks: 0,
                    total_chunks: 0,
                    bytes_done: 0,
                    total_bytes: 0,
                    bytes_per_second: 0,
                    detail: "waiting for daemon".to_string(),
                    updated_seq: transfer_seq,
                });
                self.latest_progress = format!("download queued · {share_id}");
                self.progress_fraction = 0.0;
                self.push_log(format!("download queued: {share_id}"));
                spawn_ipc_command(
                    self.active_socket_path.clone(),
                    IpcRequestKind::Download,
                    command,
                    sender,
                );
            }
            Err(error) => self.push_log(format!("download: {error}")),
        }
    }

    fn selected_share_summary(&self) -> Option<&IpcShareSummary> {
        self.selected_share.and_then(|index| self.shares.get(index))
    }

    pub(super) fn push_log(&mut self, message: impl Into<String>) {
        let compact = compact_log_line(message.into());
        if self.last_activity_message.as_deref() == Some(compact.as_str()) {
            return;
        }
        self.last_activity_message = Some(compact.clone());
        self.activity
            .push_back(format!("{}  {compact}", short_time()));
        self.trim_activity();
    }

    fn trim_activity(&mut self) {
        while self.activity.len() > self.activity_limit {
            let _ = self.activity.pop_front();
        }
    }

    fn prepare_new_task_log(&mut self) {
        if self.clear_activity_on_task {
            self.activity.clear();
            self.last_activity_message = None;
        }
    }

    fn bump_seq(&mut self) -> u64 {
        let seq = self.next_transfer_seq;
        self.next_transfer_seq = self.next_transfer_seq.saturating_add(1);
        seq
    }

    fn upsert_transfer(&mut self, transfer: GuiTransfer) {
        if self.is_transfer_hidden(&transfer) {
            return;
        }

        if let Some(existing) = self.transfers.iter_mut().find(|candidate| {
            candidate.id == transfer.id
                || (transfer.share_id.is_some()
                    && candidate.share_id == transfer.share_id
                    && candidate.kind == transfer.kind)
                || (transfer.share_id.is_none()
                    && candidate.share_id.is_none()
                    && candidate.kind == transfer.kind
                    && candidate.label == transfer.label)
        }) {
            *existing = transfer;
        } else {
            self.transfers.push(transfer);
        }
        self.sort_transfers();
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

    fn update_latest_from_transfer(&mut self, transfer: &GuiTransfer) {
        self.progress_fraction = transfer.fraction();
        self.latest_progress = format!("{} · {}", transfer.label, transfer.compact_line());
    }

    fn clear_finished_transfers(&mut self) {
        let hidden = self
            .transfers
            .iter()
            .filter(|transfer| transfer.status.is_finished())
            .flat_map(transfer_hidden_keys)
            .collect::<Vec<_>>();
        self.hidden_finished_transfers.extend(hidden);
        self.transfers
            .retain(|transfer| !transfer.status.is_finished());
        if self.transfers.is_empty() {
            self.latest_progress = "idle".to_string();
            self.progress_fraction = 0.0;
        }
    }

    fn is_transfer_hidden(&self, transfer: &GuiTransfer) -> bool {
        transfer.status.is_finished()
            && transfer_hidden_keys(transfer)
                .into_iter()
                .any(|key| self.hidden_finished_transfers.contains(&key))
    }

    fn apply_ipc_response(&mut self, kind: IpcRequestKind, result: Result<IpcResponse, String>) {
        match result {
            Ok(IpcResponse::Pong) => {
                self.connected = true;
                self.status = "online".to_string();
                self.push_log("daemon: pong");
            }
            Ok(IpcResponse::Ack { message }) => {
                self.connected = true;
                self.push_log(format!("daemon: {message}"));
            }
            Ok(IpcResponse::Shares { shares }) => {
                self.connected = true;
                self.status = "idle".to_string();
                self.shares = shares;
                if self
                    .selected_share
                    .is_some_and(|index| index >= self.shares.len())
                {
                    self.selected_share = None;
                }
                self.sync_existing_transfers_from_shares();
            }
            Ok(IpcResponse::ShareAdded { share }) => {
                self.connected = true;
                self.upsert_share(share.clone());
                self.mark_seed_completed_from_share(&share);
                self.push_log(format!("share added: {} {}", share.share_id, share.name));
            }
            Ok(IpcResponse::TransferQueued { share_id, job_id }) => {
                self.connected = true;
                let seq = self.bump_seq();
                if let Some(transfer) =
                    self.find_transfer_by_share_mut(share_id, TransferKind::Download)
                {
                    transfer.id = job_id.clone();
                    transfer.status = TransferStatus::Running;
                    transfer.detail = "downloading".to_string();
                    transfer.updated_seq = seq;
                } else {
                    self.upsert_transfer(GuiTransfer {
                        id: job_id.clone(),
                        kind: TransferKind::Download,
                        label: share_id.to_string(),
                        share_id: Some(share_id),
                        status: TransferStatus::Running,
                        completed_chunks: 0,
                        total_chunks: 0,
                        bytes_done: 0,
                        total_bytes: 0,
                        bytes_per_second: 0,
                        detail: "downloading".to_string(),
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
                let mut latest: Option<GuiTransfer> = None;
                if let Some(transfer) =
                    self.find_transfer_by_share_mut(share_id, TransferKind::Download)
                {
                    transfer.status = TransferStatus::Done;
                    transfer.label = file_name.clone();
                    transfer.completed_chunks = chunks;
                    transfer.total_chunks = chunks;
                    transfer.bytes_done = file_size;
                    transfer.total_bytes = file_size;
                    transfer.bytes_per_second = 0;
                    transfer.detail = format!("saved · {}", output.display());
                    transfer.updated_seq = seq;
                    latest = Some(transfer.clone());
                }
                if let Some(transfer) = latest.as_ref() {
                    self.update_latest_from_transfer(transfer);
                } else {
                    self.progress_fraction = 1.0;
                    self.latest_progress = format!(
                        "completed · {} · {} · {} chunks · {}",
                        file_name,
                        human_bytes(file_size),
                        chunks,
                        output.display()
                    );
                }
                self.push_log(format!(
                    "completed: share={share_id} output={}",
                    output.display()
                ));
                self.sort_transfers();
            }
            Ok(IpcResponse::Error { message }) => {
                if matches!(kind, IpcRequestKind::Seed | IpcRequestKind::Download) {
                    self.mark_recent_running_failed(&message);
                }
                self.push_log(format!("daemon error: {message}"));
            }
            Err(error) => match kind {
                IpcRequestKind::ListShares => {
                    self.refresh_inflight = false;
                    self.status = "refresh delayed".to_string();
                    self.push_log(format!("refresh: {error}"));
                }
                IpcRequestKind::Ping => {
                    self.connected = false;
                    self.status = "offline".to_string();
                    self.push_log(format!("ipc: {error}"));
                }
                IpcRequestKind::Seed | IpcRequestKind::Download => {
                    self.connected = false;
                    self.status = "request failed".to_string();
                    self.mark_recent_running_failed(&error);
                    self.push_log(format!("{} request failed: {error}", kind.label()));
                }
            },
        }
    }

    fn apply_ipc_event(&mut self, event: IpcEvent) {
        let debug_event = format!("{event:?}");

        #[allow(unreachable_patterns)]
        match event {
            IpcEvent::ServerStarted { listen } => {
                self.connected = true;
                self.push_log(format!("server started: {listen}"));
            }
            IpcEvent::ServerStopped => {
                self.connected = false;
                self.push_log("server stopped");
            }
            IpcEvent::ShareUpdated { share } => {
                self.upsert_share(share.clone());
                self.mark_seed_completed_from_share(&share);
                self.sync_existing_transfer_from_share(&share);
            }
            IpcEvent::PeerConnected { peer_id } => {
                self.push_log(format!("peer connected: {peer_id}"));
            }
            IpcEvent::ChunkCompleted {
                share_id,
                completed_chunks,
                total_chunks,
            } => {
                let seq = self.bump_seq();
                let mut latest: Option<GuiTransfer> = None;
                if let Some(transfer) =
                    self.find_transfer_by_share_mut(share_id, TransferKind::Download)
                {
                    transfer.status = TransferStatus::Running;
                    transfer.completed_chunks = completed_chunks;
                    transfer.total_chunks = total_chunks;
                    transfer.detail = "chunk verified".to_string();
                    transfer.updated_seq = seq;
                    latest = Some(transfer.clone());
                }
                if let Some(transfer) = latest.as_ref() {
                    self.update_latest_from_transfer(transfer);
                } else {
                    self.progress_fraction = fraction(completed_chunks as u64, total_chunks as u64);
                    self.latest_progress =
                        format!("chunk: {share_id} {completed_chunks}/{total_chunks}");
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
                let label = self
                    .shares
                    .iter()
                    .find(|share| share.share_id == share_id)
                    .map(|share| share.name.clone())
                    .unwrap_or_else(|| share_id.to_string());
                let existing_detail = self
                    .transfers
                    .iter()
                    .find(|transfer| {
                        transfer.id == id
                            || (transfer.share_id == Some(share_id)
                                && transfer.kind == TransferKind::Download)
                    })
                    .map(|transfer| transfer.detail.clone())
                    .unwrap_or_default();
                let detail = if is_specific_progress_detail(&existing_detail) {
                    existing_detail
                } else {
                    "receiving + verifying".to_string()
                };
                let transfer = GuiTransfer {
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
                    updated_seq: seq,
                };
                self.update_latest_from_transfer(&transfer);
                self.upsert_transfer(transfer);
            }
            IpcEvent::TransferCompleted {
                job_id,
                share_id,
                output,
            } => {
                let seq = self.bump_seq();
                let mut latest: Option<GuiTransfer> = None;
                let transfer_index = job_id
                    .as_deref()
                    .and_then(|id| self.transfers.iter().position(|transfer| transfer.id == id))
                    .or_else(|| {
                        self.transfers.iter().position(|transfer| {
                            transfer.share_id == Some(share_id)
                                && transfer.kind == TransferKind::Download
                        })
                    });
                if let Some(index) = transfer_index {
                    let transfer = &mut self.transfers[index];
                    transfer.status = TransferStatus::Done;
                    transfer.share_id = Some(share_id);
                    if let Some(share) = self.shares.iter().find(|share| share.share_id == share_id)
                    {
                        transfer.label = share.name.clone();
                        transfer.completed_chunks = share.completed_chunks;
                        transfer.total_chunks = share.total_chunks;
                    }
                    transfer.detail = format!("saved · {}", output.display());
                    transfer.updated_seq = seq;
                    latest = Some(transfer.clone());
                }
                if let Some(transfer) = latest.as_ref() {
                    self.update_latest_from_transfer(transfer);
                } else {
                    self.progress_fraction = 1.0;
                    self.latest_progress = format!(
                        "completed · share={} · output={}",
                        share_id,
                        output.display()
                    );
                }
                self.push_log(self.latest_progress.clone());
                self.sort_transfers();
            }
            IpcEvent::Error { message } => {
                self.mark_recent_running_failed(&message);
                self.push_log(format!("event error: {message}"));
            }
            _ => {
                if let Some(progress) = parse_task_progress_debug(&debug_event) {
                    self.apply_task_progress(progress);
                } else {
                    self.push_log(format!("event: {debug_event}"));
                }
            }
        }
    }

    fn apply_task_progress(&mut self, progress: TaskProgressSnapshot) {
        let kind = if progress.task.contains("seed")
            || progress.task.contains("encrypt")
            || progress.task.contains("staged")
        {
            TransferKind::Seed
        } else {
            TransferKind::Download
        };
        let share_id = progress.label.parse().ok();
        let label = share_id
            .and_then(|share_id| {
                self.shares
                    .iter()
                    .find(|share| share.share_id == share_id)
                    .map(|share| share.name.clone())
            })
            .unwrap_or_else(|| trim_path_label(&progress.label));
        let id = progress
            .job_id
            .clone()
            .unwrap_or_else(|| format!("{}:{label}", kind.label()));
        let completed =
            progress.total_chunks > 0 && progress.completed_chunks >= progress.total_chunks;
        let status =
            if progress.task.contains("completed") || (kind == TransferKind::Seed && completed) {
                TransferStatus::Done
            } else {
                TransferStatus::Running
            };
        let detail = friendly_task_detail(&progress.task);
        let seq = self.bump_seq();
        let transfer = GuiTransfer {
            id,
            kind,
            label,
            share_id,
            status,
            completed_chunks: progress.completed_chunks,
            total_chunks: progress.total_chunks,
            bytes_done: progress.bytes_done,
            total_bytes: progress.total_bytes,
            bytes_per_second: progress.bytes_per_second,
            detail,
            updated_seq: seq,
        };
        self.update_latest_from_transfer(&transfer);
        self.upsert_transfer(transfer);
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
    }

    fn sync_existing_transfers_from_shares(&mut self) {
        let shares = self.shares.clone();
        for share in &shares {
            self.sync_existing_transfer_from_share(share);
        }
    }

    fn sync_existing_transfer_from_share(&mut self, share: &IpcShareSummary) {
        let mode = share.mode.as_deref().unwrap_or("unknown");
        let done = share_is_complete(share);
        let seq = self.bump_seq();

        if let Some(index) = self
            .transfers
            .iter()
            .position(|transfer| transfer.share_id == Some(share.share_id))
        {
            let transfer = &mut self.transfers[index];
            transfer.label = share.name.clone();
            transfer.completed_chunks = share.completed_chunks;
            transfer.total_chunks = share.total_chunks;

            if done && matches!(mode, "completed" | "seeding") {
                transfer.status = TransferStatus::Done;
                if transfer.kind == TransferKind::Seed {
                    transfer.detail = "encrypted and stored".to_string();
                } else if !transfer.detail.contains("saved") {
                    transfer.detail = "completed in library".to_string();
                }
            } else if mode == "downloading" && !transfer.status.is_finished() {
                transfer.status = TransferStatus::Running;
                if !is_specific_progress_detail(&transfer.detail) {
                    transfer.detail = "waiting for live progress".to_string();
                }
            }

            transfer.updated_seq = seq;
            if transfer.status.is_finished() {
                let snapshot = transfer.clone();
                self.update_latest_from_transfer(&snapshot);
            }
            self.sort_transfers();
            return;
        }

        // Only resurrect active downloads from persisted daemon state. Completed
        // shares belong in Library unless they correspond to an already visible
        // transfer card.
        if mode == "downloading" && !done {
            self.upsert_transfer(GuiTransfer {
                id: format!("library:{}:{mode}", share.share_id),
                kind: TransferKind::Download,
                label: share.name.clone(),
                share_id: Some(share.share_id),
                status: TransferStatus::Running,
                completed_chunks: share.completed_chunks,
                total_chunks: share.total_chunks,
                bytes_done: 0,
                total_bytes: 0,
                bytes_per_second: 0,
                detail: "waiting for live progress".to_string(),
                updated_seq: seq,
            });
        }
    }

    fn mark_seed_completed_from_share(&mut self, share: &IpcShareSummary) {
        let completed = share_is_complete(share);
        if !completed {
            return;
        }

        let unbound_running_seed_count = self
            .transfers
            .iter()
            .filter(|transfer| {
                transfer.kind == TransferKind::Seed
                    && transfer.share_id.is_none()
                    && !transfer.status.is_finished()
            })
            .count();
        let seq = self.bump_seq();
        let mut updated = false;
        for transfer in &mut self.transfers {
            let matches_label = transfer.label == share.name;
            let only_unbound_running_seed = unbound_running_seed_count == 1
                && transfer.kind == TransferKind::Seed
                && transfer.share_id.is_none()
                && !transfer.status.is_finished();
            if transfer.kind == TransferKind::Seed
                && !transfer.status.is_finished()
                && (matches_label
                    || transfer.share_id == Some(share.share_id)
                    || only_unbound_running_seed)
            {
                transfer.share_id = Some(share.share_id);
                transfer.label = share.name.clone();
                transfer.status = TransferStatus::Done;
                transfer.completed_chunks = share.completed_chunks;
                transfer.total_chunks = share.total_chunks;
                transfer.detail = "encrypted and stored".to_string();
                transfer.updated_seq = seq;
                updated = true;
            }
        }

        if !updated && share.mode.as_deref().is_some_and(|mode| mode == "seeding") {
            self.sync_existing_transfer_from_share(share);
        }

        self.sort_transfers();
    }

    fn mark_recent_running_failed(&mut self, reason: &str) {
        if let Some(transfer) = self
            .transfers
            .iter_mut()
            .filter(|transfer| {
                matches!(
                    transfer.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
            })
            .max_by_key(|transfer| transfer.updated_seq)
        {
            transfer.status = TransferStatus::Failed;
            transfer.detail = compact_log_line(reason);
        }
        self.sort_transfers();
    }

    fn find_transfer_by_share_mut(
        &mut self,
        share_id: etle::file::descriptor::ShareId,
        kind: TransferKind,
    ) -> Option<&mut GuiTransfer> {
        self.transfers
            .iter_mut()
            .find(|transfer| transfer.share_id == Some(share_id) && transfer.kind == kind)
    }

    fn selected_share_detail(&self) -> String {
        let Some(share) = self.selected_share_summary() else {
            return "select a share".to_string();
        };

        let percent = if share.total_chunks == 0 {
            if share_is_complete(share) { 100.0 } else { 0.0 }
        } else {
            (share.completed_chunks as f64 / share.total_chunks as f64) * 100.0
        };
        let missing = share.total_chunks.saturating_sub(share.completed_chunks);

        format!(
            "Name       : {}\nShare ID   : {}\nMode       : {}\nChunks     : {}/{} ({percent:.2}%)\nMissing    : {}\nSecret key : {}",
            share.name,
            share.share_id,
            share.mode.as_deref().unwrap_or("unknown"),
            share.completed_chunks,
            share.total_chunks,
            missing,
            if share.has_secret { "yes" } else { "no" },
        )
    }
}

fn friendly_task_detail(task: &str) -> String {
    if task.contains("completed") {
        "completed".to_string()
    } else if task.contains("decrypted+written") {
        "decrypting + writing".to_string()
    } else if task.contains("received+verified") {
        "receiving + verifying".to_string()
    } else if task.contains("served-from-library") {
        "uploading chunks".to_string()
    } else if task.contains("staged+encrypted") {
        "encrypting + staging".to_string()
    } else if task.contains("seed") {
        "seeding".to_string()
    } else {
        task.replace(':', " · ")
    }
}

fn is_specific_progress_detail(detail: &str) -> bool {
    detail.contains("receiving")
        || detail.contains("decrypting")
        || detail.contains("encrypting")
        || detail.contains("uploading")
        || detail.contains("writing")
        || detail.contains("saved")
}

fn share_is_complete(share: &IpcShareSummary) -> bool {
    if share.total_chunks == 0 {
        return matches!(share.mode.as_deref(), Some("seeding" | "completed"));
    }

    share.completed_chunks >= share.total_chunks
}

fn transfer_hidden_keys(transfer: &GuiTransfer) -> Vec<String> {
    let mut keys = vec![
        transfer.id.clone(),
        format!("{}:label:{}", transfer.kind.label(), transfer.label),
        transfer.hide_key(),
    ];
    if let Some(share_id) = transfer.share_id {
        keys.push(format!("{}:share:{share_id}", transfer.kind.label()));
    }
    keys
}
