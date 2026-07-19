use crate::hotkey::reserved_hotkey_reason;
use crate::settings::HotkeySettings;
use crate::util::{colorref, rect_to_rect, SelectedPen, SelectedStockObject};
use screencaptn_core::{Color, Point, Rect};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SetBkMode, SetTextColor, TextOutW, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, HDC, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, LoadCursorW, RegisterClassW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDC_ARROW, MSG, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE,
    WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW, WS_CAPTION,
    WS_OVERLAPPED, WS_SYSMENU,
};

const CLASS_NAME: PCWSTR = w!("ScreenCaptnShortcutWindow");
const WINDOW_WIDTH: i32 = 640;
const WINDOW_HEIGHT: i32 = 360;

struct ShortcutWindowState {
    current: HotkeySettings,
    original: HotkeySettings,
    result: Option<Option<HotkeySettings>>,
    validation_message: Option<String>,
    save_rect: Rect,
    reset_rect: Rect,
    cancel_rect: Rect,
}

pub fn edit_hotkey(initial: HotkeySettings) -> Result<Option<HotkeySettings>> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(shortcut_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&class);

        let mut window_rect = RECT {
            left: 0,
            top: 0,
            right: WINDOW_WIDTH,
            bottom: WINDOW_HEIGHT,
        };
        let _ = AdjustWindowRectEx(
            &mut window_rect,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            false,
            WINDOW_EX_STYLE::default(),
        );
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;

        let mut state = Box::new(ShortcutWindowState {
            current: initial.clone(),
            original: initial,
            result: None,
            validation_message: None,
            save_rect: Rect::new(424.0, 292.0, 150.0, 42.0),
            reset_rect: Rect::new(254.0, 292.0, 150.0, 42.0),
            cancel_rect: Rect::new(84.0, 292.0, 150.0, 42.0),
        });
        let state_ptr = state.as_mut() as *mut ShortcutWindowState;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("Screen Cap'n Shortcut"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            None,
            HMENU::default(),
            instance,
            Some(state_ptr.cast()),
        )?;
        Box::leak(state);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }
            if let Some(state) = shortcut_state(hwnd) {
                if state.result.is_some() {
                    break;
                }
            } else {
                break;
            }
        }

        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ShortcutWindowState;
        if state_ptr.is_null() {
            return Ok(None);
        }
        let mut state = Box::from_raw(state_ptr);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let result = state.result.take().unwrap_or(None);
        if windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        Ok(result)
    }
}

