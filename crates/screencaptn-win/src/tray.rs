use crate::overlay::AppTheme;
use crate::settings::AppSettings;
use windows::core::w;
use windows::core::Result;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
};

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const TRAY_SET_SHORTCUT_COMMAND: usize = 9000;
pub const TRAY_TOGGLE_AUTO_SAVE_COMMAND: usize = 9001;
pub const TRAY_SET_AUTO_SAVE_FOLDER_COMMAND: usize = 9002;
pub const TRAY_TOGGLE_THEME_COMMAND: usize = 9003;
pub const TRAY_EXIT_COMMAND: usize = 9004;
const TRAY_UID: u32 = 1;

pub unsafe fn add_tray_icon(hwnd: HWND, settings: &AppSettings) -> Result<()> {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: crate::app_icon::load_app_icon(32).unwrap_or(LoadIconW(None, IDI_APPLICATION)?),
        ..Default::default()
    };

    set_tip(&mut data, settings);
    Shell_NotifyIconW(NIM_ADD, &data).ok()
}

pub unsafe fn update_tray_icon(hwnd: HWND, settings: &AppSettings) {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    set_tip(&mut data, settings);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
}

fn set_tip(data: &mut NOTIFYICONDATAW, settings: &AppSettings) {
    let tip = format!("Screen Cap'n - {}", settings.hotkey.display_label());
    let tip: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
    for (index, value) in tip.into_iter().enumerate().take(data.szTip.len()) {
        data.szTip[index] = value;
    }
}

pub unsafe fn remove_tray_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
}

pub unsafe fn show_tray_menu(hwnd: HWND, theme: AppTheme, settings: &AppSettings) -> Option<usize> {
    let menu = CreatePopupMenu().ok()?;
    let shortcut_label = format!("Quick access shortcut: {}", settings.hotkey.display_label());
    let shortcut_label = wide_null(&shortcut_label);
    AppendMenuW(
        menu,
        MF_STRING,
        TRAY_SET_SHORTCUT_COMMAND,
        windows::core::PCWSTR(shortcut_label.as_ptr()),
    )
    .ok()?;

    let auto_save_label = if settings.auto_save.enabled {
        "☑ Auto-save screenshots is ON - click to turn off"
    } else {
        "☐ Auto-save screenshots is OFF - click to turn on"
    };
    let auto_save_label = wide_null(auto_save_label);
    AppendMenuW(
        menu,
        MF_STRING,
        TRAY_TOGGLE_AUTO_SAVE_COMMAND,
        windows::core::PCWSTR(auto_save_label.as_ptr()),
    )
    .ok()?;
    AppendMenuW(
        menu,
        MF_STRING,
        TRAY_SET_AUTO_SAVE_FOLDER_COMMAND,
        w!("Choose auto-save folder..."),
    )
    .ok()?;
    AppendMenuW(menu, MF_SEPARATOR, 0, None).ok()?;

    let toggle_label = match theme {
        AppTheme::Light => w!("Switch to dark mode"),
        AppTheme::Dark => w!("Switch to light mode"),
    };
    AppendMenuW(menu, MF_STRING, TRAY_TOGGLE_THEME_COMMAND, toggle_label).ok()?;
    AppendMenuW(menu, MF_STRING, TRAY_EXIT_COMMAND, w!("Exit Screen Captn")).ok()?;

    let mut cursor = POINT::default();
    GetCursorPos(&mut cursor).ok()?;
    let _ = SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        cursor.x,
        cursor.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);

    (command.0 > 0).then_some(command.0 as usize)
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
