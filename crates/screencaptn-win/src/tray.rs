use crate::native_svg::{draw_svg, recolor_svg};
use crate::overlay::AppTheme;
use crate::settings::AppSettings;
use crate::startup::RunOnStartupState;
use crate::theme::toolbar_palette;
use crate::util::{colorref, point_from_lparam};
use screencaptn_core::{Color, Point, Rect};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetMonitorInfoW, LineTo, MonitorFromPoint, MoveToEx, RoundRect, SelectObject,
    SetBkMode, SetTextColor, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, IsWindow, LoadCursorW, LoadIconW, RegisterClassW, SetCursor,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU, IDC_ARROW, IDC_HAND, MSG, SW_SHOW, WM_APP,
    WM_CLOSE, WM_CREATE, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub const WM_TRAYICON: u32 = WM_APP + 1;
const TRAY_UID: u32 = 1;
const POPOVER_CLASS: PCWSTR = w!("ScreenCaptnTrayPopover");
const POPOVER_WIDTH: f32 = 312.0;
const ROW_HEIGHT: f32 = 40.0;
const SECTION_HEIGHT: f32 = 24.0;
const OUTER_PADDING: f32 = 8.0;
const SECTION_GAP: f32 = 4.0;
const ROW_CONTROL_SIZE: f32 = 18.0;
const ROW_TEXT_INDENT: f32 = 34.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayAction {
    SetShortcut,
    ToggleAutoSave,
    OpenAutoSaveFolder,
    SetAutoSaveFolder,
    ToggleRunOnStartup,
    ToggleTheme,
    ToggleCaptureTips,
    OpenSetup,
    ShowUpdate,
    ReportBug,
    Donate,
    Exit,
}

#[derive(Clone)]
pub struct TrayMenuState {
    pub theme: AppTheme,
    pub settings: AppSettings,
    pub startup_state: RunOnStartupState,
    pub update_available: bool,
}

#[derive(Clone)]
enum TrayItem {
    Section(&'static str),
    Row(TrayRow),
}

#[derive(Clone)]
struct TrayRow {
    action: TrayAction,
    label: String,
    value: Option<String>,
    icon: TrayIcon,
    kind: TrayRowKind,
    enabled: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TrayRowKind {
    Standard,
    Toggle { checked: bool },
    IconToggle { checked: bool },
    Update,
}

#[derive(Clone, Copy)]
enum TrayIcon {
    Shortcut,
    Folder,
    FolderOpen,
    Startup,
    Setup,
    LightMode,
    DarkMode,
    Tips,
    Donate,
    Bug,
    Exit,
    Update,
}

struct TrayPopoverState {
    menu: TrayMenuState,
    items: Vec<TrayItem>,
    row_rects: Vec<(usize, Rect)>,
    hover: Option<usize>,
    trailing_hover: Option<usize>,
    focus: Option<usize>,
    result: Option<Option<TrayAction>>,
    scale: f32,
    size: (i32, i32),
}

pub unsafe fn add_tray_icon(hwnd: HWND, settings: &AppSettings) -> Result<()> {
    let custom_icon = crate::app_icon::load_app_icon(32);
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: custom_icon.unwrap_or(LoadIconW(
            None,
            windows::Win32::UI::WindowsAndMessaging::IDI_APPLICATION,
        )?),
        ..Default::default()
    };

    set_tip(&mut data, settings);
    let result = Shell_NotifyIconW(NIM_ADD, &data).ok();
    if let Some(icon) = custom_icon {
        let _ = DestroyIcon(icon);
    }
    result
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

pub unsafe fn show_tray_menu(owner: HWND, menu: TrayMenuState) -> Option<TrayAction> {
    let instance = GetModuleHandleW(None).ok()?;
    let class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
        hInstance: instance.into(),
        lpszClassName: POPOVER_CLASS,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(popover_wnd_proc),
        ..Default::default()
    };
    RegisterClassW(&class);

