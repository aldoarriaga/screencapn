#![windows_subsystem = "windows"]

mod native;
mod overlay;
mod theme;
mod tray;
mod util;
mod web_ui;

use native::NativeApp;
use windows::core::Result;

fn main() -> Result<()> {
    NativeApp::new()?.run()
}
