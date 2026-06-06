#![windows_subsystem = "windows"]

mod diagnostics;
mod native;
mod overlay;
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
