use std::path::PathBuf;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, prelude::*},
};

use crate::gui::{
    components::{restore_listbox_selection, rows::SeedFileRow, sync_factory_rows},
    format::human_bytes,
    model::SeedFileEntry,
};

#[derive(Debug)]
pub enum SeedInput {
    Sync {
        entries: Vec<SeedFileEntry>,
        selected_index: Option<usize>,
        connected: bool,
        chunk_size: usize,
    },
    AddFromEntry,
    BrowseFiles,
    RowSelected(Option<usize>),
    PresetChunkSize(usize),
    ChunkChanged(usize),
    SeedSelectedClicked,
    SeedAllClicked,
    RemoveClicked,
    ClearClicked,
}

#[derive(Debug)]
pub enum SeedOutput {
    AddFiles(Vec<PathBuf>),
    SelectFile(usize),
    SetChunkSize(usize),
    SeedSelected,
    SeedAll,
    RemoveSelected,
    ClearQueue,
}

pub struct SeedPage {
    factory: FactoryVecDeque<SeedFileRow>,
    entries: Vec<SeedFileEntry>,
    selected_index: Option<usize>,
    connected: bool,
    chunk_size: usize,
    path_buf: gtk::EntryBuffer,
    chunk_adj: gtk::Adjustment,
}

#[relm4::component(pub)]
impl SimpleComponent for SeedPage {
    type Init = usize;
    type Input = SeedInput;
    type Output = SeedOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 10,

            gtk::Frame {
                set_label: Some("Seed Queue"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,

                        gtk::Entry {
                            set_buffer: &model.path_buf,
                            set_hexpand: true,
                            set_placeholder_text: Some("Enter file path and press Enter, or click Browse Files…"),
                            connect_activate => SeedInput::AddFromEntry,
                        },

                        gtk::Button {
                            set_label: "Add Path",
                            connect_clicked => SeedInput::AddFromEntry,
                        },

                        gtk::Button {
                            set_label: "Browse Files…",
                            connect_clicked => SeedInput::BrowseFiles,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Chunk size (bytes)",
                            set_width_chars: 18,
                            set_xalign: 0.0,
                        },

                        gtk::SpinButton {
                            set_adjustment: &model.chunk_adj,
                            set_digits: 0,
                            set_numeric: true,
                            connect_value_changed[sender] => move |spin| {
                                sender.input(SeedInput::ChunkChanged(spin.value_as_int().max(1) as usize));
                            },
                        },

                        gtk::Label {
                            set_xalign: 0.0,
                            set_hexpand: true,
                            add_css_class: "etle-hint",
                            #[watch]
                            set_label: &format!("{} per chunk", human_bytes(model.chunk_size as u64)),
                        },

                        gtk::Button {
                            set_label: "256 KiB",
                            connect_clicked => SeedInput::PresetChunkSize(256 * 1024),
                        },

                        gtk::Button {
                            set_label: "1 MiB",
                            connect_clicked => SeedInput::PresetChunkSize(1024 * 1024),
                        },

                        gtk::Button {
                            set_label: "4 MiB",
                            connect_clicked => SeedInput::PresetChunkSize(4 * 1024 * 1024),
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,

                        gtk::Button {
                            set_label: "Seed Selected",
                            add_css_class: "suggested-action",
                            #[watch]
                            set_sensitive: model.connected && !model.entries.is_empty(),
                            connect_clicked => SeedInput::SeedSelectedClicked,
                        },

                        gtk::Button {
                            set_label: "Seed All",
                            #[watch]
                            set_sensitive: model.connected && !model.entries.is_empty(),
                            connect_clicked => SeedInput::SeedAllClicked,
                        },

                        gtk::Button {
                            set_label: "Remove",
                            #[watch]
                            set_sensitive: model.selected_index.is_some(),
                            connect_clicked => SeedInput::RemoveClicked,
                        },

                        gtk::Button {
                            set_label: "Clear Queue",
                            #[watch]
                            set_sensitive: !model.entries.is_empty(),
                            connect_clicked => SeedInput::ClearClicked,
                        },
                    },
                },
            },

