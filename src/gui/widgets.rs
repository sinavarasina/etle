use std::{cell::RefCell, path::PathBuf, rc::Rc};

use etle::ipc::message::IpcShareSummary;
use relm4::{
    ComponentSender,
    gtk::{
        self, Button, CheckButton, Entry, Frame, Label, ListBox, ListBoxRow, Orientation,
        PasswordEntry, ProgressBar, ScrolledWindow, SpinButton, TextBuffer, TextView,
        glib::prelude::IsA, prelude::*,
    },
};

use super::{
    app::EtleGui,
    format::{file_label, human_bytes},
    model::{AppInput, GuiTransfer, TransferStatus},
};

pub struct GuiWidgets {
    pub status_label: Label,

    pub library_list: ListBox,
    pub detail_buffer: TextBuffer,
    pub delete_confirm_box: gtk::Box,
    pub delete_confirm_label: Label,

    pub seed_list: ListBox,
    pub seed_chunk_spin: SpinButton,

    pub parallel_spin: SpinButton,
    pub request_window_spin: SpinButton,
    pub discovery_port_spin: SpinButton,
    pub discovery_timeout_spin: SpinButton,
    pub resume_check: CheckButton,

    pub transfer_list: ListBox,
    pub progress_bar: ProgressBar,
    pub progress_label: Label,
    pub activity_buffer: TextBuffer,

    pub auto_refresh_check: CheckButton,
    pub clear_activity_check: CheckButton,
    pub refresh_interval_spin: SpinButton,
    pub activity_limit_spin: SpinButton,

    pub library_signature: RefCell<String>,
    pub seed_signature: RefCell<String>,
    pub transfer_signature: RefCell<String>,
    pub detail_text: RefCell<String>,
    pub activity_text: RefCell<String>,
}

pub fn section(title: &str, child: &impl IsA<gtk::Widget>) -> Frame {
    let frame = Frame::new(Some(title));
    frame.set_child(Some(child));
    frame.set_margin_top(4);
    frame.set_margin_bottom(4);
    frame.set_margin_start(4);
    frame.set_margin_end(4);
    frame
}

pub fn build_library_page(
    library_list: &ListBox,
    detail_buffer: &TextBuffer,
    delete_confirm_box: &gtk::Box,
    delete_confirm_label: &Label,
    sender: &ComponentSender<EtleGui>,
) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 8);

    let actions = gtk::Box::new(Orientation::Horizontal, 6);
    let copy_button = Button::with_label("Copy share ID");
    let delete_button = Button::with_label("Delete selected share");
    delete_button.add_css_class("destructive-action");
    let clear_finished_button = Button::with_label("Clear completed transfers");
    actions.append(&copy_button);
    actions.append(&delete_button);
    actions.append(&clear_finished_button);
    page.append(&actions);

    delete_confirm_box.set_margin_top(2);
    delete_confirm_box.set_margin_bottom(2);
    delete_confirm_box.set_margin_start(4);
    delete_confirm_box.set_margin_end(4);
    delete_confirm_box.set_visible(false);
    delete_confirm_label.set_xalign(0.0);
    delete_confirm_label.set_wrap(true);
    delete_confirm_label.set_hexpand(true);
    let cancel_delete_button = Button::with_label("Cancel");
    let confirm_delete_button = Button::with_label("Delete permanently");
    confirm_delete_button.add_css_class("destructive-action");
    delete_confirm_box.append(delete_confirm_label);
    delete_confirm_box.append(&cancel_delete_button);
    delete_confirm_box.append(&confirm_delete_button);
    page.append(delete_confirm_box);

    let body = gtk::Box::new(Orientation::Vertical, 8);
    page.append(&body);

    let list_scroll = ScrolledWindow::builder()
        .child(library_list)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(240)
        .build();
    body.append(&section("Shares", &list_scroll));

    let detail_view = TextView::builder()
        .buffer(detail_buffer)
        .editable(false)
        .monospace(true)
        .vexpand(true)
        .hexpand(true)
        .build();
    let detail_scroll = ScrolledWindow::builder()
        .child(&detail_view)
        .hexpand(true)
        .vexpand(true)
        .min_content_height(180)
        .build();
    body.append(&section("Details", &detail_scroll));

    {
        let sender = sender.clone();
        copy_button.connect_clicked(move |_| sender.input(AppInput::CopySelectedShareId));
    }
    {
        let sender = sender.clone();
        delete_button.connect_clicked(move |_| sender.input(AppInput::DeleteSelectedShare));
    }
    {
        let sender = sender.clone();
        cancel_delete_button.connect_clicked(move |_| sender.input(AppInput::CancelDeleteShare));
    }
    {
        let sender = sender.clone();
        confirm_delete_button
            .connect_clicked(move |_| sender.input(AppInput::ConfirmDeleteSelectedShare));
    }
    {
        let sender = sender.clone();
        clear_finished_button
            .connect_clicked(move |_| sender.input(AppInput::ClearFinishedTransfers));
    }

    page
}

