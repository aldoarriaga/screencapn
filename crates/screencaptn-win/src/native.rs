use crate::diagnostics;
use crate::hotkey::reserved_hotkey_reason;
use crate::overlay::{open_capture_overlay, AppTheme};
use crate::settings::{load_settings, save_settings, AppSettings, HotkeySettings};
use crate::shortcut_window::edit_hotkey;
use crate::theme::{load_theme, save_theme, toggled_theme};
use crate::tray::{
    add_tray_icon, remove_tray_icon, show_tray_menu, update_tray_icon, TrayAction, TrayMenuState,
    WM_TRAYICON,
};
use crate::update_window::{show_update_dialog, UpdateDialogAction};
use crate::updates::{
    self, UpdateEvent, UpdateInstallOutcome, UpdateService, WM_UPDATE_EVENT,
    WM_UPDATE_INSTALL_READY,
};
use std::path::PathBuf;
use windows::core::{w, Error, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, ShellExecuteW, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, IsWindow, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW,
    CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDC_ARROW, MB_ICONWARNING, MB_OK, MSG,
    SIZE_MINIMIZED, SW_HIDE, SW_SHOWNORMAL, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WM_SIZE, WM_TIMER, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

const APP_CLASS: windows::core::PCWSTR = w!("ScreenCaptnHiddenWindow");
const HOTKEY_ID: i32 = 100;
const UPDATE_CHECK_TIMER_ID: usize = 4;
const UPDATE_TICK_MILLISECONDS: u32 = 60 * 60 * 1000;

pub struct NativeApp {
    hwnd: HWND,
}

struct AppState {
    theme: AppTheme,
    settings: AppSettings,
    updates: UpdateService,
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

            let updates = UpdateService::default();
            let mut settings = load_settings();
            if updates::clear_installed_pending_update(&mut settings.update_check) {
                let _ = save_settings(&settings);
            }
            let mut state = Box::new(AppState {
                theme: load_theme(),
                settings,
                updates,
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
            let hotkey_error = register_configured_hotkey(hwnd, &mut state).err();
            if let Err(error) = add_tray_icon(hwnd, &state.settings) {
                diagnostics::log_event("startup", &format!("tray-add-failed: {error:?}"));
            }
            if let Some(error) = hotkey_error {
                diagnostics::log_event("startup", &format!("hotkey-register-failed: {error:?}"));
                show_hotkey_error(
                    hwnd,
                    "Your configured shortcut is currently used by Windows or another app. Screen Cap'n is still running in the tray; choose a different shortcut from its tray menu.",
                );
            }
            let _ = SetTimer(hwnd, UPDATE_CHECK_TIMER_ID, UPDATE_TICK_MILLISECONDS, None);
            state
                .updates
                .begin_due_check(hwnd, &state.settings.update_check);
            Box::leak(state);

            Ok(Self { hwnd })
        }
    }

    pub fn run(self) -> Result<()> {
        unsafe {
            let mut msg = MSG::default();
            loop {
                let status = GetMessageW(&mut msg, None, 0, 0);
                if status.0 == -1 {
                    return Err(Error::from_win32());
                }
                if status.0 == 0 {
                    if IsWindow(self.hwnd).as_bool() {
                        diagnostics::log_event("lifecycle", "ignored-spurious-wm-quit");
                        continue;
                    }
                    break;
                }
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
                let menu = TrayMenuState {
                    theme,
                    update_available: settings.update_check.pending.is_some(),
                    settings,
                    startup_state: crate::startup::state(),
                };
                match show_tray_menu(hwnd, menu) {
                    Some(TrayAction::SetShortcut) => edit_shortcut(hwnd),
                    Some(TrayAction::ToggleAutoSave) => toggle_auto_save(hwnd),
                    Some(TrayAction::SetAutoSaveFolder) => choose_auto_save_folder(hwnd),
                    Some(TrayAction::ToggleTheme) => toggle_theme(hwnd),
                    Some(TrayAction::ToggleRunOnStartup) => toggle_run_on_startup(hwnd),
                    Some(TrayAction::ShowUpdate) => show_available_update(hwnd),
                    Some(TrayAction::Donate) => open_donation_page(hwnd),
                    Some(TrayAction::Exit) => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == UPDATE_CHECK_TIMER_ID => {
            if let Some(state) = app_state_mut(hwnd) {
                state
                    .updates
                    .begin_due_check(hwnd, &state.settings.update_check);
            }
            LRESULT(0)
        }
        WM_UPDATE_EVENT => {
            handle_update_events(hwnd);
            LRESULT(0)
        }
        WM_UPDATE_INSTALL_READY => {
            if let Some(state) = app_state_mut(hwnd) {
                state.updates.start_install_from_message(hwnd, lparam);
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
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, UPDATE_CHECK_TIMER_ID);
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
        if let Some(reason) = reserved_hotkey_reason(&hotkey) {
            show_hotkey_error(hwnd, reason);
            return;
        }

        let previous = state.settings.hotkey.clone();
        if apply_user_hotkey(hwnd, state, hotkey).is_ok() {
            if let Err(error) = save_settings(&state.settings) {
                state.settings.hotkey = previous;
                let _ = register_configured_hotkey(hwnd, state);
                show_settings_error(hwnd, "save the shortcut", &error);
            } else {
                update_tray_icon(hwnd, &state.settings);
            }
        } else {
            state.settings.hotkey = previous;
            let _ = register_configured_hotkey(hwnd, state);
            show_hotkey_error(
                hwnd,
                "This shortcut is already used by Windows or another app. Choose a different one.",
            );
        }
    }
}

unsafe fn toggle_auto_save(hwnd: HWND) {
    if let Some(state) = app_state_mut(hwnd) {
        let previous = state.settings.auto_save.enabled;
        state.settings.auto_save.enabled = !state.settings.auto_save.enabled;
        if let Err(error) = save_settings(&state.settings) {
            state.settings.auto_save.enabled = previous;
            show_settings_error(hwnd, "change automatic saving", &error);
        } else {
            update_tray_icon(hwnd, &state.settings);
        }
    }
}

unsafe fn choose_auto_save_folder(hwnd: HWND) {
    if let Some(folder) = show_folder_picker(hwnd) {
        if let Some(state) = app_state_mut(hwnd) {
            let previous = state.settings.auto_save.folder.clone();
            state.settings.auto_save.folder = folder;
            if let Err(error) = save_settings(&state.settings) {
                state.settings.auto_save.folder = previous;
                show_settings_error(hwnd, "change the automatic-save folder", &error);
            } else {
                update_tray_icon(hwnd, &state.settings);
            }
        }
    }
}

unsafe fn toggle_run_on_startup(hwnd: HWND) {
    if let Err(message) = crate::startup::toggle() {
        let message = wide_null(&message);
        let title = wide_null("Screen Cap'n Startup");
        let _ = MessageBoxW(
            hwnd,
            windows::core::PCWSTR(message.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

unsafe fn show_available_update(hwnd: HWND) {
    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    let Some(pending) = state.settings.update_check.pending.clone() else {
        return;
    };
    let theme = state.theme;
    match show_update_dialog(hwnd, theme, pending.clone()) {
        Ok(Some(UpdateDialogAction::UpdateNow)) => state.updates.begin_install(hwnd),
        Ok(Some(UpdateDialogAction::MoreDetails)) => open_update_details(hwnd, &pending.version),
        _ => {}
    }
}

unsafe fn handle_update_events(hwnd: HWND) {
    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    for event in state.updates.take_events() {
        match event {
            UpdateEvent::CheckCompleted(outcome) => {
                updates::apply_check_outcome(&mut state.settings.update_check, outcome);
                if let Err(error) = save_settings(&state.settings) {
                    diagnostics::log_event("updates", &format!("settings-save-failed: {error}"));
                }
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::Completed) => {
                state.settings.update_check.pending = None;
                if let Err(error) = save_settings(&state.settings) {
                    diagnostics::log_event("updates", &format!("settings-save-failed: {error}"));
                }
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::NoUpdate) => {
                state.settings.update_check.pending = None;
                if let Err(error) = save_settings(&state.settings) {
                    diagnostics::log_event("updates", &format!("settings-save-failed: {error}"));
                }
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::Failed) => {
                show_update_install_error(hwnd);
            }
        }
    }
}

unsafe fn open_update_details(hwnd: HWND, version: &str) {
    let url = updates::details_url(version);
    let url = wide_null(&url);
    let _ = ShellExecuteW(
        hwnd,
        w!("open"),
        windows::core::PCWSTR(url.as_ptr()),
        None,
        None,
        SW_SHOWNORMAL,
    );
}

unsafe fn show_update_install_error(hwnd: HWND) {
    let message =
        wide_null("Screen Cap'n could not install the update right now. Please try again later.");
    let title = wide_null("Screen Cap'n Update");
    let _ = MessageBoxW(
        hwnd,
        windows::core::PCWSTR(message.as_ptr()),
        windows::core::PCWSTR(title.as_ptr()),
        MB_OK | MB_ICONWARNING,
    );
}

unsafe fn open_donation_page(hwnd: HWND) {
    let _ = ShellExecuteW(
        hwnd,
        w!("open"),
        w!("https://screencapn.com/donate"),
        None,
        None,
        SW_SHOWNORMAL,
    );
}

unsafe fn apply_user_hotkey(
    hwnd: HWND,
    state: &mut AppState,
    hotkey: HotkeySettings,
) -> Result<()> {
    if state.hotkey_registered {
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        state.hotkey_registered = false;
    }
    let modifiers = hotkey_modifiers_for_hotkey(&hotkey);
    RegisterHotKey(hwnd, HOTKEY_ID, modifiers, hotkey.key_code)?;
    state.settings.hotkey = hotkey;
    state.hotkey_registered = true;
    Ok(())
}

unsafe fn show_hotkey_error(hwnd: HWND, message: &str) {
    let message = wide_null(message);
    let title = wide_null("Screen Cap'n Shortcut");
    let _ = MessageBoxW(
        hwnd,
        windows::core::PCWSTR(message.as_ptr()),
        windows::core::PCWSTR(title.as_ptr()),
        MB_OK | MB_ICONWARNING,
    );
}

unsafe fn show_settings_error(hwnd: HWND, action: &str, error: &std::io::Error) {
    let message = format!("Screen Cap'n could not {action}.\n\n{error}");
    let message = wide_null(&message);
    let title = wide_null("Screen Cap'n Settings");
    let _ = MessageBoxW(
        hwnd,
        windows::core::PCWSTR(message.as_ptr()),
        windows::core::PCWSTR(title.as_ptr()),
        MB_OK | MB_ICONWARNING,
    );
}

unsafe fn register_configured_hotkey(hwnd: HWND, state: &mut AppState) -> Result<()> {
    if state.hotkey_registered {
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        state.hotkey_registered = false;
    }
    let modifiers = hotkey_modifiers_for_hotkey(&state.settings.hotkey);
    RegisterHotKey(hwnd, HOTKEY_ID, modifiers, state.settings.hotkey.key_code)?;
    state.hotkey_registered = true;
    Ok(())
}

fn hotkey_modifiers_for_hotkey(
    hotkey: &HotkeySettings,
) -> windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS {
    let mut modifiers = windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(0);
    if hotkey.ctrl {
        modifiers |= MOD_CONTROL;
    }
    if hotkey.shift {
        modifiers |= MOD_SHIFT;
    }
    if hotkey.alt {
        modifiers |= MOD_ALT;
    }
    if hotkey.win {
        modifiers |= MOD_WIN;
    }
    modifiers
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn show_folder_picker(owner: HWND) -> Option<PathBuf> {
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
