use etle::ipc::message::IpcJobKind;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, pango, prelude::*},
};

use crate::gui::model::{DaemonSettingsSnapshot, GuiInit};

#[derive(Debug)]
pub enum SettingsInput {
    Sync(DaemonSettingsSnapshot),
    SetSocketText(String),
    PskChanged(String),
    ApplyClicked,
    ResetDefaultClicked,
    ReconnectWatchClicked,
    RequestShutdownClicked,
    ConfirmShutdownClicked,
    CancelShutdownClicked,
    AutoRefreshToggled(bool),
    ClearActivityToggled(bool),
    RefreshIntervalChanged(u64),
    ActivityLimitChanged(usize),
}

#[derive(Debug)]
pub enum SettingsOutput {
    ApplySettings {
        socket_path: String,
        auth_psk: String,
    },
    ResetSocketDefault,
    ReconnectWatch,
    RequestShutdown,
    ConfirmShutdown,
    CancelShutdown,
    SetAutoRefresh(bool),
    SetClearActivityOnTask(bool),
    SetRefreshInterval(u64),
    SetActivityLimit(usize),
}

pub struct SettingsPage {
    socket_buf: gtk::EntryBuffer,
    psk_text: String,
    refresh_adj: gtk::Adjustment,
    activity_limit_adj: gtk::Adjustment,
    snapshot: DaemonSettingsSnapshot,
}

impl SettingsPage {
    fn connection_text(&self) -> String {
        let s = &self.snapshot;
        let conn = if s.connected { "Connected" } else { "Offline" };
        let watch = if s.watching {
            "event stream subscribed"
        } else if s.watch_connecting {
            "subscribing to events…"
        } else {
            "event stream inactive"
        };
        format!("{conn} ({watch}) · {}", s.active_socket_path)
    }

    fn peer_id_text(&self) -> String {
        self.snapshot
            .daemon_status
            .as_ref()
            .map(|d| d.peer_id.clone())
            .unwrap_or_else(|| "—".to_string())
    }

