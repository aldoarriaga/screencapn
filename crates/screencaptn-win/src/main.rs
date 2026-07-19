#![windows_subsystem = "windows"]

mod app_icon;
mod diagnostics;
mod hotkey;
mod native;
mod overlay;
mod settings;
mod shortcut_window;
mod single_instance;
mod startup;
mod theme;
mod tray;
mod update_window;
mod updates;
mod util;
mod web_ui;

use native::NativeApp;
use windows::core::Result;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn main() -> Result<()> {
    diagnostics::install();
    diagnostics::log_breadcrumb("app-main-enter");
    let Some(_instance_guard) = single_instance::SingleInstanceGuard::acquire()? else {
        diagnostics::log_breadcrumb("duplicate-instance-exit");
        return Ok(());
    };
    let _com_apartment = ComApartment::initialize()?;
    NativeApp::new()?.run()
}
