#[cfg(feature = "gui-relm4")]
#[path = "../gui/mod.rs"]
mod gui;

#[cfg(feature = "gui-relm4")]
fn main() {
    if etle::build_info::args_request_version(std::env::args().skip(1)) {
        etle::build_info::print("etle-gui");
        return;
    }

    gui::run();
}

#[cfg(not(feature = "gui-relm4"))]
fn main() {
    if etle::build_info::args_request_version(std::env::args().skip(1)) {
        etle::build_info::print("etle-gui");
        return;
    }

    eprintln!("etle-gui requires `--features gui-relm4`");
}