pub fn build_seed_page(
    seed_entry: &Entry,
    seed_list: &ListBox,
    seed_chunk_spin: &SpinButton,
    sender: &ComponentSender<EtleGui>,
) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 8);

    let input_box = gtk::Box::new(Orientation::Vertical, 8);
    let file_row = gtk::Box::new(Orientation::Horizontal, 6);
    file_row.append(seed_entry);
    let add_button = Button::with_label("Add");
    let browse_button = Button::with_label("Browse");
    file_row.append(&add_button);
    file_row.append(&browse_button);
    input_box.append(&file_row);

    let options = gtk::Box::new(Orientation::Vertical, 6);
    let chunk_row = gtk::Box::new(Orientation::Horizontal, 6);
    let chunk_label = Label::new(Some("Chunk size (bytes)"));
    chunk_label.set_width_chars(18);
    chunk_label.set_xalign(0.0);
    chunk_row.append(&chunk_label);
    chunk_row.append(seed_chunk_spin);
    options.append(&chunk_row);

    let action_row = gtk::Box::new(Orientation::Horizontal, 6);
    let seed_selected_button = Button::with_label("Seed selected file");
    let remove_button = Button::with_label("Remove");
    let clear_button = Button::with_label("Clear list");
    action_row.append(&seed_selected_button);
    action_row.append(&remove_button);
    action_row.append(&clear_button);
    options.append(&action_row);
    input_box.append(&options);
    page.append(&section("Seed queue", &input_box));

    let list_scroll = ScrolledWindow::builder()
        .child(seed_list)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(240)
        .build();
    page.append(&section("Files", &list_scroll));

    {
        let sender = sender.clone();
        let seed_entry = seed_entry.clone();
        add_button.connect_clicked(move |_| {
            sender.input(AppInput::AddSeedPathText(seed_entry.text().to_string()));
            seed_entry.set_text("");
        });
    }
    {
        let sender = sender.clone();
        let seed_entry_signal = seed_entry.clone();
        let seed_entry_value = seed_entry.clone();
        seed_entry_signal.connect_activate(move |_| {
            sender.input(AppInput::AddSeedPathText(
                seed_entry_value.text().to_string(),
            ));
            seed_entry_value.set_text("");
        });
    }
    {
        let sender = sender.clone();
        browse_button.connect_clicked(move |_| sender.input(AppInput::BrowseSeedFiles));
    }
    {
        let sender = sender.clone();
        seed_selected_button.connect_clicked(move |_| sender.input(AppInput::StartSeedSelected));
    }
    {
        let sender = sender.clone();
        remove_button.connect_clicked(move |_| sender.input(AppInput::RemoveSelectedSeedFile));
    }
    {
        let sender = sender.clone();
        clear_button.connect_clicked(move |_| sender.input(AppInput::ClearSeedFiles));
    }

    page
}