    let scale = GetDpiForWindow(owner).max(96) as f32 / 96.0;
    let mut state = Box::new(TrayPopoverState::new(menu, scale));
    let state_ptr = state.as_mut() as *mut TrayPopoverState;
    let (x, y) = popover_position(state.size.0, state.size.1);
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        POPOVER_CLASS,
        w!("Screen Cap'n"),
        WS_POPUP,
        x,
        y,
        state.size.0,
        state.size.1,
        owner,
        HMENU::default(),
        instance,
        Some(state_ptr.cast()),
    )
    .ok()?;
    Box::leak(state);

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetCapture(hwnd);

    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).into() {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
        if !IsWindow(hwnd).as_bool() {
            break;
        }
        if popover_state(hwnd).is_some_and(|state| state.result.is_some()) {
            break;
        }
    }

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayPopoverState;
    if state_ptr.is_null() {
        return None;
    }
    let mut state = Box::from_raw(state_ptr);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    let result = state.result.take().flatten();
    if GetCapture() == hwnd {
        let _ = ReleaseCapture();
    }
    if IsWindow(hwnd).as_bool() {
        let _ = DestroyWindow(hwnd);
    }
    result
}

impl TrayPopoverState {
    fn new(menu: TrayMenuState, scale: f32) -> Self {
        let items = menu_items(&menu);
        let mut state = Self {
            menu,
            items,
            row_rects: Vec::new(),
            hover: None,
            trailing_hover: None,
            focus: None,
            result: None,
            scale,
            size: (0, 0),
        };
        state.layout();
        state
    }

    fn layout(&mut self) {
        let width = scaled(self.scale, POPOVER_WIDTH);
        let mut y = scaled(self.scale, OUTER_PADDING);
        self.row_rects.clear();
        for (index, item) in self.items.iter().enumerate() {
            match item {
                TrayItem::Section(_) => y += scaled(self.scale, SECTION_HEIGHT),
                TrayItem::Row(_) => {
                    self.row_rects.push((
                        index,
                        Rect::new(
                            scaled(self.scale, OUTER_PADDING),
                            y,
                            width - scaled(self.scale, OUTER_PADDING * 2.0),
                            scaled(self.scale, ROW_HEIGHT),
                        ),
                    ));
                    y += scaled(self.scale, ROW_HEIGHT);
                }
            }
            if matches!(item, TrayItem::Section(_)) {
                y += scaled(self.scale, SECTION_GAP);
            }
        }
        y += scaled(self.scale, OUTER_PADDING);
        self.size = (width.round() as i32, y.round() as i32);
    }

    fn item_at(&self, point: Point) -> Option<usize> {
        self.row_rects.iter().find_map(|(index, rect)| {
            rect.contains(point)
                .then(|| match &self.items[*index] {
                    TrayItem::Row(row) if row.enabled => Some(*index),
                    _ => None,
                })
                .flatten()
        })
    }

    fn action_at(&self, index: usize) -> Option<TrayAction> {
        match self.items.get(index) {
            Some(TrayItem::Row(row)) if row.enabled => Some(row.action),
            _ => None,
        }
    }

    fn action_at_point(&self, point: Point) -> Option<TrayAction> {
        let index = self.item_at(point)?;
        let row = match self.items.get(index) {
            Some(TrayItem::Row(row)) => row,
            _ => return None,
        };
        let rect = self
            .row_rects
            .iter()
            .find_map(|(row_index, rect)| (*row_index == index).then_some(*rect))?;
        if trailing_action(row).is_some() && trailing_rect(self.scale, rect).contains(point) {
            return trailing_action(row);
        }
        self.action_at(index)
    }

    fn trailing_at(&self, point: Point) -> Option<usize> {
        let index = self.item_at(point)?;
        let _row = match self.items.get(index) {
            Some(TrayItem::Row(row)) if trailing_action(row).is_some() => row,
            _ => return None,
        };
        let rect = self
            .row_rects
            .iter()
            .find_map(|(row_index, rect)| (*row_index == index).then_some(*rect))?;
        trailing_rect(self.scale, rect)
            .contains(point)
            .then_some(index)
    }

    fn selectable_indices(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                TrayItem::Row(row) if row.enabled => Some(index),
                _ => None,
            })
            .collect()
    }

    fn move_focus(&mut self, delta: i32) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            return;
        }
        let current = self
            .focus
            .and_then(|index| selectable.iter().position(|candidate| *candidate == index))
            .unwrap_or(if delta < 0 {
                0
            } else {
                selectable.len().saturating_sub(1)
            });
        let next = (current as i32 + delta).rem_euclid(selectable.len() as i32) as usize;
        self.focus = Some(selectable[next]);
        self.hover = None;
        self.trailing_hover = None;
    }
}

