use crate::overlay::{open_capture_overlay, AppTheme};
use crate::theme::{load_theme, save_theme, toggled_theme};
use crate::tray::{
    add_tray_icon, remove_tray_icon, show_tray_menu, TRAY_EXIT_COMMAND, TRAY_TOGGLE_THEME_COMMAND,
    WM_TRAYICON,
};
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_SHIFT,
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
    overlay_open: bool,
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
                overlay_open: false,
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
            Box::leak(state);

            RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT, 'A' as u32)?;
            add_tray_icon(hwnd)?;

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
                let _ = open_capture_overlay(theme);
                if let Some(state) = app_state_mut(hwnd) {
                    state.overlay_open = false;
                }
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_LBUTTONUP || lparam.0 as u32 == WM_RBUTTONUP {
                match show_tray_menu(hwnd, app_theme(hwnd)) {
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

unsafe fn toggle_theme(hwnd: HWND) {
    if let Some(state) = app_state_mut(hwnd) {
        state.theme = toggled_theme(state.theme);
        save_theme(state.theme);
    }
}