#[allow(clippy::too_many_arguments)]
pub fn build_download_page(
    share_id_entry: &Entry,
    peers_entry: &Entry,
    output_entry: &Entry,
    parallel_spin: &SpinButton,
    request_window_spin: &SpinButton,
    discovery_port_spin: &SpinButton,
    discovery_timeout_spin: &SpinButton,
    discovery_multicast_entry: &Entry,
    resume_check: &CheckButton,
    psk_entry: &PasswordEntry,
    sender: &ComponentSender<EtleGui>,
) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 8);

    let form = gtk::Box::new(Orientation::Vertical, 8);
    form.append(&labeled_row("Share ID", share_id_entry));
    form.append(&labeled_row("Peers", peers_entry));

    let output_row = gtk::Box::new(Orientation::Horizontal, 6);
    let output_label = Label::new(Some("Output path"));
    output_label.set_width_chars(12);
    output_label.set_xalign(0.0);
    output_row.append(&output_label);
    output_row.append(output_entry);
    let output_button = Button::with_label("Browse");
    output_row.append(&output_button);
    form.append(&output_row);

    let advanced = gtk::Box::new(Orientation::Vertical, 6);
    advanced.append(&labeled_row("Parallel workers", parallel_spin));
    advanced.append(&labeled_row("Request window", request_window_spin));
    advanced.append(&labeled_row("Discovery port", discovery_port_spin));
    advanced.append(&labeled_row("Timeout (ms)", discovery_timeout_spin));
    advanced.append(&labeled_row("Multicast address", discovery_multicast_entry));
    form.append(&advanced);

    form.append(&labeled_row("Auth PSK", psk_entry));
    form.append(resume_check);

    let start_button = Button::with_label("Start download");
    form.append(&start_button);
    page.append(&section("Download request", &form));

    {
        let output_entry = output_entry.clone();
        output_button.connect_clicked(move |_| {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save download as")
                .save_file()
            {
                output_entry.set_text(&path.display().to_string());
            }
        });
    }

    let send_download: Rc<dyn Fn()> = {
        let sender = sender.clone();
        let share_id_entry = share_id_entry.clone();
        let peers_entry = peers_entry.clone();
        let output_entry = output_entry.clone();
        let psk_entry = psk_entry.clone();
        let discovery_multicast_entry = discovery_multicast_entry.clone();
        Rc::new(move || {
            sender.input(AppInput::StartDownloadFromForm {
                share_id: share_id_entry.text().to_string(),
                peers: peers_entry.text().to_string(),
                output: output_entry.text().to_string(),
                auth_psk: psk_entry.text().to_string(),
                discovery_multicast: discovery_multicast_entry.text().to_string(),
            });
        })
    };
    {
        let send_download = send_download.clone();
        start_button.connect_clicked(move |_| send_download());
    }
    {
        let send_download = send_download.clone();
        share_id_entry.connect_activate(move |_| send_download());
    }
    {
        let send_download = send_download.clone();
        peers_entry.connect_activate(move |_| send_download());
    }
    {
        let send_download = send_download.clone();
        output_entry.connect_activate(move |_| send_download());
    }
    {
        let send_download = send_download;
        discovery_multicast_entry.connect_activate(move |_| send_download());
    }

    page
}

pub fn build_activity_page(
    transfer_list: &ListBox,
    progress_bar: &ProgressBar,
    progress_label: &Label,
    activity_buffer: &TextBuffer,
    sender: &ComponentSender<EtleGui>,
) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 8);
    page.set_hexpand(true);

    let progress_box = gtk::Box::new(Orientation::Vertical, 4);
    progress_box.set_hexpand(true);
    progress_bar.set_hexpand(true);
    progress_label.set_hexpand(true);
    progress_label.set_width_chars(1);
    progress_label.set_max_width_chars(96);
    progress_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    progress_box.append(progress_bar);
    progress_box.append(progress_label);
    page.append(&section("Progress", &progress_box));

    transfer_list.set_hexpand(true);
    transfer_list.set_vexpand(true);

    let transfer_scroll = ScrolledWindow::builder()
        .child(transfer_list)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(220)
        .build();
    page.append(&section("Transfers", &transfer_scroll));

    let log_box = gtk::Box::new(Orientation::Vertical, 6);
    let activity_header = gtk::Box::new(Orientation::Horizontal, 6);
    let spacer = Label::new(None);
    spacer.set_hexpand(true);
    let clear_finished_button = Button::with_label("Clear completed");
    let clear_button = Button::with_label("Clear log");
    activity_header.append(&spacer);
    activity_header.append(&clear_finished_button);
    activity_header.append(&clear_button);
    log_box.append(&activity_header);

    let activity_view = TextView::builder()
        .buffer(activity_buffer)
        .editable(false)
        .monospace(true)
        .vexpand(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .hexpand(true)
        .build();
    let scroll = ScrolledWindow::builder()
        .child(&activity_view)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(180)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    log_box.append(&scroll);
    page.append(&section("Activity log", &log_box));

    {
        let sender = sender.clone();
        clear_button.connect_clicked(move |_| sender.input(AppInput::ClearLog));
    }
    {
        let sender = sender.clone();
        clear_finished_button
            .connect_clicked(move |_| sender.input(AppInput::ClearFinishedTransfers));
    }

    page
}

