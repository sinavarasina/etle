pub mod app;
pub mod format;
pub mod ipc;
pub mod model;
pub mod progress;
pub mod widgets;

use relm4::RelmApp;

use self::{app::EtleGui, model::GuiInit};

const APP_ID: &str = "dev.etle.gui";

pub fn run() {
    let app = RelmApp::new(APP_ID);
    app.run::<EtleGui>(GuiInit::default());
}


