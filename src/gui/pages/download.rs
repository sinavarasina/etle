use std::str::FromStr;

use etle::file::descriptor::ShareId;
use relm4::{
    ComponentParts, ComponentSender, RelmWidgetExt, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::gui::model::{DownloadFormRequest, GuiInit};

#[derive(Debug)]
pub enum DownloadInput {
    SetShareId(ShareId),
    ShareIdChanged(String),
    BrowseOutput,
    SetPskOverride(String),
    SetResume(bool),
    Submit,
}

#[derive(Debug)]
pub enum DownloadOutput {
    StartDownload(DownloadFormRequest),
}

pub struct DownloadPage {
    share_id_buf: gtk::EntryBuffer,
    share_id_text: String,
    peers_buf: gtk::EntryBuffer,
    output_buf: gtk::EntryBuffer,
    multicast_buf: gtk::EntryBuffer,
    psk_override: String,
    parallel_adj: gtk::Adjustment,
    window_adj: gtk::Adjustment,
    port_adj: gtk::Adjustment,
    timeout_adj: gtk::Adjustment,
    resume: bool,
}

impl DownloadPage {
    fn share_id_validation(&self) -> (&'static str, &'static str) {
        let trimmed = self.share_id_text.trim();
        if trimmed.is_empty() {
            (
                "Enter a 64-character hex Share ID (or choose Resume Download from Library).",
                "etle-hint",
            )
        } else if ShareId::from_str(trimmed).is_ok() {
            ("Valid 64-hex Share ID", "etle-hint-ok")
        } else {
            (
                "Share ID must be exactly 64 hexadecimal characters",
                "etle-hint-warn",
            )
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for DownloadPage {
    type Init = GuiInit;
    type Input = DownloadInput;
    type Output = DownloadOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 10,

            gtk::Frame {
                set_label: Some("Download Options"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Share ID",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::Entry {
                            set_buffer: &model.share_id_buf,
                            set_hexpand: true,
                            add_css_class: "etle-monospace",
                            set_placeholder_text: Some("64-hex Share ID"),
                            connect_changed[sender] => move |entry| {
                                sender.input(DownloadInput::ShareIdChanged(entry.text().into()));
                            },
                            connect_activate => DownloadInput::Submit,
                        },
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_margin_start: 140,
                        #[watch]
                        set_label: model.share_id_validation().0,
                        #[watch]
                        set_class_active: ("etle-hint", model.share_id_validation().1 == "etle-hint"),
                        #[watch]
                        set_class_active: ("etle-hint-ok", model.share_id_validation().1 == "etle-hint-ok"),
                        #[watch]
                        set_class_active: ("etle-hint-warn", model.share_id_validation().1 == "etle-hint-warn"),
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Peers (optional)",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::Entry {
                            set_buffer: &model.peers_buf,
                            set_hexpand: true,
                            set_placeholder_text: Some("Comma-separated ip:port peers (leave blank for LAN discovery)"),
                            connect_activate => DownloadInput::Submit,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Output file (opt)",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,
                            set_hexpand: true,

                            gtk::Entry {
                                set_buffer: &model.output_buf,
                                set_hexpand: true,
                                set_placeholder_text: Some("Optional output path (defaults to daemon output folder)"),
                                connect_activate => DownloadInput::Submit,
                            },

                            gtk::Button {
                                set_label: "Browse…",
                                connect_clicked => DownloadInput::BrowseOutput,
                            },
                        },
                    },

                    gtk::Expander {
                        set_label: Some("Advanced & Discovery Options"),

                        #[wrap(Some)]
                        set_child = &gtk::Grid {
                            set_row_spacing: 8,
                            set_column_spacing: 12,
                            set_margin_top: 8,

                            attach[0, 0, 1, 1] = &gtk::Label {
                                set_label: "Parallel workers",
                                set_xalign: 0.0,
                            },
                            attach[1, 0, 1, 1] = &gtk::SpinButton {
                                set_adjustment: &model.parallel_adj,
                                set_digits: 0,
                                set_numeric: true,
                                set_tooltip_text: Some("0 = auto by peer count, 1 = sequential"),
                            },

                            attach[0, 1, 1, 1] = &gtk::Label {
                                set_label: "Request window",
                                set_xalign: 0.0,
                            },
                            attach[1, 1, 1, 1] = &gtk::SpinButton {
                                set_adjustment: &model.window_adj,
                                set_digits: 0,
                                set_numeric: true,
                            },

                            attach[0, 2, 1, 1] = &gtk::Label {
                                set_label: "Discovery port",
                                set_xalign: 0.0,
                            },
                            attach[1, 2, 1, 1] = &gtk::SpinButton {
                                set_adjustment: &model.port_adj,
                                set_digits: 0,
                                set_numeric: true,
                            },

                            attach[0, 3, 1, 1] = &gtk::Label {
                                set_label: "Discovery timeout (ms)",
                                set_xalign: 0.0,
                            },
                            attach[1, 3, 1, 1] = &gtk::SpinButton {
                                set_adjustment: &model.timeout_adj,
                                set_digits: 0,
                                set_numeric: true,
                            },

                            attach[0, 4, 1, 1] = &gtk::Label {
                                set_label: "Multicast IPv4",
                                set_xalign: 0.0,
                            },
                            attach[1, 4, 1, 1] = &gtk::Entry {
                                set_buffer: &model.multicast_buf,
                                set_hexpand: true,
                                set_placeholder_text: Some("Discovery multicast IPv4 (e.g. 239.255.0.86)"),
                            },

                            attach[0, 5, 1, 1] = &gtk::Label {
                                set_label: "Transfer mode",
                                set_xalign: 0.0,
                            },
                            attach[1, 5, 1, 1] = &gtk::CheckButton {
                                set_label: Some("Reuse existing verified local chunks (resume)"),
                                #[watch]
                                #[block_signal(resume_handler)]
                                set_active: model.resume,
                                connect_toggled[sender] => move |btn| {
                                    sender.input(DownloadInput::SetResume(btn.is_active()));
                                } @resume_handler,
                            },

                            attach[0, 6, 1, 1] = &gtk::Label {
                                set_label: "Download PSK override",
                                set_xalign: 0.0,
                            },
                            attach[1, 6, 1, 1] = &gtk::PasswordEntry {
                                set_hexpand: true,
                                set_show_peek_icon: true,
                                set_placeholder_text: Some("Leave empty to use Settings default PSK"),
                                connect_changed[sender] => move |entry| {
                                    sender.input(DownloadInput::SetPskOverride(entry.text().into()));
                                },
                                connect_activate => DownloadInput::Submit,
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Button {
                            set_label: "Start Download",
                            add_css_class: "suggested-action",
                            connect_clicked => DownloadInput::Submit,
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
            share_id_buf: gtk::EntryBuffer::default(),
            share_id_text: String::new(),
            peers_buf: gtk::EntryBuffer::default(),
            output_buf: gtk::EntryBuffer::default(),
            multicast_buf: gtk::EntryBuffer::new(Some(&init.discovery_multicast.to_string())),
            psk_override: String::new(),
            parallel_adj: gtk::Adjustment::new(
                init.download_parallelism as f64,
                0.0,
                64.0,
                1.0,
                4.0,
                0.0,
            ),
            window_adj: gtk::Adjustment::new(
                init.request_window.max(1) as f64,
                1.0,
                512.0,
                1.0,
                4.0,
                0.0,
            ),
            port_adj: gtk::Adjustment::new(
                f64::from(init.discovery_port.max(1)),
                1.0,
                65535.0,
                1.0,
                10.0,
                0.0,
            ),
            timeout_adj: gtk::Adjustment::new(
                init.discovery_timeout_ms.max(1) as f64,
                100.0,
                60000.0,
                100.0,
                500.0,
                0.0,
            ),
            resume: true,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            DownloadInput::SetShareId(share_id) => {
                let text = share_id.to_string();
                self.share_id_buf.set_text(&text);
                self.share_id_text = text;
            }
            DownloadInput::ShareIdChanged(text) => {
                self.share_id_text = text;
            }
            DownloadInput::BrowseOutput => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save downloaded file as")
                    .save_file()
                {
                    self.output_buf.set_text(path.display().to_string());
                }
            }
            DownloadInput::SetPskOverride(psk) => {
                self.psk_override = psk;
            }
            DownloadInput::SetResume(resume) => {
                self.resume = resume;
            }
            DownloadInput::Submit => {
                let request = DownloadFormRequest {
                    share_id: self.share_id_buf.text().into(),
                    peers: self.peers_buf.text().into(),
                    output: self.output_buf.text().into(),
                    auth_psk: self.psk_override.clone(),
                    discovery_multicast: self.multicast_buf.text().into(),
                    parallelism: self.parallel_adj.value().max(0.0).round() as usize,
                    request_window: self.window_adj.value().max(1.0).round() as usize,
                    discovery_port: self.port_adj.value().clamp(1.0, 65535.0).round() as u16,
                    discovery_timeout_ms: self.timeout_adj.value().max(1.0).round() as u64,
                    resume: self.resume,
                };
                let _ = sender.output(DownloadOutput::StartDownload(request));
            }
        }
    }
}
