use crate::overlay::AppTheme;
use windows::core::w;
use windows::core::Result;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, IDI_APPLICATION, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP,
};

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const TRAY_TOGGLE_THEME_COMMAND: usize = 9000;
pub const TRAY_EXIT_COMMAND: usize = 9001;
const TRAY_UID: u32 = 1;

pub unsafe fn add_tray_icon(hwnd: HWND) -> Result<()> {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: LoadIconW(None, IDI_APPLICATION)?,
        ..Default::default()
    };

    let tip: Vec<u16> = "Screen Captn - Ctrl+Shift+A\0".encode_utf16().collect();
    for (index, value) in tip.into_iter().enumerate().take(data.szTip.len()) {
        data.szTip[index] = value;
    }

    Shell_NotifyIconW(NIM_ADD, &data).ok()
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

pub unsafe fn show_tray_menu(hwnd: HWND, theme: AppTheme) -> Option<usize> {
    let menu = CreatePopupMenu().ok()?;
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
