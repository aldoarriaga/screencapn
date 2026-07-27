use crate::native_svg::draw_svg;
use crate::overlay::AppTheme;
use crate::settings::{default_auto_save_folder, AppSettings};
use crate::theme::toolbar_palette;
use crate::util::{colorref, point_from_lparam, SelectedPen};
use screencaptn_core::{Color, Rect};
use std::path::PathBuf;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetMonitorInfoW, LineTo, MonitorFromPoint, MoveToEx, RoundRect, SelectObject, SetBkMode,
    SetTextColor, SetWindowRgn, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, ShellExecuteW, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SetForegroundWindow,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    CREATESTRUCTW, GWLP_USERDATA, HMENU, IDC_ARROW, LWA_ALPHA, MSG, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_SHOW, SW_SHOWNORMAL, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::UI::ViewManagement::UISettings;

const PANEL_CLASS: PCWSTR = w!("ScreenCaptnWelcomePanel");
const BACKDROP_CLASS: PCWSTR = w!("ScreenCaptnWelcomeBackdrop");
const CONTROL_COUNT: usize = 9;

#[derive(Clone)]
pub struct WelcomeChoices {
    pub auto_save: bool,
    pub folder: PathBuf,
    pub startup: bool,
    pub diagnostics: bool,
    pub theme: AppTheme,
}

pub enum WelcomeOutcome {
    Finish(WelcomeChoices),
    Skip,
}

#[derive(Clone, Copy)]
struct WelcomeLayout {
    panel: Rect,
    auto_save: Rect,
    folder: Rect,
    startup: Rect,
    diagnostics: Rect,
    light: Rect,
    dark: Rect,
    donate: Rect,
    skip: Rect,
    finish: Rect,
}

struct WelcomeState {
    choices: WelcomeChoices,
    shortcut: String,
    result: Option<WelcomeOutcome>,
    focus: usize,
    scale: f32,
    size: (i32, i32),
    is_initial: bool,
}

pub fn show_first_run(
    owner: HWND,
    settings: &AppSettings,
    initial_theme: AppTheme,
    startup_enabled: bool,
    is_initial: bool,
) -> Result<WelcomeOutcome> {
    unsafe {
        register_classes()?;
        let (work, _) = active_monitor_work_area();
        let scale = monitor_scale(owner) * windows_text_scale();
        let work_width = work.right - work.left;
        let work_height = work.bottom - work.top;
        let (panel_width, panel_height) = panel_size(work_width, work_height, scale);
        let instance = GetModuleHandleW(None)?;

        let backdrop = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            BACKDROP_CLASS,
            w!(""),
            WS_POPUP,
            work.left,
            work.top,
            work_width,
            work_height,
            owner,
            HMENU::default(),
            instance,
            None,
        )?;
        let _ = SetLayeredWindowAttributes(backdrop, colorref(Color::BLACK), 0x8e, LWA_ALPHA);

        let mut state = Box::new(WelcomeState {
            choices: WelcomeChoices {
                auto_save: if is_initial {
                    true
                } else {
                    settings.auto_save.enabled
                },
                folder: if is_initial || settings.auto_save.folder.as_os_str().is_empty() {
                    default_auto_save_folder()
                } else {
                    settings.auto_save.folder.clone()
                },
                startup: if is_initial { true } else { startup_enabled },
                diagnostics: if is_initial {
                    false
                } else {
                    settings.diagnostics.enabled
                },
                theme: initial_theme,
            },
            shortcut: settings.hotkey.display_label(),
            result: None,
            focus: 0,
            scale,
            size: (panel_width, panel_height),
            is_initial,
        });
        let state_ptr = state.as_mut() as *mut WelcomeState;
        let panel = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            PANEL_CLASS,
            w!("Screen Cap'n Setup"),
            WS_POPUP,
            work.left + (work_width - panel_width) / 2,
            work.top + (work_height - panel_height) / 2,
            panel_width,
            panel_height,
            owner,
            HMENU::default(),
            instance,
            Some(state_ptr.cast()),
        )?;
        let actual_scale = GetDpiForWindow(panel).max(96) as f32 / 96.0 * windows_text_scale();
        if (actual_scale - scale).abs() > f32::EPSILON {
            let (width, height) = panel_size(work_width, work_height, actual_scale);
            state.scale = actual_scale;
            state.size = (width, height);
            let _ = SetWindowPos(
                panel,
                None,
                work.left + (work_width - width) / 2,
                work.top + (work_height - height) / 2,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        apply_panel_shape(panel, state.size, state.scale);
        Box::leak(state);

        let _ = ShowWindow(backdrop, SW_SHOW);
        let _ = ShowWindow(panel, SW_SHOW);
        let _ = SetForegroundWindow(panel);

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
            if !IsWindow(panel).as_bool()
                || welcome_state(panel).is_none_or(|state| state.result.is_some())
            {
                break;
            }
        }

        let state_ptr = GetWindowLongPtrW(panel, GWLP_USERDATA) as *mut WelcomeState;
        let result = if state_ptr.is_null() {
            WelcomeOutcome::Skip
        } else {
            let mut state = Box::from_raw(state_ptr);
            SetWindowLongPtrW(panel, GWLP_USERDATA, 0);
            state.result.take().unwrap_or(WelcomeOutcome::Skip)
        };
        if IsWindow(panel).as_bool() {
            let _ = DestroyWindow(panel);
        }
        if IsWindow(backdrop).as_bool() {
            let _ = DestroyWindow(backdrop);
        }
        Ok(result)
    }
}