#[allow(clippy::too_many_arguments)]
pub fn build_settings_page(
    socket_entry: &Entry,
    psk_entry: &PasswordEntry,
    auto_refresh_check: &CheckButton,
    clear_activity_check: &CheckButton,
    refresh_interval_spin: &SpinButton,
    activity_limit_spin: &SpinButton,
    sender: &ComponentSender<EtleGui>,
) -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 8);

    let box_ = gtk::Box::new(Orientation::Vertical, 8);
    let socket_row = gtk::Box::new(Orientation::Horizontal, 6);
    let socket_label = Label::new(Some("IPC endpoint"));
    socket_label.set_width_chars(12);
    socket_label.set_xalign(0.0);
    socket_row.append(&socket_label);
    socket_row.append(socket_entry);
    let apply_socket_button = Button::with_label("Apply");
    socket_row.append(&apply_socket_button);
    box_.append(&socket_row);

    box_.append(&labeled_row("Default PSK", psk_entry));

    let limits = gtk::Box::new(Orientation::Vertical, 6);
    limits.append(&labeled_row("Refresh interval (s)", refresh_interval_spin));
    limits.append(&labeled_row("Activity lines", activity_limit_spin));
    box_.append(&limits);

    box_.append(auto_refresh_check);
    box_.append(clear_activity_check);

    let actions = gtk::Box::new(Orientation::Vertical, 6);
    let action_row_a = gtk::Box::new(Orientation::Horizontal, 6);
    let action_row_b = gtk::Box::new(Orientation::Horizontal, 6);
    let ping_button = Button::with_label("Ping");
    let refresh_button = Button::with_label("Refresh now");
    let watch_button = Button::with_label("Start event watch");
    let apply_settings_button = Button::with_label("Apply settings");
    action_row_a.append(&ping_button);
    action_row_a.append(&refresh_button);
    action_row_b.append(&watch_button);
    action_row_b.append(&apply_settings_button);
    actions.append(&action_row_a);
    actions.append(&action_row_b);
    box_.append(&actions);
    page.append(&section("Daemon and UI", &box_));

    let apply = {
        let sender = sender.clone();
        let socket_entry = socket_entry.clone();
        let psk_entry = psk_entry.clone();
        move || {
            sender.input(AppInput::ApplySettings {
                socket_path: socket_entry.text().to_string(),
                auth_psk: psk_entry.text().to_string(),
            });
        }
    };
    {
        let apply = apply.clone();
        apply_socket_button.connect_clicked(move |_| apply());
    }
    {
        let apply = apply.clone();
        apply_settings_button.connect_clicked(move |_| apply());
    }
    {
        let apply = apply.clone();
        socket_entry.connect_activate(move |_| apply());
    }
    {
        let apply = apply;
        psk_entry.connect_activate(move |_| apply());
    }
    {
        let sender = sender.clone();
        ping_button.connect_clicked(move |_| sender.input(AppInput::Connect));
    }
    {
        let sender = sender.clone();
        refresh_button.connect_clicked(move |_| sender.input(AppInput::Refresh));
    }
    {
        let sender = sender.clone();
        watch_button.connect_clicked(move |_| sender.input(AppInput::StartWatch));
    }

    page
}

pub fn labeled_row<W>(label: &str, widget: &W) -> gtk::Box
where
    W: IsA<gtk::Widget>,
{
    let row = gtk::Box::new(Orientation::Horizontal, 6);
    let label = Label::new(Some(label));
    label.set_width_chars(18);
    label.set_xalign(0.0);
    row.append(&label);
    widget.set_hexpand(true);
    row.append(widget);
    row
}

pub fn refill_seed_list(list: &ListBox, files: &[PathBuf]) {
    clear_list(list);
    if files.is_empty() {
        let row = ListBoxRow::new();
        let label =
            muted_label("No files queued. Add files with Browse or paste a path and press Enter.");
        row.set_child(Some(&label));
        list.append(&row);
        return;
    }

    for path in files {
        let row = ListBoxRow::new();
        let item = card_box();
        let title = Label::new(Some(&file_label(path)));
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        item.append(&title);

        let path_label = muted_label(&path.display().to_string());
        path_label.set_selectable(true);
        item.append(&path_label);
        row.set_child(Some(&item));
        list.append(&row);
    }
}

