use crate::hotkey::reserved_hotkey_reason;
use crate::native_svg::{draw_svg, recolor_svg};
use crate::settings::HotkeySettings;
use crate::theme::{toolbar_palette, AppTheme, ToolbarPalette};
use crate::util::{colorref, rect_to_rect, SelectedStockObject};
use screencaptn_core::{Color, Point, Rect};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetDC, GetMonitorInfoW, GetTextExtentPoint32W, InvalidateRect, MonitorFromPoint,
    MonitorFromWindow, ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, FW_BOLD, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CONTROL, VK_LWIN,
    VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, IsWindow, KillTimer, LoadCursorW, RegisterClassW, SetCursor,
    SetForegroundWindow, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, HMENU, HWND_TOPMOST, IDC_ARROW,
    IDC_HAND, LWA_ALPHA, MSG, SWP_NOACTIVATE, SW_SHOW, WM_CLOSE, WM_CREATE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_MOUSEMOVE,
    WM_PAINT, WM_SETTINGCHANGE, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::UI::ViewManagement::UISettings;

const PANEL_CLASS: PCWSTR = w!("ScreenCaptnShortcutPanel");
const BACKDROP_CLASS: PCWSTR = w!("ScreenCaptnShortcutBackdrop");
const REFERENCE_WIDTH_DIP: f32 = 736.0;
const REFERENCE_HEIGHT_DIP: f32 = 333.0;
const WORK_AREA_MARGIN_DIP: f32 = 16.0;
const PANEL_MARGIN_DIP: f32 = 30.0;
const ACTION_ICON_DIP: f32 = 46.0;
const ACTION_GAP_DIP: f32 = 12.0;
const LOGO_DIP: f32 = 44.0;
const TITLE_FONT_DIP: f32 = 21.0;
const SUBTITLE_FONT_DIP: f32 = 13.0;
const KEY_FONT_DIP: f32 = 14.0;
const KEY_HEIGHT_DIP: f32 = 44.0;
const STATUS_FONT_DIP: f32 = 12.0;
const TOOLTIP_TIMER_ID: usize = 1;
const TOOLTIP_DELAY_MS: u32 = 2_000;
const WM_MOUSELEAVE_MESSAGE: u32 = 0x02A3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShortcutAction {
    Close,
    Reset,
    Record,
    Save,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecorderMode {
    Viewing,
    Recording,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhysicalKey {
    virtual_key: u32,
    scan_code: u16,
    extended: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedPress {
    key: PhysicalKey,
    label: String,
    order: usize,
}

struct ShortcutWindowState {
    saved: HotkeySettings,
    candidate: Option<HotkeySettings>,
    result: Option<Option<HotkeySettings>>,
    validation_message: Option<String>,
    theme: AppTheme,
    mode: RecorderMode,
    hovered: Option<ShortcutAction>,
    tooltip_action: Option<ShortcutAction>,
    focused: ShortcutAction,
    held_keys: Vec<PhysicalKey>,
    recorded_presses: Vec<RecordedPress>,
    trigger_count: usize,
    tracking_mouse: bool,
    dpi_scale: f32,
    text_scale: f32,
}

#[derive(Clone, Copy)]
struct ShortcutLayout {
    client: Rect,
    title: Rect,
    subtitle: Rect,
    logo: Rect,
    timeline: Rect,
    status: Rect,
    close: Rect,
    reset: Rect,
    record: Rect,
    save: Rect,
    dpi_scale: f32,
    text_scale: f32,
    spacing_scale: f32,
}

impl ShortcutLayout {
    fn action_at(self, point: Point) -> Option<ShortcutAction> {
        [
            (ShortcutAction::Close, self.close),
            (ShortcutAction::Reset, self.reset),
            (ShortcutAction::Record, self.record),
            (ShortcutAction::Save, self.save),
        ]
        .into_iter()
        .find_map(|(action, rect)| rect.contains(point).then_some(action))
    }
}

struct DisplayTile {
    key: PhysicalKey,
    label: String,
}

#[derive(Clone)]
struct TileRow {
    indexes: Vec<usize>,
    width: f32,
}

pub fn edit_hotkey(
    owner: HWND,
    initial: HotkeySettings,
    theme: AppTheme,
) -> Result<Option<HotkeySettings>> {
    unsafe {
        register_shortcut_classes()?;

        let monitor = monitor_info_under_cursor();
        let instance = GetModuleHandleW(None)?;
        let backdrop = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            BACKDROP_CLASS,
            w!(""),
            WS_POPUP,
            monitor.rcMonitor.left,
            monitor.rcMonitor.top,
            monitor.rcMonitor.right - monitor.rcMonitor.left,
            monitor.rcMonitor.bottom - monitor.rcMonitor.top,
            owner,
            HMENU::default(),
            instance,
            None,
        )?;
        let _ = SetLayeredWindowAttributes(backdrop, COLORREF(0), 142, LWA_ALPHA);

        let dpi_scale = GetDpiForWindow(backdrop).max(96) as f32 / 96.0;
        let text_scale = windows_text_scale();
        let (panel_width, panel_height) =
            desired_panel_size(backdrop, &initial, dpi_scale, text_scale, monitor.rcWork);
        let panel_x = monitor.rcWork.left
            + ((monitor.rcWork.right - monitor.rcWork.left - panel_width) / 2).max(0);
        let panel_y = monitor.rcWork.top
            + ((monitor.rcWork.bottom - monitor.rcWork.top - panel_height) / 2).max(0);

        let mut state = Box::new(ShortcutWindowState {
            saved: initial,
            candidate: None,
            result: None,
            validation_message: None,
            theme,
            mode: RecorderMode::Viewing,
            hovered: None,
            tooltip_action: None,
            focused: ShortcutAction::Record,
            held_keys: Vec::new(),
            recorded_presses: Vec::new(),
            trigger_count: 0,
            tracking_mouse: false,
            dpi_scale,
            text_scale,
        });
        let state_ptr = state.as_mut() as *mut ShortcutWindowState;
        let panel = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            PANEL_CLASS,
            w!("Quick access shortcut"),
            WS_POPUP,
            panel_x,
            panel_y,
            panel_width,
            panel_height,
            backdrop,
            HMENU::default(),
            instance,
            Some(state_ptr.cast()),
        )?;
        Box::leak(state);
        SetWindowLongPtrW(backdrop, GWLP_USERDATA, panel.0 as isize);
        apply_panel_region(panel, panel_width, panel_height, dpi_scale, text_scale);

        let _ = ShowWindow(backdrop, SW_SHOW);
        let _ = ShowWindow(panel, SW_SHOW);
        let _ = SetForegroundWindow(panel);
        let _ = SetFocus(panel);

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
            if !IsWindow(panel).as_bool()
                || shortcut_state(panel).is_none_or(|state| state.result.is_some())
            {
                break;
            }
        }

        let state_ptr = GetWindowLongPtrW(panel, GWLP_USERDATA) as *mut ShortcutWindowState;
        if state_ptr.is_null() {
            let _ = DestroyWindow(backdrop);
            return Ok(None);
        }
        let mut state = Box::from_raw(state_ptr);
        SetWindowLongPtrW(panel, GWLP_USERDATA, 0);
        let result = state.result.take().unwrap_or(None);
        if IsWindow(panel).as_bool() {
            let _ = DestroyWindow(panel);
        }
        if IsWindow(backdrop).as_bool() {
            let _ = DestroyWindow(backdrop);
        }
        Ok(result)
    }
}

