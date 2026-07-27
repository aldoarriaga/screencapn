use crate::diagnostics;
use crate::hotkey::reserved_hotkey_reason;
use crate::overlay::{
    open_capture_overlay, AppTheme, WM_OVERLAY_CLOSED, WM_OVERLAY_OPENED, WM_OVERLAY_SHOW_UPDATE,
    WM_OVERLAY_UPDATE_CHANGED,
};
use crate::settings::{
    load_settings, load_settings_state, save_settings, update_settings, AppSettings, HotkeySettings,
};
use crate::shortcut_window::edit_hotkey;
use crate::theme::{load_theme, save_theme, toggled_theme, windows_theme};
use crate::tray::{
    add_tray_icon, remove_tray_icon, show_tray_menu, update_tray_icon, TrayAction, TrayMenuState,
    WM_TRAYICON,
};
use crate::update_window::{show_update_dialog, UpdateDialogAction};
use crate::updates::{
    self, UpdateEvent, UpdateInstallOutcome, UpdateService, WM_UPDATE_EVENT,
    WM_UPDATE_INSTALL_READY,
};
use crate::welcome_window::{show_first_run, WelcomeOutcome};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
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
    GetWindowLongPtrW, IsWindow, LoadCursorW, MessageBoxW, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetTimer, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HMENU,
    IDC_ARROW, MB_ICONWARNING, MB_OK, MSG, SIZE_MINIMIZED, SW_HIDE, SW_SHOWNORMAL, WINDOW_EX_STYLE,
    WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP,
    WM_SIZE, WM_TIMER, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const APP_CLASS: windows::core::PCWSTR = w!("ScreenCaptnHiddenWindow");
static TASKBAR_CREATED_MESSAGE: AtomicU32 = AtomicU32::new(0);
const HOTKEY_ID: i32 = 100;
const UPDATE_CHECK_TIMER_ID: usize = 4;
const UPDATE_TICK_MILLISECONDS: u32 = 60 * 60 * 1000;
const WM_SHOW_FIRST_RUN: u32 = WM_APP + 0x51;

pub struct NativeApp {
    hwnd: HWND,
}

struct AppState {
    theme: AppTheme,
    settings: AppSettings,
    updates: UpdateService,
    overlay_open: bool,
    overlay_hwnd: Option<HWND>,
    welcome_open: bool,
    hotkey_registered: bool,
}