    fn listen_text(&self) -> String {
        self.snapshot
            .daemon_status
            .as_ref()
            .map(|d| {
                d.listen
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| "Disabled".to_string())
            })
            .unwrap_or_else(|| "—".to_string())
    }

    fn library_root_text(&self) -> String {
        self.snapshot
            .daemon_status
            .as_ref()
            .map(|d| d.library_root.display().to_string())
            .unwrap_or_else(|| "—".to_string())
    }

    fn discovery_text(&self) -> String {
        self.snapshot
            .daemon_status
            .as_ref()
            .map(|d| {
                if d.auto_discovery {
                    format!("Enabled (UDP port {})", d.discovery_port)
                } else {
                    "Disabled".to_string()
                }
            })
            .unwrap_or_else(|| "—".to_string())
    }

    fn auth_text(&self) -> &'static str {
        match self.snapshot.daemon_status.as_ref() {
            Some(d) if d.auth_required => "PSK required for P2P connections",
            Some(_) => "Open P2P (no PSK configured on daemon)",
            None => "—",
        }
    }

    fn active_jobs_text(&self) -> String {
        let Some(d) = self.snapshot.daemon_status.as_ref() else {
            return "—".to_string();
        };
        if d.active_jobs.is_empty() {
            return format!("Idle · {} share(s) in library", d.share_count);
        }
        let summary = d
            .active_jobs
            .iter()
            .map(|j| {
                let k = match j.kind {
                    IpcJobKind::Seed => "seed",
                    IpcJobKind::Download => "download",
                    IpcJobKind::Upload => "upload",
                };
                format!("{k}: {}", j.label)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} active ({summary})", d.active_jobs.len())
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsPage {
    type Init = (GuiInit, DaemonSettingsSnapshot);
    type Input = SettingsInput;
    type Output = SettingsOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 10,

            gtk::Frame {
                set_label: Some("Daemon Runtime Status"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Grid {
                    set_row_spacing: 6,
                    set_column_spacing: 12,

                    attach[0, 0, 1, 1] = &gtk::Label {
                        set_label: "Connection",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 0, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        set_wrap_mode: pango::WrapMode::Char,
                        #[watch]
                        set_label: &model.connection_text(),
                    },

                    attach[0, 1, 1, 1] = &gtk::Label {
                        set_label: "Peer ID",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 1, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        set_wrap_mode: pango::WrapMode::Char,
                        set_selectable: true,
                        add_css_class: "etle-monospace",
                        #[watch]
                        set_label: &model.peer_id_text(),
                    },

                    attach[0, 2, 1, 1] = &gtk::Label {
                        set_label: "P2P Listen",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 2, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        set_wrap_mode: pango::WrapMode::Char,
                        add_css_class: "etle-monospace",
                        #[watch]
                        set_label: &model.listen_text(),
                    },

                    attach[0, 3, 1, 1] = &gtk::Label {
                        set_label: "Library Root",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 3, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        set_wrap_mode: pango::WrapMode::Char,
                        set_selectable: true,
                        add_css_class: "etle-monospace",
                        #[watch]
                        set_label: &model.library_root_text(),
                    },

                    attach[0, 4, 1, 1] = &gtk::Label {
                        set_label: "Discovery",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 4, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        #[watch]
                        set_label: &model.discovery_text(),
                    },

                    attach[0, 5, 1, 1] = &gtk::Label {
                        set_label: "P2P Auth Mode",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 5, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        #[watch]
                        set_label: model.auth_text(),
                    },

                    attach[0, 6, 1, 1] = &gtk::Label {
                        set_label: "Active Jobs",
                        set_xalign: 0.0,
                        set_valign: gtk::Align::Start,
                        set_width_chars: 18,
                        add_css_class: "etle-kv-key",
                    },
                    attach[1, 6, 1, 1] = &gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_wrap: true,
                        #[watch]
                        set_label: &model.active_jobs_text(),
                    },
                },
            },

            gtk::Frame {
                set_label: Some("Daemon Connection & Security"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "IPC endpoint",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,
                            set_hexpand: true,

                            gtk::Entry {
                                set_buffer: &model.socket_buf,
                                set_hexpand: true,
                                add_css_class: "etle-monospace",
                                set_placeholder_text: Some(if cfg!(windows) {
                                    r"\\.\pipe\etled"
                                } else {
                                    "/path/to/etled.sock"
                                }),
                                connect_activate => SettingsInput::ApplyClicked,
                            },

                            gtk::Button {
                                set_label: "Apply",
                                add_css_class: "suggested-action",
                                connect_clicked => SettingsInput::ApplyClicked,
                            },

                            gtk::Button {
                                set_label: "Default",
                                connect_clicked => SettingsInput::ResetDefaultClicked,
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Default download PSK",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::PasswordEntry {
                            set_hexpand: true,
                            set_show_peek_icon: true,
                            set_text: &init.0.auth_psk,
                            set_placeholder_text: Some("Default pre-shared key used when Download PSK override is empty"),
                            connect_changed[sender] => move |entry| {
                                sender.input(SettingsInput::PskChanged(entry.text().into()));
                            },
                            connect_activate => SettingsInput::ApplyClicked,
                        },
                    },

                    gtk::Label {
                        set_label: "Note: Daemon listen address, discovery settings, and daemon PSK are set when starting etled.",
                        set_xalign: 0.0,
                        set_wrap: true,
                        add_css_class: "etle-hint",
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,

                        gtk::Button {
                            set_label: "Reconnect Event Stream",
                            #[watch]
                            set_sensitive: model.snapshot.connected
                                && !model.snapshot.watching
                                && !model.snapshot.watch_connecting,
                            connect_clicked => SettingsInput::ReconnectWatchClicked,
                        },

                        gtk::Button {
                            set_label: "Stop Daemon",
                            add_css_class: "destructive-action",
                            #[watch]
                            set_sensitive: model.snapshot.connected,
                            connect_clicked => SettingsInput::RequestShutdownClicked,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        add_css_class: "etle-confirm-bar",
                        #[watch]
                        set_visible: model.snapshot.connected && model.snapshot.pending_shutdown_confirm,

                        gtk::Label {
                            set_label: "Stop the running etled daemon now? Active transfers will be aborted.",
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                        },

                        gtk::Button {
                            set_label: "Confirm Shutdown",
                            add_css_class: "destructive-action",
                            connect_clicked => SettingsInput::ConfirmShutdownClicked,
                        },

                        gtk::Button {
                            set_label: "Cancel",
                            connect_clicked => SettingsInput::CancelShutdownClicked,
                        },
                    },
                },
            },

            gtk::Frame {
                set_label: Some("GUI Preferences"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::CheckButton {
                        set_label: Some("Auto refresh status and share list"),
                        #[watch]
                        #[block_signal(auto_refresh_handler)]
                        set_active: model.snapshot.auto_refresh,
                        connect_toggled[sender] => move |btn| {
                            sender.input(SettingsInput::AutoRefreshToggled(btn.is_active()));
                        } @auto_refresh_handler,
                    },

                    gtk::CheckButton {
                        set_label: Some("Clear activity log when starting a new seed or download"),
                        #[watch]
                        #[block_signal(clear_act_handler)]
                        set_active: model.snapshot.clear_activity_on_task,
                        connect_toggled[sender] => move |btn| {
                            sender.input(SettingsInput::ClearActivityToggled(btn.is_active()));
                        } @clear_act_handler,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Refresh interval (sec)",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::SpinButton {
                            set_adjustment: &model.refresh_adj,
                            set_digits: 0,
                            set_numeric: true,
                            connect_value_changed[sender] => move |spin| {
                                sender.input(SettingsInput::RefreshIntervalChanged(
                                    spin.value_as_int().max(1) as u64,
                                ));
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Activity history limit",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::SpinButton {
                            set_adjustment: &model.activity_limit_adj,
                            set_digits: 0,
                            set_numeric: true,
                            connect_value_changed[sender] => move |spin| {
                                sender.input(SettingsInput::ActivityLimitChanged(
                                    spin.value_as_int().max(20) as usize,
                                ));
                            },
                        },
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
        let model = Self {
            socket_buf: gtk::EntryBuffer::new(Some(&init.0.socket_path)),
            psk_text: init.0.auth_psk.clone(),
            refresh_adj: gtk::Adjustment::new(
                init.1.refresh_interval_secs as f64,
                1.0,
                60.0,
                1.0,
                5.0,
                0.0,
            ),
            activity_limit_adj: gtk::Adjustment::new(
                init.1.activity_limit as f64,
                20.0,
                2000.0,
                20.0,
                100.0,
                0.0,
            ),
            snapshot: init.1,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SettingsInput::Sync(snapshot) => {
                self.snapshot = snapshot;
            }
            SettingsInput::SetSocketText(text) => {
                self.socket_buf.set_text(&text);
            }
            SettingsInput::PskChanged(psk) => {
                self.psk_text = psk;
            }
            SettingsInput::ApplyClicked => {
                let socket_path: String = self.socket_buf.text().into();
                let _ = sender.output(SettingsOutput::ApplySettings {
                    socket_path,
                    auth_psk: self.psk_text.clone(),
                });
            }
            SettingsInput::ResetDefaultClicked => {
                let _ = sender.output(SettingsOutput::ResetSocketDefault);
            }
            SettingsInput::ReconnectWatchClicked => {
                let _ = sender.output(SettingsOutput::ReconnectWatch);
            }
            SettingsInput::RequestShutdownClicked => {
                let _ = sender.output(SettingsOutput::RequestShutdown);
            }
            SettingsInput::ConfirmShutdownClicked => {
                let _ = sender.output(SettingsOutput::ConfirmShutdown);
            }
            SettingsInput::CancelShutdownClicked => {
                let _ = sender.output(SettingsOutput::CancelShutdown);
            }
            SettingsInput::AutoRefreshToggled(val) => {
                let _ = sender.output(SettingsOutput::SetAutoRefresh(val));
            }
            SettingsInput::ClearActivityToggled(val) => {
                let _ = sender.output(SettingsOutput::SetClearActivityOnTask(val));
            }
            SettingsInput::RefreshIntervalChanged(secs) => {
                let _ = sender.output(SettingsOutput::SetRefreshInterval(secs));
            }
            SettingsInput::ActivityLimitChanged(limit) => {
                let _ = sender.output(SettingsOutput::SetActivityLimit(limit));
            }
        }
    }
}
