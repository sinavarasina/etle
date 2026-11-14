use std::cell::Cell;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, glib, prelude::*},
};

use crate::gui::{
    components::{rows::TransferRow, sync_factory_rows},
    model::TransferView,
};

#[derive(Debug)]
pub enum ActivityInput {
    Sync {
        transfers: Vec<TransferView>,
        log_text: String,
    },
    ClearFinishedClicked,
    CopyLogClicked,
    ClearLogClicked,
}

#[derive(Debug)]
pub enum ActivityOutput {
    ClearFinishedTransfers,
    CopyActivityLog,
    ClearLog,
}

pub struct ActivityPage {
    factory: FactoryVecDeque<TransferRow>,
    has_transfers: bool,
    has_finished: bool,
    has_log: bool,
    log_buffer: gtk::TextBuffer,
    last_log_text: String,
    pending_scroll_bottom: Cell<bool>,
}

#[relm4::component(pub)]
impl SimpleComponent for ActivityPage {
    type Init = ();
    type Input = ActivityInput;
    type Output = ActivityOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 10,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,

                gtk::Button {
                    set_label: "Clear Finished",
                    #[watch]
                    set_sensitive: model.has_finished,
                    connect_clicked => ActivityInput::ClearFinishedClicked,
                },

                gtk::Button {
                    set_label: "Copy Log",
                    #[watch]
                    set_sensitive: model.has_log,
                    connect_clicked => ActivityInput::CopyLogClicked,
                },

                gtk::Button {
                    set_label: "Clear Log",
                    #[watch]
                    set_sensitive: model.has_log,
                    connect_clicked => ActivityInput::ClearLogClicked,
                },
            },

            gtk::Frame {
                set_label: Some("Active & Recent Transfers"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                set_child = &gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_min_content_height: 220,
                    set_propagate_natural_height: true,
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Label {
                            set_label: "No active or recent transfers. Queued, running, and completed Seed / Download / Upload jobs appear here.",
                            set_xalign: 0.0,
                            set_wrap: true,
                            add_css_class: "dim-label",
                            set_margin_top: 12,
                            set_margin_bottom: 12,
                            set_margin_start: 12,
                            set_margin_end: 12,
                            #[watch]
                            set_visible: !model.has_transfers,
                        },

                        #[local_ref]
                        transfer_list -> gtk::ListBox {
                            set_vexpand: true,
                            set_selection_mode: gtk::SelectionMode::None,
                            #[watch]
                            set_visible: model.has_transfers,
                        },
                    },
                },
            },

            gtk::Frame {
                set_label: Some("Daemon Activity Log"),
                add_css_class: "etle-section",

                #[wrap(Some)]
                #[name = "log_scroll"]
                set_child = &gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_min_content_height: 200,
                    set_propagate_natural_height: true,
                    set_policy: (gtk::PolicyType::Automatic, gtk::PolicyType::Automatic),

                    #[wrap(Some)]
                    set_child = &gtk::TextView {
                        set_editable: false,
                        set_cursor_visible: false,
                        set_monospace: true,
                        set_wrap_mode: gtk::WrapMode::WordChar,
                        set_buffer: Some(&model.log_buffer),
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
        let factory: FactoryVecDeque<TransferRow> = FactoryVecDeque::builder()
            .launch(gtk::ListBox::new())
            .detach();
        let transfer_list = factory.widget().clone();

        let model = Self {
            factory,
            has_transfers: false,
            has_finished: false,
            has_log: false,
            log_buffer: gtk::TextBuffer::default(),
            last_log_text: String::new(),
            pending_scroll_bottom: Cell::new(false),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            ActivityInput::Sync {
                transfers,
                log_text,
            } => {
                self.has_transfers = !transfers.is_empty();
                self.has_finished = transfers.iter().any(|t| t.status.is_finished());
                sync_factory_rows(&mut self.factory, transfers);

                self.has_log = !log_text.is_empty();
                if self.last_log_text != log_text {
                    self.log_buffer.set_text(&log_text);
                    self.last_log_text = log_text;
                    self.pending_scroll_bottom.set(true);
                }
            }
            ActivityInput::ClearFinishedClicked => {
                let _ = sender.output(ActivityOutput::ClearFinishedTransfers);
            }
            ActivityInput::CopyLogClicked => {
                let _ = sender.output(ActivityOutput::CopyActivityLog);
            }
            ActivityInput::ClearLogClicked => {
                let _ = sender.output(ActivityOutput::ClearLog);
            }
        }
    }

    fn post_view() {
        if model.pending_scroll_bottom.replace(false) {
            let scroll = widgets.log_scroll.clone();
            glib::idle_add_local_once(move || {
                let adj = scroll.vadjustment();
                adj.set_value((adj.upper() - adj.page_size()).max(0.0));
            });
        }
    }
}
