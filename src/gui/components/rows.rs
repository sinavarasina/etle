use relm4::{
    RelmWidgetExt,
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, pango, prelude::*},
};

use super::DiffRow;
use crate::gui::{
    format::{file_label, human_bytes, share_mode_display},
    model::{
        SeedFileEntry, ShareRowData, TransferKind, TransferStatus, TransferView, share_is_complete,
    },
};

#[derive(Debug)]
pub struct ShareRow {
    data: ShareRowData,
}

impl ShareRow {
    fn subtitle_text(&self) -> String {
        let s = &self.data.summary;
        let size = if s.total_bytes > 0 {
            human_bytes(s.total_bytes)
        } else {
            "unknown size".to_string()
        };
        let short_id = s.share_id.to_string();
        let prefix = &short_id[..12.min(short_id.len())];
        format!(
            "{size} · {}/{} chunks · {prefix}…",
            s.completed_chunks, s.total_chunks
        )
    }

    fn status_badge(&self) -> (&'static str, &'static str) {
        let s = &self.data.summary;
        share_mode_display(
            s.mode.as_deref(),
            self.data.is_downloading,
            share_is_complete(s),
        )
    }
}

impl DiffRow for ShareRow {
    fn matches_data(&self, data: &Self::Init) -> bool {
        self.data == *data
    }

    fn apply_data(&mut self, data: Self::Init) {
        self.data = data;
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for ShareRow {
    type Init = ShareRowData;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            add_css_class: "etle-row",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                    set_hexpand: true,

                    gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_width_chars: 1,
                        set_ellipsize: pango::EllipsizeMode::End,
                        add_css_class: "etle-row-title",
                        #[watch]
                        set_label: &self.data.summary.name,
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_width_chars: 1,
                        set_ellipsize: pango::EllipsizeMode::Middle,
                        add_css_class: "dim-label",
                        add_css_class: "etle-row-subtitle",
                        #[watch]
                        set_label: &self.subtitle_text(),
                    },
                },

                gtk::Label {
                    set_valign: gtk::Align::Center,
                    add_css_class: "etle-badge",
                    #[watch]
                    set_label: self.status_badge().0,
                    #[watch]
                    set_class_active: ("etle-badge-ok", self.status_badge().1 == "etle-badge-ok"),
                    #[watch]
                    set_class_active: ("etle-badge-accent", self.status_badge().1 == "etle-badge-accent"),
                    #[watch]
                    set_class_active: ("etle-badge-warn", self.status_badge().1 == "etle-badge-warn"),
                    #[watch]
                    set_class_active: ("etle-badge-muted", self.status_badge().1 == "etle-badge-muted"),
                },
            }
        }
    }

    fn init_model(data: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { data }
    }
}

#[derive(Debug)]
pub struct SeedFileRow {
    entry: SeedFileEntry,
}

impl DiffRow for SeedFileRow {
    fn matches_data(&self, data: &Self::Init) -> bool {
        self.entry == *data
    }

    fn apply_data(&mut self, data: Self::Init) {
        self.entry = data;
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for SeedFileRow {
    type Init = SeedFileEntry;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            add_css_class: "etle-row",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                    set_hexpand: true,

                    gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_width_chars: 1,
                        set_ellipsize: pango::EllipsizeMode::End,
                        add_css_class: "etle-row-title",
                        #[watch]
                        set_label: &file_label(&self.entry.path),
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_width_chars: 1,
                        set_ellipsize: pango::EllipsizeMode::Start,
                        add_css_class: "dim-label",
                        add_css_class: "etle-row-subtitle",
                        #[watch]
                        set_label: &self.entry.path.display().to_string(),
                    },
                },

                gtk::Label {
                    set_valign: gtk::Align::Center,
                    add_css_class: "etle-badge",
                    add_css_class: "etle-badge-muted",
                    #[watch]
                    set_label: &if self.entry.size_bytes > 0 {
                        human_bytes(self.entry.size_bytes)
                    } else {
                        "missing / unreadable".to_string()
                    },
                },
            }
        }
    }

    fn init_model(entry: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { entry }
    }
}

#[derive(Debug)]
pub struct TransferRow {
    view: TransferView,
}

impl DiffRow for TransferRow {
    fn matches_data(&self, data: &Self::Init) -> bool {
        self.view == *data
    }

    fn apply_data(&mut self, data: Self::Init) {
        self.view = data;
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for TransferRow {
    type Init = TransferView;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::ListBoxRow {
            add_css_class: "etle-row",
            set_selectable: false,
            set_activatable: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,

                    gtk::Label {
                        #[watch]
                        set_label: self.view.status.icon(),
                    },

                    gtk::Label {
                        add_css_class: "etle-kind-pill",
                        #[watch]
                        set_label: self.view.kind.display_label(),
                        #[watch]
                        set_class_active: ("etle-kind-seed", self.view.kind == TransferKind::Seed),
                        #[watch]
                        set_class_active: ("etle-kind-download", self.view.kind == TransferKind::Download),
                        #[watch]
                        set_class_active: ("etle-kind-upload", self.view.kind == TransferKind::Upload),
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_width_chars: 1,
                        set_ellipsize: pango::EllipsizeMode::Middle,
                        add_css_class: "etle-row-title",
                        #[watch]
                        set_label: &self.view.label,
                    },

                    gtk::Label {
                        set_valign: gtk::Align::Center,
                        add_css_class: "etle-badge",
                        #[watch]
                        set_label: self.view.status.label(),
                        #[watch]
                        set_class_active: ("etle-badge-ok", self.view.status == TransferStatus::Done),
                        #[watch]
                        set_class_active: ("etle-badge-accent", self.view.status == TransferStatus::Running),
                        #[watch]
                        set_class_active: ("etle-badge-warn", self.view.status == TransferStatus::Queued),
                        #[watch]
                        set_class_active: ("etle-badge-error", self.view.status == TransferStatus::Failed),
                    },
                },

                gtk::ProgressBar {
                    #[watch]
                    set_visible: self.view.progress_visible,
                    #[watch]
                    set_fraction: f64::from(self.view.fraction_milli) / 1000.0,
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_wrap: true,
                    add_css_class: "etle-monospace",
                    add_css_class: "etle-row-subtitle",
                    #[watch]
                    set_label: &self.view.metrics_line,
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_wrap: true,
                    add_css_class: "dim-label",
                    add_css_class: "etle-row-subtitle",
                    #[watch]
                    set_label: &self.view.detail,
                },
            }
        }
    }

    fn init_model(view: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { view }
    }
}
