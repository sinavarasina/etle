#[cfg(target_os = "windows")]
use relm4::gtk::{self, gdk};

pub fn install_platform_style() {
    #[cfg(target_os = "windows")]
    install_windows_style();
}

#[cfg(target_os = "windows")]
fn install_windows_style() {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_data(WINDOWS_NATIVE_CSS);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

#[cfg(target_os = "windows")]
const WINDOWS_NATIVE_CSS: &str = include_str!("style/windows.css");