            gtk::Frame {
                set_label: Some("Queued Files"),
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
                            set_label: "Queue is empty. Add files above to seed them into the daemon library.",
                            set_xalign: 0.0,
                            add_css_class: "dim-label",
                            set_margin_top: 12,
                            set_margin_bottom: 12,
                            set_margin_start: 12,
                            set_margin_end: 12,
                            #[watch]
                            set_visible: model.entries.is_empty(),
                        },

                        #[local_ref]
                        seed_list -> gtk::ListBox {
                            set_vexpand: true,
                            set_selection_mode: gtk::SelectionMode::Single,
                            #[watch]
                            set_visible: !model.entries.is_empty(),
                            connect_row_selected[sender] => move |_, row| {
                                let idx = row.and_then(|r| usize::try_from(r.index()).ok());
                                sender.input(SeedInput::RowSelected(idx));
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(
        chunk_size: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let initial_chunk = chunk_size.max(1);
        let factory: FactoryVecDeque<SeedFileRow> = FactoryVecDeque::builder()
            .launch(gtk::ListBox::new())
            .detach();
        let seed_list = factory.widget().clone();

        let model = Self {
            factory,
            entries: Vec::new(),
            selected_index: None,
            connected: false,
            chunk_size: initial_chunk,
            path_buf: gtk::EntryBuffer::default(),
            chunk_adj: gtk::Adjustment::new(
                initial_chunk as f64,
                1.0,
                67_108_864.0,
                1024.0,
                1024.0,
                0.0,
            ),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SeedInput::Sync {
                entries,
                selected_index,
                connected,
                chunk_size,
            } => {
                self.entries = entries.clone();
                self.selected_index = selected_index;
                self.connected = connected;
                if self.chunk_size != chunk_size {
                    self.chunk_size = chunk_size;
                    self.chunk_adj.set_value(chunk_size as f64);
                }
                sync_factory_rows(&mut self.factory, entries);
            }
            SeedInput::AddFromEntry => {
                let raw: String = self.path_buf.text().into();
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    let _ = sender.output(SeedOutput::AddFiles(vec![PathBuf::from(trimmed)]));
                    self.path_buf.set_text("");
                }
            }
            SeedInput::BrowseFiles => {
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("Add seed files")
                    .pick_files()
                {
                    let _ = sender.output(SeedOutput::AddFiles(paths));
                }
            }
            SeedInput::RowSelected(Some(idx)) => {
                if idx < self.entries.len() && self.selected_index != Some(idx) {
                    self.selected_index = Some(idx);
                    let _ = sender.output(SeedOutput::SelectFile(idx));
                }
            }
            SeedInput::RowSelected(None) => {}
            SeedInput::PresetChunkSize(bytes) => {
                self.chunk_size = bytes.max(1);
                self.chunk_adj.set_value(self.chunk_size as f64);
                let _ = sender.output(SeedOutput::SetChunkSize(self.chunk_size));
            }
            SeedInput::ChunkChanged(bytes) => {
                let next = bytes.max(1);
                if self.chunk_size != next {
                    self.chunk_size = next;
                    let _ = sender.output(SeedOutput::SetChunkSize(next));
                }
            }
            SeedInput::SeedSelectedClicked => {
                let _ = sender.output(SeedOutput::SeedSelected);
            }
            SeedInput::SeedAllClicked => {
                let _ = sender.output(SeedOutput::SeedAll);
            }
            SeedInput::RemoveClicked => {
                let _ = sender.output(SeedOutput::RemoveSelected);
            }
            SeedInput::ClearClicked => {
                let _ = sender.output(SeedOutput::ClearQueue);
            }
        }
    }

    fn post_view() {
        restore_listbox_selection(&widgets.seed_list, model.selected_index);
    }
}