unsafe fn register_shortcut_classes() -> Result<()> {
    let instance = GetModuleHandleW(None)?;
    let panel_class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hInstance: instance.into(),
        lpszClassName: PANEL_CLASS,
        lpfnWndProc: Some(shortcut_wnd_proc),
        ..Default::default()
    };
    let backdrop_class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hInstance: instance.into(),
        lpszClassName: BACKDROP_CLASS,
        lpfnWndProc: Some(backdrop_wnd_proc),
        ..Default::default()
    };
    RegisterClassW(&panel_class);
    RegisterClassW(&backdrop_class);
    Ok(())
}

unsafe extern "system" fn backdrop_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            let panel = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut _);
            if !panel.0.is_null() {
                if let Some(state) = shortcut_state(panel) {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint);
            let brush = CreateSolidBrush(COLORREF(0));
            let _ = FillRect(hdc, &paint.rcPaint, brush);
            let _ = DeleteObject(brush);
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn shortcut_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint_shortcut_window(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let point = point_from_lparam(lparam);
            if let Some(state) = shortcut_state(hwnd) {
                if !state.tracking_mouse {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if TrackMouseEvent(&mut tracking).is_ok() {
                        state.tracking_mouse = true;
                    }
                }
                let layout = shortcut_layout(hwnd, state);
                let hovered = interactive_action_at(state, layout, point);
                if state.hovered != hovered {
                    clear_tooltip_timer(hwnd, state);
                    state.hovered = hovered;
                    if hovered.is_some() {
                        let _ = SetTimer(hwnd, TOOLTIP_TIMER_ID, TOOLTIP_DELAY_MS, None);
                    }
                    invalidate(hwnd);
                }
                set_pointer(hovered.is_some());
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE_MESSAGE => {
            if let Some(state) = shortcut_state(hwnd) {
                clear_tooltip_timer(hwnd, state);
                state.hovered = None;
                state.tracking_mouse = false;
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TOOLTIP_TIMER_ID => {
            if let Some(state) = shortcut_state(hwnd) {
                let _ = KillTimer(hwnd, TOOLTIP_TIMER_ID);
                state.tooltip_action = state.hovered;
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let point = point_from_lparam(lparam);
            if let Some(state) = shortcut_state(hwnd) {
                let layout = shortcut_layout(hwnd, state);
                if let Some(action) = interactive_action_at(state, layout, point) {
                    clear_tooltip_timer(hwnd, state);
                    state.focused = action;
                    activate_action(state, action);
                    invalidate(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(state) = shortcut_state(hwnd) {
                let key = wparam.0 as u32;
                if state.mode == RecorderMode::Recording {
                    if key == 0x1B {
                        cancel_recording(state);
                    } else {
                        record_key_down(state, physical_key(key, lparam));
                    }
                } else if key == 0x1B {
                    state.result = Some(None);
                } else if key == 0x09 {
                    state.focused = if shift_down() {
                        previous_action(state.focused)
                    } else {
                        next_action(state.focused)
                    };
                } else if key == VK_RETURN.0 as u32 || key == VK_SPACE.0 as u32 {
                    activate_action(state, state.focused);
                }
                clear_tooltip_timer(hwnd, state);
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if let Some(state) = shortcut_state(hwnd) {
                if state.mode == RecorderMode::Recording {
                    record_key_up(state, physical_key(wparam.0 as u32, lparam));
                    invalidate(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED | WM_SETTINGCHANGE => {
            if let Some(state) = shortcut_state(hwnd) {
                resize_panel_for_settings(hwnd, state);
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            if let Some(state) = shortcut_state(hwnd) {
                state.result = Some(None);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(state) = shortcut_state(hwnd) {
                if state.mode == RecorderMode::Recording {
                    cancel_recording(state);
                    invalidate(hwnd);
                } else {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(state) = shortcut_state(hwnd) {
                let _ = KillTimer(hwnd, TOOLTIP_TIMER_ID);
                if state.result.is_none() {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn shortcut_state(hwnd: HWND) -> Option<&'static mut ShortcutWindowState> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ShortcutWindowState;
    (!pointer.is_null()).then(|| &mut *pointer)
}

unsafe fn paint_shortcut_window(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let target = BeginPaint(hwnd, &mut paint);
    let mut client = RECT::default();
    if GetClientRect(hwnd, &mut client).is_ok() {
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);
        let memory = CreateCompatibleDC(target);
        let bitmap = CreateCompatibleBitmap(target, width, height);
        if !bitmap.is_invalid() {
            let previous = SelectObject(memory, bitmap);
            if let Some(state) = shortcut_state(hwnd) {
                draw_shortcut_window(hwnd, memory, state);
            }
            let _ = BitBlt(target, 0, 0, width, height, memory, 0, 0, SRCCOPY);
            let _ = SelectObject(memory, previous);
            let _ = DeleteObject(bitmap);
        }
        let _ = DeleteDC(memory);
    }
    let _ = EndPaint(hwnd, &paint);
}

unsafe fn shortcut_layout(hwnd: HWND, state: &ShortcutWindowState) -> ShortcutLayout {
    let mut native = RECT::default();
    let _ = GetClientRect(hwnd, &mut native);
    let client = Rect::new(
        0.0,
        0.0,
        (native.right - native.left).max(1) as f32,
        (native.bottom - native.top).max(1) as f32,
    );
    let dpi = state.dpi_scale;
    let text = state.text_scale;
    let spacing = dpi * (1.0 + (text - 1.0) * 0.55);
    let font = dpi * text;
    let control = dpi * (1.0 + (text - 1.0) * 0.8);
    let margin = PANEL_MARGIN_DIP * spacing;
    let title_height = 33.0 * font;
    let subtitle_height = 22.0 * font;
    let icon_size = ACTION_ICON_DIP * control;
    let logo_size = LOGO_DIP * control;
    let title_y = 23.0 * spacing;
    let title = Rect::new(
        margin,
        title_y,
        (client.width - margin * 2.0 - logo_size - 14.0 * spacing).max(1.0),
        title_height,
    );
    let logo = Rect::new(
        client.right() - margin - logo_size,
        title_y,
        logo_size,
        logo_size,
    );
    let subtitle = Rect::new(
        margin,
        title.bottom() + 2.0 * spacing,
        client.width - margin * 2.0,
        subtitle_height,
    );
    let button_gap = ACTION_GAP_DIP * spacing;
    let action_y = client.bottom() - margin - icon_size;
    let save = Rect::new(
        client.right() - margin - icon_size,
        action_y,
        icon_size,
        icon_size,
    );
    let record = Rect::new(
        save.x - button_gap - icon_size,
        action_y,
        icon_size,
        icon_size,
    );
    let reset = Rect::new(
        record.x - button_gap - icon_size,
        action_y,
        icon_size,
        icon_size,
    );
    let close = Rect::new(
        reset.x - button_gap - icon_size,
        action_y,
        icon_size,
        icon_size,
    );
    let status_height = 22.0 * font;
    let status = Rect::new(
        margin,
        action_y - 8.0 * spacing - status_height,
        (close.x - margin - 14.0 * spacing).max(client.width * 0.38),
        status_height,
    );
    let timeline_y = subtitle.bottom() + 17.0 * spacing;
    let timeline_bottom = (status.y - 16.0 * spacing).max(timeline_y + 54.0 * font);
    let timeline = Rect::new(
        margin,
        timeline_y,
        client.width - margin * 2.0,
        timeline_bottom - timeline_y,
    );
    ShortcutLayout {
        client,
        title,
        subtitle,
        logo,
        timeline,
        status,
        close,
        reset,
        record,
        save,
        dpi_scale: dpi,
        text_scale: text,
        spacing_scale: spacing,
    }
}

unsafe fn draw_shortcut_window(hwnd: HWND, hdc: HDC, state: &ShortcutWindowState) {
    let layout = shortcut_layout(hwnd, state);
    let palette = shortcut_palette(state.theme);
    fill(hdc, layout.client, palette.background);
    draw_panel_border(hdc, layout.client, palette);

    let inactive_alpha = if state.mode == RecorderMode::Recording {
        0.30
    } else {
        1.0
    };
    let title_color = fade_to_background(palette.icon, palette.background, inactive_alpha);
    let muted = fade_to_background(muted_text(state.theme), palette.background, inactive_alpha);
    draw_text(
        hdc,
        layout.title,
        "Quick access shortcut",
        TITLE_FONT_DIP * layout.dpi_scale * layout.text_scale,
        title_color,
        true,
        false,
    );
    draw_text(
        hdc,
        layout.subtitle,
        subtitle_text(state.mode),
        SUBTITLE_FONT_DIP * layout.dpi_scale * layout.text_scale,
        muted,
        false,
        false,
    );
    let logo_svg = include_str!("../assets/app-icon/screencapn-icon.svg");
    let _ = draw_svg(hdc, logo_svg, layout.logo);

    draw_shortcut_tiles(hdc, state, layout, palette);

    let status = shortcut_status(state);
    draw_text(
        hdc,
        layout.status,
        &status.0,
        STATUS_FONT_DIP * layout.dpi_scale * layout.text_scale,
        status.1,
        false,
        false,
    );

    for (action, rect) in [
        (ShortcutAction::Close, layout.close),
        (ShortcutAction::Reset, layout.reset),
        (ShortcutAction::Record, layout.record),
        (ShortcutAction::Save, layout.save),
    ] {
        draw_action_icon(hdc, state, action, rect, palette);
    }
    if let Some(action) = state.tooltip_action {
        draw_action_tooltip(hdc, state, action, layout, palette);
    }
}

unsafe fn draw_shortcut_tiles(
    hdc: HDC,
    state: &ShortcutWindowState,
    layout: ShortcutLayout,
    palette: ToolbarPalette,
) {
    let tiles = display_tiles(state);
    if state.mode == RecorderMode::Recording && tiles.is_empty() {
        draw_text(
            hdc,
            layout.timeline,
            "Press your shortcut keys",
            13.0 * layout.dpi_scale * layout.text_scale,
            muted_text(state.theme),
            false,
            true,
        );
        return;
    }
    if tiles.is_empty() {
        return;
    }

    let font_size = KEY_FONT_DIP * layout.dpi_scale * layout.text_scale;
    let horizontal_padding = 17.0 * layout.spacing_scale;
    let minimum_width = 58.0 * layout.spacing_scale;
    let plus_width = measure_text(hdc, "+", font_size, true).0 + 14.0 * layout.spacing_scale;
    let widths: Vec<f32> = tiles
        .iter()
        .map(|tile| {
            (measure_text(hdc, &tile.label, font_size, true).0 + horizontal_padding * 2.0)
                .max(minimum_width)
        })
        .collect();
    let available = (layout.timeline.width - 20.0 * layout.spacing_scale).max(minimum_width);
    let rows = pack_tile_rows(&widths, plus_width, available);
    let tile_height = KEY_HEIGHT_DIP * layout.dpi_scale * layout.text_scale;
    let row_gap = 9.0 * layout.spacing_scale;
    let total_height =
        tile_height * rows.len() as f32 + row_gap * rows.len().saturating_sub(1) as f32;
    let mut y = layout.timeline.center().y - total_height / 2.0;
    for row in rows {
        let mut x = layout.timeline.center().x - row.width / 2.0;
        for (position, index) in row.indexes.iter().enumerate() {
            let tile = &tiles[*index];
            let width = widths[*index];
            let rect = Rect::new(x, y, width, tile_height);
            let pressed = state.held_keys.contains(&tile.key);
            draw_surface(
                hdc,
                rect,
                5.0 * layout.spacing_scale,
                key_background(state.theme),
                if pressed {
                    palette.accent
                } else {
                    key_background(state.theme)
                },
            );
            draw_text(hdc, rect, &tile.label, font_size, Color::WHITE, true, true);
            x += width;
            if position + 1 < row.indexes.len() {
                draw_text(
                    hdc,
                    Rect::new(x, y, plus_width, tile_height),
                    "+",
                    font_size,
                    palette.icon,
                    true,
                    true,
                );
                x += plus_width;
            }
        }
        y += tile_height + row_gap;
    }
}

unsafe fn draw_action_icon(
    hdc: HDC,
    state: &ShortcutWindowState,
    action: ShortcutAction,
    rect: Rect,
    palette: ToolbarPalette,
) {
    let recording = state.mode == RecorderMode::Recording;
    let enabled = if recording {
        action == ShortcutAction::Record
    } else {
        action != ShortcutAction::Save || save_enabled(state)
    };
    let active = enabled && (state.hovered == Some(action) || state.focused == action);
    if active && !(recording && action == ShortcutAction::Record) {
        draw_surface(
            hdc,
            rect,
            5.0 * rect.height / 32.0,
            palette.selected_icon_background,
            palette.selected_icon_background,
        );
    }
    if recording && action == ShortcutAction::Record {
        let stop_size = rect.height * 0.38;
        let stop = Rect::new(
            rect.center().x - stop_size / 2.0,
            rect.center().y - stop_size / 2.0,
            stop_size,
            stop_size,
        );
        fill(hdc, stop, palette.accent);
        return;
    }
    let base_color = if enabled {
        palette.icon
    } else {
        disabled_text(state.theme)
    };
    let color = if recording {
        fade_to_background(base_color, palette.background, 0.30)
    } else {
        base_color
    };
    let source = match action {
        ShortcutAction::Close => include_str!("../assets/toolbar/cancel.svg"),
        ShortcutAction::Reset => include_str!("../assets/shortcut/reset.svg"),
        ShortcutAction::Record => include_str!("../assets/shortcut/record.svg"),
        ShortcutAction::Save => include_str!("../assets/toolbar/save.svg"),
    };
    let source = recolor_svg(source, color);
    let _ = draw_svg(hdc, &source, inset_rect(rect, rect.height * 0.20));
}

unsafe fn draw_action_tooltip(
    hdc: HDC,
    state: &ShortcutWindowState,
    action: ShortcutAction,
    layout: ShortcutLayout,
    palette: ToolbarPalette,
) {
    let label = action_label(state, action);
    let font_size = 11.0 * layout.dpi_scale * layout.text_scale;
    let measured = measure_text(hdc, label, font_size, false);
    let width = measured.0 + 20.0 * layout.spacing_scale;
    let height =
        measured.1.max(18.0 * layout.dpi_scale * layout.text_scale) + 8.0 * layout.spacing_scale;
    let action_rect = action_rect(layout, action);
    let x = (action_rect.center().x - width / 2.0)
        .max(8.0 * layout.spacing_scale)
        .min(layout.client.right() - width - 8.0 * layout.spacing_scale);
    let rect = Rect::new(
        x,
        action_rect.y - height - 7.0 * layout.spacing_scale,
        width,
        height,
    );
    draw_surface(
        hdc,
        rect,
        4.0 * layout.spacing_scale,
        palette.icon,
        palette.icon,
    );
    draw_text(hdc, rect, label, font_size, palette.background, false, true);
}

fn display_tiles(state: &ShortcutWindowState) -> Vec<DisplayTile> {
    if !state.recorded_presses.is_empty() {
        return state
            .recorded_presses
            .iter()
            .map(|press| DisplayTile {
                key: press.key,
                label: press.label.clone(),
            })
            .collect();
    }
    let hotkey = state.candidate.as_ref().unwrap_or(&state.saved);
    chord_keys(hotkey)
        .into_iter()
        .map(|(key, label)| DisplayTile { key, label })
        .collect()
}

fn pack_tile_rows(widths: &[f32], separator: f32, available: f32) -> Vec<TileRow> {
    let total = widths.iter().sum::<f32>() + separator * widths.len().saturating_sub(1) as f32;
    if total <= available || widths.len() <= 1 {
        return vec![TileRow {
            indexes: (0..widths.len()).collect(),
            width: total,
        }];
    }
    let mut best_split = 1;
    let mut best_balance = f32::MAX;
    for split in 1..widths.len() {
        let first = row_width(&widths[..split], separator);
        let second = row_width(&widths[split..], separator);
        let overflow = (first - available).max(0.0) + (second - available).max(0.0);
        let balance = (first - second).abs() + overflow * 10.0;
        if balance < best_balance {
            best_balance = balance;
            best_split = split;
        }
    }
    vec![
        TileRow {
            indexes: (0..best_split).collect(),
            width: row_width(&widths[..best_split], separator),
        },
        TileRow {
            indexes: (best_split..widths.len()).collect(),
            width: row_width(&widths[best_split..], separator),
        },
    ]
}

fn row_width(widths: &[f32], separator: f32) -> f32 {
    widths.iter().sum::<f32>() + separator * widths.len().saturating_sub(1) as f32
}

fn subtitle_text(mode: RecorderMode) -> &'static str {
    match mode {
        RecorderMode::Viewing => "Current shortcut",
        RecorderMode::Recording => "Press your shortcut, then stop recording",
        RecorderMode::Ready => "Review your shortcut before saving",
    }
}

fn shortcut_status(state: &ShortcutWindowState) -> (String, Color) {
    if let Some(message) = &state.validation_message {
        return (message.clone(), toolbar_palette(state.theme).accent);
    }
    match state.mode {
        RecorderMode::Viewing => (String::new(), muted_text(state.theme)),
        RecorderMode::Recording => (
            "Recording one press per key".to_string(),
            muted_text(state.theme),
        ),
        RecorderMode::Ready => (
            "Shortcut ready to save".to_string(),
            success_text(state.theme),
        ),
    }
}

fn interactive_action_at(
    state: &ShortcutWindowState,
    layout: ShortcutLayout,
    point: Point,
) -> Option<ShortcutAction> {
    let action = layout.action_at(point)?;
    if state.mode == RecorderMode::Recording && action != ShortcutAction::Record {
        return None;
    }
    if action == ShortcutAction::Save && !save_enabled(state) {
        return None;
    }
    Some(action)
}

fn action_rect(layout: ShortcutLayout, action: ShortcutAction) -> Rect {
    match action {
        ShortcutAction::Close => layout.close,
        ShortcutAction::Reset => layout.reset,
        ShortcutAction::Record => layout.record,
        ShortcutAction::Save => layout.save,
    }
}

fn activate_action(state: &mut ShortcutWindowState, action: ShortcutAction) {
    if state.mode == RecorderMode::Recording && action != ShortcutAction::Record {
        return;
    }
    match action {
        ShortcutAction::Close => state.result = Some(None),
        ShortcutAction::Reset => {
            state.candidate = Some(HotkeySettings::default());
            state.mode = RecorderMode::Ready;
            state.recorded_presses.clear();
            state.held_keys.clear();
            state.trigger_count = 0;
            state.validation_message = state.candidate.as_ref().and_then(hotkey_validation_message);
        }
        ShortcutAction::Record if state.mode == RecorderMode::Recording => stop_recording(state),
        ShortcutAction::Record => start_recording(state),
        ShortcutAction::Save => {
            if let Some(candidate) = state.candidate.as_ref().or(Some(&state.saved)) {
                state.validation_message = hotkey_validation_message(candidate);
                if state.validation_message.is_none() && state.trigger_count <= 1 {
                    state.result = Some(Some(candidate.clone()));
                }
            }
        }
    }
}

fn start_recording(state: &mut ShortcutWindowState) {
    state.mode = RecorderMode::Recording;
    state.candidate = None;
    state.recorded_presses.clear();
    state.held_keys.clear();
    state.trigger_count = 0;
    state.validation_message = None;
    state.hovered = None;
    state.tooltip_action = None;
}

fn stop_recording(state: &mut ShortcutWindowState) {
    state.held_keys.clear();
    if state.recorded_presses.is_empty() {
        state.mode = RecorderMode::Viewing;
        state.candidate = None;
        state.validation_message = None;
        return;
    }
    state.mode = RecorderMode::Ready;
    state.validation_message = if state.trigger_count > 1 {
        Some("Use one non-modifier key in the shortcut.".to_string())
    } else {
        state
            .candidate
            .as_ref()
            .and_then(hotkey_validation_message)
            .or_else(|| Some("Hold a modifier while pressing one trigger key.".to_string()))
    };
}

fn cancel_recording(state: &mut ShortcutWindowState) {
    state.mode = RecorderMode::Viewing;
    state.candidate = None;
    state.recorded_presses.clear();
    state.held_keys.clear();
    state.trigger_count = 0;
    state.validation_message = None;
    state.hovered = None;
    state.tooltip_action = None;
}

fn save_enabled(state: &ShortcutWindowState) -> bool {
    let candidate = state.candidate.as_ref().unwrap_or(&state.saved);
    state.mode != RecorderMode::Recording
        && state.trigger_count <= 1
        && hotkey_validation_message(candidate).is_none()
}

fn record_key_down(state: &mut ShortcutWindowState, key: PhysicalKey) {
    if state.held_keys.contains(&key) {
        return;
    }
    state.held_keys.push(key);
    let is_new_press = !state.recorded_presses.iter().any(|press| press.key == key);
    if is_new_press {
        state.recorded_presses.push(RecordedPress {
            key,
            label: physical_key_label(key),
            order: state.recorded_presses.len(),
        });
    }

    if is_new_press && !is_modifier(key.virtual_key) {
        state.trigger_count += 1;
        if state.trigger_count == 1 {
            let (ctrl, shift, alt, win) = pressed_modifiers(&state.held_keys);
            state.candidate = Some(HotkeySettings {
                ctrl,
                shift,
                alt,
                win,
                key_code: key.virtual_key,
                key_label: key_label(key.virtual_key),
            });
            state.validation_message = state.candidate.as_ref().and_then(hotkey_validation_message);
        } else {
            state.validation_message =
                Some("Use one non-modifier key in the shortcut.".to_string());
        }
    }
}

fn record_key_up(state: &mut ShortcutWindowState, key: PhysicalKey) {
    if let Some(index) = state.held_keys.iter().position(|held| *held == key) {
        state.held_keys.remove(index);
    }
}

fn pressed_modifiers(keys: &[PhysicalKey]) -> (bool, bool, bool, bool) {
    (
        keys.iter()
            .any(|key| key.virtual_key == VK_CONTROL.0 as u32),
        keys.iter().any(|key| key.virtual_key == VK_SHIFT.0 as u32),
        keys.iter().any(|key| key.virtual_key == VK_MENU.0 as u32),
        keys.iter()
            .any(|key| key.virtual_key == VK_LWIN.0 as u32 || key.virtual_key == VK_RWIN.0 as u32),
    )
}

fn physical_key(virtual_key: u32, lparam: LPARAM) -> PhysicalKey {
    PhysicalKey {
        virtual_key,
        scan_code: ((lparam.0 as u64 >> 16) & 0xff) as u16,
        extended: ((lparam.0 as u64 >> 24) & 1) != 0,
    }
}

fn physical_key_label(key: PhysicalKey) -> String {
    match key.virtual_key {
        code if code == VK_CONTROL.0 as u32 => if key.extended {
            "Right Ctrl"
        } else {
            "Left Ctrl"
        }
        .to_string(),
        code if code == VK_SHIFT.0 as u32 => if key.scan_code == 0x36 {
            "Right Shift"
        } else {
            "Left Shift"
        }
        .to_string(),
        code if code == VK_MENU.0 as u32 => if key.extended {
            "Right Alt"
        } else {
            "Left Alt"
        }
        .to_string(),
        code if code == VK_LWIN.0 as u32 => "Left Win".to_string(),
        code if code == VK_RWIN.0 as u32 => "Right Win".to_string(),
        _ => key_label(key.virtual_key),
    }
}

fn is_modifier(key_code: u32) -> bool {
    key_code == VK_CONTROL.0 as u32
        || key_code == VK_SHIFT.0 as u32
        || key_code == VK_MENU.0 as u32
        || key_code == VK_LWIN.0 as u32
        || key_code == VK_RWIN.0 as u32
}

fn chord_keys(hotkey: &HotkeySettings) -> Vec<(PhysicalKey, String)> {
    let mut keys = Vec::new();
    if hotkey.ctrl {
        keys.push((
            PhysicalKey {
                virtual_key: VK_CONTROL.0 as u32,
                scan_code: 0,
                extended: false,
            },
            "Ctrl".to_string(),
        ));
    }
    if hotkey.shift {
        keys.push((
            PhysicalKey {
                virtual_key: VK_SHIFT.0 as u32,
                scan_code: 0,
                extended: false,
            },
            "Shift".to_string(),
        ));
    }
    if hotkey.alt {
        keys.push((
            PhysicalKey {
                virtual_key: VK_MENU.0 as u32,
                scan_code: 0,
                extended: false,
            },
            "Alt".to_string(),
        ));
    }
    if hotkey.win {
        keys.push((
            PhysicalKey {
                virtual_key: VK_LWIN.0 as u32,
                scan_code: 0,
                extended: false,
            },
            "Win".to_string(),
        ));
    }
    keys.push((
        PhysicalKey {
            virtual_key: hotkey.key_code,
            scan_code: 0,
            extended: false,
        },
        hotkey.key_label.clone(),
    ));
    keys
}

fn hotkey_validation_message(hotkey: &HotkeySettings) -> Option<String> {
    if !hotkey.is_valid() {
        return Some("Choose at least one modifier and one key.".to_string());
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

fn next_action(action: ShortcutAction) -> ShortcutAction {
    match action {
        ShortcutAction::Close => ShortcutAction::Reset,
        ShortcutAction::Reset => ShortcutAction::Record,
        ShortcutAction::Record => ShortcutAction::Save,
        ShortcutAction::Save => ShortcutAction::Close,
    }
}

fn previous_action(action: ShortcutAction) -> ShortcutAction {
    match action {
        ShortcutAction::Close => ShortcutAction::Save,
        ShortcutAction::Reset => ShortcutAction::Close,
        ShortcutAction::Record => ShortcutAction::Reset,
        ShortcutAction::Save => ShortcutAction::Record,
    }
}

fn action_label(state: &ShortcutWindowState, action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::Close => "Close",
        ShortcutAction::Reset => "Reset to default",
        ShortcutAction::Record if state.mode == RecorderMode::Recording => "Stop recording",
        ShortcutAction::Record => "Record shortcut",
        ShortcutAction::Save => "Save shortcut",
    }
}

unsafe fn desired_panel_size(
    hwnd: HWND,
    hotkey: &HotkeySettings,
    dpi_scale: f32,
    text_scale: f32,
    work: RECT,
) -> (i32, i32) {
    let spacing = dpi_scale * (1.0 + (text_scale - 1.0) * 0.55);
    let font = dpi_scale * text_scale;
    let hdc = GetDC(hwnd);
    let title_size = measure_text(hdc, "Quick access shortcut", TITLE_FONT_DIP * font, true);
    let subtitle_size = measure_text(
        hdc,
        "Press your shortcut, then stop recording",
        SUBTITLE_FONT_DIP * font,
        false,
    );
    let labels: Vec<String> = chord_keys(hotkey)
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    let tile_font = KEY_FONT_DIP * font;
    let plus = measure_text(hdc, "+", tile_font, true).0 + 14.0 * spacing;
    let key_width = labels
        .iter()
        .map(|label| {
            (measure_text(hdc, label, tile_font, true).0 + 34.0 * spacing).max(58.0 * spacing)
        })
        .sum::<f32>()
        + plus * labels.len().saturating_sub(1) as f32;
    let status_height = measure_text(hdc, "Shortcut ready to save", STATUS_FONT_DIP * font, false)
        .1
        .max(20.0 * font);
    let _ = ReleaseDC(hwnd, hdc);

    let reference_width = REFERENCE_WIDTH_DIP * dpi_scale;
    let control = dpi_scale * (1.0 + (text_scale - 1.0) * 0.8);
    let logo_width = LOGO_DIP * control;
    let action_width = 4.0 * ACTION_ICON_DIP * control + 3.0 * ACTION_GAP_DIP * spacing;
    let content_width = (title_size.0 + logo_width + 14.0 * spacing)
        .max(subtitle_size.0)
        .max(key_width)
        .max(action_width)
        + PANEL_MARGIN_DIP * 2.0 * spacing;
    let (maximum_width, maximum_height) = maximum_panel_size(work, dpi_scale);
    let width = reference_width.max(content_width).min(maximum_width);
    let available_key_width = (width - PANEL_MARGIN_DIP * 2.0 * spacing).max(1.0);
    let key_rows = if key_width > available_key_width {
        2.0
    } else {
        1.0
    };
    let title_height = title_size.1.max(28.0 * font);
    let subtitle_height = subtitle_size.1.max(18.0 * font);
    let tile_height = KEY_HEIGHT_DIP * font;
    let action_height = ACTION_ICON_DIP * control;
    let measured_height = 23.0 * spacing
        + title_height
        + 2.0 * spacing
        + subtitle_height
        + 17.0 * spacing
        + tile_height * key_rows
        + 9.0 * spacing * (key_rows - 1.0)
        + 24.0 * spacing
        + status_height
        + 10.0 * spacing
        + action_height
        + PANEL_MARGIN_DIP * spacing;
    let height = (REFERENCE_HEIGHT_DIP * dpi_scale)
        .max(measured_height)
        .min(maximum_height);
    (width.round() as i32, height.round() as i32)
}

fn maximum_panel_size(work: RECT, dpi_scale: f32) -> (f32, f32) {
    let safety = WORK_AREA_MARGIN_DIP * dpi_scale;
    (
        ((work.right - work.left) as f32 - safety * 2.0).max(1.0),
        ((work.bottom - work.top) as f32 - safety * 2.0).max(1.0),
    )
}

unsafe fn resize_panel_for_settings(hwnd: HWND, state: &mut ShortcutWindowState) {
    state.dpi_scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
    state.text_scale = windows_text_scale();
    let monitor = monitor_info_for_window(hwnd);
    let display = state.candidate.as_ref().unwrap_or(&state.saved);
    let (width, height) = desired_panel_size(
        hwnd,
        display,
        state.dpi_scale,
        state.text_scale,
        monitor.rcWork,
    );
    let x = monitor.rcWork.left + ((monitor.rcWork.right - monitor.rcWork.left - width) / 2).max(0);
    let y = monitor.rcWork.top + ((monitor.rcWork.bottom - monitor.rcWork.top - height) / 2).max(0);
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
    apply_panel_region(hwnd, width, height, state.dpi_scale, state.text_scale);
    invalidate(hwnd);
}

unsafe fn apply_panel_region(hwnd: HWND, width: i32, height: i32, dpi_scale: f32, text_scale: f32) {
    let radius = (10.0 * dpi_scale * (1.0 + (text_scale - 1.0) * 0.35))
        .round()
        .max(4.0) as i32;
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
    if SetWindowRgn(hwnd, region, true) == 0 {
        let _ = DeleteObject(region);
    }
}

fn windows_text_scale() -> f32 {
    UISettings::new()
        .and_then(|settings| settings.TextScaleFactor())
        .map(|value| value as f32)
        .unwrap_or(1.0)
        .clamp(1.0, 2.25)
}

unsafe fn measure_text(hdc: HDC, text: &str, size: f32, bold: bool) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0);
    }
    let font = create_ui_font(size, bold);
    let previous = SelectObject(hdc, font);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut measured = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &wide, &mut measured);
    let _ = SelectObject(hdc, previous);
    let _ = DeleteObject(font);
    (measured.cx.max(0) as f32, measured.cy.max(0) as f32)
}

unsafe fn create_ui_font(size: f32, bold: bool) -> windows::Win32::Graphics::Gdi::HFONT {
    CreateFontW(
        -size.round().max(1.0) as i32,
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
        w!("Segoe UI Variable Text"),
    )
}

unsafe fn clear_tooltip_timer(hwnd: HWND, state: &mut ShortcutWindowState) {
    let _ = KillTimer(hwnd, TOOLTIP_TIMER_ID);
    state.tooltip_action = None;
}

unsafe fn invalidate(hwnd: HWND) {
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe fn set_pointer(clickable: bool) {
    if let Ok(cursor) = LoadCursorW(None, if clickable { IDC_HAND } else { IDC_ARROW }) {
        let _ = SetCursor(cursor);
    }
}

fn point_from_lparam(lparam: LPARAM) -> Point {
    Point::new(
        (lparam.0 as u32 & 0xffff) as i16 as f32,
        ((lparam.0 as u32 >> 16) & 0xffff) as i16 as f32,
    )
}

fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 }
}

unsafe fn monitor_info_under_cursor() -> MONITORINFO {
    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    monitor_info(monitor)
}

unsafe fn monitor_info_for_window(hwnd: HWND) -> MONITORINFO {
    monitor_info(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST))
}

unsafe fn monitor_info(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> MONITORINFO {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    info
}

unsafe fn draw_panel_border(hdc: HDC, rect: Rect, palette: ToolbarPalette) {
    let pen = CreatePen(PS_SOLID, 1, colorref(palette.border_bottom));
    let previous = SelectObject(hdc, pen);
    let _brush = SelectedStockObject::null_brush(hdc);
    let native = rect_to_rect(Rect::new(
        rect.x,
        rect.y,
        rect.width - 1.0,
        rect.height - 1.0,
    ));
    let _ = RoundRect(
        hdc,
        native.left,
        native.top,
        native.right,
        native.bottom,
        10,
        10,
    );
    let _ = SelectObject(hdc, previous);
    let _ = DeleteObject(pen);
}

unsafe fn draw_surface(hdc: HDC, rect: Rect, radius: f32, fill_color: Color, border: Color) {
    let brush = CreateSolidBrush(colorref(fill_color));
    let pen = CreatePen(PS_SOLID, 1, colorref(border));
    let previous_brush = SelectObject(hdc, brush);
    let previous_pen = SelectObject(hdc, pen);
    let native = rect_to_rect(rect);
    let diameter = (radius * 2.0).round().max(1.0) as i32;
    let _ = RoundRect(
        hdc,
        native.left,
        native.top,
        native.right,
        native.bottom,
        diameter,
        diameter,
    );
    let _ = SelectObject(hdc, previous_pen);
    let _ = SelectObject(hdc, previous_brush);
    let _ = DeleteObject(pen);
    let _ = DeleteObject(brush);
}

unsafe fn fill(hdc: HDC, rect: Rect, color: Color) {
    let brush = CreateSolidBrush(colorref(color));
    let _ = FillRect(hdc, &rect_to_rect(rect), brush);
    let _ = DeleteObject(brush);
}

unsafe fn draw_text(
    hdc: HDC,
    rect: Rect,
    text: &str,
    size: f32,
    color: Color,
    bold: bool,
    centered: bool,
) {
    if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let font = create_ui_font(size, bold);
    let previous = SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(color));
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut native = rect_to_rect(rect);
    let mut flags = DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
    if centered {
        flags |= DT_CENTER;
    }
    let _ = DrawTextW(hdc, &mut wide, &mut native, flags);
    let _ = SelectObject(hdc, previous);
    let _ = DeleteObject(font);
}

fn shortcut_palette(theme: AppTheme) -> ToolbarPalette {
    let mut palette = toolbar_palette(theme);
    if theme == AppTheme::Light {
        palette.background = Color::rgb(0xf2, 0xf2, 0xf2);
        palette.icon = Color::rgb(0x3a, 0x3a, 0x3a);
    }
    palette
}

fn inset_rect(rect: Rect, amount: f32) -> Rect {
    Rect::new(
        rect.x + amount,
        rect.y + amount,
        (rect.width - amount * 2.0).max(0.0),
        (rect.height - amount * 2.0).max(0.0),
    )
}

fn fade_to_background(color: Color, background: Color, opacity: f32) -> Color {
    let opacity = opacity.clamp(0.0, 1.0);
    Color::rgb(
        (background.r as f32 + (color.r as f32 - background.r as f32) * opacity).round() as u8,
        (background.g as f32 + (color.g as f32 - background.g as f32) * opacity).round() as u8,
        (background.b as f32 + (color.b as f32 - background.b as f32) * opacity).round() as u8,
    )
}

fn muted_text(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x68, 0x68, 0x68),
        AppTheme::Dark => Color::rgb(0x92, 0x92, 0x92),
    }
}

fn disabled_text(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0xb5, 0xb5, 0xb5),
        AppTheme::Dark => Color::rgb(0x54, 0x54, 0x54),
    }
}

fn success_text(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x18, 0x7a, 0x43),
        AppTheme::Dark => Color::rgb(0x65, 0xd1, 0x8a),
    }
}

fn key_background(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x3a, 0x3a, 0x3a),
        AppTheme::Dark => Color::rgb(0x42, 0x42, 0x42),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording_state() -> ShortcutWindowState {
        ShortcutWindowState {
            saved: HotkeySettings::default(),
            candidate: None,
            result: None,
            validation_message: None,
            theme: AppTheme::Dark,
            mode: RecorderMode::Recording,
            hovered: None,
            tooltip_action: None,
            focused: ShortcutAction::Record,
            held_keys: Vec::new(),
            recorded_presses: Vec::new(),
            trigger_count: 0,
            tracking_mouse: false,
            dpi_scale: 1.0,
            text_scale: 1.0,
        }
    }

    fn key(virtual_key: u32, scan_code: u16, extended: bool) -> PhysicalKey {
        PhysicalKey {
            virtual_key,
            scan_code,
            extended,
        }
    }

    #[test]
    fn held_key_and_auto_repeat_produce_one_press() {
        let mut state = recording_state();
        let a = key(0x41, 0x1e, false);
        record_key_down(&mut state, a);
        record_key_down(&mut state, a);
        assert_eq!(state.recorded_presses.len(), 1);
    }

    #[test]
    fn releasing_key_does_not_add_a_tile() {
        let mut state = recording_state();
        let ctrl = key(VK_CONTROL.0 as u32, 0x1d, true);
        record_key_down(&mut state, ctrl);
        record_key_up(&mut state, ctrl);
        assert_eq!(state.recorded_presses.len(), 1);
        assert!(state.held_keys.is_empty());
    }

    #[test]
    fn repeated_physical_key_is_not_added_after_release() {
        let mut state = recording_state();
        let ctrl = key(VK_CONTROL.0 as u32, 0x1d, true);
        record_key_down(&mut state, ctrl);
        record_key_up(&mut state, ctrl);
        record_key_down(&mut state, ctrl);
        assert_eq!(state.recorded_presses.len(), 1);
    }

    #[test]
    fn chord_uses_modifiers_held_when_trigger_is_pressed() {
        let mut state = recording_state();
        let ctrl = key(VK_CONTROL.0 as u32, 0x1d, true);
        let w = key(0x57, 0x11, false);
        record_key_down(&mut state, ctrl);
        record_key_down(&mut state, w);
        let candidate = state.candidate.expect("candidate");
        assert!(candidate.ctrl);
        assert_eq!(candidate.key_code, 0x57);
        assert_eq!(state.recorded_presses.len(), 2);
    }

    #[test]
    fn stop_with_no_keys_restores_saved_shortcut() {
        let mut state = recording_state();
        stop_recording(&mut state);
        assert_eq!(state.mode, RecorderMode::Viewing);
        assert!(state.candidate.is_none());
        assert!(state.recorded_presses.is_empty());
    }

    #[test]
    fn second_trigger_remains_visible_and_disables_save() {
        let mut state = recording_state();
        record_key_down(&mut state, key(VK_CONTROL.0 as u32, 0x1d, false));
        record_key_down(&mut state, key(0x41, 0x1e, false));
        record_key_up(&mut state, key(0x41, 0x1e, false));
        record_key_down(&mut state, key(0x42, 0x30, false));
        stop_recording(&mut state);
        assert_eq!(state.recorded_presses.len(), 3);
        assert!(!save_enabled(&state));
        assert!(state.validation_message.is_some());
    }

    #[test]
    fn long_key_rows_wrap_into_at_most_two_balanced_rows() {
        let rows = pack_tile_rows(&[100.0, 100.0, 100.0, 100.0], 20.0, 250.0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].indexes, [0, 1]);
        assert_eq!(rows[1].indexes, [2, 3]);
    }

    #[test]
    fn enlarged_panel_clamps_inside_small_and_scaled_work_areas() {
        let small = RECT {
            left: 0,
            top: 0,
            right: 640,
            bottom: 480,
        };
        assert_eq!(maximum_panel_size(small, 1.0), (608.0, 448.0));

        let four_k_scaled = RECT {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        };
        let maximum = maximum_panel_size(four_k_scaled, 2.0);
        assert!(REFERENCE_WIDTH_DIP * 2.0 <= maximum.0);
        assert!(REFERENCE_HEIGHT_DIP * 2.0 <= maximum.1);
    }
}