unsafe fn register_classes() -> Result<()> {
    let instance = GetModuleHandleW(None)?;
    let backdrop = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hInstance: instance.into(),
        lpszClassName: BACKDROP_CLASS,
        lpfnWndProc: Some(backdrop_wnd_proc),
        hbrBackground: CreateSolidBrush(colorref(Color::BLACK)),
        ..Default::default()
    };
    RegisterClassW(&backdrop);
    let panel = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hInstance: instance.into(),
        lpszClassName: PANEL_CLASS,
        lpfnWndProc: Some(welcome_wnd_proc),
        ..Default::default()
    };
    RegisterClassW(&panel);
    Ok(())
}

unsafe extern "system" fn backdrop_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, message, wparam, lparam)
}

unsafe extern "system" fn welcome_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CREATE {
        let create = lparam.0 as *const CREATESTRUCTW;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
        return LRESULT(0);
    }
    let Some(state) = welcome_state(hwnd) else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    match message {
        WM_PAINT => {
            paint_welcome(hwnd, state);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            handle_click(hwnd, state, point_from_lparam(lparam));
            LRESULT(0)
        }
        WM_KEYDOWN => {
            handle_key(hwnd, state, wparam.0 as u32);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            state.scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0 * windows_text_scale();
            let suggested = &*(lparam.0 as *const RECT);
            state.size = (
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                state.size.0,
                state.size.1,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            apply_panel_shape(hwnd, state.size, state.scale);
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_CLOSE => {
            state.result = Some(WelcomeOutcome::Skip);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn welcome_state(hwnd: HWND) -> Option<&'static mut WelcomeState> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WelcomeState;
    (!pointer.is_null()).then(|| &mut *pointer)
}

unsafe fn handle_click(hwnd: HWND, state: &mut WelcomeState, point: screencaptn_core::Point) {
    let layout = welcome_layout(state);
    if layout.auto_save.contains(point) {
        state.choices.auto_save = !state.choices.auto_save;
        state.focus = 0;
    } else if layout.folder.contains(point) {
        if let Some(folder) = choose_folder(hwnd) {
            state.choices.folder = folder;
        }
        state.focus = 1;
    } else if layout.startup.contains(point) {
        state.choices.startup = !state.choices.startup;
        state.focus = 2;
    } else if layout.diagnostics.contains(point) {
        state.choices.diagnostics = !state.choices.diagnostics;
        state.focus = 3;
    } else if layout.light.contains(point) {
        state.choices.theme = AppTheme::Light;
        state.focus = 4;
    } else if layout.dark.contains(point) {
        state.choices.theme = AppTheme::Dark;
        state.focus = 5;
    } else if layout.donate.contains(point) {
        let _ = ShellExecuteW(
            hwnd,
            w!("open"),
            w!("https://screencapn.com/donate"),
            None,
            None,
            SW_SHOWNORMAL,
        );
        state.focus = 6;
    } else if layout.skip.contains(point) {
        state.result = Some(WelcomeOutcome::Skip);
    } else if layout.finish.contains(point) {
        state.result = Some(WelcomeOutcome::Finish(state.choices.clone()));
    }
    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
}

unsafe fn handle_key(hwnd: HWND, state: &mut WelcomeState, key: u32) {
    if key == VK_TAB.0 as u32 {
        let backward = GetKeyState(VK_SHIFT.0 as i32) < 0;
        state.focus = if backward {
            (state.focus + CONTROL_COUNT - 1) % CONTROL_COUNT
        } else {
            (state.focus + 1) % CONTROL_COUNT
        };
    } else if key == VK_SPACE.0 as u32 || key == VK_RETURN.0 as u32 {
        let layout = welcome_layout(state);
        let point = match state.focus {
            0 => layout.auto_save.center(),
            1 => layout.folder.center(),
            2 => layout.startup.center(),
            3 => layout.diagnostics.center(),
            4 => layout.light.center(),
            5 => layout.dark.center(),
            6 => layout.donate.center(),
            7 => layout.skip.center(),
            _ => layout.finish.center(),
        };
        handle_click(hwnd, state, point);
        return;
    } else if key == 0x1B {
        state.result = Some(WelcomeOutcome::Skip);
    }
    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
}

unsafe fn paint_welcome(hwnd: HWND, state: &WelcomeState) {
    let mut paint = PAINTSTRUCT::default();
    let target = BeginPaint(hwnd, &mut paint);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let width = (client.right - client.left).max(1);
    let height = (client.bottom - client.top).max(1);
    let buffer = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, width, height);
    let previous = SelectObject(buffer, bitmap);
    draw_welcome(buffer, state);
    let _ = BitBlt(target, 0, 0, width, height, buffer, 0, 0, SRCCOPY);
    let _ = SelectObject(buffer, previous);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(buffer);
    let _ = EndPaint(hwnd, &paint);
}

unsafe fn draw_welcome(hdc: HDC, state: &WelcomeState) {
    let layout = welcome_layout(state);
    let palette = toolbar_palette(state.choices.theme);
    let full = RECT {
        left: 0,
        top: 0,
        right: state.size.0,
        bottom: state.size.1,
    };
    let background = CreateSolidBrush(colorref(palette.background));
    let _ = FillRect(hdc, &full, background);
    let _ = DeleteObject(background);
    draw_text(
        hdc,
        Rect::new(
            layout.panel.x + 36.0 * state.scale,
            layout.panel.y + 28.0 * state.scale,
            layout.panel.width - 132.0 * state.scale,
            34.0 * state.scale,
        ),
        if state.is_initial {
            "Screen Cap'n is ready"
        } else {
            "Screen Cap'n setup"
        },
        24.0 * state.scale,
        palette.icon,
        true,
        DT_LEFT,
    );
    let _ = draw_svg(
        hdc,
        include_str!("../assets/app-icon/screencapn-icon.svg"),
        Rect::new(
            layout.panel.right() - 76.0 * state.scale,
            layout.panel.y + 24.0 * state.scale,
            42.0 * state.scale,
            42.0 * state.scale,
        ),
    );
    draw_text(
        hdc,
        Rect::new(
            layout.panel.x + 36.0 * state.scale,
            layout.panel.y + 66.0 * state.scale,
            layout.panel.width - 72.0 * state.scale,
            46.0 * state.scale,
        ),
        &format!(
            "Version {}.0 is running in the tray. Capture anytime with {}.",
            env!("CARGO_PKG_VERSION"),
            state.shortcut
        ),
        13.0 * state.scale,
        muted(state.choices.theme),
        false,
        DT_LEFT,
    );

    draw_toggle_row(
        hdc,
        state,
        layout.auto_save,
        "Auto-save screenshots",
        state.choices.auto_save,
        0,
    );
    draw_action_row(
        hdc,
        state,
        layout.folder,
        "Save folder",
        &state.choices.folder.to_string_lossy(),
        1,
    );
    draw_toggle_row(
        hdc,
        state,
        layout.startup,
        "Run on startup",
        state.choices.startup,
        2,
    );
    draw_toggle_row(
        hdc,
        state,
        layout.diagnostics,
        "Help improve Screen Cap'n with local diagnostics",
        state.choices.diagnostics,
        3,
    );
    draw_theme_choice(hdc, state, layout.light, "Light", AppTheme::Light, 4);
    draw_theme_choice(hdc, state, layout.dark, "Dark", AppTheme::Dark, 5);
    draw_link(hdc, state, layout.donate, "Keep the ship flying: donate", 6);
    draw_button(
        hdc,
        state,
        layout.skip,
        if state.is_initial { "Skip" } else { "Cancel" },
        false,
        7,
    );
    draw_button(
        hdc,
        state,
        layout.finish,
        if state.is_initial {
            "Finish setup"
        } else {
            "Save setup"
        },
        true,
        8,
    );
}

fn welcome_layout(state: &WelcomeState) -> WelcomeLayout {
    let scale = state.scale;
    let panel = Rect::new(0.0, 0.0, state.size.0 as f32, state.size.1 as f32);
    let x = panel.x + 36.0 * scale;
    let width = panel.width - 72.0 * scale;
    let row_height = 44.0 * scale;
    let mut y = panel.y + 126.0 * scale;
    let auto_save = Rect::new(x, y, width, row_height);
    y += 50.0 * scale;
    let folder = Rect::new(x, y, width, row_height);
    y += 50.0 * scale;
    let startup = Rect::new(x, y, width, row_height);
    y += 50.0 * scale;
    let diagnostics = Rect::new(x, y, width, row_height);
    y += 56.0 * scale;
    let light = Rect::new(x, y, 112.0 * scale, 38.0 * scale);
    let dark = Rect::new(x + 120.0 * scale, y, 112.0 * scale, 38.0 * scale);
    let donate = Rect::new(x + 250.0 * scale, y, width - 250.0 * scale, 38.0 * scale);
    let button_y = panel.bottom() - 58.0 * scale;
    let finish = Rect::new(
        panel.right() - 168.0 * scale,
        button_y,
        132.0 * scale,
        38.0 * scale,
    );
    let skip = Rect::new(
        finish.x - 92.0 * scale,
        button_y,
        80.0 * scale,
        38.0 * scale,
    );
    WelcomeLayout {
        panel,
        auto_save,
        folder,
        startup,
        diagnostics,
        light,
        dark,
        donate,
        skip,
        finish,
    }
}

unsafe fn draw_toggle_row(
    hdc: HDC,
    state: &WelcomeState,
    rect: Rect,
    label: &str,
    checked: bool,
    focus: usize,
) {
    draw_focus(hdc, state, rect, focus);
    let palette = toolbar_palette(state.choices.theme);
    let control = Rect::new(
        rect.x + 12.0 * state.scale,
        rect.y + 13.0 * state.scale,
        18.0 * state.scale,
        18.0 * state.scale,
    );
    rounded_fill(
        hdc,
        control,
        4.0 * state.scale,
        if checked {
            palette.accent
        } else {
            palette.selected_icon_background
        },
    );
    if checked {
        let _pen = SelectedPen::new(hdc, (2.0 * state.scale).max(1.0), Color::WHITE);
        let _ = MoveToEx(
            hdc,
            (control.x + 4.0 * state.scale).round() as i32,
            (control.y + 9.5 * state.scale).round() as i32,
            None,
        );
        let _ = LineTo(
            hdc,
            (control.x + 7.5 * state.scale).round() as i32,
            (control.y + 13.0 * state.scale).round() as i32,
        );
        let _ = LineTo(
            hdc,
            (control.x + 14.5 * state.scale).round() as i32,
            (control.y + 5.5 * state.scale).round() as i32,
        );
    }
    draw_text(
        hdc,
        Rect::new(
            rect.x + 42.0 * state.scale,
            rect.y,
            rect.width - 50.0 * state.scale,
            rect.height,
        ),
        label,
        13.0 * state.scale,
        palette.icon,
        false,
        DT_LEFT,
    );
}

unsafe fn draw_action_row(
    hdc: HDC,
    state: &WelcomeState,
    rect: Rect,
    label: &str,
    value: &str,
    focus: usize,
) {
    draw_focus(hdc, state, rect, focus);
    let palette = toolbar_palette(state.choices.theme);
    draw_text(
        hdc,
        Rect::new(
            rect.x + 12.0 * state.scale,
            rect.y,
            92.0 * state.scale,
            rect.height,
        ),
        label,
        12.0 * state.scale,
        muted(state.choices.theme),
        true,
        DT_LEFT,
    );
    draw_text(
        hdc,
        Rect::new(
            rect.x + 106.0 * state.scale,
            rect.y,
            rect.width - 118.0 * state.scale,
            rect.height,
        ),
        value,
        12.0 * state.scale,
        palette.icon,
        false,
        DT_LEFT,
    );
}

unsafe fn draw_theme_choice(
    hdc: HDC,
    state: &WelcomeState,
    rect: Rect,
    label: &str,
    theme: AppTheme,
    focus: usize,
) {
    draw_focus(hdc, state, rect, focus);
    let palette = toolbar_palette(state.choices.theme);
    rounded_fill(
        hdc,
        rect,
        6.0 * state.scale,
        if state.choices.theme == theme {
            palette.accent
        } else {
            palette.selected_icon_background
        },
    );
    draw_text(
        hdc,
        rect,
        label,
        12.0 * state.scale,
        if state.choices.theme == theme {
            Color::WHITE
        } else {
            palette.icon
        },
        true,
        DT_CENTER,
    );
}

unsafe fn draw_link(hdc: HDC, state: &WelcomeState, rect: Rect, label: &str, focus: usize) {
    draw_focus(hdc, state, rect, focus);
    draw_text(
        hdc,
        rect,
        label,
        11.0 * state.scale,
        toolbar_palette(state.choices.theme).accent,
        false,
        DT_CENTER,
    );
}

unsafe fn draw_button(
    hdc: HDC,
    state: &WelcomeState,
    rect: Rect,
    label: &str,
    primary: bool,
    focus: usize,
) {
    draw_focus(hdc, state, rect, focus);
    let palette = toolbar_palette(state.choices.theme);
    rounded_fill(
        hdc,
        rect,
        6.0 * state.scale,
        if primary {
            palette.accent
        } else {
            palette.selected_icon_background
        },
    );
    draw_text(
        hdc,
        rect,
        label,
        12.0 * state.scale,
        if primary { Color::WHITE } else { palette.icon },
        true,
        DT_CENTER,
    );
}

unsafe fn draw_focus(hdc: HDC, state: &WelcomeState, rect: Rect, focus: usize) {
    if state.focus == focus {
        rounded_fill(
            hdc,
            Rect::new(
                rect.x - 2.0,
                rect.y - 2.0,
                rect.width + 4.0,
                rect.height + 4.0,
            ),
            7.0 * state.scale,
            match state.choices.theme {
                AppTheme::Light => Color::rgb(0xff, 0xe9, 0xe7),
                AppTheme::Dark => Color::rgb(0x4c, 0x25, 0x22),
            },
        );
    }
}

fn panel_size(work_width: i32, work_height: i32, scale: f32) -> (i32, i32) {
    let width = (620.0 * scale)
        .round()
        .min((work_width as f32 - 32.0 * scale).max(320.0 * scale)) as i32;
    let height = (486.0 * scale)
        .round()
        .min((work_height as f32 - 32.0 * scale).max(360.0 * scale)) as i32;
    (width, height)
}

unsafe fn apply_panel_shape(hwnd: HWND, size: (i32, i32), scale: f32) {
    let radius = (20.0 * scale).round().max(1.0) as i32;
    let region = CreateRoundRectRgn(0, 0, size.0 + 1, size.1 + 1, radius, radius);
    if region.0.is_null() {
        return;
    }
    if SetWindowRgn(hwnd, region, true) == 0 {
        let _ = DeleteObject(region);
    }
}

unsafe fn rounded_fill(hdc: HDC, rect: Rect, radius: f32, color: Color) {
    let brush = CreateSolidBrush(colorref(color));
    let old = SelectObject(hdc, brush);
    let _ = RoundRect(
        hdc,
        rect.x.round() as i32,
        rect.y.round() as i32,
        rect.right().round() as i32,
        rect.bottom().round() as i32,
        radius.round() as i32,
        radius.round() as i32,
    );
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(brush);
}

unsafe fn draw_text(
    hdc: HDC,
    rect: Rect,
    text: &str,
    size: f32,
    color: Color,
    bold: bool,
    alignment: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let font = CreateFontW(
        -size.round().max(1.0) as i32,
        0,
        0,
        0,
        if bold { 600 } else { 400 },
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        w!("Segoe UI Variable"),
    );
    let old = SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(color));
    let mut native = RECT {
        left: rect.x.round() as i32,
        top: rect.y.round() as i32,
        right: rect.right().round() as i32,
        bottom: rect.bottom().round() as i32,
    };
    let mut wide = text.encode_utf16().collect::<Vec<_>>();
    let _ = DrawTextW(
        hdc,
        &mut wide,
        &mut native,
        alignment | DT_VCENTER | DT_SINGLELINE,
    );
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

unsafe fn choose_folder(owner: HWND) -> Option<PathBuf> {
    let dialog: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let _ = dialog.SetTitle(w!("Choose Screen Cap'n auto-save folder"));
    let options = dialog.GetOptions().ok()?;
    let _ = dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
    dialog.Show(owner).ok()?;
    let item = dialog.GetResult().ok()?;
    let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let result = path.to_string().ok().map(PathBuf::from);
    CoTaskMemFree(Some(path.as_ptr().cast()));
    result
}

fn muted(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x72, 0x72, 0x72),
        AppTheme::Dark => Color::rgb(0x94, 0x94, 0x94),
    }
}

unsafe fn active_monitor_work_area() -> (RECT, windows::Win32::Graphics::Gdi::HMONITOR) {
    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    (info.rcWork, monitor)
}

unsafe fn monitor_scale(owner: HWND) -> f32 {
    GetDpiForWindow(owner).max(96) as f32 / 96.0
}

fn windows_text_scale() -> f32 {
    UISettings::new()
        .and_then(|settings| settings.TextScaleFactor())
        .map(|value| value as f32)
        .unwrap_or(1.0)
        .clamp(1.0, 2.25)
}
