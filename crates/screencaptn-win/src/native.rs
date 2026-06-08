use crate::overlay::{open_capture_overlay, AppTheme};
use crate::settings::{load_settings, save_settings, AppSettings};
use crate::shortcut_window::edit_hotkey;
use crate::theme::{load_theme, save_theme, toggled_theme};
use crate::tray::{
    add_tray_icon, remove_tray_icon, show_tray_menu, update_tray_icon, TRAY_EXIT_COMMAND,
    TRAY_SET_AUTO_SAVE_FOLDER_COMMAND, TRAY_SET_SHORTCUT_COMMAND, TRAY_TOGGLE_AUTO_SAVE_COMMAND,
    TRAY_TOGGLE_THEME_COMMAND, WM_TRAYICON,
};
use std::path::PathBuf;
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HMENU,
    IDC_ARROW, MSG, SIZE_MINIMIZED, SW_HIDE, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const APP_CLASS: windows::core::PCWSTR = w!("ScreenCaptnHiddenWindow");
const HOTKEY_ID: i32 = 100;

pub struct NativeApp {
    hwnd: HWND,
}

struct AppState {
    theme: AppTheme,
    settings: AppSettings,
    overlay_open: bool,
    hotkey_registered: bool,
}

impl NativeApp {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hInstance: instance.into(),
                lpszClassName: APP_CLASS,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(app_wnd_proc),
                ..Default::default()
            };
            RegisterClassW(&class);

            let mut state = Box::new(AppState {
                theme: load_theme(),
                settings: load_settings(),
                overlay_open: false,
                hotkey_registered: false,
            });
            let state_ptr = state.as_mut() as *mut AppState;
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                APP_CLASS,
                w!("Screen Captn"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                HMENU::default(),
                instance,
                Some(state_ptr.cast()),
            )?;
            register_configured_hotkey(hwnd, &mut state)?;
            add_tray_icon(hwnd, &state.settings)?;
            Box::leak(state);

            Ok(Self { hwnd })
        }
    }

    pub fn run(self) -> Result<()> {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnregisterHotKey(self.hwnd, HOTKEY_ID);
            Ok(())
        }
    }
}

unsafe extern "system" fn app_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            let state_ptr = (*create).lpCreateParams as *mut AppState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            LRESULT(0)
        }
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            if let Some(state) = app_state_mut(hwnd) {
                if state.overlay_open {
                    return LRESULT(0);
                }
                state.overlay_open = true;
                state.theme = load_theme();
                let theme = state.theme;
                state.settings = load_settings();
                let settings = state.settings.clone();
                let _ = open_capture_overlay(theme, settings);
                if let Some(state) = app_state_mut(hwnd) {
                    state.overlay_open = false;
                    state.settings = load_settings();
                }
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_LBUTTONUP || lparam.0 as u32 == WM_RBUTTONUP {
                let theme = app_theme(hwnd);
                let settings = app_settings(hwnd);
                match show_tray_menu(hwnd, theme, &settings) {
                    Some(TRAY_SET_SHORTCUT_COMMAND) => edit_shortcut(hwnd),
                    Some(TRAY_TOGGLE_AUTO_SAVE_COMMAND) => toggle_auto_save(hwnd),
                    Some(TRAY_SET_AUTO_SAVE_FOLDER_COMMAND) => choose_auto_save_folder(hwnd),
                    Some(TRAY_TOGGLE_THEME_COMMAND) => toggle_theme(hwnd),
                    Some(TRAY_EXIT_COMMAND) => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => LRESULT(0),
        WM_SIZE if wparam.0 == SIZE_MINIMIZED as usize => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            if let Some(state) = app_state_mut(hwnd) {
                if state.hotkey_registered {
                    let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
                    state.hotkey_registered = false;
                }
            }
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                let _ = Box::from_raw(state_ptr);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn app_state_mut(hwnd: HWND) -> Option<&'static mut AppState> {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    (!state_ptr.is_null()).then(|| &mut *state_ptr)
}

unsafe fn app_theme(hwnd: HWND) -> AppTheme {
    let theme = load_theme();
    if let Some(state) = app_state_mut(hwnd) {
        state.theme = theme;
    }
    theme
}

unsafe fn app_settings(hwnd: HWND) -> AppSettings {
    let settings = load_settings();
    if let Some(state) = app_state_mut(hwnd) {
        state.settings = settings.clone();
    }
    settings
}

unsafe fn toggle_theme(hwnd: HWND) {
    if let Some(state) = app_state_mut(hwnd) {
        state.theme = toggled_theme(state.theme);
        save_theme(state.theme);
    }
}

unsafe fn edit_shortcut(hwnd: HWND) {
    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    if let Ok(Some(hotkey)) = edit_hotkey(state.settings.hotkey.clone()) {
        state.settings.hotkey = hotkey;
        if register_configured_hotkey(hwnd, state).is_ok() {
            save_settings(&state.settings);
            update_tray_icon(hwnd, &state.settings);
        }
    }
}

unsafe fn toggle_auto_save(hwnd: HWND) {
    if let Some(state) = app_state_mut(hwnd) {
        state.settings.auto_save.enabled = !state.settings.auto_save.enabled;
        save_settings(&state.settings);
        update_tray_icon(hwnd, &state.settings);
    }
}

unsafe fn choose_auto_save_folder(hwnd: HWND) {
    if let Some(folder) = show_folder_picker(hwnd) {
        if let Some(state) = app_state_mut(hwnd) {
            state.settings.auto_save.folder = folder;
            save_settings(&state.settings);
            update_tray_icon(hwnd, &state.settings);
        }
    }
}

unsafe fn register_configured_hotkey(hwnd: HWND, state: &mut AppState) -> Result<()> {
    if state.hotkey_registered {
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        state.hotkey_registered = false;
    }
    let modifiers = hotkey_modifiers(&state.settings);
    match RegisterHotKey(hwnd, HOTKEY_ID, modifiers, state.settings.hotkey.key_code) {
        Ok(()) => {
            state.hotkey_registered = true;
            Ok(())
        }
        Err(_) => {
            state.settings.hotkey = Default::default();
            RegisterHotKey(
                hwnd,
                HOTKEY_ID,
                MOD_CONTROL | MOD_SHIFT,
                state.settings.hotkey.key_code,
            )?;
            state.hotkey_registered = true;
            save_settings(&state.settings);
            Ok(())
        }
    }
}

fn hotkey_modifiers(
    settings: &AppSettings,
) -> windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS {
    let mut modifiers = windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(0);
    if settings.hotkey.ctrl {
        modifiers |= MOD_CONTROL;
    }
    if settings.hotkey.shift {
        modifiers |= MOD_SHIFT;
    }
    if settings.hotkey.alt {
        modifiers |= MOD_ALT;
    }
    if settings.hotkey.win {
        modifiers |= MOD_WIN;
    }
    modifiers
}

unsafe fn show_folder_picker(owner: HWND) -> Option<PathBuf> {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
    let dialog: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let _ = dialog.SetTitle(w!("Choose Screen Cap'n auto-save folder"));
    let options = dialog.GetOptions().ok()?;
    let _ = dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
    if dialog.Show(owner).is_err() {
        return None;
    }
    let item = dialog.GetResult().ok()?;
    let path_ptr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    if path_ptr.is_null() {
        return None;
    }
    let path = path_ptr.to_string().ok().map(PathBuf::from);
    CoTaskMemFree(Some(path_ptr.as_ptr().cast()));
    path
}