unsafe extern "system" fn shortcut_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if let Some(state) = shortcut_state(hwnd) {
                draw_shortcut_window(hdc, state);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(state) = shortcut_state(hwnd) {
                if let Some(next) = hotkey_from_key(wparam.0 as u32) {
                    state.current = next;
                    state.validation_message = hotkey_validation_message(&state.current);
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let point = Point::new(
                (lparam.0 as u32 & 0xffff) as i16 as f32,
                ((lparam.0 as u32 >> 16) & 0xffff) as i16 as f32,
            );
            if let Some(state) = shortcut_state(hwnd) {
                if state.save_rect.contains(point) {
                    state.validation_message = hotkey_validation_message(&state.current);
                    if state.validation_message.is_none() {
                        state.result = Some(Some(state.current.clone()));
                    } else {
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                } else if state.reset_rect.contains(point) {
                    state.current = HotkeySettings::default();
                    state.validation_message = None;
                    let _ = InvalidateRect(hwnd, None, false);
                } else if state.cancel_rect.contains(point) {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(state) = shortcut_state(hwnd) {
                state.result = Some(None);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(state) = shortcut_state(hwnd) {
                if state.result.is_none() {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn shortcut_state(hwnd: HWND) -> Option<&'static mut ShortcutWindowState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ShortcutWindowState;
    (!ptr.is_null()).then(|| &mut *ptr)
}

unsafe fn draw_shortcut_window(hdc: HDC, state: &ShortcutWindowState) {
    fill(
        hdc,
        Rect::new(0.0, 0.0, WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32),
        Color::BLACK,
    );
    draw_text(hdc, 44, 34, "Quick Access Shortcut", 30, Color::WHITE, true);
    draw_text(
        hdc,
        44,
        82,
        "Press the combination you want to use. Include at least one modifier.",
        15,
        Color::rgb(190, 190, 190),
        false,
    );

    let mut x = 76.0;
    for label in key_tiles(&state.current) {
        draw_key_tile(hdc, Rect::new(x, 168.0, 72.0, 56.0), &label);
        x += 86.0;
    }

    if let Some(message) = &state.validation_message {
        draw_text(hdc, 44, 244, message, 14, Color::rgb(255, 99, 0), false);
    } else if !state.current.is_valid() {
        draw_text(
            hdc,
            44,
            244,
            "Choose a modifier plus one key.",
            14,
            Color::rgb(255, 99, 0),
            false,
        );
    } else if state.current.display_label() != state.original.display_label() {
        draw_text(
            hdc,
            44,
            244,
            &format!("New shortcut: {}", state.current.display_label()),
            14,
            Color::rgb(150, 210, 255),
            false,
        );
    }

    draw_button(
        hdc,
        state.cancel_rect,
        "Cancel",
        Color::rgb(28, 28, 28),
        Color::WHITE,
    );
    draw_button(
        hdc,
        state.reset_rect,
        "Reset",
        Color::rgb(28, 28, 28),
        Color::WHITE,
    );
    let save_color = if hotkey_validation_message(&state.current).is_none() {
        Color::rgb(0, 96, 120)
    } else {
        Color::rgb(45, 45, 45)
    };
    draw_button(hdc, state.save_rect, "Save", save_color, Color::WHITE);
}

unsafe fn draw_key_tile(hdc: HDC, rect: Rect, label: &str) {
    fill(hdc, rect, Color::rgb(126, 0, 32));
    let _pen = SelectedPen::new(hdc, 1.0, Color::rgb(160, 0, 44));
    let _brush = SelectedStockObject::null_brush(hdc);
    let r = rect_to_rect(rect);
    let _ = windows::Win32::Graphics::Gdi::Rectangle(hdc, r.left, r.top, r.right, r.bottom);
    draw_centered_text(hdc, rect, label, 21, Color::WHITE, true);
}

unsafe fn draw_button(hdc: HDC, rect: Rect, label: &str, background: Color, foreground: Color) {
    fill(hdc, rect, background);
    draw_centered_text(hdc, rect, label, 15, foreground, true);
}

unsafe fn fill(hdc: HDC, rect: Rect, color: Color) {
    let brush = CreateSolidBrush(colorref(color));
    let _ = FillRect(hdc, &rect_to_rect(rect), brush);
    let _ = DeleteObject(brush);
}

unsafe fn draw_text(hdc: HDC, x: i32, y: i32, text: &str, size: i32, color: Color, bold: bool) {
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { FW_BOLD.0 as i32 } else { 400 },
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI"),
    );
    let old = windows::Win32::Graphics::Gdi::SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(color));
    let wide: Vec<u16> = text.encode_utf16().collect();
    let _ = TextOutW(hdc, x, y, &wide);
    let _ = windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

unsafe fn draw_centered_text(
    hdc: HDC,
    rect: Rect,
    text: &str,
    size: i32,
    color: Color,
    bold: bool,
) {
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { FW_BOLD.0 as i32 } else { 400 },
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI"),
    );
    let old = windows::Win32::Graphics::Gdi::SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(color));
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut native_rect = rect_to_rect(rect);
    let _ = DrawTextW(
        hdc,
        &mut wide,
        &mut native_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    let _ = windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

fn key_tiles(hotkey: &HotkeySettings) -> Vec<String> {
    let mut tiles = Vec::new();
    if hotkey.ctrl {
        tiles.push("CTRL".to_string());
    }
    if hotkey.shift {
        tiles.push("SHIFT".to_string());
    }
    if hotkey.alt {
        tiles.push("ALT".to_string());
    }
    if hotkey.win {
        tiles.push("WIN".to_string());
    }
    tiles.push(hotkey.key_label.clone());
    tiles
}

fn hotkey_from_key(key_code: u32) -> Option<HotkeySettings> {
    if matches!(
        key_code,
        code if code == VK_SHIFT.0 as u32
            || code == VK_CONTROL.0 as u32
            || code == VK_MENU.0 as u32
            || code == VK_LWIN.0 as u32
            || code == VK_RWIN.0 as u32
    ) {
        return None;
    }
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 };
    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 };
    let alt = unsafe { GetKeyState(VK_MENU.0 as i32) < 0 };
    let win = unsafe { GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 };
    Some(HotkeySettings {
        ctrl,
        shift,
        alt,
        win,
        key_code,
        key_label: key_label(key_code),
    })
}

fn hotkey_validation_message(hotkey: &HotkeySettings) -> Option<String> {
    if !hotkey.is_valid() {
        return Some("Choose a modifier plus one key.".to_string());
    }
    reserved_hotkey_reason(hotkey).map(str::to_string)
}

fn key_label(key_code: u32) -> String {
    match key_code {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(key_code).unwrap_or('?').to_string(),
        0x70..=0x87 => format!("F{}", key_code - 0x6F),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x24 => "Home".to_string(),
        0x23 => "End".to_string(),
        0x21 => "Page Up".to_string(),
        0x22 => "Page Down".to_string(),
        0x20 => "Space".to_string(),
        0x0D => "Enter".to_string(),
        0x09 => "Tab".to_string(),
        0x1B => "Esc".to_string(),
        0x2C => "Print Screen".to_string(),
        _ => format!("VK {}", key_code),
    }
}
