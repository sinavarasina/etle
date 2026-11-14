use relm4::gtk::{self, gdk};

pub fn configure_platform_environment() {
    #[cfg(target_os = "windows")]
    configure_windows_environment();
}

pub fn install_platform_style() {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    install_css(
        &display,
        COMMON_CSS,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    #[cfg(target_os = "windows")]
    install_css(
        &display,
        WINDOWS_NATIVE_CSS,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
}

fn install_css(display: &gdk::Display, css: &str, priority: u32) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    gtk::style_context_add_provider_for_display(display, &provider, priority);
}

#[cfg(target_os = "windows")]
fn configure_windows_environment() {
    if std::env::var_os("GTK_CSD").is_none() {
        unsafe {
            std::env::set_var("GTK_CSD", "0");
        }
    }
}

const COMMON_CSS: &str = include_str!("style/common.css");

#[cfg(target_os = "windows")]
const WINDOWS_NATIVE_CSS: &str = include_str!("style/windows.css");
