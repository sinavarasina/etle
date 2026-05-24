#[cfg(feature = "gui-relm4")]
#[path = "../gui/mod.rs"]
mod gui;

#[cfg(feature = "gui-relm4")]
fn main() {
    gui::run();
}

#[cfg(not(feature = "gui-relm4"))]
fn main() {
    eprintln!("etle-gui requires `--features gui-relm4`");
}