pub fn refill_library_list(list: &ListBox, shares: &[IpcShareSummary]) {
    clear_list(list);

    if shares.is_empty() {
        let row = ListBoxRow::new();
        row.set_child(Some(&muted_label("No shares in this daemon library.")));
        list.append(&row);
        return;
    }

    for share in shares {
        let row = ListBoxRow::new();
        let item = card_box();

        let title_row = gtk::Box::new(Orientation::Horizontal, 8);
        let title = Label::new(Some(&share.name));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_width_chars(1);
        title.set_max_width_chars(48);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_row.append(&title);
        let mode = share.mode.as_deref().unwrap_or("unknown");
        let badge = Label::new(Some(mode));
        badge.add_css_class("dim-label");
        title_row.append(&badge);
        item.append(&title_row);

        let secret = if share.has_secret {
            "key: yes"
        } else {
            "key: no"
        };
        let percent = share_percent(share);
        let meta = muted_label(&format!(
            "{}/{} chunks · {percent:.1}% · {secret}",
            share.completed_chunks, share.total_chunks
        ));
        item.append(&meta);

        let bar = ProgressBar::new();
        bar.set_fraction(share_fraction(share));
        item.append(&bar);

        let id = muted_label(&share.share_id.to_string());
        id.set_selectable(true);
        id.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        item.append(&id);

        row.set_child(Some(&item));
        list.append(&row);
    }
}

pub fn refill_transfer_list(list: &ListBox, transfers: &[GuiTransfer]) {
    clear_list(list);

    if transfers.is_empty() {
        let row = ListBoxRow::new();
        row.set_child(Some(&muted_label("No transfer yet.")));
        list.append(&row);
        return;
    }

    for transfer in transfers {
        let row = ListBoxRow::new();
        let item = card_box();

        let title_row = gtk::Box::new(Orientation::Horizontal, 8);
        let title = Label::new(Some(&transfer.label));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_width_chars(1);
        title.set_max_width_chars(48);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_row.append(&title);
        let badge_text = if transfer.status == TransferStatus::Running {
            format!(
                "{} {}",
                transfer.status.icon(),
                short_phase(&transfer.detail)
            )
        } else {
            format!("{} {}", transfer.status.icon(), transfer.status.label())
        };
        let badge = Label::new(Some(&badge_text));
        badge.add_css_class("dim-label");
        title_row.append(&badge);
        item.append(&title_row);

        let meta = muted_label(&transfer.compact_line());
        item.append(&meta);

        let bar = ProgressBar::new();
        bar.set_fraction(transfer.fraction());
        if transfer.status == TransferStatus::Running
            && transfer.total_bytes == 0
            && transfer.total_chunks == 0
        {
            bar.pulse();
        }
        item.append(&bar);

        let detail = muted_label(&transfer.detail);
        item.append(&detail);

        row.set_child(Some(&item));
        list.append(&row);
    }
}

pub fn seed_signature(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn library_signature(shares: &[IpcShareSummary]) -> String {
    shares
        .iter()
        .map(|share| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                share.share_id,
                share.name,
                share.mode.as_deref().unwrap_or(""),
                share.completed_chunks,
                share.total_chunks,
                share.has_secret
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn transfer_signature(transfers: &[GuiTransfer]) -> String {
    transfers
        .iter()
        .map(|transfer| {
            format!(
                "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                transfer.id,
                transfer.kind.label(),
                transfer.label,
                transfer.status.label(),
                transfer
                    .share_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                transfer.completed_chunks,
                transfer.total_chunks,
                transfer.bytes_done,
                transfer.total_bytes,
                transfer.bytes_per_second,
                transfer.detail
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn set_spin_if_changed(spin: &SpinButton, value: f64) {
    if (spin.value() - value).abs() > f64::EPSILON {
        spin.set_value(value);
    }
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn card_box() -> gtk::Box {
    let item = gtk::Box::new(Orientation::Vertical, 5);
    item.set_margin_top(8);
    item.set_margin_bottom(8);
    item.set_margin_start(10);
    item.set_margin_end(10);
    item
}

fn muted_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    label.set_width_chars(1);
    label.set_max_width_chars(96);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn short_phase(detail: &str) -> &str {
    if detail.contains("decrypting") {
        "decrypt"
    } else if detail.contains("receiving") {
        "receive"
    } else if detail.contains("encrypting") {
        "encrypt"
    } else if detail.contains("uploading") {
        "upload"
    } else {
        "running"
    }
}

fn share_fraction(share: &IpcShareSummary) -> f64 {
    if share.total_chunks == 0 {
        if matches!(share.mode.as_deref(), Some("seeding" | "completed")) {
            1.0
        } else {
            0.0
        }
    } else {
        (share.completed_chunks as f64 / share.total_chunks as f64).clamp(0.0, 1.0)
    }
}

fn share_percent(share: &IpcShareSummary) -> f64 {
    share_fraction(share) * 100.0
}

#[allow(dead_code)]
fn _debug_bytes(bytes: u64) -> String {
    human_bytes(bytes)
}
