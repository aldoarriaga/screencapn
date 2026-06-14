#![windows_subsystem = "windows"]

mod app_icon;
mod diagnostics;
mod hotkey;
mod native;
mod overlay;
mod settings;
mod shortcut_window;
mod theme;
mod tray;
mod util;
mod web_ui;

use native::NativeApp;
use windows::core::Result;

fn main() -> Result<()> {
    diagnostics::install();
    diagnostics::log_breadcrumb("app-main-enter");
    NativeApp::new()?.run()
}
