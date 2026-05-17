use crate::overlay::open_capture_overlay;
use crate::tray::{
    add_tray_icon, remove_tray_icon, show_tray_menu, TRAY_EXIT_COMMAND, WM_TRAYICON,
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
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
    PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, HMENU, IDC_ARROW, MSG, SIZE_MINIMIZED, SW_HIDE, WINDOW_EX_STYLE, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WM_SIZE, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

const APP_CLASS: windows::core::PCWSTR = w!("ScreenCaptnHiddenWindow");
const HOTKEY_ID: i32 = 100;

pub struct NativeApp {
    hwnd: HWND,
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
                None,
            )?;

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
        WM_CREATE => LRESULT(0),
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            let _ = open_capture_overlay();
            LRESULT(0)
        }
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_LBUTTONUP {
                let _ = open_capture_overlay();
            } else if lparam.0 as u32 == WM_RBUTTONUP {
                if show_tray_menu(hwnd) == Some(TRAY_EXIT_COMMAND) {
                    let _ = DestroyWindow(hwnd);
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
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
