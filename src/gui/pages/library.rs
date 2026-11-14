use etle::file::descriptor::ShareId;
use relm4::{
    ComponentParts, ComponentSender, RelmWidgetExt, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, pango, prelude::*},
};

use crate::gui::{
    components::{restore_listbox_selection, rows::ShareRow, sync_factory_rows},
    format::{human_bytes, share_mode_display},
    model::{ShareRowData, share_is_complete},
};

#[derive(Debug)]
pub enum LibraryInput {
    Sync {
        rows: Vec<ShareRowData>,
        selected_share_id: Option<ShareId>,
        pending_delete: Option<ShareId>,
        connected: bool,
    },
    RowSelected(Option<usize>),
    CopyClicked,
    ResumeClicked,
    RequestDeleteClicked,
    ConfirmDeleteClicked,
    CancelDeleteClicked,
    RefreshClicked,
}

#[derive(Debug)]
pub enum LibraryOutput {
    SelectShare(ShareId),
    CopyShareId(ShareId),
    PrepareDownload(ShareId),
    RequestDelete(ShareId),
    ConfirmDelete(ShareId),
    CancelDelete,
    Refresh,
}

pub struct LibraryPage {
    factory: FactoryVecDeque<ShareRow>,
    rows: Vec<ShareRowData>,
    selected_share_id: Option<ShareId>,
    pending_delete: Option<ShareId>,
    connected: bool,
}

impl LibraryPage {
    fn selected_row(&self) -> Option<&ShareRowData> {
        let id = self.selected_share_id?;
        self.rows.iter().find(|r| r.summary.share_id == id)
    }

    fn selected_index(&self) -> Option<usize> {
        let id = self.selected_share_id?;
        self.rows.iter().position(|r| r.summary.share_id == id)
    }

    fn selected_badge(&self) -> (&'static str, &'static str) {
        match self.selected_row() {
            Some(row) => share_mode_display(
                row.summary.mode.as_deref(),
                row.is_downloading,
                share_is_complete(&row.summary),
            ),
            None => ("—", "etle-badge-muted"),
        }
    }

    fn detail_name(&self) -> String {
        self.selected_row()
            .map(|r| r.summary.name.clone())
            .unwrap_or_default()
    }

    fn detail_share_id(&self) -> String {
        self.selected_row()
            .map(|r| r.summary.share_id.to_string())
            .unwrap_or_default()
    }

    fn detail_size(&self) -> String {
        self.selected_row()
            .map(|r| {
                let s = &r.summary;
                if s.total_bytes > 0 {
                    format!(
                        "{} ({} bytes; chunk {})",
                        human_bytes(s.total_bytes),
                        s.total_bytes,
                        human_bytes(s.chunk_size)
                    )
                } else if s.chunk_size > 0 {
                    format!("Unknown (chunk {})", human_bytes(s.chunk_size))
                } else {
                    "Unknown".to_string()
                }
            })
            .unwrap_or_default()
    }

    fn detail_chunks(&self) -> String {
        self.selected_row()
            .map(|r| {
                let s = &r.summary;
                format!(
                    "{} / {} complete",
                    s.completed_chunks.min(s.total_chunks),
                    s.total_chunks
                )
            })
            .unwrap_or_default()
    }

    fn detail_missing(&self) -> String {
        self.selected_row()
            .map(|r| {
                let missing = r
                    .summary
                    .total_chunks
                    .saturating_sub(r.summary.completed_chunks);
                if missing == 0 {
                    "0 (all verified)".to_string()
                } else {
                    format!("{missing} remaining")
                }
            })
            .unwrap_or_default()
    }

    fn detail_key(&self) -> &'static str {
        match self.selected_row() {
            Some(row) if row.summary.has_secret => "Present in local daemon state",
            Some(_) => "Not stored locally (descriptor only)",
            None => "",
        }
    }

    fn pending_delete_message(&self) -> String {
        let Some(pending_id) = self.pending_delete else {
            return String::new();
        };
        let Some(row) = self.rows.iter().find(|r| r.summary.share_id == pending_id) else {
            return String::new();
        };
        format!(
            "Delete local share \"{}\" ({}) from daemon library?",
            row.summary.name, row.summary.share_id
        )
    }

    fn can_resume(&self) -> bool {
        self.connected
            && self
                .selected_row()
                .is_some_and(|r| !share_is_complete(&r.summary) && !r.is_downloading)
    }

    fn can_delete(&self) -> bool {
        self.connected && self.selected_row().is_some_and(|r| !r.is_downloading)
    }
}

