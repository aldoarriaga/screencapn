#![windows_subsystem = "windows"]

mod native;
mod overlay;
mod tray;
mod util;

use native::NativeApp;
use windows::core::Result;

fn main() -> Result<()> {
    NativeApp::new()?.run()
}