impl NativeApp {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let taskbar_created_message = RegisterWindowMessageW(w!("TaskbarCreated"));
            TASKBAR_CREATED_MESSAGE.store(taskbar_created_message, Ordering::Relaxed);
            if taskbar_created_message == 0 {
                diagnostics::log_event("startup", "taskbar-created-message-registration-failed");
            }
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
            let loaded = load_settings_state();
            let show_first_run = loaded.is_new_install && !loaded.settings.onboarding.completed;
            let mut settings = loaded.settings;
            if updates::clear_installed_pending_update(&mut settings.update_check) {
                let _ = save_settings(&settings);
            }
            let mut state = Box::new(AppState {
                theme: if show_first_run {
                    windows_theme()
                } else {
                    load_theme()
                },
                settings,
                updates,
                overlay_open: false,
                overlay_hwnd: None,
                welcome_open: false,
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
            if show_first_run {
                let _ = PostMessageW(hwnd, WM_SHOW_FIRST_RUN, WPARAM(0), LPARAM(0));
            }

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
    let taskbar_created_message = TASKBAR_CREATED_MESSAGE.load(Ordering::Relaxed);
    if taskbar_created_message != 0 && msg == taskbar_created_message {
        if let Some(state) = app_state_mut(hwnd) {
            if let Err(error) = add_tray_icon(hwnd, &state.settings) {
                diagnostics::log_event("lifecycle", &format!("tray-restore-failed: {error:?}"));
            } else {
                diagnostics::log_event("lifecycle", "tray-restored-after-explorer-restart");
            }
        }
        return LRESULT(0);
    }

    match msg {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            let state_ptr = (*create).lpCreateParams as *mut AppState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            LRESULT(0)
        }
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            if let Some(state) = app_state_mut(hwnd) {
                if state.overlay_open || state.welcome_open {
                    return LRESULT(0);
                }
                state.overlay_open = true;
                state.theme = load_theme();
                let theme = state.theme;
                state.settings = load_settings();
                let settings = state.settings.clone();
                let _ = open_capture_overlay(hwnd, theme, settings);
                if let Some(state) = app_state_mut(hwnd) {
                    state.overlay_open = false;
                    state.settings = load_settings();
                }
            }
            LRESULT(0)
        }
        WM_SHOW_FIRST_RUN => {
            show_first_run_setup(hwnd, false);
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
                    Some(TrayAction::OpenAutoSaveFolder) => open_auto_save_folder(hwnd),
                    Some(TrayAction::SetAutoSaveFolder) => choose_auto_save_folder(hwnd),
                    Some(TrayAction::ToggleTheme) => toggle_theme(hwnd),
                    Some(TrayAction::ToggleCaptureTips) => toggle_capture_tips(hwnd),
                    Some(TrayAction::OpenSetup) => show_first_run_setup(hwnd, true),
                    Some(TrayAction::ToggleRunOnStartup) => toggle_run_on_startup(hwnd),
                    Some(TrayAction::ShowUpdate) => show_available_update(hwnd, None),
                    Some(TrayAction::ReportBug) => open_bug_report_page(hwnd),
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
        WM_OVERLAY_OPENED => {
            if let Some(state) = app_state_mut(hwnd) {
                state.overlay_hwnd = Some(HWND(wparam.0 as *mut _));
                notify_overlay_update_state(state);
            }
            LRESULT(0)
        }
        WM_OVERLAY_CLOSED => {
            if let Some(state) = app_state_mut(hwnd) {
                let closed = HWND(wparam.0 as *mut _);
                if state.overlay_hwnd == Some(closed) {
                    state.overlay_hwnd = None;
                    state.overlay_open = false;
                }
            }
            LRESULT(0)
        }
        WM_OVERLAY_SHOW_UPDATE => {
            let owner = HWND(wparam.0 as *mut _);
            show_available_update(hwnd, Some(owner));
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

unsafe fn toggle_capture_tips(hwnd: HWND) {
    if let Some(state) = app_state_mut(hwnd) {
        let previous = state.settings.show_capture_tips;
        let enabled = !previous;
        match update_settings(|settings| settings.show_capture_tips = enabled) {
            Ok(settings) => state.settings = settings,
            Err(error) => {
                state.settings.show_capture_tips = previous;
                show_settings_error(hwnd, "change capture tips", &error);
            }
        }
    }
}

unsafe fn edit_shortcut(hwnd: HWND) {
    let Some((initial, theme)) =
        app_state_mut(hwnd).map(|state| (state.settings.hotkey.clone(), state.theme))
    else {
        return;
    };
    if let Ok(Some(hotkey)) = edit_hotkey(hwnd, initial, theme) {
        if let Some(reason) = reserved_hotkey_reason(&hotkey) {
            show_hotkey_error(hwnd, reason);
            return;
        }

        let Some(state) = app_state_mut(hwnd) else {
            return;
        };
        let previous = state.settings.hotkey.clone();
        if apply_user_hotkey(hwnd, state, hotkey).is_ok() {
            let next = state.settings.hotkey.clone();
            match update_settings(|settings| settings.hotkey = next) {
                Ok(settings) => {
                    state.settings = settings;
                    update_tray_icon(hwnd, &state.settings);
                }
                Err(error) => {
                    state.settings.hotkey = previous;
                    let _ = register_configured_hotkey(hwnd, state);
                    show_settings_error(hwnd, "save the shortcut", &error);
                }
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
        let enabled = state.settings.auto_save.enabled;
        match update_settings(|settings| settings.auto_save.enabled = enabled) {
            Ok(settings) => {
                state.settings = settings;
                update_tray_icon(hwnd, &state.settings);
            }
            Err(error) => {
                state.settings.auto_save.enabled = previous;
                show_settings_error(hwnd, "change automatic saving", &error);
            }
        }
    }
}

unsafe fn choose_auto_save_folder(hwnd: HWND) {
    if let Some(folder) = show_folder_picker(hwnd) {
        if let Some(state) = app_state_mut(hwnd) {
            let previous = state.settings.auto_save.folder.clone();
            state.settings.auto_save.folder = folder;
            let next = state.settings.auto_save.folder.clone();
            match update_settings(|settings| settings.auto_save.folder = next) {
                Ok(settings) => {
                    state.settings = settings;
                    update_tray_icon(hwnd, &state.settings);
                }
                Err(error) => {
                    state.settings.auto_save.folder = previous;
                    show_settings_error(hwnd, "change the automatic-save folder", &error);
                }
            }
        }
    }
}

unsafe fn open_auto_save_folder(hwnd: HWND) {
    let folder = app_settings(hwnd).auto_save.folder;
    if let Err(error) = std::fs::create_dir_all(&folder) {
        show_settings_error(hwnd, "open the automatic-save folder", &error);
        return;
    }
    let folder = wide_null(&folder.to_string_lossy());
    let _ = ShellExecuteW(
        hwnd,
        w!("open"),
        windows::core::PCWSTR(folder.as_ptr()),
        None,
        None,
        SW_SHOWNORMAL,
    );
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

unsafe fn show_available_update(hwnd: HWND, dialog_owner: Option<HWND>) {
    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    let Some(pending) = state.settings.update_check.pending.clone() else {
        return;
    };
    let theme = state.theme;
    match show_update_dialog(dialog_owner.unwrap_or(hwnd), theme, pending.clone()) {
        Ok(Some(UpdateDialogAction::UpdateNow)) => state.updates.begin_install(hwnd),
        Ok(Some(UpdateDialogAction::MoreDetails)) => open_update_details(hwnd, &pending),
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
                let next = state.settings.update_check.clone();
                match update_settings(|settings| settings.update_check = next) {
                    Ok(settings) => state.settings = settings,
                    Err(error) => {
                        diagnostics::log_event(
                            "updates",
                            &format!("settings-save-failed: {error}"),
                        );
                    }
                }
                notify_overlay_update_state(state);
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::Completed) => {
                state.settings.update_check.pending = None;
                let next = state.settings.update_check.clone();
                match update_settings(|settings| settings.update_check = next) {
                    Ok(settings) => state.settings = settings,
                    Err(error) => {
                        diagnostics::log_event(
                            "updates",
                            &format!("settings-save-failed: {error}"),
                        );
                    }
                }
                notify_overlay_update_state(state);
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::NoUpdate) => {
                state.settings.update_check.pending = None;
                let next = state.settings.update_check.clone();
                match update_settings(|settings| settings.update_check = next) {
                    Ok(settings) => state.settings = settings,
                    Err(error) => {
                        diagnostics::log_event(
                            "updates",
                            &format!("settings-save-failed: {error}"),
                        );
                    }
                }
                notify_overlay_update_state(state);
            }
            UpdateEvent::InstallCompleted(UpdateInstallOutcome::Failed) => {
                show_update_install_error(hwnd);
            }
        }
    }
}

unsafe fn open_update_details(hwnd: HWND, pending: &crate::settings::PendingUpdate) {
    let url = updates::details_url(pending);
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

unsafe fn open_bug_report_page(hwnd: HWND) {
    let _ = ShellExecuteW(
        hwnd,
        w!("open"),
        w!("https://screencapn.com/feedback/"),
        None,
        None,
        SW_SHOWNORMAL,
    );
}

unsafe fn show_first_run_setup(hwnd: HWND, manual: bool) {
    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    if state.welcome_open || (!manual && state.settings.onboarding.completed) {
        return;
    }
    state.welcome_open = true;
    let settings = state.settings.clone();
    let theme = state.theme;
    let startup_enabled = crate::startup::state().is_enabled();
    let outcome = show_first_run(
        hwnd,
        &settings,
        theme,
        startup_enabled,
        !settings.onboarding.completed,
    )
    .unwrap_or(WelcomeOutcome::Skip);

    let Some(state) = app_state_mut(hwnd) else {
        return;
    };
    state.welcome_open = false;
    match outcome {
        WelcomeOutcome::Finish(choices) => {
            state.settings.auto_save.enabled = choices.auto_save;
            state.settings.auto_save.folder = choices.folder;
            state.settings.diagnostics.enabled = choices.diagnostics;
            state.settings.onboarding.completed = true;
            state.theme = choices.theme;
            save_theme(choices.theme);
            diagnostics::set_enabled(choices.diagnostics);
            if let Err(message) = crate::startup::set_enabled(choices.startup) {
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
        WelcomeOutcome::Skip => {
            state.settings.onboarding.completed = true;
        }
    }
    let auto_save = state.settings.auto_save.clone();
    let diagnostics = state.settings.diagnostics.clone();
    let onboarding = state.settings.onboarding.clone();
    match update_settings(|settings| {
        settings.auto_save = auto_save;
        settings.diagnostics = diagnostics;
        settings.onboarding = onboarding;
    }) {
        Ok(settings) => {
            state.settings = settings;
            update_tray_icon(hwnd, &state.settings);
        }
        Err(error) => show_settings_error(hwnd, "finish first-run setup", &error),
    }
}

unsafe fn notify_overlay_update_state(state: &AppState) {
    let Some(overlay) = state.overlay_hwnd else {
        return;
    };
    let available = usize::from(state.settings.update_check.pending.is_some());
    let _ = PostMessageW(
        overlay,
        WM_OVERLAY_UPDATE_CHANGED,
        WPARAM(available),
        LPARAM(0),
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