unsafe extern "system" fn popover_wnd_proc(
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
            let mut paint = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint);
            if let Some(state) = popover_state(hwnd) {
                draw_popover(hdc, state);
            }
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(state) = popover_state(hwnd) {
                let next = state.item_at(point_from_lparam(lparam));
                let trailing = state.trailing_at(point_from_lparam(lparam));
                if state.hover != next || state.trailing_hover != trailing || state.focus.is_some()
                {
                    state.hover = next;
                    state.trailing_hover = trailing;
                    state.focus = None;
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                }
                if let Ok(cursor) =
                    LoadCursorW(None, if next.is_some() { IDC_HAND } else { IDC_ARROW })
                {
                    let _ = SetCursor(cursor);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(state) = popover_state(hwnd) {
                let point = point_from_lparam(lparam);
                if let Some(action) = state.action_at_point(point) {
                    state.result = Some(Some(action));
                } else if !Rect::new(0.0, 0.0, state.size.0 as f32, state.size.1 as f32)
                    .contains(point)
                {
                    state.result = Some(None);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(state) = popover_state(hwnd) {
                match wparam.0 as u32 {
                    0x1B => state.result = Some(None),
                    0x26 => {
                        state.move_focus(-1);
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                    }
                    0x28 => {
                        state.move_focus(1);
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                    }
                    0x0D | 0x20 => {
                        if let Some(action) = state.focus.and_then(|index| state.action_at(index)) {
                            state.result = Some(Some(action));
                        }
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_KILLFOCUS | WM_CLOSE => {
            if let Some(state) = popover_state(hwnd) {
                state.result = Some(None);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn popover_state(hwnd: HWND) -> Option<&'static mut TrayPopoverState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayPopoverState;
    (!ptr.is_null()).then(|| &mut *ptr)
}

fn menu_items(menu: &TrayMenuState) -> Vec<TrayItem> {
    let mut items = Vec::new();
    if menu.update_available {
        items.push(TrayItem::Section("UPDATE"));
        items.push(TrayItem::Row(TrayRow {
            action: TrayAction::ShowUpdate,
            label: "Update available".to_string(),
            value: None,
            icon: TrayIcon::Update,
            kind: TrayRowKind::Update,
            enabled: true,
        }));
    }

    items.push(TrayItem::Section("CAPTURE"));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::SetShortcut,
        label: "Quick access shortcut".to_string(),
        value: Some(menu.settings.hotkey.display_label()),
        icon: TrayIcon::Shortcut,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::ToggleAutoSave,
        label: "Auto-save screenshots".to_string(),
        value: None,
        icon: TrayIcon::Shortcut,
        kind: TrayRowKind::Toggle {
            checked: menu.settings.auto_save.enabled,
        },
        enabled: true,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::SetAutoSaveFolder,
        label: "Choose auto-save folder".to_string(),
        value: None,
        icon: TrayIcon::Folder,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));

    items.push(TrayItem::Section("PREFERENCES"));
    let startup_disabled = matches!(
        menu.startup_state,
        RunOnStartupState::DisabledByUser
            | RunOnStartupState::DisabledByPolicy
            | RunOnStartupState::EnabledByPolicy
    );
    let startup_label = if startup_disabled {
        "Run on startup (managed by Windows)"
    } else {
        "Run on startup"
    };
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::ToggleRunOnStartup,
        label: startup_label.to_string(),
        value: None,
        icon: TrayIcon::Startup,
        kind: TrayRowKind::Toggle {
            checked: menu.startup_state.is_enabled(),
        },
        enabled: !startup_disabled,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::ToggleTheme,
        label: match menu.theme {
            AppTheme::Light => "Switch to dark mode",
            AppTheme::Dark => "Switch to light mode",
        }
        .to_string(),
        value: None,
        icon: match menu.theme {
            AppTheme::Light => TrayIcon::DarkMode,
            AppTheme::Dark => TrayIcon::LightMode,
        },
        kind: TrayRowKind::Standard,
        enabled: true,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::ToggleCaptureTips,
        label: "Show capture tips".to_string(),
        value: None,
        icon: TrayIcon::Tips,
        kind: TrayRowKind::IconToggle {
            checked: menu.settings.show_capture_tips,
        },
        enabled: true,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::OpenSetup,
        label: "Screen Cap'n setup".to_string(),
        value: None,
        icon: TrayIcon::Setup,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));

    items.push(TrayItem::Section("SUPPORT"));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::ReportBug,
        label: "Report a bug".to_string(),
        value: None,
        icon: TrayIcon::Bug,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::Donate,
        label: "Enjoying Screen Cap'n? Consider donating".to_string(),
        value: None,
        icon: TrayIcon::Donate,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));
    items.push(TrayItem::Section("APP"));
    items.push(TrayItem::Row(TrayRow {
        action: TrayAction::Exit,
        label: "Exit Screen Cap'n".to_string(),
        value: None,
        icon: TrayIcon::Exit,
        kind: TrayRowKind::Standard,
        enabled: true,
    }));
    items
}

unsafe fn draw_popover(hdc: HDC, state: &TrayPopoverState) {
    let palette = toolbar_palette(state.menu.theme);
    let bounds = RECT {
        left: 0,
        top: 0,
        right: state.size.0,
        bottom: state.size.1,
    };
    let brush = CreateSolidBrush(colorref(palette.background));
    let pen = CreatePen(
        PS_SOLID,
        scaled(state.scale, 1.0).max(1.0) as i32,
        colorref(palette.border_bottom),
    );
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    let radius = scaled(state.scale, 8.0) as i32;
    let _ = RoundRect(
        hdc,
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom,
        radius,
        radius,
    );
    let _ = SelectObject(hdc, old_brush);
    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);

    let mut y = scaled(state.scale, OUTER_PADDING);
    for (index, item) in state.items.iter().enumerate() {
        match item {
            TrayItem::Section(label) => {
                if index > 0 {
                    draw_section_separator(hdc, state, y, palette);
                }
                draw_text(
                    hdc,
                    Rect::new(
                        scaled(state.scale, 18.0),
                        y,
                        state.size.0 as f32 - scaled(state.scale, 36.0),
                        scaled(state.scale, SECTION_HEIGHT),
                    ),
                    label,
                    scaled(state.scale, 10.0) as i32,
                    section_color(state.menu.theme),
                    true,
                    DT_LEFT,
                );
                y += scaled(state.scale, SECTION_HEIGHT + SECTION_GAP);
            }
            TrayItem::Row(row) => {
                let rect = state
                    .row_rects
                    .iter()
                    .find_map(|(row_index, rect)| (*row_index == index).then_some(*rect))
                    .unwrap_or_default();
                let active = state.hover == Some(index) || state.focus == Some(index);
                let trailing_active = state.trailing_hover == Some(index);
                if active && row.enabled {
                    fill_rect(
                        hdc,
                        inset_rect(rect, scaled(state.scale, 2.0)),
                        hover_color(state.menu.theme, row.kind),
                    );
                }
                draw_row(hdc, state, row, rect, active, trailing_active, palette);
                y = rect.bottom();
            }
        }
    }
}

unsafe fn draw_row(
    hdc: HDC,
    state: &TrayPopoverState,
    row: &TrayRow,
    rect: Rect,
    active: bool,
    trailing_active: bool,
    palette: crate::theme::ToolbarPalette,
) {
    let mut label_rect = inset_rect(rect, scaled(state.scale, 10.0));
    // All actions share this left control column, whether it contains a
    // checkbox, an icon, or intentional empty space.
    label_rect.x += scaled(state.scale, ROW_TEXT_INDENT);
    label_rect.width -= scaled(state.scale, ROW_TEXT_INDENT);
    if row.value.is_some() {
        label_rect.width -= scaled(state.scale, 96.0);
    }
    if trailing_action(row).is_some() {
        label_rect.width -= scaled(state.scale, 34.0);
    }
    let foreground = if row.enabled {
        palette.icon
    } else {
        disabled_color(state.menu.theme)
    };
    draw_text(
        hdc,
        label_rect,
        &row.label,
        scaled(state.scale, 13.0) as i32,
        foreground,
        false,
        DT_LEFT,
    );
    if let Some(value) = &row.value {
        draw_text(
            hdc,
            Rect::new(
                rect.x,
                rect.y,
                rect.width - scaled(state.scale, 14.0),
                rect.height,
            ),
            value,
            scaled(state.scale, 12.0) as i32,
            foreground,
            false,
            DT_RIGHT,
        );
    }
    let icon_rect = check_rect(state.scale, rect);
    match row.kind {
        TrayRowKind::Toggle { checked } => {
            let preview = if active && !trailing_active && row.enabled {
                !checked
            } else {
                checked
            };
            let color = if preview {
                foreground
            } else {
                inactive_toggle_color(state.menu.theme)
            };
            draw_checkmark(hdc, icon_rect, color, state.scale);
        }
        TrayRowKind::IconToggle { checked } => {
            let preview = if active && row.enabled {
                !checked
            } else {
                checked
            };
            let color = if preview {
                foreground
            } else {
                inactive_toggle_color(state.menu.theme)
            };
            let icon_center = icon_rect.center();
            let icon_size = icon_rect.width * 1.3225;
            draw_row_icon(
                hdc,
                row.icon,
                Rect::new(
                    icon_center.x - icon_size / 2.0,
                    icon_center.y - icon_size / 2.0,
                    icon_size,
                    icon_size,
                ),
                color,
            );
        }
        TrayRowKind::Standard | TrayRowKind::Update => {
            draw_row_icon(hdc, row.icon, icon_rect, foreground);
        }
    }
    if trailing_action(row).is_some() {
        let color = if trailing_active {
            palette.accent
        } else {
            foreground
        };
        draw_row_icon(
            hdc,
            TrayIcon::FolderOpen,
            trailing_rect(state.scale, rect),
            color,
        );
    }
}

unsafe fn draw_checkmark(hdc: HDC, rect: Rect, color: Color, scale: f32) {
    let pen = CreatePen(
        PS_SOLID,
        scaled(scale, 2.0).round().max(1.0) as i32,
        colorref(color),
    );
    let old = SelectObject(hdc, pen);
    let left = rect.x;
    let top = rect.y;
    let size = rect.width;
    let _ = MoveToEx(
        hdc,
        (left + size * 0.12).round() as i32,
        (top + size * 0.53).round() as i32,
        None,
    );
    let _ = LineTo(
        hdc,
        (left + size * 0.40).round() as i32,
        (top + size * 0.78).round() as i32,
    );
    let _ = LineTo(
        hdc,
        (left + size * 0.88).round() as i32,
        (top + size * 0.20).round() as i32,
    );
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(pen);
}

fn check_rect(scale: f32, rect: Rect) -> Rect {
    let size = scaled(scale, ROW_CONTROL_SIZE);
    Rect::new(
        rect.x + scaled(scale, 10.0),
        rect.y + (rect.height - size) / 2.0 + scaled(scale, 1.5),
        size,
        size,
    )
}

fn trailing_rect(scale: f32, rect: Rect) -> Rect {
    let size = scaled(scale, ROW_CONTROL_SIZE);
    Rect::new(
        rect.right() - scaled(scale, 10.0) - size,
        rect.y + (rect.height - size) / 2.0 + scaled(scale, 1.5),
        size,
        size,
    )
}

fn trailing_action(row: &TrayRow) -> Option<TrayAction> {
    (row.action == TrayAction::ToggleAutoSave).then_some(TrayAction::OpenAutoSaveFolder)
}

unsafe fn draw_row_icon(hdc: HDC, icon: TrayIcon, rect: Rect, color: Color) {
    let source = match icon {
        TrayIcon::Shortcut => include_str!("../assets/tray/shortcut.svg"),
        TrayIcon::Folder => include_str!("../assets/tray/folder.svg"),
        TrayIcon::FolderOpen => include_str!("../assets/tray/folder-open.svg"),
        TrayIcon::Startup => include_str!("../assets/tray/startup.svg"),
        TrayIcon::Setup => include_str!("../assets/tray/setup.svg"),
        TrayIcon::LightMode => include_str!("../assets/toolbar/light-mode.svg"),
        TrayIcon::DarkMode => include_str!("../assets/toolbar/dark-mode.svg"),
        TrayIcon::Tips => include_str!("../assets/tips/tooltip.svg"),
        TrayIcon::Donate => include_str!("../assets/tray/donate.svg"),
        TrayIcon::Bug => include_str!("../assets/tray/bug.svg"),
        TrayIcon::Exit => include_str!("../assets/tray/exit.svg"),
        TrayIcon::Update => include_str!("../assets/tray/update.svg"),
    };
    // The toolbar theme artwork has intentionally generous SVG padding. Crop
    // only its tray presentation so its visible vector matches the 24px icons.
    let source = match icon {
        TrayIcon::LightMode => source
            .replace("viewBox=\"0 0 16 16\"", "viewBox=\"3 3 10 10\"")
            .replace(
                "stroke-linecap:",
                "stroke-width: .75px;\n        stroke-linecap:",
            )
            .replace("r=\".5\"", "r=\".4\""),
        TrayIcon::DarkMode => source
            .replace("viewBox=\"0 0 16 16\"", "viewBox=\"5 5 6 6\"")
            .replace(
                "stroke-linecap:",
                "stroke-width: .5px;\n        stroke-linecap:",
            ),
        TrayIcon::Bug => source.replace("viewBox=\"0 0 24 24\"", "viewBox=\"2 2 20 20\""),
        _ => source.to_string(),
    };
    let source = recolor_svg(&source, color);
    let _ = draw_svg(hdc, &source, rect);
}

unsafe fn draw_section_separator(
    hdc: HDC,
    state: &TrayPopoverState,
    y: f32,
    palette: crate::theme::ToolbarPalette,
) {
    let color = blend_color(palette.background, palette.divider, 0.24);
    let pen = CreatePen(PS_SOLID, 1, colorref(color));
    let old = SelectObject(hdc, pen);
    let inset = scaled(state.scale, 18.0);
    let line_y = y.round() as i32;
    let _ = MoveToEx(hdc, inset.round() as i32, line_y, None);
    let _ = LineTo(hdc, (state.size.0 as f32 - inset).round() as i32, line_y);
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(pen);
}

fn blend_color(background: Color, foreground: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |back: u8, front: u8| {
        (back as f32 + (front as f32 - back as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgb(
        channel(background.r, foreground.r),
        channel(background.g, foreground.g),
        channel(background.b, foreground.b),
    )
}

unsafe fn draw_text(
    hdc: HDC,
    rect: Rect,
    text: &str,
    size: i32,
    color: Color,
    bold: bool,
    align: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let font = CreateFontW(
        -size.max(1),
        0,
        0,
        0,
        if bold {
            windows::Win32::Graphics::Gdi::FW_BOLD.0 as i32
        } else {
            400
        },
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
    let old = SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(color));
    let mut native = RECT {
        left: rect.x.round() as i32,
        top: rect.y.round() as i32,
        right: rect.right().round() as i32,
        bottom: rect.bottom().round() as i32,
    };
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let _ = DrawTextW(
        hdc,
        &mut wide,
        &mut native,
        align | DT_VCENTER | DT_SINGLELINE,
    );
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

unsafe fn fill_rect(hdc: HDC, rect: Rect, color: Color) {
    let brush = CreateSolidBrush(colorref(color));
    let native = RECT {
        left: rect.x.round() as i32,
        top: rect.y.round() as i32,
        right: rect.right().round() as i32,
        bottom: rect.bottom().round() as i32,
    };
    let _ = FillRect(hdc, &native, brush);
    let _ = DeleteObject(brush);
}

fn hover_color(theme: AppTheme, kind: TrayRowKind) -> Color {
    let _ = kind;
    match theme {
        AppTheme::Light => Color::rgb(0xe4, 0xe4, 0xe4),
        AppTheme::Dark => Color::rgb(0x2a, 0x2a, 0x2a),
    }
}

fn inactive_toggle_color(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0xd8, 0xd8, 0xd8),
        AppTheme::Dark => Color::rgb(0x28, 0x28, 0x28),
    }
}

fn section_color(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x7d, 0x7d, 0x7d),
        AppTheme::Dark => Color::rgb(0x91, 0x91, 0x91),
    }
}

fn disabled_color(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0xa4, 0xa4, 0xa4),
        AppTheme::Dark => Color::rgb(0x66, 0x66, 0x66),
    }
}

fn scaled(scale: f32, value: f32) -> f32 {
    scale * value
}

fn inset_rect(rect: Rect, amount: f32) -> Rect {
    Rect::new(
        rect.x + amount,
        rect.y + amount,
        (rect.width - amount * 2.0).max(0.0),
        (rect.height - amount * 2.0).max(0.0),
    )
}

unsafe fn popover_position(width: i32, height: i32) -> (i32, i32) {
    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    let mut x = cursor.x - width + 24;
    let mut y = cursor.y - height - 8;
    if y < info.rcWork.top {
        y = cursor.y + 8;
    }
    x = x.clamp(info.rcWork.left + 4, info.rcWork.right - width - 4);
    y = y.clamp(info.rcWork.top + 4, info.rcWork.bottom - height - 4);
    (x, y)
}