#[relm4::component(pub)]
impl SimpleComponent for LibraryPage {
    type Init = ();
    type Input = LibraryInput;
    type Output = LibraryOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 10,

            gtk::Frame {
                set_label: Some("Actions"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,

                        gtk::Button {
                            set_label: "Copy Share ID",
                            #[watch]
                            set_sensitive: model.selected_row().is_some(),
                            connect_clicked => LibraryInput::CopyClicked,
                        },

                        gtk::Button {
                            set_label: "Resume Download",
                            #[watch]
                            set_sensitive: model.can_resume(),
                            connect_clicked => LibraryInput::ResumeClicked,
                        },

                        gtk::Button {
                            set_label: "Delete Share",
                            add_css_class: "destructive-action",
                            #[watch]
                            set_sensitive: model.can_delete(),
                            connect_clicked => LibraryInput::RequestDeleteClicked,
                        },

                        gtk::Box {
                            set_hexpand: true,
                        },

                        gtk::Button {
                            set_label: "Refresh",
                            connect_clicked => LibraryInput::RefreshClicked,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        add_css_class: "etle-confirm-bar",
                        #[watch]
                        set_visible: model.connected && model.pending_delete.is_some() && !model.pending_delete_message().is_empty(),

                        gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            #[watch]
                            set_label: &model.pending_delete_message(),
                        },

                        gtk::Button {
                            set_label: "Confirm Delete",
                            add_css_class: "destructive-action",
                            connect_clicked => LibraryInput::ConfirmDeleteClicked,
                        },

                        gtk::Button {
                            set_label: "Cancel",
                            connect_clicked => LibraryInput::CancelDeleteClicked,
                        },
                    },
                },
            },

            gtk::Frame {
                set_label: Some("Library Shares"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_min_content_height: 240,
                    set_propagate_natural_height: true,
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Label {
                            set_xalign: 0.0,
                            set_margin_all: 12,
                            add_css_class: "dim-label",
                            #[watch]
                            set_visible: model.rows.is_empty(),
                            #[watch]
                            set_label: if model.connected {
                                "No local shares yet. Seed a file or start a download to populate the library."
                            } else {
                                "Daemon is offline. Start etled or check endpoint in Settings."
                            },
                        },

                        #[local_ref]
                        share_list -> gtk::ListBox {
                            set_vexpand: true,
                            set_selection_mode: gtk::SelectionMode::Single,
                            #[watch]
                            set_visible: !model.rows.is_empty(),
                            connect_row_selected[sender] => move |_, row| {
                                let idx = row.and_then(|r| usize::try_from(r.index()).ok());
                                sender.input(LibraryInput::RowSelected(idx));
                            },
                        },
                    },
                },
            },

            gtk::Frame {
                set_label: Some("Details"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,

                    gtk::Label {
                        set_label: "Select a share above to inspect metadata.",
                        set_xalign: 0.0,
                        add_css_class: "dim-label",
                        #[watch]
                        set_visible: model.selected_row().is_none(),
                    },

                    gtk::Grid {
                        set_row_spacing: 6,
                        set_column_spacing: 12,
                        #[watch]
                        set_visible: model.selected_row().is_some(),

                        attach[0, 0, 1, 1] = &gtk::Label {
                            set_label: "Name",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 0, 1, 1] = &gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            set_wrap_mode: pango::WrapMode::Char,
                            set_selectable: true,
                            #[watch]
                            set_label: &model.detail_name(),
                        },

                        attach[0, 1, 1, 1] = &gtk::Label {
                            set_label: "Share ID",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 1, 1, 1] = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_hexpand: true,

                            gtk::Label {
                                set_xalign: 0.0,
                                set_hexpand: true,
                                set_wrap: true,
                                set_wrap_mode: pango::WrapMode::Char,
                                set_selectable: true,
                                add_css_class: "etle-monospace",
                                #[watch]
                                set_label: &model.detail_share_id(),
                            },

                            gtk::Button {
                                set_label: "Copy",
                                connect_clicked => LibraryInput::CopyClicked,
                            },
                        },

                        attach[0, 2, 1, 1] = &gtk::Label {
                            set_label: "Status",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 2, 1, 1] = &gtk::Label {
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            add_css_class: "etle-badge",
                            #[watch]
                            set_label: model.selected_badge().0,
                            #[watch]
                            set_class_active: ("etle-badge-ok", model.selected_badge().1 == "etle-badge-ok"),
                            #[watch]
                            set_class_active: ("etle-badge-accent", model.selected_badge().1 == "etle-badge-accent"),
                            #[watch]
                            set_class_active: ("etle-badge-warn", model.selected_badge().1 == "etle-badge-warn"),
                            #[watch]
                            set_class_active: ("etle-badge-muted", model.selected_badge().1 == "etle-badge-muted"),
                        },

                        attach[0, 3, 1, 1] = &gtk::Label {
                            set_label: "File Size",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 3, 1, 1] = &gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            set_wrap_mode: pango::WrapMode::Char,
                            #[watch]
                            set_label: &model.detail_size(),
                        },

                        attach[0, 4, 1, 1] = &gtk::Label {
                            set_label: "Chunks",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 4, 1, 1] = &gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            #[watch]
                            set_label: &model.detail_chunks(),
                        },

                        attach[0, 5, 1, 1] = &gtk::Label {
                            set_label: "Missing Chunks",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 5, 1, 1] = &gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            #[watch]
                            set_label: &model.detail_missing(),
                        },

                        attach[0, 6, 1, 1] = &gtk::Label {
                            set_label: "Local File Key",
                            set_xalign: 0.0,
                            set_valign: gtk::Align::Start,
                            set_width_chars: 16,
                            add_css_class: "etle-kv-key",
                        },
                        attach[1, 6, 1, 1] = &gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            set_wrap: true,
                            #[watch]
                            set_label: model.detail_key(),
                        },
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let factory: FactoryVecDeque<ShareRow> = FactoryVecDeque::builder()
            .launch(gtk::ListBox::new())
            .detach();
        let share_list = factory.widget().clone();

        let model = Self {
            factory,
            rows: Vec::new(),
            selected_share_id: None,
            pending_delete: None,
            connected: false,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            LibraryInput::Sync {
                rows,
                selected_share_id,
                pending_delete,
                connected,
            } => {
                self.rows = rows.clone();
                self.selected_share_id = selected_share_id;
                self.pending_delete = pending_delete;
                self.connected = connected;
                sync_factory_rows(&mut self.factory, rows);
            }
            LibraryInput::RowSelected(Some(idx)) => {
                if let Some(row) = self.rows.get(idx) {
                    let share_id = row.summary.share_id;
                    if self.selected_share_id != Some(share_id) {
                        self.selected_share_id = Some(share_id);
                        let _ = sender.output(LibraryOutput::SelectShare(share_id));
                    }
                }
            }
            LibraryInput::RowSelected(None) => {}
            LibraryInput::CopyClicked => {
                if let Some(id) = self.selected_share_id {
                    let _ = sender.output(LibraryOutput::CopyShareId(id));
                }
            }
            LibraryInput::ResumeClicked => {
                if let Some(id) = self.selected_share_id {
                    let _ = sender.output(LibraryOutput::PrepareDownload(id));
                }
            }
            LibraryInput::RequestDeleteClicked => {
                if let Some(id) = self.selected_share_id {
                    let _ = sender.output(LibraryOutput::RequestDelete(id));
                }
            }
            LibraryInput::ConfirmDeleteClicked => {
                if let Some(id) = self.pending_delete.or(self.selected_share_id) {
                    let _ = sender.output(LibraryOutput::ConfirmDelete(id));
                }
            }
            LibraryInput::CancelDeleteClicked => {
                let _ = sender.output(LibraryOutput::CancelDelete);
            }
            LibraryInput::RefreshClicked => {
                let _ = sender.output(LibraryOutput::Refresh);
            }
        }
    }

    fn post_view() {
        restore_listbox_selection(&widgets.share_list, model.selected_index());
    }
}
