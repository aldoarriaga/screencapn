use crate::util::{point_from_lparam, rect_to_rect, SelectedPen, SelectedStockObject};
use screencaptn_core::{
    Annotation, AnnotationId, AnnotationKind, CaptureDocument, Color, HighlightShape, History,
    MosaicMode, Point, Rect, ResizeHandle, StrokeStyle, ToolKind,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{w, Result, PCWSTR, PWSTR};
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Globalization::{GetDateFormatEx, DATE_SHORTDATE};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, Ellipse,
    FillRect, FrameRect, GetDC, GetDIBits, GetMonitorInfoW, GetPixel, InvalidateRect, LineTo,
    MonitorFromPoint, MoveToEx, Rectangle, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    TextOutW, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, DT_LEFT, DT_WORDBREAK, HBITMAP, HDC, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_RETURN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GetAncestor,
    GetClientRect, GetCursorPos, GetMessageW, GetShellWindow, GetSystemMetrics, GetWindow,
    GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible, KillTimer, LoadCursorW,
    RegisterClassW, SetCursor, SetForegroundWindow, SetTimer, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GA_ROOT, GWLP_USERDATA, GWL_EXSTYLE,
    GWL_STYLE, GW_OWNER, HMENU, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_SIZEALL, IDC_SIZENESW,
    IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, WM_CHAR, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    WS_CAPTION, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_THICKFRAME,
};

const OVERLAY_CLASS: windows::core::PCWSTR = w!("ScreenCaptnCaptureOverlay");
const CF_BITMAP_FORMAT: u32 = 2;
const HANDLE_RADIUS: f32 = 6.0;
const MIN_REGION_SIZE: f32 = 24.0;
const TOOLBAR_BUTTON: f32 = 36.0;
const TOOLBAR_HEIGHT: f32 = 36.0;
const FRAME_HIT_WIDTH: f32 = 8.0;
const CLICK_DRAG_THRESHOLD: f32 = 5.0;
const TOP_EDGE_FULLSCREEN_THRESHOLD: f32 = 1.0;
const TOP_CHROME_HEIGHT: f32 = 72.0;
const WINDOW_OVERLAP_TOLERANCE: f32 = 4.0;
const TOOLBAR_RADIUS: f32 = 10.0;
const TOOL_ICON_SIZE: f32 = 24.0;
const TOOL_ICON_SELECTED_RADIUS: f32 = 6.0;
const HIGHLIGHTER_RADIUS: f32 = 8.0;
const PEN_POINT_SPACING: f32 = 5.5;
const DEFAULT_TEXT_FONT_SIZE: f32 = 27.0;
const TAG_DEFAULT_WIDTH: f32 = 146.0;
const TAG_DEFAULT_HEIGHT: f32 = 55.0;
const TAG_FRAME: f32 = 14.0;
const TAG_RADIUS: f32 = 10.0;
const CARET_TIMER_ID: usize = 1;
const NUMBERING_TOGGLE_TIMER_ID: usize = 2;
const REGION_BORDER_TIMER_ID: usize = 3;
const SUBMENU_HEIGHT: f32 = 24.0;
const SUBMENU_RADIUS: f32 = 8.0;
const SUBMENU_NOTCH: f32 = 10.0;
const SUBMENU_SWATCH: f32 = 16.0;
const SUBMENU_GAP: f32 = 6.0;
const SUBMENU_DIVIDER: f32 = 10.0;
const SUBMENU_EDGE_PAD: f32 = 6.0;
const MIN_STROKE_WIDTH: f32 = 1.0;
const MAX_STROKE_WIDTH: f32 = 24.0;
const MIN_FONT_SIZE: f32 = 27.0;
const MAX_FONT_SIZE: f32 = 56.0;
const WATERMARK_OPACITY: f32 = 0.5;
const WEB_EXPORT_ENABLED: bool = false;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppTheme {
    Light,
    Dark,
}

pub fn open_capture_overlay(theme: AppTheme) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_CROSS)?,
            hInstance: instance.into(),
            lpszClassName: OVERLAY_CLASS,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&class);

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let screen_bounds = Rect::new(x as f32, y as f32, width as f32, height as f32);
        let detected_regions = collect_detected_regions(screen_bounds);
        let background_bitmap = capture_screen_bitmap(screen_bounds);
        let initial_hover_region =
            monitor_work_region_at(Point::new(x as f32, y as f32)).unwrap_or(screen_bounds);
        let mut state = Box::new(OverlayState::new(
            screen_bounds,
            background_bitmap,
            detected_regions,
            theme,
        ));
        state.hover_region = Some(initial_hover_region);
        let state_ptr = state.as_mut() as *mut OverlayState;

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            OVERLAY_CLASS,
            w!("Screen Captn Capture"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            HMENU::default(),
            instance,
            Some(state_ptr.cast()),
        )?;
        state.hwnd = hwnd;
        match crate::web_ui::WebUi::create(hwnd, state_ptr.cast::<c_void>(), overlay_web_message) {
            Ok(web_ui) => {
                state.web_ui = Some(web_ui);
            }
            Err(error) => {
                write_web_ui_debug(&format!("webui-create-error: {error:?}"));
            }
        }
        sync_web_full_snapshot(&mut state);
        Box::leak(state);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);

        let mut msg = MSG::default();
        while BOOL::from(GetMessageW(&mut msg, None, 0, 0)).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }
        }
        Ok(())
    }
}

struct OverlayState {
    hwnd: HWND,
    screen_bounds: Rect,
    background_bitmap: HBITMAP,
    detected_regions: Vec<DetectedRegion>,
    hover_region: Option<Rect>,
    cursor_position: Point,
    document: CaptureDocument,
    history: History<CaptureDocument>,
    active_tool: ToolKind,
    numbering_enabled: bool,
    numbering_toggle_progress: f32,
    current_stroke: StrokeStyle,
    tool_stroke_widths: [f32; 11],
    normal_stroke_color: Color,
    highlighter_opacity: f32,
    mosaic_brush_size: f32,
    font_size: f32,
    next_step_number: u32,
    editing_text_id: Option<AnnotationId>,
    editing_step_number_id: Option<AnnotationId>,
    editing_step_number_replace: bool,
    toolbar_origin: Option<Point>,
    drag: Option<DragState>,
    toolbar_buttons: Vec<ToolbarButton>,
    submenu_buttons: Vec<SubmenuButton>,
    submenu_sliders: Vec<SubmenuSlider>,
    submenu_rect: Option<Rect>,
    active_submenu: Option<ToolKind>,
    pen_mode: PenMode,
    highlighter_shape: HighlightShape,
    text_filled: bool,
    watermark_mode: WatermarkMode,
    watermark_date_enabled: bool,
    watermark_text: String,
    watermark_color: Color,
    watermark_image_path: Option<PathBuf>,
    watermark_image_bitmap: Option<WatermarkBitmap>,
    watermark_image_data_url: Option<String>,
    editing_watermark_text: bool,
    ui_scale: f32,
    theme: AppTheme,
    web_ui: Option<crate::web_ui::WebUi>,
    web_pointer_raw_mode: bool,
    web_revision: u64,
    web_sync_baseline: Option<WebSyncBaseline>,
    force_web_full_snapshot: bool,
    render_cache: Option<RenderCache>,
    static_layer_dirty: bool,
}

impl OverlayState {
    fn new(
        screen_bounds: Rect,
        background_bitmap: HBITMAP,
        detected_regions: Vec<DetectedRegion>,
        theme: AppTheme,
    ) -> Self {
        let ui_scale = ui_scale_for_screen(screen_bounds);
        let current_stroke = StrokeStyle::default();
        Self {
            hwnd: HWND::default(),
            screen_bounds,
            background_bitmap,
            detected_regions,
            hover_region: None,
            cursor_position: Point::new(0.0, 0.0),
            document: CaptureDocument::new(),
            history: History::new(100),
            active_tool: ToolKind::Rectangle,
            numbering_enabled: false,
            numbering_toggle_progress: 0.0,
            normal_stroke_color: current_stroke.color,
            current_stroke,
            tool_stroke_widths: [current_stroke.width; 11],
            highlighter_opacity: 0.30,
            mosaic_brush_size: 16.0,
            font_size: DEFAULT_TEXT_FONT_SIZE,
            next_step_number: 1,
            editing_text_id: None,
            editing_step_number_id: None,
            editing_step_number_replace: false,
            toolbar_origin: None,
            drag: None,
            toolbar_buttons: Vec::new(),
            submenu_buttons: Vec::new(),
            submenu_sliders: Vec::new(),
            submenu_rect: None,
            active_submenu: Some(ToolKind::Rectangle),
            pen_mode: PenMode::Free,
            highlighter_shape: HighlightShape::RoundedRectangle,
            text_filled: false,
            watermark_mode: WatermarkMode::Text,
            watermark_date_enabled: false,
            watermark_text: String::new(),
            watermark_color: current_stroke.color,
            watermark_image_path: None,
            watermark_image_bitmap: None,
            watermark_image_data_url: None,
            editing_watermark_text: false,
            ui_scale,
            theme,
            web_ui: None,
            web_pointer_raw_mode: false,
            web_revision: 0,
            web_sync_baseline: None,
            force_web_full_snapshot: true,
            render_cache: None,
            static_layer_dirty: true,
        }
    }

    fn checkpoint(&mut self) {
        self.history.checkpoint(&self.document);
    }

    fn screen_to_overlay(&self, point: Point) -> Point {
        point.translate(-self.screen_bounds.x, -self.screen_bounds.y)
    }

    fn overlay_to_screen(&self, point: Point) -> Point {
        point.translate(self.screen_bounds.x, self.screen_bounds.y)
    }

    fn region_overlay(&self) -> Option<Rect> {
        self.document
            .capture_region
            .map(|region| region.translate(-self.screen_bounds.x, -self.screen_bounds.y))
    }

    fn mark_static_dirty(&mut self) {
        self.static_layer_dirty = true;
    }
}

#[derive(Deserialize)]
struct WebUiMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "requestId")]
    request_id: Option<u64>,
    x: Option<f32>,
    y: Option<f32>,
    #[serde(rename = "rawX")]
    raw_x: Option<f32>,
    #[serde(rename = "rawY")]
    raw_y: Option<f32>,
    tool: Option<String>,
    color: Option<String>,
    value: Option<f32>,
    mode: Option<String>,
    shape: Option<String>,
    filled: Option<bool>,
    text: Option<String>,
    id: Option<AnnotationId>,
    format: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(rename = "keyCode")]
    key_code: Option<u32>,
    #[serde(rename = "charCode")]
    char_code: Option<u32>,
    #[serde(rename = "shiftKey")]
    shift_key: Option<bool>,
    reason: Option<String>,
}

#[derive(Clone, Copy)]
enum ExportTarget {
    Clipboard,
    Save,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct WebColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl From<Color> for WebColor {
    fn from(color: Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderStyle {
    corner_radius: f32,
    tag_radius: f32,
    tag_frame: f32,
    tag_inner_pad: f32,
    arrow_min_head: f32,
    arrow_head_width_factor: f32,
    arrow_head_length_factor: f32,
    pen_tension: f32,
    highlighter_opacity: f32,
    highlighter_line_base: f32,
    highlighter_line_width_factor: f32,
    mosaic_cell: f32,
    selection_color: WebColor,
    selection_handle_size: f32,
    step_badge_size: f32,
    step_badge_font_size: f32,
    step_badge_single_digit_x: f32,
    step_badge_multi_digit_x: f32,
    step_badge_text_y: f32,
}

impl RenderStyle {
    fn for_state(state: &OverlayState) -> Self {
        Self {
            corner_radius: scaled(state, HIGHLIGHTER_RADIUS),
            tag_radius: scaled(state, TAG_RADIUS),
            tag_frame: scaled(state, TAG_FRAME),
            tag_inner_pad: scaled(state, 8.0),
            arrow_min_head: 12.0,
            arrow_head_width_factor: 3.0,
            arrow_head_length_factor: 3.0,
            pen_tension: 0.35,
            highlighter_opacity: state.highlighter_opacity,
            highlighter_line_base: 24.0,
            highlighter_line_width_factor: 3.0,
            mosaic_cell: scaled(state, 10.8),
            selection_color: Color::BLUE.into(),
            selection_handle_size: scaled(state, 8.0),
            step_badge_size: 24.0,
            step_badge_font_size: 14.0,
            step_badge_single_digit_x: 8.0,
            step_badge_multi_digit_x: 5.0,
            step_badge_text_y: 4.0,
        }
    }
}

unsafe fn overlay_web_message(context: *mut c_void, message: String) {
    if context.is_null() {
        return;
    }
    let state = &mut *(context as *mut OverlayState);
    let should_sync = handle_web_ui_message(state, &message);
    if should_sync {
        state.mark_static_dirty();
        let _ = InvalidateRect(state.hwnd, None, false);
        sync_web_after_change(state);
    }
}

fn handle_web_ui_message(state: &mut OverlayState, message: &str) -> bool {
    let Ok(message) = serde_json::from_str::<WebUiMessage>(message) else {
        return false;
    };

    match message.kind.as_str() {
        "ready" => {
            state.force_web_full_snapshot = true;
            true
        }
        "pointerDown" => {
            if let Some(point) = web_pointer_down_point(state, &message) {
                handle_mouse_down(state, point);
                true
            } else {
                false
            }
        }
        "pointerMove" => {
            if let Some(point) = web_pointer_point(state, &message) {
                state.cursor_position = point;
                let drawing_annotation =
                    matches!(state.drag, Some(DragState::DrawingAnnotation { .. }));
                let changed = handle_mouse_move(state, point);
                if drawing_annotation {
                    if state.active_tool == ToolKind::Mosaic {
                        unsafe {
                            let _ = InvalidateRect(state.hwnd, None, false);
                        }
                    }
                    false
                } else {
                    changed
                }
            } else {
                false
            }
        }
        "pointerUp" => {
            if let Some(point) = web_pointer_point(state, &message) {
                handle_mouse_up(state, point);
                state.web_pointer_raw_mode = false;
                true
            } else {
                false
            }
        }
        "setToolbarOrigin" => {
            if let Some(point) = message_point(&message) {
                state.toolbar_origin = Some(point);
                true
            } else {
                false
            }
        }
        "selectTool" => {
            if let Some(tool) = message.tool.as_deref().and_then(tool_from_web_name) {
                handle_toolbar_action(state, ToolbarAction::Tool(tool));
                true
            } else {
                false
            }
        }
        "toggleNumbering" => {
            handle_toolbar_action(state, ToolbarAction::Numbering);
            true
        }
        "setNumberingMode" => {
            if let Some(mode) = message.mode.as_deref() {
                handle_numbering_mode(state, mode);
                true
            } else {
                false
            }
        }
        "editStepNumber" => {
            if let Some(id) = message.id {
                start_step_number_editing(state, id);
                true
            } else {
                false
            }
        }
        "toggleTheme" => {
            state.theme = crate::theme::toggled_theme(state.theme);
            crate::theme::save_theme(state.theme);
            true
        }
        "undo" => {
            handle_toolbar_action(state, ToolbarAction::Undo);
            true
        }
        "copy" => {
            handle_toolbar_action(state, ToolbarAction::Copy);
            false
        }
        "save" => {
            handle_toolbar_action(state, ToolbarAction::Save);
            true
        }
        "cancel" => {
            handle_toolbar_action(state, ToolbarAction::Cancel);
            false
        }
        "setColor" => {
            if let Some(color) = message.color.as_deref().and_then(color_from_hex) {
                handle_submenu_action(state, SubmenuAction::Color(color));
                true
            } else {
                false
            }
        }
        "setStrokeWidth" => {
            if let Some(value) = message.value {
                handle_submenu_action(state, SubmenuAction::StrokeWidth(value));
                true
            } else {
                false
            }
        }
        "setFontSize" => {
            if let Some(value) = message.value {
                handle_submenu_action(state, SubmenuAction::FontSize(value));
                true
            } else {
                false
            }
        }
        "setPenMode" => {
            if let Some(mode) = message.mode.as_deref().and_then(pen_mode_from_web_name) {
                handle_submenu_action(state, SubmenuAction::PenMode(mode));
                true
            } else {
                false
            }
        }
        "setHighlighterShape" => {
            if let Some(shape) = message
                .shape
                .as_deref()
                .and_then(highlighter_shape_from_web_name)
            {
                handle_submenu_action(state, SubmenuAction::HighlighterShape(shape));
                true
            } else {
                false
            }
        }
        "setTextFilled" => {
            if let Some(filled) = message.filled {
                handle_submenu_action(state, SubmenuAction::TextFilled(filled));
                true
            } else {
                false
            }
        }
        "setWatermarkMode" => {
            if let Some(mode) = message
                .mode
                .as_deref()
                .and_then(watermark_mode_from_web_name)
            {
                handle_submenu_action(state, SubmenuAction::WatermarkMode(mode));
                true
            } else {
                false
            }
        }
        "clearWatermark" => {
            clear_watermark(state);
            true
        }
        "focusWatermarkText" => {
            state.watermark_mode = WatermarkMode::Text;
            state.editing_watermark_text = true;
            state.editing_text_id = None;
            state.editing_step_number_id = None;
            true
        }
        "blurWatermarkText" => {
            state.editing_watermark_text = false;
            true
        }
        "setWatermarkText" => {
            if let Some(text) = message.text {
                state.watermark_mode = WatermarkMode::Text;
                state.watermark_text = text;
                state.editing_watermark_text = true;
                true
            } else {
                false
            }
        }
        "keyDown" => {
            if let Some(key) = message.key_code {
                let was_text_editing = state.editing_text_id.is_some()
                    || state.editing_watermark_text
                    || state.editing_step_number_id.is_some();
                handle_key_down(state, key, message.shift_key.unwrap_or(false));
                !(!was_text_editing && (key == 0x1B || key == VK_RETURN.0 as u32))
            } else {
                false
            }
        }
        "char" => {
            if let Some(code) = message.char_code {
                handle_char(state, code);
                true
            } else {
                false
            }
        }
        "exportReady" => {
            handle_web_export_ready(&message);
            false
        }
        "exportFailed" => {
            handle_web_export_failed(&message);
            false
        }
        _ => false,
    }
}

fn handle_web_export_ready(message: &WebUiMessage) {
    write_web_ui_debug(&format!(
        "web-export-ready: request={:?} format={:?} size={:?}x{:?}",
        message.request_id, message.format, message.width, message.height
    ));
}

#[derive(Clone, Debug, Default)]
struct WebSyncBaseline {
    state_patch_signature: String,
    annotations: Vec<(AnnotationId, String)>,
}

fn handle_web_export_failed(message: &WebUiMessage) {
    write_web_ui_debug(&format!(
        "web-export-failed: request={:?} reason={}",
        message.request_id,
        message.reason.as_deref().unwrap_or("unknown")
    ));
}

fn write_web_ui_debug(message: &str) {
    let path = std::env::temp_dir().join("screencaptn-web-ui-debug.log");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

fn web_ui_owns_pointer_input(state: &OverlayState) -> bool {
    state.web_ui.is_some() && state.document.capture_region.is_some()
}

fn sync_web_full_snapshot(state: &mut OverlayState) {
    let Some(web_ui) = &state.web_ui else {
        return;
    };
    web_ui.set_visible(state.document.capture_region.is_some());
    state.web_revision = state.web_revision.saturating_add(1);
    let payload = serde_json::json!({
        "type": "state",
        "revision": state.web_revision,
        "state": web_ui_state(state),
    })
    .to_string();
    web_ui.post_json(&payload);
    state.web_sync_baseline = Some(capture_web_sync_baseline(state));
    state.force_web_full_snapshot = false;
}

fn sync_web_after_change(state: &mut OverlayState) {
    if state.force_web_full_snapshot || state.web_sync_baseline.is_none() {
        sync_web_full_snapshot(state);
    } else {
        sync_web_render_diff(state);
    }
}

fn sync_web_render_diff(state: &mut OverlayState) {
    let Some(web_ui) = &state.web_ui else {
        return;
    };
    web_ui.set_visible(state.document.capture_region.is_some());

    let previous = state.web_sync_baseline.clone().unwrap_or_default();
    let next = capture_web_sync_baseline(state);
    let previous_annotations: BTreeMap<AnnotationId, String> =
        previous.annotations.iter().cloned().collect();
    let next_annotations: BTreeMap<AnnotationId, String> =
        next.annotations.iter().cloned().collect();

    let removed: Vec<AnnotationId> = previous_annotations
        .keys()
        .copied()
        .filter(|id| !next_annotations.contains_key(id))
        .collect();
    let added: Vec<serde_json::Value> = next_annotations
        .keys()
        .copied()
        .filter(|id| !previous_annotations.contains_key(id))
        .filter_map(|id| web_annotation_json_by_id(state, id))
        .collect();
    let updated: Vec<serde_json::Value> = next_annotations
        .iter()
        .filter(|(id, signature)| {
            previous_annotations
                .get(id)
                .is_some_and(|previous_signature| previous_signature != *signature)
        })
        .filter_map(|(id, _)| web_annotation_json_by_id(state, *id))
        .collect();
    let state_patch_changed = previous.state_patch_signature != next.state_patch_signature;

    if !state_patch_changed && added.is_empty() && updated.is_empty() && removed.is_empty() {
        state.web_sync_baseline = Some(next);
        return;
    }

    state.web_revision = state.web_revision.saturating_add(1);
    let payload = serde_json::json!({
        "type": "renderDiff",
        "revision": state.web_revision,
        "state": if state_patch_changed { web_ui_state_patch(state) } else { serde_json::Value::Null },
        "added": added,
        "updated": updated,
        "removed": removed,
    })
    .to_string();
    web_ui.post_json(&payload);
    state.web_sync_baseline = Some(next);
}

fn capture_web_sync_baseline(state: &OverlayState) -> WebSyncBaseline {
    WebSyncBaseline {
        state_patch_signature: web_ui_state_patch(state).to_string(),
        annotations: web_annotation_signatures(state),
    }
}

fn web_annotation_signatures(state: &OverlayState) -> Vec<(AnnotationId, String)> {
    state
        .document
        .annotations
        .iter()
        .map(|annotation| {
            (
                annotation.id,
                web_annotation_json(state, annotation).to_string(),
            )
        })
        .collect()
}

fn web_annotation_json_by_id(state: &OverlayState, id: AnnotationId) -> Option<serde_json::Value> {
    state
        .document
        .annotations
        .iter()
        .find(|annotation| annotation.id == id)
        .map(|annotation| web_annotation_json(state, annotation))
}

fn maybe_request_web_export(state: &OverlayState, target: ExportTarget) -> bool {
    if !WEB_EXPORT_ENABLED {
        return false;
    }
    request_web_export(state, target)
}

fn request_web_export(state: &OverlayState, target: ExportTarget) -> bool {
    let Some(web_ui) = &state.web_ui else {
        return false;
    };
    let Some(region) = state.document.capture_region else {
        return false;
    };
    let request_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let payload = serde_json::json!({
        "type": "exportRequest",
        "requestId": request_id,
        "target": export_target_name(target),
        "format": "png",
        "region": rect_json(Rect::new(0.0, 0.0, region.width, region.height)),
        "background": serde_json::Value::Null,
        "backgroundRequired": true,
        "annotations": web_export_annotations_json(state, region),
        "watermark": {
            "text": state.watermark_text,
            "dateEnabled": state.watermark_date_enabled,
            "mode": watermark_mode_web_name(state.watermark_mode),
            "opacity": WATERMARK_OPACITY,
        },
        "renderStyle": web_render_style_json(state),
    })
    .to_string();
    web_ui.post_json(&payload);
    true
}

fn export_target_name(target: ExportTarget) -> &'static str {
    match target {
        ExportTarget::Clipboard => "clipboard",
        ExportTarget::Save => "save",
    }
}

fn web_ui_state(state: &OverlayState) -> serde_json::Value {
    let mut value = web_ui_state_patch(state);
    if let Some(object) = value.as_object_mut() {
        object.insert("annotations".to_string(), web_annotations_json(state));
    }
    value
}

fn web_ui_state_patch(state: &OverlayState) -> serde_json::Value {
    let region = state.region_overlay();
    let toolbar = region.map(|region| {
        let origin = state
            .toolbar_origin
            .unwrap_or_else(|| default_toolbar_origin(state, region));
        rect_json(Rect::new(
            origin.x,
            origin.y,
            toolbar_width(state),
            scaled(state, TOOLBAR_HEIGHT),
        ))
    });
    serde_json::json!({
        "theme": theme_name(state.theme),
        "uiScale": state.ui_scale,
        "screen": {
            "x": state.screen_bounds.x,
            "y": state.screen_bounds.y,
            "width": state.screen_bounds.width,
            "height": state.screen_bounds.height,
        },
        "captureRegion": region.map(rect_json),
        "toolbar": toolbar,
        "activeTool": tool_web_name(state.active_tool),
        "activeSubmenu": state.active_submenu.map(tool_web_name),
        "numberingEnabled": state.numbering_enabled,
        "nextStepNumber": state.next_step_number,
        "currentStroke": {
            "width": state.current_stroke.width,
            "color": color_json(state.current_stroke.color),
            "opacity": state.current_stroke.opacity,
        },
        "fontSize": state.font_size,
        "penMode": pen_mode_web_name(state.pen_mode),
        "highlighterShape": highlighter_shape_web_name(state.highlighter_shape),
        "textFilled": state.text_filled,
        "watermarkMode": watermark_mode_web_name(state.watermark_mode),
        "watermarkDateEnabled": state.watermark_date_enabled,
        "watermarkText": state.watermark_text,
        "watermarkColor": color_json(state.watermark_color),
        "watermarkImageUrl": state.watermark_image_path.as_ref().map(watermark_file_url),
        "watermarkImageDataUrl": state.watermark_image_data_url,
        "editingWatermarkText": state.editing_watermark_text,
        "selectedAnnotationId": state.document.selected_annotation_id,
        "editingTextId": state.editing_text_id,
        "editingStepNumberId": state.editing_step_number_id,
        "renderStyle": web_render_style_json(state),
    })
}

fn watermark_file_url(path: &PathBuf) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::from("file:///");
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-' => {
                encoded.push(*byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
fn web_render_style_json(state: &OverlayState) -> serde_json::Value {
    serde_json::to_value(RenderStyle::for_state(state)).unwrap_or_else(|_| serde_json::json!({}))
}

fn web_annotations_json(state: &OverlayState) -> serde_json::Value {
    let annotations: Vec<serde_json::Value> = state
        .document
        .annotations
        .iter()
        .map(|annotation| web_annotation_json(state, annotation))
        .collect();
    serde_json::Value::Array(annotations)
}

fn web_export_annotations_json(state: &OverlayState, region: Rect) -> serde_json::Value {
    let annotations: Vec<serde_json::Value> = state
        .document
        .annotations
        .iter()
        .map(|annotation| {
            web_annotation_json_with_origin(state, annotation, Point::new(region.x, region.y))
        })
        .collect();
    serde_json::Value::Array(annotations)
}

fn web_annotation_json(state: &OverlayState, annotation: &Annotation) -> serde_json::Value {
    web_annotation_json_with_origin(
        state,
        annotation,
        Point::new(state.screen_bounds.x, state.screen_bounds.y),
    )
}

fn web_annotation_json_with_origin(
    _state: &OverlayState,
    annotation: &Annotation,
    origin: Point,
) -> serde_json::Value {
    let bounds = annotation.bounds.translate(-origin.x, -origin.y);
    let mut value = serde_json::json!({
        "id": annotation.id,
        "bounds": rect_json(bounds),
        "stroke": {
            "width": annotation.stroke.width,
            "color": color_json(annotation.stroke.color),
            "opacity": annotation.stroke.opacity,
        },
        "stepNumber": annotation.step_number,
    });
    let kind = match &annotation.kind {
        AnnotationKind::Rectangle => serde_json::json!({
            "type": "rectangle",
        }),
        AnnotationKind::Oval => serde_json::json!({
            "type": "oval",
        }),
        AnnotationKind::Line { start, end } => serde_json::json!({
            "type": "line",
            "start": point_json(start.translate(-origin.x, -origin.y)),
            "end": point_json(end.translate(-origin.x, -origin.y)),
        }),
        AnnotationKind::Arrow { start, end } => serde_json::json!({
            "type": "arrow",
            "start": point_json(start.translate(-origin.x, -origin.y)),
            "end": point_json(end.translate(-origin.x, -origin.y)),
        }),
        AnnotationKind::StepNumber { number } => serde_json::json!({
            "type": "step",
            "number": number,
        }),
        AnnotationKind::Text {
            text,
            font_size,
            framed,
            filled,
        } => serde_json::json!({
            "type": "text",
            "text": text,
            "fontSize": font_size,
            "framed": framed,
            "filled": filled,
        }),
        AnnotationKind::Tag {
            label,
            anchor,
            font_size,
        } => serde_json::json!({
            "type": "tag",
            "label": label,
            "anchor": point_json(anchor.translate(-origin.x, -origin.y)),
            "fontSize": font_size,
        }),
        AnnotationKind::Mosaic { mode, brush_size } => serde_json::json!({
            "type": "mosaic",
            "mode": mosaic_mode_web_name(*mode),
            "brushSize": brush_size,
        }),
        AnnotationKind::Highlighter {
            shape,
            opacity,
            start,
            end,
        } => serde_json::json!({
            "type": "highlighter",
            "shape": highlighter_shape_web_name(*shape),
            "opacity": opacity,
            "start": point_json(start.translate(-origin.x, -origin.y)),
            "end": point_json(end.translate(-origin.x, -origin.y)),
        }),
        AnnotationKind::Pen { points } => serde_json::json!({
            "type": "pen",
            "points": points_json_with_origin(points, origin),
        }),
        AnnotationKind::PenArrow { points } => serde_json::json!({
            "type": "penArrow",
            "points": points_json_with_origin(points, origin),
        }),
        AnnotationKind::Watermark { text, opacity } => serde_json::json!({
            "type": "watermark",
            "text": text,
            "opacity": opacity,
        }),
    };
    value["kind"] = kind;
    value
}

fn point_json(point: Point) -> serde_json::Value {
    serde_json::json!({
        "x": point.x,
        "y": point.y,
    })
}

fn points_json_with_origin(points: &[Point], origin: Point) -> serde_json::Value {
    let points: Vec<serde_json::Value> = points
        .iter()
        .copied()
        .map(|point| point_json(point.translate(-origin.x, -origin.y)))
        .collect();
    serde_json::Value::Array(points)
}

fn mosaic_mode_web_name(mode: MosaicMode) -> &'static str {
    match mode {
        MosaicMode::Area => "area",
        MosaicMode::Brush => "brush",
    }
}

fn message_point(message: &WebUiMessage) -> Option<Point> {
    Some(Point::new(message.x?, message.y?))
}

fn message_raw_point(message: &WebUiMessage) -> Option<Point> {
    Some(Point::new(message.raw_x?, message.raw_y?))
}

fn web_pointer_down_point(state: &mut OverlayState, message: &WebUiMessage) -> Option<Point> {
    let scaled = message_point(message)?;
    state.web_pointer_raw_mode = false;

    let Some(raw) = message_raw_point(message) else {
        return Some(scaled);
    };
    let Some(region) = state.region_overlay() else {
        return Some(scaled);
    };

    if !region.contains(scaled) && region.contains(raw) {
        state.web_pointer_raw_mode = true;
        Some(raw)
    } else {
        Some(scaled)
    }
}

fn web_pointer_point(state: &OverlayState, message: &WebUiMessage) -> Option<Point> {
    if state.web_pointer_raw_mode {
        message_raw_point(message).or_else(|| message_point(message))
    } else {
        message_point(message)
    }
}

fn rect_json(rect: Rect) -> serde_json::Value {
    serde_json::json!({
        "x": rect.x,
        "y": rect.y,
        "width": rect.width,
        "height": rect.height,
    })
}

fn color_json(color: Color) -> serde_json::Value {
    serde_json::json!({
        "r": color.r,
        "g": color.g,
        "b": color.b,
        "a": color.a,
    })
}

fn color_from_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ))
}

fn theme_name(theme: AppTheme) -> &'static str {
    match theme {
        AppTheme::Light => "light",
        AppTheme::Dark => "dark",
    }
}

fn tool_web_name(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::StepNumber => "step",
        ToolKind::Rectangle => "rectangle",
        ToolKind::Oval => "oval",
        ToolKind::Line => "line",
        ToolKind::Arrow => "arrow",
        ToolKind::Pen => "pen",
        ToolKind::Text => "text",
        ToolKind::Tag => "tag",
        ToolKind::Mosaic => "mosaic",
        ToolKind::Highlighter => "highlighter",
        ToolKind::Watermark => "watermark",
    }
}

fn tool_from_web_name(name: &str) -> Option<ToolKind> {
    match name {
        "rectangle" => Some(ToolKind::Rectangle),
        "oval" => Some(ToolKind::Oval),
        "line" => Some(ToolKind::Line),
        "arrow" => Some(ToolKind::Arrow),
        "pen" => Some(ToolKind::Pen),
        "text" => Some(ToolKind::Text),
        "tag" => Some(ToolKind::Tag),
        "mosaic" => Some(ToolKind::Mosaic),
        "highlighter" => Some(ToolKind::Highlighter),
        "watermark" => Some(ToolKind::Watermark),
        "step" => Some(ToolKind::StepNumber),
        _ => None,
    }
}

fn pen_mode_web_name(mode: PenMode) -> &'static str {
    match mode {
        PenMode::Free => "free",
        PenMode::Arrow => "arrow",
    }
}

fn pen_mode_from_web_name(name: &str) -> Option<PenMode> {
    match name {
        "free" => Some(PenMode::Free),
        "arrow" => Some(PenMode::Arrow),
        _ => None,
    }
}

fn highlighter_shape_web_name(shape: HighlightShape) -> &'static str {
    match shape {
        HighlightShape::Rectangle => "line",
        HighlightShape::RoundedRectangle => "area",
        HighlightShape::Oval => "area",
    }
}

fn highlighter_shape_from_web_name(name: &str) -> Option<HighlightShape> {
    match name {
        "line" => Some(HighlightShape::Rectangle),
        "area" => Some(HighlightShape::RoundedRectangle),
        _ => None,
    }
}

fn watermark_mode_web_name(mode: WatermarkMode) -> &'static str {
    match mode {
        WatermarkMode::Date => "date",
        WatermarkMode::Text => "text",
        WatermarkMode::Image => "image",
    }
}

fn watermark_mode_from_web_name(name: &str) -> Option<WatermarkMode> {
    match name {
        "date" => Some(WatermarkMode::Date),
        "text" => Some(WatermarkMode::Text),
        "image" => Some(WatermarkMode::Image),
        _ => None,
    }
}

struct RenderCache {
    width: i32,
    height: i32,
    back_dc: HDC,
    back_bitmap: HBITMAP,
    back_old: HGDIOBJ,
    static_dc: HDC,
    static_bitmap: HBITMAP,
    static_old: HGDIOBJ,
}

fn ui_scale_for_screen(screen_bounds: Rect) -> f32 {
    ((screen_bounds.height / 1080.0) * 1.38).clamp(1.38, 2.76)
}

fn scaled(state: &OverlayState, value: f32) -> f32 {
    value * state.ui_scale
}

fn top_chrome_height(state: &OverlayState, window: Rect) -> f32 {
    scaled(state, TOP_CHROME_HEIGHT).min(window.height * 0.28)
}

#[derive(Clone, Debug)]
struct DetectedRegion {
    window: Rect,
    client: Option<Rect>,
}

#[derive(Clone, Copy)]
struct ToolbarButton {
    rect: Rect,
    tool: ToolbarAction,
}

#[derive(Clone, Copy)]
struct SubmenuButton {
    rect: Rect,
    action: SubmenuAction,
}

#[derive(Clone, Copy)]
struct SubmenuSlider {
    rect: Rect,
    start_x: f32,
    end_x: f32,
    min: f32,
    max: f32,
    kind: SubmenuSliderKind,
}

#[derive(Clone, Copy)]
enum SubmenuSliderKind {
    StrokeWidth,
    FontSize,
}

#[derive(Clone, Copy)]
enum ToolbarAction {
    Grip,
    Numbering,
    Tool(ToolKind),
    Divider,
    Undo,
    Copy,
    Save,
    Cancel,
}

#[derive(Clone, Copy)]
enum SubmenuAction {
    StrokeWidth(f32),
    Color(Color),
    PenMode(PenMode),
    HighlighterShape(HighlightShape),
    TextFilled(bool),
    FontSize(f32),
    WatermarkMode(WatermarkMode),
    WatermarkTextInput,
    ClearWatermark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PenMode {
    Free,
    Arrow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatermarkMode {
    Date,
    Text,
    Image,
}

enum DragState {
    Selecting {
        start: Point,
        current: Point,
    },
    MovingRegion {
        start: Point,
        original: Rect,
    },
    MovingToolbar {
        start: Point,
        original: Point,
    },
    ResizingRegion {
        handle: ResizeHandle,
    },
    MovingAnnotation {
        start: Point,
        id: AnnotationId,
        original: Annotation,
    },
    EditingAnnotation {
        id: AnnotationId,
        edit: AnnotationEdit,
        original: Annotation,
    },
    DrawingAnnotation {
        start: Point,
        current: Point,
        points: Vec<Point>,
    },
    AdjustingSubmenuSlider {
        slider: SubmenuSlider,
    },
}

#[derive(Clone, Copy, Debug)]
enum AnnotationEdit {
    BoxResize(ResizeHandle),
    LineStart,
    LineEnd,
    TagAnchor,
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        let create = lparam.0 as *const CREATESTRUCTW;
        let state_ptr = (*create).lpCreateParams as *mut OverlayState;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        let _ = SetTimer(hwnd, CARET_TIMER_ID, 500, None);
        let _ = SetTimer(hwnd, REGION_BORDER_TIMER_ID, 80, None);
        return LRESULT(0);
    }

    let state_ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        as *mut OverlayState;
    if state_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *state_ptr;

    match msg {
        WM_PAINT => {
            paint_overlay(state);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == CARET_TIMER_ID
                && (state.editing_watermark_text || state.editing_text_id.is_some())
            {
                let _ = InvalidateRect(hwnd, None, false);
            } else if wparam.0 == NUMBERING_TOGGLE_TIMER_ID {
                if step_numbering_toggle_animation(state) {
                    let _ = InvalidateRect(hwnd, None, false);
                } else {
                    let _ = KillTimer(hwnd, NUMBERING_TOGGLE_TIMER_ID);
                }
            } else if wparam.0 == REGION_BORDER_TIMER_ID
                && (state.document.capture_region.is_some()
                    || state.hover_region.is_some()
                    || matches!(state.drag, Some(DragState::Selecting { .. })))
            {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if web_ui_owns_pointer_input(state) {
                return LRESULT(0);
            }
            let point = point_from_lparam(lparam);
            handle_mouse_down(state, point);
            SetCapture(hwnd);
            let _ = InvalidateRect(hwnd, None, false);
            sync_web_after_change(state);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if web_ui_owns_pointer_input(state) {
                return LRESULT(0);
            }
            let point = point_from_lparam(lparam);
            state.cursor_position = point;
            if handle_mouse_move(state, point) {
                let _ = InvalidateRect(hwnd, None, false);
                sync_web_after_change(state);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if web_ui_owns_pointer_input(state) {
                return LRESULT(1);
            }
            let mut cursor = POINT::default();
            if GetCursorPos(&mut cursor).is_ok() {
                update_cursor_for_hover(
                    state,
                    Point::new(
                        cursor.x as f32 - state.screen_bounds.x,
                        cursor.y as f32 - state.screen_bounds.y,
                    ),
                );
            }
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            if web_ui_owns_pointer_input(state) {
                return LRESULT(0);
            }
            let point = point_from_lparam(lparam);
            handle_mouse_up(state, point);
            state.mark_static_dirty();
            let _ = ReleaseCapture();
            let _ = InvalidateRect(hwnd, None, false);
            sync_web_after_change(state);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = wparam.0 as u32;
            let was_text_editing = state.editing_text_id.is_some() || state.editing_watermark_text;
            handle_key_down(state, key, shift_key_down());
            if was_text_editing || (key != 0x1B && key != VK_RETURN.0 as u32) {
                state.mark_static_dirty();
                let _ = InvalidateRect(hwnd, None, false);
                sync_web_after_change(state);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            handle_char(state, wparam.0 as u32);
            let _ = InvalidateRect(hwnd, None, false);
            sync_web_after_change(state);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = DeleteObject(state.background_bitmap);
            destroy_render_cache(state);
            let _ = Box::from_raw(state_ptr);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn handle_mouse_down(state: &mut OverlayState, point: Point) {
    if let Some(slider) = state
        .submenu_sliders
        .iter()
        .copied()
        .find(|slider| slider.rect.contains(point))
    {
        apply_submenu_slider_at(state, slider, point);
        state.drag = Some(DragState::AdjustingSubmenuSlider { slider });
        return;
    }

    if let Some(action) = state
        .submenu_buttons
        .iter()
        .find(|button| button.rect.contains(point))
        .map(|button| button.action)
    {
        handle_submenu_action(state, action);
        return;
    }

    if let Some(action) = state
        .toolbar_buttons
        .iter()
        .find(|button| button.rect.contains(point))
        .map(|b| b.tool)
    {
        if matches!(action, ToolbarAction::Grip) {
            let original = state
                .toolbar_origin
                .or_else(|| {
                    state
                        .region_overlay()
                        .map(|region| default_toolbar_origin(state, region))
                })
                .unwrap_or(point);
            state.drag = Some(DragState::MovingToolbar {
                start: point,
                original,
            });
            return;
        }
        handle_toolbar_action(state, action);
        return;
    }

    if let Some(region) = state.region_overlay() {
        if let Some(handle) = region.hit_resize_handle(point, HANDLE_RADIUS + 4.0) {
            finish_text_editing(state);
            state.checkpoint();
            state.drag = Some(DragState::ResizingRegion { handle });
            return;
        }

        let screen_point = state.overlay_to_screen(point);
        if let Some(id) = select_annotation_at(state, screen_point) {
            state.editing_text_id = editable_text_annotation(state, id).then_some(id);
            state.editing_step_number_id = None;
            state.editing_step_number_replace = false;
            if let Some(original) = state.document.annotation(id).cloned() {
                state.checkpoint();
                if let Some(edit) = hit_annotation_edit_handle(state, &original, screen_point) {
                    state.drag = Some(DragState::EditingAnnotation { id, edit, original });
                    return;
                }
                if matches!(original.kind, AnnotationKind::Text { .. }) {
                    state.editing_text_id = Some(id);
                    return;
                }
                state.drag = Some(DragState::MovingAnnotation {
                    start: screen_point,
                    id,
                    original,
                });
                return;
            }
        }

        if region_frame_contains(region, point) {
            finish_text_editing(state);
            state.checkpoint();
            state.drag = Some(DragState::MovingRegion {
                start: point,
                original: state
                    .document
                    .capture_region
                    .expect("overlay region maps from document region"),
            });
            return;
        }

        if region.contains(point) {
            if state.active_tool == ToolKind::Watermark {
                return;
            }
            finish_text_editing(state);
            let start = state.overlay_to_screen(point);
            state.drag = Some(DragState::DrawingAnnotation {
                start,
                current: start,
                points: vec![start],
            });
            return;
        }
    }

    state.checkpoint();
    state.drag = Some(DragState::Selecting {
        start: point,
        current: point,
    });
}

fn region_frame_contains(region: Rect, point: Point) -> bool {
    if !region.contains(point) {
        return false;
    }
    point.x <= region.x + FRAME_HIT_WIDTH
        || point.x >= region.right() - FRAME_HIT_WIDTH
        || point.y <= region.y + FRAME_HIT_WIDTH
        || point.y >= region.bottom() - FRAME_HIT_WIDTH
}

fn select_annotation_at(state: &mut OverlayState, screen_point: Point) -> Option<AnnotationId> {
    let hit = state
        .document
        .annotations
        .iter()
        .rev()
        .find(|annotation| annotation_hit_test(annotation, screen_point))
        .map(|annotation| annotation.id);
    state.document.selected_annotation_id = hit;
    if let Some(id) = hit {
        sync_selected_annotation_controls(state, id);
    }
    hit
}

fn sync_selected_annotation_controls(state: &mut OverlayState, id: AnnotationId) {
    let Some(annotation) = state.document.annotation(id).cloned() else {
        return;
    };
    let tool = tool_for_annotation(&annotation);
    state.active_tool = tool;
    state.active_submenu = Some(tool);
    state.current_stroke = annotation.stroke;
    remember_tool_stroke_width(state, tool, annotation.stroke.width);
    match &annotation.kind {
        AnnotationKind::Pen { .. } => state.pen_mode = PenMode::Free,
        AnnotationKind::PenArrow { .. } => state.pen_mode = PenMode::Arrow,
        AnnotationKind::Highlighter { shape, .. } => state.highlighter_shape = *shape,
        AnnotationKind::Text {
            font_size, filled, ..
        } => {
            state.font_size = *font_size;
            state.text_filled = *filled;
        }
        AnnotationKind::Tag { font_size, .. } => state.font_size = *font_size,
        AnnotationKind::Watermark { .. } => {
            state.watermark_color = annotation.stroke.color;
            state.editing_watermark_text = true;
        }
        _ => {}
    }
}

fn tool_for_annotation(annotation: &Annotation) -> ToolKind {
    match &annotation.kind {
        AnnotationKind::Rectangle => ToolKind::Rectangle,
        AnnotationKind::Oval => ToolKind::Oval,
        AnnotationKind::Line { .. } => ToolKind::Line,
        AnnotationKind::Arrow { .. } => ToolKind::Arrow,
        AnnotationKind::StepNumber { .. } => ToolKind::StepNumber,
        AnnotationKind::Text { .. } => ToolKind::Text,
        AnnotationKind::Tag { .. } => ToolKind::Tag,
        AnnotationKind::Mosaic { .. } => ToolKind::Mosaic,
        AnnotationKind::Highlighter { .. } => ToolKind::Highlighter,
        AnnotationKind::Pen { .. } | AnnotationKind::PenArrow { .. } => ToolKind::Pen,
        AnnotationKind::Watermark { .. } => ToolKind::Watermark,
    }
}

fn annotation_hit_test(annotation: &Annotation, screen_point: Point) -> bool {
    match &annotation.kind {
        AnnotationKind::Text {
            text,
            font_size,
            framed,
            ..
        } => {
            if *framed {
                annotation.bounds.contains(screen_point)
            } else {
                inline_text_hit_bounds(annotation.bounds, text, *font_size).contains(screen_point)
            }
        }
        AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => near_segment(
            screen_point,
            *start,
            *end,
            line_hit_radius(annotation.stroke.width),
        ),
        AnnotationKind::Highlighter {
            shape: HighlightShape::Rectangle,
            start,
            end,
            ..
        } => near_segment(
            screen_point,
            *start,
            *end,
            line_hit_radius(highlighter_line_width(annotation.stroke.width)),
        ),
        AnnotationKind::Pen { points } | AnnotationKind::PenArrow { points } => {
            points.windows(2).any(|pair| {
                near_segment(
                    screen_point,
                    pair[0],
                    pair[1],
                    line_hit_radius(annotation.stroke.width),
                )
            })
        }
        AnnotationKind::Tag { anchor, .. } => {
            annotation.bounds.contains(screen_point)
                || near_point(screen_point, *anchor, HANDLE_RADIUS + 12.0)
                || tag_pointer_hit(annotation.bounds, *anchor, screen_point)
        }
        _ => annotation.bounds.contains(screen_point),
    }
}

fn inline_text_hit_bounds(bounds: Rect, text: &str, font_size: f32) -> Rect {
    let padding = 8.0;
    let text_width = inline_text_width(text, font_size).max(font_size);
    let line_count = text.lines().count().max(1) as f32;
    Rect::new(
        bounds.x - padding,
        bounds.y - font_size * 1.12 - padding,
        text_width + padding * 2.0,
        font_size * 1.55 * line_count + padding * 2.0,
    )
}

fn tag_pointer_hit(bounds: Rect, anchor: Point, point: Point) -> bool {
    let left = anchor.x.min(bounds.x).min(bounds.right()) - HANDLE_RADIUS;
    let top = anchor.y.min(bounds.y).min(bounds.bottom()) - HANDLE_RADIUS;
    let right = anchor.x.max(bounds.x).max(bounds.right()) + HANDLE_RADIUS;
    let bottom = anchor.y.max(bounds.y).max(bounds.bottom()) + HANDLE_RADIUS;
    Rect::new(left, top, right - left, bottom - top).contains(point)
}

fn editable_text_annotation(state: &OverlayState, id: AnnotationId) -> bool {
    state.document.annotation(id).is_some_and(|annotation| {
        matches!(
            annotation.kind,
            AnnotationKind::Text { .. } | AnnotationKind::Tag { .. }
        )
    })
}

fn detect_hover_region(state: &OverlayState, screen_point: Point) -> Option<Rect> {
    if let Some(monitor) = unsafe { monitor_full_region_at_top_edge(screen_point) } {
        return Some(monitor);
    }

    if let Some((index, detected)) = top_app_window_at_point(state, screen_point) {
        let covered = state.detected_regions[..index].iter().any(|higher| {
            overlaps_with_tolerance(higher.window, detected.window, WINDOW_OVERLAP_TOLERANCE)
        });

        if !covered {
            if let Some(client) = client_region_for_hover(state, detected, screen_point) {
                return Some(client);
            }

            return Some(detected.window);
        }
    }

    unsafe { monitor_region_at(screen_point) }
}

fn top_app_window_at_point(
    state: &OverlayState,
    screen_point: Point,
) -> Option<(usize, &DetectedRegion)> {
    state
        .detected_regions
        .iter()
        .enumerate()
        .find(|(_, detected)| detected.window.contains(screen_point))
}

fn client_region_for_hover(
    state: &OverlayState,
    detected: &DetectedRegion,
    screen_point: Point,
) -> Option<Rect> {
    let client = detected.client.unwrap_or(detected.window);
    let client = if client.contains(screen_point) {
        client
    } else {
        detected.window
    };
    let chrome_height = top_chrome_height(state, detected.window);
    let content_top = if same_rect(client, detected.window, 4.0) {
        detected.window.y + chrome_height
    } else {
        client.y + chrome_height
    };
    if screen_point.y <= content_top {
        None
    } else {
        let content = Rect::new(
            client.x,
            content_top,
            client.width,
            (client.bottom() - content_top).max(0.0),
        );
        content.is_visible().then_some(content)
    }
}

fn same_rect(a: Rect, b: Rect, tolerance: f32) -> bool {
    (a.x - b.x).abs() <= tolerance
        && (a.y - b.y).abs() <= tolerance
        && (a.width - b.width).abs() <= tolerance
        && (a.height - b.height).abs() <= tolerance
}

unsafe fn collect_detected_regions(screen_bounds: Rect) -> Vec<DetectedRegion> {
    let mut context = WindowDetectionContext {
        regions: Vec::new(),
    };
    let context_ptr = &mut context as *mut WindowDetectionContext;
    let _ = EnumWindows(Some(enum_detected_window), LPARAM(context_ptr as isize));
    context
        .regions
        .into_iter()
        .filter(|region| {
            region.window.is_visible()
                && region.window.width > 48.0
                && region.window.height > 48.0
                && overlaps(region.window, screen_bounds)
        })
        .collect()
}

struct WindowDetectionContext {
    regions: Vec<DetectedRegion>,
}

unsafe extern "system" fn enum_detected_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut WindowDetectionContext);
    if !is_app_root_window(hwnd) {
        return BOOL(1);
    }

    let Some(region) = detected_region_from_hwnd(hwnd) else {
        return BOOL(1);
    };
    context.regions.push(region);
    BOOL(1)
}

unsafe fn is_app_root_window(hwnd: HWND) -> bool {
    if hwnd == GetShellWindow() || IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
        return false;
    }

    if GetAncestor(hwnd, GA_ROOT) != hwnd {
        return false;
    }

    if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| owner.0 != std::ptr::null_mut()) {
        return false;
    }

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }

    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    style & (WS_CAPTION.0 | WS_THICKFRAME.0) != 0
}

unsafe fn detected_region_from_hwnd(hwnd: HWND) -> Option<DetectedRegion> {
    let Some(window) = frame_rect_for_window(hwnd) else {
        return None;
    };
    if !window.is_visible() {
        return None;
    }

    let client = client_rect_for_window(hwnd, window);
    Some(DetectedRegion { window, client })
}

unsafe fn frame_rect_for_window(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut rect as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_ok()
    {
        let frame = rect_from_win32(rect);
        if frame.is_visible() {
            return Some(frame);
        }
    }

    if GetWindowRect(hwnd, &mut rect).is_err() {
        return None;
    }
    Some(rect_from_win32(rect))
}

unsafe fn client_rect_for_window(hwnd: HWND, window: Rect) -> Option<Rect> {
    let mut client_rect = RECT::default();
    if GetClientRect(hwnd, &mut client_rect).is_err() {
        return None;
    }

    let mut top_left = POINT {
        x: client_rect.left,
        y: client_rect.top,
    };
    let mut bottom_right = POINT {
        x: client_rect.right,
        y: client_rect.bottom,
    };
    if !ClientToScreen(hwnd, &mut top_left).as_bool()
        || !ClientToScreen(hwnd, &mut bottom_right).as_bool()
    {
        return None;
    }

    let client = Rect::new(
        top_left.x as f32,
        top_left.y as f32,
        (bottom_right.x - top_left.x) as f32,
        (bottom_right.y - top_left.y) as f32,
    );
    if client.is_visible()
        && client.width < window.width + 1.0
        && client.height < window.height + 1.0
    {
        Some(client)
    } else {
        None
    }
}

unsafe fn monitor_region_at(screen_point: Point) -> Option<Rect> {
    if screen_point.y
        <= monitor_full_region_at_top_edge(screen_point)?.y + TOP_EDGE_FULLSCREEN_THRESHOLD
    {
        return monitor_full_region_at_top_edge(screen_point);
    }
    monitor_work_region_at(screen_point)
}

unsafe fn monitor_work_region_at(screen_point: Point) -> Option<Rect> {
    let monitor = MonitorFromPoint(
        POINT {
            x: screen_point.x.round() as i32,
            y: screen_point.y.round() as i32,
        },
        MONITOR_DEFAULTTONEAREST,
    );
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return None;
    }
    Some(rect_from_win32(info.rcWork))
}

unsafe fn monitor_full_region_at_top_edge(screen_point: Point) -> Option<Rect> {
    let monitor = MonitorFromPoint(
        POINT {
            x: screen_point.x.round() as i32,
            y: screen_point.y.round() as i32,
        },
        MONITOR_DEFAULTTONEAREST,
    );
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return None;
    }
    let monitor = rect_from_win32(info.rcMonitor);
    (screen_point.y <= monitor.y + TOP_EDGE_FULLSCREEN_THRESHOLD).then_some(monitor)
}

fn rect_from_win32(rect: RECT) -> Rect {
    Rect::new(
        rect.left as f32,
        rect.top as f32,
        (rect.right - rect.left) as f32,
        (rect.bottom - rect.top) as f32,
    )
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

fn overlaps_with_tolerance(a: Rect, b: Rect, tolerance: f32) -> bool {
    a.x + tolerance < b.right()
        && a.right() - tolerance > b.x
        && a.y + tolerance < b.bottom()
        && a.bottom() - tolerance > b.y
}

fn handle_mouse_move(state: &mut OverlayState, point: Point) -> bool {
    if let Some(DragState::AdjustingSubmenuSlider { slider }) = &state.drag {
        apply_submenu_slider_at(state, *slider, point);
        return true;
    }

    let screen_point = state.overlay_to_screen(point);
    match &mut state.drag {
        Some(DragState::Selecting { current, .. }) => {
            *current = point;
            true
        }
        Some(DragState::MovingRegion { start, original }) => {
            let dx = point.x - start.x;
            let dy = point.y - start.y;
            state
                .document
                .set_capture_region(original.translate(dx, dy));
            state.mark_static_dirty();
            true
        }
        Some(DragState::MovingToolbar { start, original }) => {
            let dx = point.x - start.x;
            let dy = point.y - start.y;
            state.toolbar_origin = Some(original.translate(dx, dy));
            true
        }
        Some(DragState::ResizingRegion { handle }) => {
            if let Some(region) = state.document.capture_region {
                state.document.set_capture_region(region.resize_from_handle(
                    *handle,
                    screen_point,
                    MIN_REGION_SIZE,
                ));
                state.mark_static_dirty();
            }
            true
        }
        Some(DragState::MovingAnnotation {
            start,
            id,
            original,
        }) => {
            let dx = screen_point.x - start.x;
            let dy = screen_point.y - start.y;
            if let Some(annotation) = state.document.annotation_mut(*id) {
                *annotation = original.translated(dx, dy);
                state.document.selected_annotation_id = Some(*id);
                state.mark_static_dirty();
            }
            true
        }
        Some(DragState::EditingAnnotation { id, edit, original }) => {
            if let Some(annotation) = state.document.annotation_mut(*id) {
                *annotation = edited_annotation(original, *edit, screen_point);
                state.document.selected_annotation_id = Some(*id);
                state.mark_static_dirty();
            }
            true
        }
        Some(DragState::DrawingAnnotation {
            start,
            current,
            points,
        }) => {
            *current = constrained_drawing_point(state.active_tool, *start, screen_point);
            if state.active_tool == ToolKind::Pen {
                if points.last().is_none_or(|last| {
                    ((screen_point.x - last.x).powi(2) + (screen_point.y - last.y).powi(2)).sqrt()
                        >= PEN_POINT_SPACING
                }) {
                    points.push(screen_point);
                }
            } else if state.active_tool == ToolKind::Mosaic {
                points.push(screen_point);
            }
            true
        }
        Some(DragState::AdjustingSubmenuSlider { .. }) => true,
        None => {
            update_cursor_for_hover(state, point);
            if state.document.capture_region.is_none() {
                let next = detect_hover_region(state, screen_point);
                if state.hover_region != next {
                    state.hover_region = next;
                    return true;
                }
                return true;
            }
            false
        }
    }
}

fn constrained_drawing_point(tool: ToolKind, start: Point, current: Point) -> Point {
    if tool == ToolKind::Highlighter && unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 } {
        let dx = current.x - start.x;
        let dy = current.y - start.y;
        if dx.abs() >= dy.abs() {
            Point::new(current.x, start.y)
        } else {
            Point::new(start.x, current.y)
        }
    } else {
        current
    }
}

fn update_cursor_for_hover(state: &OverlayState, point: Point) {
    unsafe {
        let cursor = if state.submenu_rect.is_some_and(|rect| rect.contains(point))
            || state
                .submenu_buttons
                .iter()
                .any(|button| button.rect.contains(point))
            || state
                .submenu_sliders
                .iter()
                .any(|slider| slider.rect.contains(point))
        {
            IDC_ARROW
        } else if state
            .toolbar_buttons
            .iter()
            .any(|button| matches!(button.tool, ToolbarAction::Grip) && button.rect.contains(point))
        {
            IDC_HAND
        } else if state
            .toolbar_buttons
            .iter()
            .any(|button| button.rect.contains(point))
        {
            IDC_ARROW
        } else if let Some(region) = state.region_overlay() {
            if let Some(handle) = region.hit_resize_handle(point, HANDLE_RADIUS + 4.0) {
                cursor_for_resize_handle(handle)
            } else if region_frame_contains(region, point) {
                IDC_SIZEALL
            } else {
                let screen_point = state.overlay_to_screen(point);
                if let Some(annotation) = state
                    .document
                    .annotations
                    .iter()
                    .rev()
                    .find(|annotation| annotation_hit_test(annotation, screen_point))
                {
                    if let Some(edit) = hit_annotation_edit_handle(state, annotation, screen_point)
                    {
                        match edit {
                            AnnotationEdit::BoxResize(handle) => cursor_for_resize_handle(handle),
                            AnnotationEdit::LineStart
                            | AnnotationEdit::LineEnd
                            | AnnotationEdit::TagAnchor => IDC_SIZEALL,
                        }
                    } else {
                        IDC_SIZEALL
                    }
                } else if region.contains(point) {
                    IDC_CROSS
                } else {
                    IDC_ARROW
                }
            }
        } else {
            IDC_CROSS
        };
        if let Ok(handle) = LoadCursorW(None, cursor) {
            let _ = SetCursor(handle);
        }
    }
}

fn hit_annotation_edit_handle(
    _state: &OverlayState,
    annotation: &Annotation,
    screen_point: Point,
) -> Option<AnnotationEdit> {
    match &annotation.kind {
        AnnotationKind::Line { start, end }
        | AnnotationKind::Arrow { start, end }
        | AnnotationKind::Highlighter {
            shape: HighlightShape::Rectangle,
            start,
            end,
            ..
        } => {
            if near_point(screen_point, *start, HANDLE_RADIUS + 4.0) {
                Some(AnnotationEdit::LineStart)
            } else if near_point(screen_point, *end, HANDLE_RADIUS + 4.0) {
                Some(AnnotationEdit::LineEnd)
            } else {
                None
            }
        }
        AnnotationKind::Tag { anchor, .. } => {
            if near_point(screen_point, *anchor, HANDLE_RADIUS + 6.0) {
                Some(AnnotationEdit::TagAnchor)
            } else {
                annotation
                    .bounds
                    .hit_resize_handle(screen_point, HANDLE_RADIUS + 4.0)
                    .map(AnnotationEdit::BoxResize)
            }
        }
        AnnotationKind::Pen { .. } | AnnotationKind::PenArrow { .. } => None,
        _ => annotation
            .bounds
            .hit_resize_handle(screen_point, HANDLE_RADIUS + 4.0)
            .map(AnnotationEdit::BoxResize),
    }
}

fn near_point(point: Point, target: Point, radius: f32) -> bool {
    (point.x - target.x).abs() <= radius && (point.y - target.y).abs() <= radius
}

fn line_hit_radius(width: f32) -> f32 {
    (width / 2.0 + 6.0).max(8.0)
}

fn near_segment(point: Point, start: Point, end: Point, radius: f32) -> bool {
    distance_to_segment(point, start, end) <= radius
}

fn distance_to_segment(point: Point, start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return distance(point, start);
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / len_sq).clamp(0.0, 1.0);
    let closest = Point::new(start.x + dx * t, start.y + dy * t);
    distance(point, closest)
}

fn edited_annotation(original: &Annotation, edit: AnnotationEdit, to: Point) -> Annotation {
    let mut next = original.clone();
    match (&mut next.kind, edit) {
        (AnnotationKind::Line { start, .. }, AnnotationEdit::LineStart)
        | (AnnotationKind::Arrow { start, .. }, AnnotationEdit::LineStart)
        | (
            AnnotationKind::Highlighter {
                shape: HighlightShape::Rectangle,
                start,
                ..
            },
            AnnotationEdit::LineStart,
        ) => {
            *start = to;
            next.bounds = line_bounds(&next.kind);
        }
        (AnnotationKind::Line { end, .. }, AnnotationEdit::LineEnd)
        | (AnnotationKind::Arrow { end, .. }, AnnotationEdit::LineEnd)
        | (
            AnnotationKind::Highlighter {
                shape: HighlightShape::Rectangle,
                end,
                ..
            },
            AnnotationEdit::LineEnd,
        ) => {
            *end = to;
            next.bounds = line_bounds(&next.kind);
        }
        (_, AnnotationEdit::BoxResize(handle)) => {
            next.bounds = original
                .bounds
                .resize_from_handle(handle, to, MIN_REGION_SIZE);
        }
        (AnnotationKind::Tag { anchor, .. }, AnnotationEdit::TagAnchor) => {
            *anchor = to;
        }
        _ => {}
    }
    next
}

fn line_bounds(kind: &AnnotationKind) -> Rect {
    match kind {
        AnnotationKind::Line { start, end }
        | AnnotationKind::Arrow { start, end }
        | AnnotationKind::Highlighter {
            shape: HighlightShape::Rectangle,
            start,
            end,
            ..
        } => Rect::from_points(*start, *end),
        _ => Rect::default(),
    }
}

fn cursor_for_resize_handle(handle: ResizeHandle) -> windows::core::PCWSTR {
    match handle {
        ResizeHandle::North | ResizeHandle::South => IDC_SIZENS,
        ResizeHandle::East | ResizeHandle::West => IDC_SIZEWE,
        ResizeHandle::NorthWest | ResizeHandle::SouthEast => IDC_SIZENWSE,
        ResizeHandle::NorthEast | ResizeHandle::SouthWest => IDC_SIZENESW,
    }
}

fn handle_mouse_up(state: &mut OverlayState, point: Point) {
    let Some(drag) = state.drag.take() else {
        return;
    };

    match drag {
        DragState::Selecting { start, current } => {
            let rect = Rect::from_points(
                state.overlay_to_screen(start),
                state.overlay_to_screen(current),
            );
            let drag_distance =
                ((current.x - start.x).powi(2) + (current.y - start.y).powi(2)).sqrt();
            if drag_distance <= CLICK_DRAG_THRESHOLD {
                if let Some(region) = state.hover_region {
                    state.document.set_capture_region(region);
                    state.toolbar_origin = Some(toolbar_origin_near_point(state, point, region));
                }
            } else if rect.is_visible() {
                state.document.set_capture_region(rect);
                state.toolbar_origin = Some(toolbar_origin_near_point(
                    state,
                    current,
                    rect.translate(-state.screen_bounds.x, -state.screen_bounds.y),
                ));
            }
        }
        DragState::MovingRegion { .. }
        | DragState::MovingToolbar { .. }
        | DragState::ResizingRegion { .. }
        | DragState::MovingAnnotation { .. }
        | DragState::EditingAnnotation { .. }
        | DragState::AdjustingSubmenuSlider { .. } => {}
        DragState::DrawingAnnotation {
            start,
            current,
            points,
        } => {
            state.checkpoint();
            let end = state.overlay_to_screen(point);
            let bounds = Rect::from_points(start, end);
            if bounds.is_visible()
                || matches!(
                    state.active_tool,
                    ToolKind::Text
                        | ToolKind::Tag
                        | ToolKind::Watermark
                        | ToolKind::Pen
                        | ToolKind::Highlighter
                )
            {
                let mut annotation = annotation_from_tool(state, start, current, points);
                let is_textual = matches!(
                    annotation.kind,
                    AnnotationKind::Text { .. } | AnnotationKind::Tag { .. }
                );
                let is_step = matches!(annotation.kind, AnnotationKind::StepNumber { .. });
                if state.numbering_enabled && annotation.accepts_auto_numbering() {
                    annotation.step_number = Some(state.next_step_number);
                    state.next_step_number += 1;
                }
                let id = state.document.add_annotation(annotation);
                if is_textual {
                    state.editing_text_id = Some(id);
                }
                if is_step {
                    state.next_step_number += 1;
                }
            }
        }
    }
}

fn shift_key_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 }
}

fn handle_key_down(state: &mut OverlayState, key: u32, shift_down: bool) {
    let ctrl_down = unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 };
    if state.editing_watermark_text && !ctrl_down {
        match key {
            0x1B => state.editing_watermark_text = false,
            key if key == VK_RETURN.0 as u32 => state.editing_watermark_text = false,
            0x08 => {
                state.watermark_text.pop();
            }
            0x2E => state.watermark_text.clear(),
            _ => {}
        }
        return;
    }
    if state.editing_text_id.is_some() && !ctrl_down {
        match key {
            0x1B => state.editing_text_id = None,
            key if key == VK_RETURN.0 as u32 => {
                if shift_down && editing_text_accepts_line_break(state) {
                    edit_selected_text(state, |text| text.push('\n'));
                } else {
                    state.editing_text_id = None;
                }
            }
            0x08 => {
                edit_selected_text(state, |text| {
                    text.pop();
                });
            }
            _ => {}
        }
        return;
    }
    if state.editing_step_number_id.is_some() && !ctrl_down {
        match key {
            0x1B => {
                state.editing_step_number_id = None;
                state.editing_step_number_replace = false;
            }
            key if key == VK_RETURN.0 as u32 => {
                state.editing_step_number_id = None;
                state.editing_step_number_replace = false;
            }
            0x08 => edit_selected_step_number(state, |number| number / 10),
            0x2E => edit_selected_step_number(state, |_| 0),
            _ => {}
        }
        return;
    }

    match key {
        0x1B => unsafe {
            let _ = DestroyWindow(state.hwnd);
        },
        key if key == VK_RETURN.0 as u32 => {
            let _ = unsafe { copy_capture_to_clipboard(state) };
            unsafe {
                let _ = DestroyWindow(state.hwnd);
            }
        }
        0x5A if ctrl_down => {
            if let Some(previous) = state.history.undo(&state.document) {
                state.document = previous;
                state.force_web_full_snapshot = true;
                state.mark_static_dirty();
            }
        }
        0x53 if ctrl_down => {
            let _ = unsafe { save_capture_to_file(state) };
        }
        0x2E => {
            state.checkpoint();
            let _ = state.document.remove_selected();
            state.mark_static_dirty();
        }
        _ => {}
    }
}

fn handle_char(state: &mut OverlayState, char_code: u32) {
    if char_code < 32 || char_code == 127 {
        return;
    }
    let Some(ch) = char::from_u32(char_code) else {
        return;
    };
    if state.editing_watermark_text {
        state.watermark_text.push(ch);
        state.mark_static_dirty();
        return;
    }
    if state.editing_step_number_id.is_some() {
        if let Some(digit) = ch.to_digit(10) {
            let replace = state.editing_step_number_replace;
            edit_selected_step_number(state, |number| {
                if replace {
                    digit
                } else {
                    number.saturating_mul(10).saturating_add(digit)
                }
            });
            state.editing_step_number_replace = false;
            state.mark_static_dirty();
        }
        return;
    }
    if state.editing_text_id.is_none() {
        return;
    }
    edit_selected_text(state, |text| text.push(ch));
    state.mark_static_dirty();
}

fn editing_text_accepts_line_break(state: &OverlayState) -> bool {
    let Some(id) = state.editing_text_id else {
        return false;
    };
    state
        .document
        .annotation(id)
        .is_some_and(|annotation| matches!(annotation.kind, AnnotationKind::Text { .. }))
}

fn edit_selected_step_number(state: &mut OverlayState, edit: impl FnOnce(u32) -> u32) {
    let Some(id) = state.editing_step_number_id else {
        return;
    };
    if let Some(annotation) = state.document.annotation_mut(id) {
        let current = annotation.display_step_number().unwrap_or(0);
        let next = edit(current);
        match &mut annotation.kind {
            AnnotationKind::StepNumber { number } => *number = next,
            _ => annotation.step_number = Some(next),
        }
        state.next_step_number = state.next_step_number.max(next.saturating_add(1));
        state.editing_step_number_replace = false;
    }
}

fn edit_selected_text(state: &mut OverlayState, edit: impl FnOnce(&mut String)) {
    let Some(id) = state.editing_text_id else {
        return;
    };
    if let Some(annotation) = state.document.annotation_mut(id) {
        let mut edited_tag = false;
        match &mut annotation.kind {
            AnnotationKind::Text { text, .. } => edit(text),
            AnnotationKind::Tag { label, .. } => {
                edit(label);
                edited_tag = true;
            }
            _ => {}
        }
        if edited_tag {
            let font_size = match &annotation.kind {
                AnnotationKind::Tag { font_size, .. } => *font_size,
                _ => state.font_size,
            };
            resize_tag_for_text(annotation, font_size);
        }
    }
}

fn handle_toolbar_action(state: &mut OverlayState, action: ToolbarAction) {
    match action {
        ToolbarAction::Grip => {}
        ToolbarAction::Numbering => {
            state.numbering_enabled = !state.numbering_enabled;
            state.numbering_toggle_progress = if state.numbering_enabled { 0.0 } else { 1.0 };
            if state.numbering_enabled {
                state.next_step_number = next_available_step_number(state);
            }
            state.active_submenu = Some(ToolKind::StepNumber);
            unsafe {
                let _ = SetTimer(state.hwnd, NUMBERING_TOGGLE_TIMER_ID, 8, None);
            }
        }
        ToolbarAction::Tool(tool) => {
            finish_text_editing(state);
            if state.active_tool == ToolKind::Highlighter && tool != ToolKind::Highlighter {
                state.current_stroke.color = state.normal_stroke_color;
            } else if state.active_tool != ToolKind::Highlighter && tool == ToolKind::Highlighter {
                state.normal_stroke_color = state.current_stroke.color;
            }
            state.active_tool = tool;
            set_active_tool_stroke_width(state, tool);
            if tool == ToolKind::Highlighter {
                state.current_stroke.color = annotation_colors()[2];
            }
            if tool == ToolKind::Watermark {
                state.editing_watermark_text = true;
            }
            state.active_submenu = configurable_submenu_tool(tool);
        }
        ToolbarAction::Divider => state.active_submenu = None,
        ToolbarAction::Undo => {
            state.active_submenu = None;
            if let Some(previous) = state.history.undo(&state.document) {
                state.document = previous;
                state.force_web_full_snapshot = true;
                state.mark_static_dirty();
            }
        }
        ToolbarAction::Copy => {
            state.active_submenu = None;
            let _ = unsafe { copy_capture_to_clipboard(state) };
            unsafe {
                let _ = DestroyWindow(state.hwnd);
            }
        }
        ToolbarAction::Save => {
            state.active_submenu = None;
            let _ = unsafe { save_capture_to_file(state) };
        }
        ToolbarAction::Cancel => unsafe {
            let _ = DestroyWindow(state.hwnd);
        },
    }
}

fn step_numbering_toggle_animation(state: &mut OverlayState) -> bool {
    let target = if state.numbering_enabled { 1.0 } else { 0.0 };
    let delta = target - state.numbering_toggle_progress;
    let step = 0.55;
    if delta.abs() <= step {
        state.numbering_toggle_progress = target;
        return false;
    }
    state.numbering_toggle_progress += delta.signum() * step;
    true
}

fn tool_width_index(tool: ToolKind) -> usize {
    match tool {
        ToolKind::StepNumber => 0,
        ToolKind::Rectangle => 1,
        ToolKind::Oval => 2,
        ToolKind::Line => 3,
        ToolKind::Arrow => 4,
        ToolKind::Pen => 5,
        ToolKind::Text => 6,
        ToolKind::Tag => 7,
        ToolKind::Mosaic => 8,
        ToolKind::Highlighter => 9,
        ToolKind::Watermark => 10,
    }
}

fn tool_has_stroke_width(tool: ToolKind) -> bool {
    matches!(
        tool,
        ToolKind::Rectangle
            | ToolKind::Oval
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Pen
            | ToolKind::Highlighter
            | ToolKind::Tag
    )
}

fn set_active_tool_stroke_width(state: &mut OverlayState, tool: ToolKind) {
    if tool_has_stroke_width(tool) {
        state.current_stroke.width = state.tool_stroke_widths[tool_width_index(tool)];
    }
}

fn remember_tool_stroke_width(state: &mut OverlayState, tool: ToolKind, width: f32) {
    if tool_has_stroke_width(tool) {
        state.tool_stroke_widths[tool_width_index(tool)] = width;
    }
}

fn configurable_submenu_tool(tool: ToolKind) -> Option<ToolKind> {
    matches!(
        tool,
        ToolKind::Rectangle
            | ToolKind::Oval
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Pen
            | ToolKind::Highlighter
            | ToolKind::Text
            | ToolKind::Tag
            | ToolKind::Watermark
    )
    .then_some(tool)
}

fn finish_text_editing(state: &mut OverlayState) {
    state.editing_text_id = None;
    state.editing_watermark_text = false;
    state.editing_step_number_id = None;
    state.editing_step_number_replace = false;
}

fn handle_numbering_mode(state: &mut OverlayState, mode: &str) {
    match mode {
        "restart" => {
            state.next_step_number = 1;
            state.numbering_enabled = true;
        }
        "continue" => {
            state.next_step_number = next_available_step_number(state);
            state.numbering_enabled = true;
        }
        _ => return,
    }
    state.numbering_toggle_progress = 1.0;
    state.active_submenu = Some(ToolKind::StepNumber);
}

fn start_step_number_editing(state: &mut OverlayState, id: AnnotationId) {
    if state
        .document
        .annotations
        .iter()
        .any(|annotation| annotation.id == id && annotation.display_step_number().is_some())
    {
        state.checkpoint();
        state.editing_text_id = None;
        state.editing_watermark_text = false;
        state.editing_step_number_id = Some(id);
        state.editing_step_number_replace = true;
        state.document.selected_annotation_id = Some(id);
    }
}

fn handle_submenu_action(state: &mut OverlayState, action: SubmenuAction) {
    let apply_to_selected = !matches!(
        action,
        SubmenuAction::Color(_) if state.active_submenu == Some(ToolKind::Watermark)
    );
    match action {
        SubmenuAction::StrokeWidth(width) => {
            state.current_stroke.width = width;
            if let Some(tool) = state.active_submenu {
                remember_tool_stroke_width(state, tool, width);
            }
        }
        SubmenuAction::Color(color) => {
            if state.active_submenu == Some(ToolKind::Watermark) {
                state.watermark_color = color;
            } else {
                state.current_stroke.color = color;
                if state.active_tool != ToolKind::Highlighter {
                    state.normal_stroke_color = color;
                }
            }
        }
        SubmenuAction::PenMode(mode) => state.pen_mode = mode,
        SubmenuAction::HighlighterShape(shape) => state.highlighter_shape = shape,
        SubmenuAction::TextFilled(filled) => state.text_filled = filled,
        SubmenuAction::FontSize(size) => state.font_size = size,
        SubmenuAction::WatermarkMode(mode) => handle_watermark_mode_action(state, mode),
        SubmenuAction::WatermarkTextInput => {
            state.editing_watermark_text = true;
            state.editing_text_id = None;
        }
        SubmenuAction::ClearWatermark => clear_watermark(state),
    }
    if apply_to_selected {
        apply_submenu_to_selected_annotation(state, action);
    }
    state.mark_static_dirty();
}

fn handle_watermark_mode_action(state: &mut OverlayState, mode: WatermarkMode) {
    state.watermark_mode = mode;
    match mode {
        WatermarkMode::Date => state.watermark_date_enabled = !state.watermark_date_enabled,
        WatermarkMode::Text => state.editing_watermark_text = true,
        WatermarkMode::Image => {
            if let Some(path) = unsafe { show_open_image_dialog(state.hwnd) } {
                let bitmap =
                    load_watermark_bitmap(&path, scaled(state, 72.0).round().max(1.0) as u32)
                        .map(|bitmap| faded_watermark_bitmap(&bitmap, WATERMARK_OPACITY));
                state.watermark_image_data_url =
                    bitmap.as_ref().and_then(watermark_bitmap_data_url);
                state.watermark_image_path = Some(path);
                state.watermark_image_bitmap = bitmap;
            }
        }
    }
    state.mark_static_dirty();
}

fn clear_watermark(state: &mut OverlayState) {
    state.watermark_mode = WatermarkMode::Text;
    state.watermark_date_enabled = false;
    state.watermark_text.clear();
    state.watermark_image_path = None;
    state.watermark_image_bitmap = None;
    state.watermark_image_data_url = None;
    state.editing_watermark_text = false;
    state.mark_static_dirty();
}

fn apply_submenu_slider_at(state: &mut OverlayState, slider: SubmenuSlider, point: Point) {
    let t = ((point.x - slider.start_x) / (slider.end_x - slider.start_x)).clamp(0.0, 1.0);
    let value = slider.min + (slider.max - slider.min) * t;
    let action = match slider.kind {
        SubmenuSliderKind::StrokeWidth => SubmenuAction::StrokeWidth(value),
        SubmenuSliderKind::FontSize => SubmenuAction::FontSize(value),
    };
    handle_submenu_action(state, action);
}

fn apply_submenu_to_selected_annotation(state: &mut OverlayState, action: SubmenuAction) {
    let Some(id) = state.document.selected_annotation_id else {
        return;
    };
    let Some(annotation) = state.document.annotation_mut(id) else {
        return;
    };
    match action {
        SubmenuAction::StrokeWidth(width) => match annotation.kind {
            AnnotationKind::Rectangle
            | AnnotationKind::Oval
            | AnnotationKind::Line { .. }
            | AnnotationKind::Arrow { .. }
            | AnnotationKind::Pen { .. }
            | AnnotationKind::PenArrow { .. }
            | AnnotationKind::Tag { .. }
            | AnnotationKind::Highlighter {
                shape: HighlightShape::Rectangle,
                ..
            } => annotation.stroke.width = width,
            _ => {}
        },
        SubmenuAction::Color(color) => {
            annotation.stroke.color = color;
        }
        SubmenuAction::FontSize(size) => match &mut annotation.kind {
            AnnotationKind::Text { font_size, .. } | AnnotationKind::Tag { font_size, .. } => {
                *font_size = size;
            }
            _ => {}
        },
        SubmenuAction::TextFilled(next_filled) => {
            if let AnnotationKind::Text { filled, .. } = &mut annotation.kind {
                *filled = next_filled;
            }
        }
        SubmenuAction::PenMode(mode) => {
            let replacement = match (&annotation.kind, mode) {
                (AnnotationKind::Pen { points }, PenMode::Arrow) => {
                    Some(AnnotationKind::PenArrow {
                        points: points.clone(),
                    })
                }
                (AnnotationKind::PenArrow { points }, PenMode::Free) => Some(AnnotationKind::Pen {
                    points: points.clone(),
                }),
                _ => None,
            };
            if let Some(kind) = replacement {
                annotation.kind = kind;
            }
        }
        SubmenuAction::HighlighterShape(_) => {}
        _ => {}
    }
}

fn annotation_from_tool(
    state: &OverlayState,
    start: Point,
    end: Point,
    points: Vec<Point>,
) -> Annotation {
    let bounds = if state.active_tool == ToolKind::Tag {
        tag_bounds_from_drag(state, start, end)
    } else {
        Rect::from_points(start, end)
    };
    let stroke = state.current_stroke;
    let kind = match state.active_tool {
        ToolKind::StepNumber => AnnotationKind::StepNumber {
            number: state.next_step_number,
        },
        ToolKind::Rectangle => AnnotationKind::Rectangle,
        ToolKind::Oval => AnnotationKind::Oval,
        ToolKind::Line => AnnotationKind::Line { start, end },
        ToolKind::Arrow => AnnotationKind::Arrow { start, end },
        ToolKind::Text => AnnotationKind::Text {
            text: String::new(),
            font_size: state.font_size,
            framed: Rect::from_points(start, end).is_visible(),
            filled: state.text_filled,
        },
        ToolKind::Tag => AnnotationKind::Tag {
            label: String::new(),
            anchor: start,
            font_size: state.font_size,
        },
        ToolKind::Mosaic => AnnotationKind::Mosaic {
            mode: if points.len() > 2 {
                MosaicMode::Brush
            } else {
                MosaicMode::Area
            },
            brush_size: state.mosaic_brush_size,
        },
        ToolKind::Highlighter => AnnotationKind::Highlighter {
            shape: state.highlighter_shape,
            opacity: state.highlighter_opacity,
            start,
            end,
        },
        ToolKind::Pen => {
            let points = smooth_pen_points(&points);
            match state.pen_mode {
                PenMode::Free => AnnotationKind::Pen { points },
                PenMode::Arrow => AnnotationKind::PenArrow { points },
            }
        }
        ToolKind::Watermark => AnnotationKind::Watermark {
            text: String::new(),
            opacity: 0.0,
        },
    };
    Annotation::new(0, kind, bounds, stroke)
}

fn smooth_pen_points(points: &[Point]) -> Vec<Point> {
    if points.len() < 4 {
        return points.to_vec();
    }
    let mut smoothed = points.to_vec();
    for _ in 0..2 {
        let mut next = Vec::with_capacity(smoothed.len());
        next.push(smoothed[0]);
        for index in 1..smoothed.len() - 1 {
            let prev = smoothed[index - 1];
            let current = smoothed[index];
            let after = smoothed[index + 1];
            next.push(Point::new(
                prev.x * 0.22 + current.x * 0.56 + after.x * 0.22,
                prev.y * 0.22 + current.y * 0.56 + after.y * 0.22,
            ));
        }
        next.push(*smoothed.last().expect("smoothed has endpoints"));
        smoothed = next;
    }
    smoothed
}

fn next_available_step_number(state: &OverlayState) -> u32 {
    state
        .document
        .annotations
        .iter()
        .filter_map(Annotation::display_step_number)
        .max()
        .unwrap_or(0)
        + 1
}

fn tag_default_size(state: &OverlayState) -> (f32, f32) {
    (
        scaled(state, TAG_DEFAULT_WIDTH),
        scaled(state, TAG_DEFAULT_HEIGHT),
    )
}

fn tag_bounds_from_drag(state: &OverlayState, anchor: Point, release: Point) -> Rect {
    let (width, height) = tag_default_size(state);
    if distance(anchor, release) <= CLICK_DRAG_THRESHOLD {
        Rect::new(
            anchor.x + scaled(state, 28.0),
            anchor.y - height / 2.0,
            width,
            height,
        )
    } else {
        let dx = release.x - anchor.x;
        let dy = release.y - anchor.y;
        if dx.abs() >= dy.abs() {
            let x = if dx >= 0.0 {
                release.x
            } else {
                release.x - width
            };
            Rect::new(x, release.y - height / 2.0, width, height)
        } else {
            let y = if dy >= 0.0 {
                release.y
            } else {
                release.y - height
            };
            Rect::new(release.x - width / 2.0, y, width, height)
        }
    }
}

unsafe fn paint_overlay(state: &mut OverlayState) {
    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = windows::Win32::Graphics::Gdi::BeginPaint(state.hwnd, &mut ps);

    ensure_render_cache(state, hdc);
    let width = state.screen_bounds.width.round() as i32;
    let height = state.screen_bounds.height.round() as i32;

    if state.region_overlay().is_some() {
        if state.static_layer_dirty {
            if let Some(cache) = state.render_cache.as_ref() {
                paint_static_capture_surface(cache.static_dc, state);
            }
            state.static_layer_dirty = false;
        }
        if let Some(cache) = state.render_cache.as_ref() {
            let back_dc = cache.back_dc;
            let _ = BitBlt(back_dc, 0, 0, width, height, cache.static_dc, 0, 0, SRCCOPY);
            if let Some(region) = state.region_overlay() {
                draw_region_gradient_border(back_dc, state, region);
            }
            if web_ui_owns_pointer_input(state) && state.active_tool == ToolKind::Mosaic {
                draw_drag_preview(back_dc, state);
            }
            if !web_ui_owns_pointer_input(state) {
                draw_selected_annotation(back_dc, state);
                draw_drag_preview(back_dc, state);
                if let Some(region) = state.region_overlay() {
                    draw_toolbar(back_dc, state, region);
                    draw_tool_submenu(back_dc, state);
                }
            }
        }
    } else if let Some(cache) = state.render_cache.as_ref() {
        paint_overlay_surface(cache.back_dc, state);
    }

    if let Some(cache) = state.render_cache.as_ref() {
        let _ = BitBlt(hdc, 0, 0, width, height, cache.back_dc, 0, 0, SRCCOPY);
    }

    let _ = windows::Win32::Graphics::Gdi::EndPaint(state.hwnd, &ps);
}

unsafe fn ensure_render_cache(state: &mut OverlayState, hdc: HDC) {
    let width = state.screen_bounds.width.round() as i32;
    let height = state.screen_bounds.height.round() as i32;
    let needs_rebuild = state
        .render_cache
        .as_ref()
        .is_none_or(|cache| cache.width != width || cache.height != height);
    if !needs_rebuild {
        return;
    }

    destroy_render_cache(state);

    let back_dc = CreateCompatibleDC(hdc);
    let back_bitmap = CreateCompatibleBitmap(hdc, width, height);
    let back_old = SelectObject(back_dc, back_bitmap);

    let static_dc = CreateCompatibleDC(hdc);
    let static_bitmap = CreateCompatibleBitmap(hdc, width, height);
    let static_old = SelectObject(static_dc, static_bitmap);

    state.render_cache = Some(RenderCache {
        width,
        height,
        back_dc,
        back_bitmap,
        back_old,
        static_dc,
        static_bitmap,
        static_old,
    });
    state.static_layer_dirty = true;
}

unsafe fn destroy_render_cache(state: &mut OverlayState) {
    let Some(cache) = state.render_cache.take() else {
        return;
    };
    let _ = SelectObject(cache.back_dc, cache.back_old);
    let _ = DeleteObject(cache.back_bitmap);
    let _ = DeleteDC(cache.back_dc);
    let _ = SelectObject(cache.static_dc, cache.static_old);
    let _ = DeleteObject(cache.static_bitmap);
    let _ = DeleteDC(cache.static_dc);
}

unsafe fn paint_static_capture_surface(hdc: HDC, state: &OverlayState) {
    paint_background(hdc, state);
    let Some(region) = state.region_overlay() else {
        return;
    };
    draw_dim_outside_region(hdc, state.screen_bounds, region);
    draw_handles(hdc, region);
    if web_ui_owns_pointer_input(state) {
        draw_native_backed_annotations(hdc, state);
    } else {
        draw_annotations(hdc, state);
        draw_watermark_pattern(hdc, state, region);
        draw_watermark_annotations(hdc, state);
    }
}

unsafe fn paint_overlay_surface(hdc: HDC, state: &mut OverlayState) {
    paint_background(hdc, state);

    if let Some(region) = state.region_overlay() {
        draw_dim_outside_region(hdc, state.screen_bounds, region);
        draw_region_gradient_border(hdc, state, region);
        draw_handles(hdc, region);
        if web_ui_owns_pointer_input(state) {
            draw_native_backed_annotations(hdc, state);
        } else {
            draw_annotations(hdc, state);
            draw_selected_annotation(hdc, state);
            draw_drag_preview(hdc, state);
            draw_watermark_pattern(hdc, state, region);
            draw_watermark_annotations(hdc, state);
            draw_toolbar(hdc, state, region);
            draw_tool_submenu(hdc, state);
        }
    } else if let Some(DragState::Selecting { start, current }) = state.drag {
        let region = Rect::from_points(start, current);
        draw_region_gradient_border(hdc, state, region);
        if !web_ui_owns_pointer_input(state) {
            draw_sniper_cursor(hdc, current);
        }
    } else if let Some(region) = state
        .hover_region
        .map(|region| region.translate(-state.screen_bounds.x, -state.screen_bounds.y))
    {
        draw_dim_outside_region(hdc, state.screen_bounds, region);
        draw_region_gradient_border(hdc, state, region);
        if !web_ui_owns_pointer_input(state) {
            draw_sniper_cursor(hdc, state.cursor_position);
        }
    } else if !web_ui_owns_pointer_input(state) {
        draw_sniper_cursor(hdc, state.cursor_position);
    }
}

unsafe fn draw_sniper_cursor(hdc: HDC, point: Point) {
    let color = Color::rgb(255, 255, 255);
    let _pen = SelectedPen::new(hdc, 2.0, color);
    let radius = 10.0;
    let gap = 5.0;
    let arm = 20.0;
    let rect = rect_to_rect(Rect::new(
        point.x - radius,
        point.y - radius,
        radius * 2.0,
        radius * 2.0,
    ));
    let _brush = SelectedStockObject::null_brush(hdc);
    let _ = Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
    for (start, end) in [
        (
            Point::new(point.x - arm, point.y),
            Point::new(point.x - gap, point.y),
        ),
        (
            Point::new(point.x + gap, point.y),
            Point::new(point.x + arm, point.y),
        ),
        (
            Point::new(point.x, point.y - arm),
            Point::new(point.x, point.y - gap),
        ),
        (
            Point::new(point.x, point.y + gap),
            Point::new(point.x, point.y + arm),
        ),
    ] {
        let _ = MoveToEx(hdc, start.x.round() as i32, start.y.round() as i32, None);
        let _ = LineTo(hdc, end.x.round() as i32, end.y.round() as i32);
    }
}

unsafe fn draw_dim_outside_region(hdc: HDC, screen_bounds: Rect, region: Rect) {
    let full = Rect::new(0.0, 0.0, screen_bounds.width, screen_bounds.height);
    for rect in [
        Rect::new(full.x, full.y, full.width, region.y.max(0.0)),
        Rect::new(
            full.x,
            region.bottom(),
            full.width,
            (full.bottom() - region.bottom()).max(0.0),
        ),
        Rect::new(full.x, region.y, region.x.max(0.0), region.height),
        Rect::new(
            region.right(),
            region.y,
            (full.right() - region.right()).max(0.0),
            region.height,
        ),
    ] {
        if rect.is_visible() {
            alpha_fill_rect(hdc, rect, Color::BLACK, 85);
        }
    }
}

unsafe fn draw_region_gradient_border(hdc: HDC, state: &OverlayState, region: Rect) {
    if !region.is_visible() {
        return;
    }
    let left = region.x.max(0.0);
    let top = region.y.max(0.0);
    let right = region.right().min(state.screen_bounds.width);
    let bottom = region.bottom().min(state.screen_bounds.height);
    let visible = Rect::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0));
    if !visible.is_visible() {
        return;
    }
    let inset = 3.0;
    let rect = Rect::new(
        visible.x + inset,
        visible.y + inset,
        (visible.width - inset * 2.0).max(1.0),
        (visible.height - inset * 2.0).max(1.0),
    );
    let radius = 14.0_f32.min(rect.width / 2.0).min(rect.height / 2.0);
    let points = rounded_region_border_points(rect, radius);
    if points.len() < 2 {
        return;
    }
    let phase = animated_region_phase();
    let count = points.len() - 1;
    for index in 0..count {
        let color = animated_region_border_color(index as f32 / count as f32 + phase);
        let _pen = SelectedPen::new(hdc, 2.0, color);
        let start = points[index];
        let end = points[index + 1];
        let _ = MoveToEx(hdc, start.x.round() as i32, start.y.round() as i32, None);
        let _ = LineTo(hdc, end.x.round() as i32, end.y.round() as i32);
    }
}

fn rounded_region_border_points(rect: Rect, radius: f32) -> Vec<Point> {
    let mut points = Vec::with_capacity(129);
    let straight_steps = 24;
    let arc_steps = 8;
    push_line_points(
        &mut points,
        Point::new(rect.x + radius, rect.y),
        Point::new(rect.right() - radius, rect.y),
        straight_steps,
    );
    push_arc_points(
        &mut points,
        Point::new(rect.right() - radius, rect.y + radius),
        radius,
        -std::f32::consts::FRAC_PI_2,
        0.0,
        arc_steps,
    );
    push_line_points(
        &mut points,
        Point::new(rect.right(), rect.y + radius),
        Point::new(rect.right(), rect.bottom() - radius),
        straight_steps,
    );
    push_arc_points(
        &mut points,
        Point::new(rect.right() - radius, rect.bottom() - radius),
        radius,
        0.0,
        std::f32::consts::FRAC_PI_2,
        arc_steps,
    );
    push_line_points(
        &mut points,
        Point::new(rect.right() - radius, rect.bottom()),
        Point::new(rect.x + radius, rect.bottom()),
        straight_steps,
    );
    push_arc_points(
        &mut points,
        Point::new(rect.x + radius, rect.bottom() - radius),
        radius,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        arc_steps,
    );
    push_line_points(
        &mut points,
        Point::new(rect.x, rect.bottom() - radius),
        Point::new(rect.x, rect.y + radius),
        straight_steps,
    );
    push_arc_points(
        &mut points,
        Point::new(rect.x + radius, rect.y + radius),
        radius,
        std::f32::consts::PI,
        std::f32::consts::PI * 1.5,
        arc_steps,
    );
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}

fn push_line_points(points: &mut Vec<Point>, start: Point, end: Point, steps: usize) {
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        points.push(Point::new(
            start.x + (end.x - start.x) * t,
            start.y + (end.y - start.y) * t,
        ));
    }
}

fn push_arc_points(
    points: &mut Vec<Point>,
    center: Point,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    steps: usize,
) {
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let angle = start_angle + (end_angle - start_angle) * t;
        points.push(Point::new(
            center.x + angle.cos() * radius,
            center.y + angle.sin() * radius,
        ));
    }
}

fn animated_region_phase() -> f32 {
    const REGION_BORDER_CYCLE_MS: u128 = 6087;
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            (duration.as_millis() % REGION_BORDER_CYCLE_MS) as f32 / REGION_BORDER_CYCLE_MS as f32
        })
        .unwrap_or(0.0)
}

fn animated_region_border_color(t: f32) -> Color {
    let colors = region_preview_colors();
    let wrapped = t.rem_euclid(1.0) * colors.len() as f32;
    let index = wrapped.floor() as usize % colors.len();
    let next = (index + 1) % colors.len();
    let local = wrapped - wrapped.floor();
    lerp_color(colors[index], colors[next], local)
}

fn region_preview_colors() -> [Color; 6] {
    [
        Color::rgb(0xff, 0x7f, 0x83),
        Color::rgb(0xff, 0x63, 0x00),
        Color::rgb(0xff, 0x3b, 0x30),
        Color::rgb(0xff, 0xaa, 0xac),
        Color::rgb(0xff, 0x63, 0x00),
        Color::rgb(0xff, 0x7f, 0x83),
    ]
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let mix = |from: u8, to: u8| -> u8 {
        (from as f32 + (to as f32 - from as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

unsafe fn alpha_fill_rect(hdc: HDC, rect: Rect, color: Color, alpha: u8) {
    let width = rect.width.round() as i32;
    let height = rect.height.round() as i32;
    if width <= 0 || height <= 0 {
        return;
    }
    let mem_dc = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, width, height);
    let old = SelectObject(mem_dc, bitmap);
    let brush = CreateSolidBrush(crate::util::colorref(color));
    let local_rect = Rect::new(0.0, 0.0, rect.width, rect.height);
    FillRect(mem_dc, &rect_to_rect(local_rect), brush);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: 0,
    };
    let _ = AlphaBlend(
        hdc,
        rect.x.round() as i32,
        rect.y.round() as i32,
        width,
        height,
        mem_dc,
        0,
        0,
        width,
        height,
        blend,
    );
    let _ = DeleteObject(brush);
    let _ = SelectObject(mem_dc, old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem_dc);
}

unsafe fn capture_screen_bitmap(screen_bounds: Rect) -> HBITMAP {
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let bitmap = CreateCompatibleBitmap(
        screen_dc,
        screen_bounds.width.round() as i32,
        screen_bounds.height.round() as i32,
    );
    let _ = SelectObject(mem_dc, bitmap);
    let _ = BitBlt(
        mem_dc,
        0,
        0,
        screen_bounds.width.round() as i32,
        screen_bounds.height.round() as i32,
        screen_dc,
        screen_bounds.x.round() as i32,
        screen_bounds.y.round() as i32,
        SRCCOPY,
    );
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    bitmap
}

unsafe fn paint_background(hdc: HDC, state: &OverlayState) {
    let mem_dc = CreateCompatibleDC(hdc);
    let _ = SelectObject(mem_dc, state.background_bitmap);
    let _ = BitBlt(
        hdc,
        0,
        0,
        state.screen_bounds.width.round() as i32,
        state.screen_bounds.height.round() as i32,
        mem_dc,
        0,
        0,
        SRCCOPY,
    );
    let _ = DeleteDC(mem_dc);
}

unsafe fn draw_handles(hdc: HDC, region: Rect) {
    let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00ffffff));
    let points = [
        Point::new(region.x, region.y),
        Point::new(region.center().x, region.y),
        Point::new(region.right(), region.y),
        Point::new(region.right(), region.center().y),
        Point::new(region.right(), region.bottom()),
        Point::new(region.center().x, region.bottom()),
        Point::new(region.x, region.bottom()),
        Point::new(region.x, region.center().y),
    ];
    for point in points {
        let handle = Rect::new(
            point.x - HANDLE_RADIUS,
            point.y - HANDLE_RADIUS,
            HANDLE_RADIUS * 2.0,
            HANDLE_RADIUS * 2.0,
        );
        FillRect(hdc, &rect_to_rect(handle), brush);
    }
    let _ = DeleteObject(brush);
}

#[derive(Clone, Copy)]
struct ToolbarPalette {
    background: Color,
    icon: Color,
    icon_background: Color,
    selected_icon_background: Color,
    border_top: Color,
    border_bottom: Color,
    divider: Color,
}

fn toolbar_palette(theme: AppTheme) -> ToolbarPalette {
    match theme {
        AppTheme::Light => ToolbarPalette {
            background: Color::rgb(0xf2, 0xf2, 0xf2),
            icon: Color::rgb(0x4d, 0x4d, 0x4d),
            icon_background: Color::rgb(0xf2, 0xf2, 0xf2),
            selected_icon_background: Color::WHITE,
            border_top: Color::WHITE,
            border_bottom: Color::rgb(0xd4, 0xd4, 0xd4),
            divider: Color::rgb(0xb8, 0xb8, 0xb8),
        },
        AppTheme::Dark => ToolbarPalette {
            background: Color::rgb(0x1a, 0x1a, 0x1a),
            icon: Color::rgb(0xb3, 0xb3, 0xb3),
            icon_background: Color::rgb(0x1a, 0x1a, 0x1a),
            selected_icon_background: Color::rgb(0x33, 0x33, 0x33),
            border_top: Color::rgb(0x36, 0x36, 0x36),
            border_bottom: Color::BLACK,
            divider: Color::rgb(0x66, 0x66, 0x66),
        },
    }
}

fn toolbar_width(state: &OverlayState) -> f32 {
    scaled(state, 36.0 + 24.0 + 10.0 * 36.0 + 12.0 + 4.0 * 36.0 + 12.0)
}

fn toolbar_action_width(state: &OverlayState, action: ToolbarAction) -> f32 {
    match action {
        ToolbarAction::Grip => scaled(state, 36.0),
        ToolbarAction::Numbering => scaled(state, 24.0),
        ToolbarAction::Divider => scaled(state, 12.0),
        _ => scaled(state, TOOLBAR_BUTTON),
    }
}

fn annotation_colors() -> [Color; 5] {
    [
        Color::rgb(0xff, 0x3b, 0x30),
        Color::rgb(0x0a, 0x84, 0xff),
        Color::rgb(0xff, 0xd6, 0x0a),
        Color::rgb(0x00, 0xc8, 0x53),
        Color::rgb(0xbf, 0x5a, 0xf2),
    ]
}

fn centered_icon_rect(state: &OverlayState, rect: Rect) -> Rect {
    let size = scaled(state, TOOL_ICON_SIZE);
    Rect::new(
        rect.x + (rect.width - size) / 2.0,
        rect.y + (rect.height - size) / 2.0,
        size,
        size,
    )
}

fn numbering_icon_rect(state: &OverlayState, rect: Rect) -> Rect {
    let size = scaled(state, 22.0);
    Rect::new(
        rect.x + (rect.width - size) / 2.0,
        rect.y + (rect.height - size) / 2.0,
        size,
        size,
    )
}

fn numbering_toggle_rect(state: &OverlayState, rect: Rect) -> Rect {
    let width = scaled(state, 11.0);
    let height = scaled(state, 1.05);
    Rect::new(
        rect.x + (rect.width - width) / 2.0,
        rect.bottom() - scaled(state, 7.0),
        width,
        height,
    )
}

fn rounded_rect_coverage(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    if radius <= 0.0 {
        return 1.0;
    }
    let cx = x.clamp(radius, width - radius);
    let cy = y.clamp(radius, height - radius);
    let dx = x - cx;
    let dy = y - cy;
    if dx * dx + dy * dy <= radius * radius {
        1.0
    } else {
        0.0
    }
}

unsafe fn alpha_blend_bgra(
    hdc: HDC,
    rect: Rect,
    width: u32,
    height: u32,
    bgra: &[u8],
) -> std::result::Result<(), ()> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: (width * height * 4) as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap =
        CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0).map_err(|_| ())?;
    if bits.is_null() {
        let _ = DeleteObject(bitmap);
        return Err(());
    }
    std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len());

    let mem_dc = CreateCompatibleDC(hdc);
    let old = SelectObject(mem_dc, bitmap);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = AlphaBlend(
        hdc,
        rect.x.round() as i32,
        rect.y.round() as i32,
        width as i32,
        height as i32,
        mem_dc,
        0,
        0,
        width as i32,
        height as i32,
        blend,
    );
    let _ = SelectObject(mem_dc, old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem_dc);
    Ok(())
}

unsafe fn fill_rounded_rect_antialias(hdc: HDC, rect: Rect, radius: f32, color: Color) {
    let width = rect.width.round().max(1.0) as u32;
    let height = rect.height.round().max(1.0) as u32;
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    let samples = 2;
    let sample_count = (samples * samples) as f32;

    for y in 0..height {
        for x in 0..width {
            let mut covered = 0.0;
            for sy in 0..samples {
                for sx in 0..samples {
                    let px = x as f32 + (sx as f32 + 0.5) / samples as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / samples as f32;
                    covered += rounded_rect_coverage(px, py, width as f32, height as f32, radius);
                }
            }
            let alpha = ((color.a as f32 * covered / sample_count).round() as u8).min(color.a);
            let offset = ((y * width + x) * 4) as usize;
            bgra[offset] = ((color.b as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 1] = ((color.g as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 2] = ((color.r as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 3] = alpha;
        }
    }

    let _ = alpha_blend_bgra(hdc, rect, width, height, &bgra);
}

unsafe fn stroke_rounded_rect_antialias(
    hdc: HDC,
    rect: Rect,
    radius: f32,
    stroke_width: f32,
    color: Color,
) {
    let width = rect.width.round().max(1.0) as u32;
    let height = rect.height.round().max(1.0) as u32;
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    let samples = 2;
    let sample_count = (samples * samples) as f32;
    let inner_width = (width as f32 - stroke_width * 2.0).max(0.0);
    let inner_height = (height as f32 - stroke_width * 2.0).max(0.0);
    let inner_radius = (radius - stroke_width).max(0.0);

    for y in 0..height {
        for x in 0..width {
            let mut covered = 0.0;
            for sy in 0..samples {
                for sx in 0..samples {
                    let px = x as f32 + (sx as f32 + 0.5) / samples as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / samples as f32;
                    let outer = rounded_rect_coverage(px, py, width as f32, height as f32, radius);
                    let inner = rounded_rect_coverage(
                        px - stroke_width,
                        py - stroke_width,
                        inner_width,
                        inner_height,
                        inner_radius,
                    );
                    covered += (outer - inner).clamp(0.0, 1.0);
                }
            }
            let alpha = ((color.a as f32 * covered / sample_count).round() as u8).min(color.a);
            let offset = ((y * width + x) * 4) as usize;
            bgra[offset] = ((color.b as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 1] = ((color.g as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 2] = ((color.r as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 3] = alpha;
        }
    }

    let _ = alpha_blend_bgra(hdc, rect, width, height, &bgra);
}

unsafe fn draw_toolbar_shadow(hdc: HDC, state: &OverlayState, rect: Rect) {
    for (inflate, y_offset, alpha) in [(2.0_f32, 3.0_f32, 18_u8), (4.0_f32, 6.0_f32, 8_u8)] {
        let shadow = Rect::new(
            rect.x - scaled(state, inflate),
            rect.y + scaled(state, y_offset),
            rect.width + scaled(state, inflate * 2.0),
            rect.height + scaled(state, inflate * 2.0),
        );
        fill_rounded_rect_antialias(
            hdc,
            shadow,
            scaled(state, TOOLBAR_RADIUS + inflate),
            Color::rgba(0, 0, 0, alpha),
        );
    }
}

unsafe fn draw_toolbar_border(hdc: HDC, state: &OverlayState, rect: Rect, palette: ToolbarPalette) {
    let radius = scaled(state, TOOLBAR_RADIUS);
    let _top = SelectedPen::new(hdc, 1.0, palette.border_top);
    let _ = MoveToEx(
        hdc,
        (rect.x + radius).round() as i32,
        rect.y.round() as i32,
        None,
    );
    let _ = LineTo(
        hdc,
        (rect.right() - radius).round() as i32,
        rect.y.round() as i32,
    );
    drop(_top);

    let _bottom = SelectedPen::new(hdc, 1.0, palette.border_bottom);
    let _ = MoveToEx(
        hdc,
        (rect.x + radius).round() as i32,
        rect.bottom().round() as i32 - 1,
        None,
    );
    let _ = LineTo(
        hdc,
        (rect.right() - radius).round() as i32,
        rect.bottom().round() as i32 - 1,
    );
}

unsafe fn draw_toolbar(hdc: HDC, state: &mut OverlayState, region: Rect) {
    state.toolbar_buttons.clear();
    let actions = [
        ToolbarAction::Grip,
        ToolbarAction::Numbering,
        ToolbarAction::Tool(ToolKind::Rectangle),
        ToolbarAction::Tool(ToolKind::Oval),
        ToolbarAction::Tool(ToolKind::Line),
        ToolbarAction::Tool(ToolKind::Arrow),
        ToolbarAction::Tool(ToolKind::Pen),
        ToolbarAction::Tool(ToolKind::Highlighter),
        ToolbarAction::Tool(ToolKind::Text),
        ToolbarAction::Tool(ToolKind::Tag),
        ToolbarAction::Tool(ToolKind::Watermark),
        ToolbarAction::Tool(ToolKind::Mosaic),
        ToolbarAction::Divider,
        ToolbarAction::Undo,
        ToolbarAction::Copy,
        ToolbarAction::Save,
        ToolbarAction::Cancel,
    ];
    let toolbar_height = scaled(state, TOOLBAR_HEIGHT);
    let width = toolbar_width(state);
    let total_height = toolbar_height;
    let origin = state
        .toolbar_origin
        .unwrap_or_else(|| default_toolbar_origin(state, region));
    let x = origin
        .x
        .max(8.0)
        .min((state.screen_bounds.width - width - 8.0).max(8.0));
    let y = origin
        .y
        .max(8.0)
        .min((state.screen_bounds.height - total_height - 8.0).max(8.0));
    state.toolbar_origin = Some(Point::new(x, y));

    let palette = toolbar_palette(state.theme);
    let bar = Rect::new(x, y, width, total_height);
    draw_toolbar_shadow(hdc, state, bar);
    fill_rounded_rect_antialias(hdc, bar, scaled(state, TOOLBAR_RADIUS), palette.background);
    draw_toolbar_border(hdc, state, bar, palette);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, crate::util::colorref(palette.icon));

    let mut x_cursor = x;
    for action in actions {
        let button_width = toolbar_action_width(state, action);
        let rect = Rect::new(x_cursor, y, button_width, toolbar_height);
        draw_toolbar_button(hdc, rect, action, state, palette);
        if !matches!(action, ToolbarAction::Divider) {
            state
                .toolbar_buttons
                .push(ToolbarButton { rect, tool: action });
        }
        x_cursor += button_width;
    }
}

fn default_toolbar_origin(state: &OverlayState, region: Rect) -> Point {
    let width = toolbar_width(state);
    let height = scaled(state, TOOLBAR_HEIGHT);
    let x = region
        .x
        .max(8.0)
        .min((state.screen_bounds.width - width - 8.0).max(8.0));
    let y = if region.y > height + 16.0 {
        region.y - height - 8.0
    } else {
        region.bottom() + 8.0
    };
    Point::new(x, y)
}

fn toolbar_origin_near_point(state: &OverlayState, click: Point, screen_region: Rect) -> Point {
    let width = toolbar_width(state);
    let height = scaled(state, TOOLBAR_HEIGHT);
    let margin = scaled(state, 12.0);
    let region = screen_region.translate(-state.screen_bounds.x, -state.screen_bounds.y);
    if click.y <= region.y + scaled(state, 96.0) {
        let x = (region.x + region.width / 2.0 - width / 2.0)
            .max(margin)
            .min((state.screen_bounds.width - width - margin).max(margin));
        let y = (region.y + region.height * 0.64)
            .max(margin)
            .min((state.screen_bounds.height - height - margin).max(margin));
        return Point::new(x, y);
    }
    let x = (click.x - width / 2.0)
        .max(margin)
        .min((state.screen_bounds.width - width - margin).max(margin));
    let preferred_below = click.y + margin;
    let preferred_above = click.y - height - margin;
    let y = if preferred_below + height <= state.screen_bounds.height - margin {
        preferred_below
    } else if preferred_above >= margin {
        preferred_above
    } else {
        default_toolbar_origin(state, region).y
    };
    Point::new(x, y)
}

fn color_distance(a: Color, b: Color) -> u16 {
    (a.r as i16 - b.r as i16).unsigned_abs()
        + (a.g as i16 - b.g as i16).unsigned_abs()
        + (a.b as i16 - b.b as i16).unsigned_abs()
}

fn contrast_text_color(background: Color) -> Color {
    if color_distance(background, Color::rgb(0xff, 0xd6, 0x0a)) < 24 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

unsafe fn draw_toolbar_button(
    hdc: HDC,
    rect: Rect,
    action: ToolbarAction,
    state: &OverlayState,
    palette: ToolbarPalette,
) {
    if matches!(action, ToolbarAction::Divider) {
        let x = rect.x + rect.width / 2.0;
        let _pen = SelectedPen::new(hdc, 1.0, palette.divider);
        let _ = MoveToEx(
            hdc,
            x.round() as i32,
            (rect.y + scaled(state, 7.0)).round() as i32,
            None,
        );
        let _ = LineTo(
            hdc,
            x.round() as i32,
            (rect.bottom() - scaled(state, 7.0)).round() as i32,
        );
        return;
    }

    let selected = matches!(action, ToolbarAction::Tool(tool) if tool == state.active_tool)
        || matches!(action, ToolbarAction::Numbering if state.numbering_enabled);
    let icon_bg = if selected {
        palette.selected_icon_background
    } else {
        palette.icon_background
    };
    let icon_rect = match action {
        ToolbarAction::Grip => rect,
        ToolbarAction::Numbering => numbering_icon_rect(state, rect),
        _ => centered_icon_rect(state, rect),
    };
    if !matches!(action, ToolbarAction::Grip | ToolbarAction::Numbering) {
        fill_rounded_rect_antialias(
            hdc,
            icon_rect,
            scaled(state, TOOL_ICON_SELECTED_RADIUS),
            icon_bg,
        );
    }
    draw_toolbar_icon(hdc, state, action, icon_rect, palette);
    if matches!(action, ToolbarAction::Numbering) {
        draw_numbering_toggle(hdc, state, numbering_toggle_rect(state, rect), palette);
    }
}

unsafe fn draw_numbering_toggle(
    hdc: HDC,
    state: &OverlayState,
    rect: Rect,
    palette: ToolbarPalette,
) {
    let progress = state.numbering_toggle_progress.clamp(0.0, 1.0);
    let off = match state.theme {
        AppTheme::Light => Color::rgb(0xc9, 0xc9, 0xc9),
        AppTheme::Dark => Color::rgb(0x58, 0x58, 0x58),
    };
    let green_left = Color::rgb(0x48, 0x62, 0x4e);
    let green_right = Color::rgb(0x67, 0xc4, 0x5d);
    let track_color = blend_color(off, green_left, progress);
    fill_rounded_rect_antialias(hdc, rect, rect.height / 2.0, track_color);

    let active_width = rect.width * progress;
    if active_width > 1.0 {
        fill_rounded_rect_antialias(
            hdc,
            Rect::new(rect.x, rect.y, active_width, rect.height),
            rect.height / 2.0,
            blend_color(green_left, green_right, progress),
        );
    }

    let thumb_size = scaled(state, 6.2);
    let thumb_x = rect.x - thumb_size * 0.25 + (rect.width - thumb_size * 0.5) * progress;
    let thumb = Rect::new(
        thumb_x,
        rect.y + rect.height / 2.0 - thumb_size / 2.0,
        thumb_size,
        thumb_size,
    );
    fill_oval_antialias(hdc, thumb, blend_color(palette.icon, green_right, progress));
}

fn blend_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::rgba(
        (a.r as f32 + (b.r as f32 - a.r as f32) * t).round() as u8,
        (a.g as f32 + (b.g as f32 - a.g as f32) * t).round() as u8,
        (a.b as f32 + (b.b as f32 - a.b as f32) * t).round() as u8,
        (a.a as f32 + (b.a as f32 - a.a as f32) * t).round() as u8,
    )
}

unsafe fn draw_tool_submenu(hdc: HDC, state: &mut OverlayState) {
    state.submenu_buttons.clear();
    state.submenu_sliders.clear();
    let Some(tool) = state.active_submenu else {
        state.submenu_rect = None;
        return;
    };
    let Some(anchor) = state
        .toolbar_buttons
        .iter()
        .find(
            |button| matches!(button.tool, ToolbarAction::Tool(button_tool) if button_tool == tool),
        )
        .map(|button| button.rect)
    else {
        state.submenu_rect = None;
        return;
    };

    let palette = toolbar_palette(state.theme);
    let height = scaled(state, SUBMENU_HEIGHT);
    let width = submenu_width(state, tool);
    let margin = scaled(state, 8.0);
    let x = (anchor.center().x - width / 2.0)
        .max(margin)
        .min((state.screen_bounds.width - width - margin).max(margin));
    let y = (anchor.bottom() + scaled(state, 18.0))
        .min((state.screen_bounds.height - height - margin).max(margin));
    let rect = Rect::new(x, y, width, height);
    state.submenu_rect = Some(rect);

    draw_toolbar_shadow(hdc, state, rect);
    fill_rounded_rect_antialias(hdc, rect, scaled(state, SUBMENU_RADIUS), palette.background);
    draw_submenu_notch(hdc, state, rect, anchor.center().x, palette.background);
    draw_toolbar_border(hdc, state, rect, palette);

    let mut x_cursor = rect.x + scaled(state, SUBMENU_EDGE_PAD);
    match tool {
        ToolKind::Rectangle | ToolKind::Oval | ToolKind::Line | ToolKind::Arrow => {
            draw_submenu_slider(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.width,
                MIN_STROKE_WIDTH,
                MAX_STROKE_WIDTH,
                SubmenuSliderKind::StrokeWidth,
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        ToolKind::Pen => {
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "line",
                state.pen_mode == PenMode::Free,
                SubmenuAction::PenMode(PenMode::Free),
            );
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "arrow",
                state.pen_mode == PenMode::Arrow,
                SubmenuAction::PenMode(PenMode::Arrow),
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_submenu_slider(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.width,
                MIN_STROKE_WIDTH,
                MAX_STROKE_WIDTH,
                SubmenuSliderKind::StrokeWidth,
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        ToolKind::Highlighter => {
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "line",
                state.highlighter_shape == HighlightShape::Rectangle,
                SubmenuAction::HighlighterShape(HighlightShape::Rectangle),
            );
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "area",
                state.highlighter_shape == HighlightShape::RoundedRectangle,
                SubmenuAction::HighlighterShape(HighlightShape::RoundedRectangle),
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_submenu_slider(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.width,
                MIN_STROKE_WIDTH,
                MAX_STROKE_WIDTH,
                SubmenuSliderKind::StrokeWidth,
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        ToolKind::Text => {
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "line-text",
                !state.text_filled,
                SubmenuAction::TextFilled(false),
            );
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "solid-text",
                state.text_filled,
                SubmenuAction::TextFilled(true),
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_font_slider(hdc, state, &mut x_cursor, palette);
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        ToolKind::Tag => {
            draw_font_slider(hdc, state, &mut x_cursor, palette);
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        ToolKind::Watermark => {
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "calendar",
                state.watermark_date_enabled,
                SubmenuAction::WatermarkMode(WatermarkMode::Date),
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "line-text",
                state.editing_watermark_text || !state.watermark_text.is_empty(),
                SubmenuAction::WatermarkMode(WatermarkMode::Text),
            );
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "image",
                state.watermark_image_path.is_some(),
                SubmenuAction::WatermarkMode(WatermarkMode::Image),
            );
            draw_submenu_icon_button(
                hdc,
                state,
                &mut x_cursor,
                palette,
                "cancel",
                false,
                SubmenuAction::ClearWatermark,
            );
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            let input = Rect::new(
                x_cursor,
                rect.y + scaled(state, 3.0),
                scaled(state, 112.0),
                height - scaled(state, 6.0),
            );
            fill_rounded_rect_antialias(
                hdc,
                input,
                scaled(state, 2.0),
                palette.selected_icon_background,
            );
            draw_label_sized_color(
                hdc,
                input.x + scaled(state, 5.0),
                input.y + scaled(state, 2.0),
                if state.watermark_text.is_empty() {
                    "Type text..."
                } else {
                    &state.watermark_text
                },
                scaled(state, 8.0),
                palette.icon,
            );
            if state.editing_watermark_text && watermark_caret_visible() {
                let text_width = if state.watermark_text.is_empty() {
                    0.0
                } else {
                    approximate_text_width(&state.watermark_text, scaled(state, 8.0))
                };
                let caret_x = (input.x + scaled(state, 5.0) + text_width)
                    .min(input.right() - scaled(state, 5.0));
                fill_rounded_rect_antialias(
                    hdc,
                    Rect::new(
                        caret_x,
                        input.y + scaled(state, 4.0),
                        scaled(state, 1.0),
                        input.height - scaled(state, 8.0),
                    ),
                    scaled(state, 0.5),
                    palette.icon,
                );
            }
            state.submenu_buttons.push(SubmenuButton {
                rect: input,
                action: SubmenuAction::WatermarkTextInput,
            });
            x_cursor += input.width + scaled(state, SUBMENU_GAP);
            draw_submenu_divider(hdc, state, &mut x_cursor, palette);
            draw_color_swatches(
                hdc,
                state,
                &mut x_cursor,
                palette,
                state.current_stroke.color,
            );
        }
        _ => {}
    }
}

fn submenu_width(state: &OverlayState, tool: ToolKind) -> f32 {
    let base = match tool {
        ToolKind::Rectangle | ToolKind::Oval | ToolKind::Line | ToolKind::Arrow => 268.0,
        ToolKind::Pen | ToolKind::Highlighter => 340.0,
        ToolKind::Text => 348.0,
        ToolKind::Tag => 286.0,
        ToolKind::Watermark => 420.0,
        _ => 0.0,
    };
    scaled(state, base)
}

unsafe fn draw_submenu_notch(
    hdc: HDC,
    state: &OverlayState,
    rect: Rect,
    anchor_x: f32,
    color: Color,
) {
    let half = scaled(state, SUBMENU_NOTCH);
    let top = rect.y - scaled(state, 8.0);
    let center = anchor_x.clamp(rect.x + half, rect.right() - half);
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(center - half, rect.y + 1.0);
    builder.line_to(center, top);
    builder.line_to(center + half, rect.y + 1.0);
    builder.close();
    if let Some(path) = builder.finish() {
        draw_filled_tiny_path(
            hdc,
            Rect::new(rect.x, top, rect.width, rect.height + (rect.y - top)),
            path,
            color,
        );
    }
}

unsafe fn draw_filled_tiny_path(
    hdc: HDC,
    surface: Rect,
    path: resvg::tiny_skia::Path,
    color: Color,
) {
    let width = surface.width.round().max(1.0) as u32;
    let height = surface.height.round().max(1.0) as u32;
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(width, height) else {
        return;
    };
    let mut paint = resvg::tiny_skia::Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        resvg::tiny_skia::FillRule::Winding,
        resvg::tiny_skia::Transform::from_translate(-surface.x, -surface.y),
        None,
    );
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    for (source, dest) in pixmap.data().chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        dest[0] = source[2];
        dest[1] = source[1];
        dest[2] = source[0];
        dest[3] = source[3];
    }
    let _ = alpha_blend_bgra(hdc, surface, width, height, &bgra);
}

unsafe fn draw_submenu_divider(
    hdc: HDC,
    state: &OverlayState,
    x_cursor: &mut f32,
    palette: ToolbarPalette,
) {
    let x = *x_cursor + scaled(state, SUBMENU_DIVIDER / 2.0);
    let _pen = SelectedPen::new(hdc, 1.0, palette.divider);
    let y = state
        .submenu_buttons
        .first()
        .map(|button| button.rect.y)
        .unwrap_or(0.0);
    let _ = MoveToEx(
        hdc,
        x.round() as i32,
        (y + scaled(state, 3.0)).round() as i32,
        None,
    );
    let _ = LineTo(
        hdc,
        x.round() as i32,
        (y + scaled(state, 21.0)).round() as i32,
    );
    *x_cursor += scaled(state, SUBMENU_DIVIDER + SUBMENU_GAP);
}

unsafe fn draw_submenu_slider(
    hdc: HDC,
    state: &mut OverlayState,
    x_cursor: &mut f32,
    palette: ToolbarPalette,
    value: f32,
    min: f32,
    max: f32,
    kind: SubmenuSliderKind,
) {
    let y = state.toolbar_origin.map(|p| p.y).unwrap_or(0.0) + scaled(state, TOOLBAR_HEIGHT + 18.0);
    let rect = Rect::new(
        *x_cursor,
        y,
        scaled(state, 116.0),
        scaled(state, SUBMENU_HEIGHT),
    );
    let line_y = rect.center().y;
    let left_dot = rect.x + scaled(state, 8.0);
    let start = rect.x + scaled(state, 22.0);
    let end = rect.right() - scaled(state, 22.0);
    let right_dot = rect.right() - scaled(state, 8.0);
    let track_height = scaled(state, 2.5);
    fill_rounded_rect_antialias(
        hdc,
        Rect::new(
            start,
            line_y - track_height / 2.0,
            (end - start).max(1.0),
            track_height,
        ),
        track_height / 2.0,
        palette.icon,
    );
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let knob_x = start + (end - start) * t;
    let knob_outer = scaled(state, 15.0);
    let knob_inner = scaled(state, 11.0);
    fill_oval_antialias(
        hdc,
        Rect::new(
            knob_x - knob_outer / 2.0,
            line_y - knob_outer / 2.0,
            knob_outer,
            knob_outer,
        ),
        Color::rgb(0xc8, 0xc8, 0xc8),
    );
    fill_oval_antialias(
        hdc,
        Rect::new(
            knob_x - knob_inner / 2.0,
            line_y - knob_inner / 2.0,
            knob_inner,
            knob_inner,
        ),
        palette.selected_icon_background,
    );
    fill_rounded_rect_antialias(
        hdc,
        Rect::new(
            left_dot - scaled(state, 2.0),
            line_y - scaled(state, 2.0),
            scaled(state, 4.0),
            scaled(state, 4.0),
        ),
        scaled(state, 2.0),
        palette.icon,
    );
    fill_rounded_rect_antialias(
        hdc,
        Rect::new(
            right_dot - scaled(state, 4.8),
            line_y - scaled(state, 4.8),
            scaled(state, 9.6),
            scaled(state, 9.6),
        ),
        scaled(state, 4.8),
        palette.icon,
    );
    state.submenu_sliders.push(SubmenuSlider {
        rect,
        start_x: start,
        end_x: end,
        min,
        max,
        kind,
    });
    *x_cursor += rect.width + scaled(state, SUBMENU_GAP);
}

unsafe fn draw_font_slider(
    hdc: HDC,
    state: &mut OverlayState,
    x_cursor: &mut f32,
    palette: ToolbarPalette,
) {
    let y = state.toolbar_origin.map(|p| p.y).unwrap_or(0.0) + scaled(state, TOOLBAR_HEIGHT + 18.0);
    draw_submenu_static_svg(
        hdc,
        Rect::new(
            *x_cursor,
            y + scaled(state, 4.0),
            scaled(state, 16.0),
            scaled(state, 16.0),
        ),
        include_str!("../assets/toolbar/smaller-font.svg"),
        palette.icon,
    );
    *x_cursor += scaled(state, 16.0);
    draw_submenu_slider(
        hdc,
        state,
        x_cursor,
        palette,
        state.font_size,
        MIN_FONT_SIZE,
        MAX_FONT_SIZE,
        SubmenuSliderKind::FontSize,
    );
    draw_submenu_static_svg(
        hdc,
        Rect::new(
            *x_cursor - scaled(state, 2.0),
            y + scaled(state, 2.0),
            scaled(state, 18.0),
            scaled(state, 18.0),
        ),
        include_str!("../assets/toolbar/larger-font.svg"),
        palette.icon,
    );
    *x_cursor += scaled(state, 12.0);
}

unsafe fn draw_color_swatches(
    hdc: HDC,
    state: &mut OverlayState,
    x_cursor: &mut f32,
    palette: ToolbarPalette,
    selected: Color,
) {
    let y = state.toolbar_origin.map(|p| p.y).unwrap_or(0.0) + scaled(state, TOOLBAR_HEIGHT + 18.0);
    for color in annotation_colors() {
        let rect = Rect::new(
            *x_cursor,
            y + scaled(state, 4.0),
            scaled(state, SUBMENU_SWATCH),
            scaled(state, SUBMENU_SWATCH),
        );
        let button_rect = Rect::new(
            rect.x - scaled(state, 4.0),
            rect.y - scaled(state, 4.0),
            rect.width + scaled(state, 8.0),
            rect.height + scaled(state, 8.0),
        );
        let radius = scaled(state, 2.0);
        if color == selected {
            fill_rounded_rect_antialias(
                hdc,
                button_rect,
                scaled(state, 4.0),
                palette.selected_icon_background,
            );
        }
        if color == selected {
            let border_color = match state.theme {
                AppTheme::Light => Color::rgb(0x4d, 0x4d, 0x4d),
                AppTheme::Dark => Color::rgb(0xf2, 0xf2, 0xf2),
            };
            fill_rounded_rect_antialias(hdc, rect, radius, color);
            stroke_rounded_rect_antialias(hdc, rect, radius, 3.0, border_color);
        } else {
            fill_rounded_rect_antialias(hdc, rect, radius, color);
        }
        state.submenu_buttons.push(SubmenuButton {
            rect: button_rect,
            action: SubmenuAction::Color(color),
        });
        *x_cursor += rect.width + scaled(state, SUBMENU_GAP + 2.0);
    }
}

unsafe fn draw_submenu_icon_button(
    hdc: HDC,
    state: &mut OverlayState,
    x_cursor: &mut f32,
    palette: ToolbarPalette,
    icon: &str,
    selected: bool,
    action: SubmenuAction,
) {
    let y = state.toolbar_origin.map(|p| p.y).unwrap_or(0.0) + scaled(state, TOOLBAR_HEIGHT + 18.0);
    let rect = Rect::new(
        *x_cursor,
        y,
        scaled(state, 22.0),
        scaled(state, SUBMENU_HEIGHT),
    );
    if selected {
        fill_rounded_rect_antialias(
            hdc,
            rect,
            scaled(state, 4.0),
            palette.selected_icon_background,
        );
    }
    let icon_rect = Rect::new(
        rect.x + scaled(state, 3.0),
        rect.y + scaled(state, 4.0),
        scaled(state, 16.0),
        scaled(state, 16.0),
    );
    match icon {
        "line" => {
            let svg = recolor_svg(
                include_str!("../assets/toolbar/mini-line.svg"),
                palette.icon,
            );
            let _ = draw_svg(hdc, &svg, icon_rect);
        }
        "arrow" => {
            let svg = recolor_svg(
                include_str!("../assets/toolbar/mini-arrow.svg"),
                palette.icon,
            );
            let _ = draw_svg(hdc, &svg, icon_rect);
        }
        "highlight-line" => {
            let line = Rect::new(
                icon_rect.x,
                icon_rect.center().y - scaled(state, 2.5),
                icon_rect.width,
                scaled(state, 5.0),
            );
            fill_rounded_rect_antialias(hdc, line, scaled(state, 2.5), palette.icon);
        }
        "area" => {
            let outer = Rect::new(
                icon_rect.x + scaled(state, 1.0),
                icon_rect.y + scaled(state, 2.0),
                icon_rect.width - scaled(state, 2.0),
                icon_rect.height - scaled(state, 4.0),
            );
            let inner = Rect::new(
                outer.x + scaled(state, 2.0),
                outer.y + scaled(state, 2.0),
                outer.width - scaled(state, 4.0),
                outer.height - scaled(state, 4.0),
            );
            fill_rounded_rect_antialias(hdc, outer, scaled(state, 2.0), palette.icon);
            fill_rounded_rect_antialias(
                hdc,
                inner,
                scaled(state, 1.0),
                if selected {
                    palette.selected_icon_background
                } else {
                    palette.background
                },
            );
        }
        "line-text" => draw_submenu_static_svg(
            hdc,
            icon_rect,
            include_str!("../assets/toolbar/line-text.svg"),
            palette.icon,
        ),
        "solid-text" => draw_submenu_static_svg(
            hdc,
            icon_rect,
            include_str!("../assets/toolbar/solid-text.svg"),
            palette.icon,
        ),
        "calendar" => draw_submenu_static_svg(
            hdc,
            icon_rect,
            include_str!("../assets/toolbar/calendar.svg"),
            palette.icon,
        ),
        "image" => draw_submenu_static_svg(
            hdc,
            icon_rect,
            include_str!("../assets/toolbar/image.svg"),
            palette.icon,
        ),
        "cancel" => draw_submenu_static_svg(
            hdc,
            icon_rect,
            include_str!("../assets/toolbar/cancel.svg"),
            palette.icon,
        ),
        _ => {}
    }
    state.submenu_buttons.push(SubmenuButton { rect, action });
    *x_cursor += rect.width + scaled(state, SUBMENU_GAP);
}

unsafe fn draw_submenu_static_svg(hdc: HDC, rect: Rect, svg: &str, color: Color) {
    let svg = recolor_svg(svg, color);
    let _ = draw_svg(hdc, &svg, rect);
}

fn toolbar_label(action: ToolbarAction) -> &'static str {
    match action {
        ToolbarAction::Grip => "::",
        ToolbarAction::Numbering => "1",
        ToolbarAction::Divider => "|",
        ToolbarAction::Tool(ToolKind::StepNumber) => "1",
        ToolbarAction::Tool(ToolKind::Rectangle) => "[]",
        ToolbarAction::Tool(ToolKind::Oval) => "O",
        ToolbarAction::Tool(ToolKind::Line) => "/",
        ToolbarAction::Tool(ToolKind::Arrow) => "->",
        ToolbarAction::Tool(ToolKind::Pen) => "P",
        ToolbarAction::Tool(ToolKind::Text) => "T",
        ToolbarAction::Tool(ToolKind::Tag) => "#",
        ToolbarAction::Tool(ToolKind::Mosaic) => "M",
        ToolbarAction::Tool(ToolKind::Highlighter) => "H",
        ToolbarAction::Tool(ToolKind::Watermark) => "W",
        ToolbarAction::Undo => "U",
        ToolbarAction::Copy => "C",
        ToolbarAction::Save => "Sv",
        ToolbarAction::Cancel => "X",
    }
}

fn toolbar_svg(action: ToolbarAction, theme: AppTheme) -> Option<&'static str> {
    match action {
        ToolbarAction::Grip => Some(match theme {
            AppTheme::Light => include_str!("../assets/toolbar/drag-bkcg-light.svg"),
            AppTheme::Dark => include_str!("../assets/toolbar/drag-bkcg-dark.svg"),
        }),
        ToolbarAction::Numbering | ToolbarAction::Tool(ToolKind::StepNumber) => {
            Some(include_str!("../assets/toolbar/numbering.svg"))
        }
        ToolbarAction::Tool(ToolKind::Rectangle) => {
            Some(include_str!("../assets/toolbar/rectangle.svg"))
        }
        ToolbarAction::Tool(ToolKind::Oval) => Some(include_str!("../assets/toolbar/ellipse.svg")),
        ToolbarAction::Tool(ToolKind::Pen) => Some(include_str!("../assets/toolbar/pen.svg")),
        ToolbarAction::Tool(ToolKind::Line) => Some(include_str!("../assets/toolbar/line.svg")),
        ToolbarAction::Tool(ToolKind::Arrow) => Some(include_str!("../assets/toolbar/arrow.svg")),
        ToolbarAction::Tool(ToolKind::Highlighter) => {
            Some(include_str!("../assets/toolbar/highlighter.svg"))
        }
        ToolbarAction::Tool(ToolKind::Text) => Some(include_str!("../assets/toolbar/text.svg")),
        ToolbarAction::Tool(ToolKind::Watermark) => {
            Some(include_str!("../assets/toolbar/watermark.svg"))
        }
        ToolbarAction::Tool(ToolKind::Tag) => Some(include_str!("../assets/toolbar/tag.svg")),
        ToolbarAction::Tool(ToolKind::Mosaic) => {
            Some(include_str!("../assets/toolbar/pixelate.svg"))
        }
        ToolbarAction::Undo => Some(include_str!("../assets/toolbar/undo.svg")),
        ToolbarAction::Copy => Some(include_str!("../assets/toolbar/copy.svg")),
        ToolbarAction::Save => Some(include_str!("../assets/toolbar/save.svg")),
        ToolbarAction::Cancel => Some(include_str!("../assets/toolbar/cancel.svg")),
        ToolbarAction::Divider => None,
    }
}

unsafe fn draw_toolbar_icon(
    hdc: HDC,
    state: &OverlayState,
    action: ToolbarAction,
    rect: Rect,
    palette: ToolbarPalette,
) {
    let Some(svg) = toolbar_svg(action, state.theme) else {
        return;
    };
    let svg = recolor_svg(svg, palette.icon);
    if draw_svg(hdc, &svg, rect).is_err() {
        draw_label(
            hdc,
            rect.x + scaled(state, 7.0),
            rect.y + scaled(state, 6.0),
            toolbar_label(action),
        );
    }
}

fn recolor_svg(svg: &str, color: Color) -> String {
    let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    svg.replace("#4d4d4d", &hex)
        .replace("#4D4D4D", &hex)
        .replace("#b3b3b3", &hex)
        .replace("#B3B3B3", &hex)
}

unsafe fn draw_svg(hdc: HDC, svg: &str, rect: Rect) -> std::result::Result<(), ()> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|_| ())?;
    let width = rect.width.round().max(1.0) as u32;
    let height = rect.height.round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or(())?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / tree_size.width(),
        height as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    for (source, dest) in pixmap.data().chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        dest[0] = source[2];
        dest[1] = source[1];
        dest[2] = source[0];
        dest[3] = source[3];
    }

    alpha_blend_bgra(hdc, rect, width, height, &bgra)
}

fn effective_stroke_color(stroke: StrokeStyle) -> Color {
    Color::rgba(
        stroke.color.r,
        stroke.color.g,
        stroke.color.b,
        ((stroke.color.a as f32 * stroke.opacity).clamp(0.0, 255.0)).round() as u8,
    )
}

fn highlighter_color(color: Color, opacity: f32) -> Color {
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

unsafe fn draw_highlighter(
    hdc: HDC,
    rect: Rect,
    shape: HighlightShape,
    color: Color,
    stroke_width: f32,
    start: Point,
    end: Point,
) {
    match shape {
        HighlightShape::Rectangle => {
            draw_tiny_line_with_cap(
                hdc,
                start,
                end,
                StrokeStyle {
                    width: highlighter_line_width(stroke_width),
                    color,
                    opacity: 1.0,
                },
                resvg::tiny_skia::LineCap::Round,
            );
        }
        HighlightShape::RoundedRectangle => {
            fill_rounded_rect_antialias(hdc, rect, HIGHLIGHTER_RADIUS, color)
        }
        HighlightShape::Oval => fill_oval_antialias(hdc, rect, color),
    }
}

fn highlighter_line_width(stroke_width: f32) -> f32 {
    24.0 + stroke_width.max(1.0) * 3.0
}

unsafe fn fill_oval_antialias(hdc: HDC, rect: Rect, color: Color) {
    let width = rect.width.round().max(1.0) as u32;
    let height = rect.height.round().max(1.0) as u32;
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    let rx = width as f32 / 2.0;
    let ry = height as f32 / 2.0;
    let samples = 2;
    let sample_count = (samples * samples) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut covered = 0.0;
            for sy in 0..samples {
                for sx in 0..samples {
                    let px = (x as f32 + (sx as f32 + 0.5) / samples as f32 - rx) / rx.max(1.0);
                    let py = (y as f32 + (sy as f32 + 0.5) / samples as f32 - ry) / ry.max(1.0);
                    if px * px + py * py <= 1.0 {
                        covered += 1.0;
                    }
                }
            }
            let alpha = ((color.a as f32 * covered / sample_count).round() as u8).min(color.a);
            let offset = ((y * width + x) * 4) as usize;
            bgra[offset] = ((color.b as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 1] = ((color.g as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 2] = ((color.r as u16 * alpha as u16) / 255) as u8;
            bgra[offset + 3] = alpha;
        }
    }
    let _ = alpha_blend_bgra(hdc, rect, width, height, &bgra);
}

unsafe fn draw_blur_region(hdc: HDC, rect: Rect, radius: f32) {
    if !rect.is_visible() {
        return;
    }
    let surface_width = rect.width.ceil().max(1.0) as u32;
    let surface_height = rect.height.ceil().max(1.0) as u32;
    let mut bgra = vec![0u8; (surface_width * surface_height * 4) as usize];
    let cell = (14.0_f32.max(rect.width.min(rect.height) / 21.0).min(24.0) * 0.6).max(6.0);
    let cols = (rect.width / cell).ceil().max(1.0) as i32;
    let rows = (rect.height / cell).ceil().max(1.0) as i32;
    for row in 0..rows {
        for col in 0..cols {
            let x = rect.x + col as f32 * cell;
            let y = rect.y + row as f32 * cell;
            let local = Rect::new(
                (x - rect.x).max(0.0),
                (y - rect.y).max(0.0),
                (rect.right() - x).min(cell).max(0.0),
                (rect.bottom() - y).min(cell).max(0.0),
            );
            if !local.is_visible() {
                continue;
            }
            let sample_x = (x + local.width / 2.0).round() as i32;
            let sample_y = (y + local.height / 2.0).round() as i32;
            let pixel = GetPixel(hdc, sample_x, sample_y).0;
            let sampled = Color::rgb(
                (pixel & 0xff) as u8,
                ((pixel >> 8) & 0xff) as u8,
                ((pixel >> 16) & 0xff) as u8,
            );
            let color = mosaic_sample_color(sampled, row, col);
            fill_mosaic_tile_bgra(
                &mut bgra,
                surface_width,
                surface_height,
                local,
                rect.width,
                rect.height,
                radius,
                color,
            );
        }
    }
    let _ = alpha_blend_bgra(hdc, rect, surface_width, surface_height, &bgra);
}

fn fill_mosaic_tile_bgra(
    bgra: &mut [u8],
    surface_width: u32,
    surface_height: u32,
    tile: Rect,
    region_width: f32,
    region_height: f32,
    radius: f32,
    color: Color,
) {
    let left = tile.x.floor().max(0.0) as u32;
    let top = tile.y.floor().max(0.0) as u32;
    let right = (tile.right().ceil().max(0.0) as u32).min(surface_width);
    let bottom = (tile.bottom().ceil().max(0.0) as u32).min(surface_height);
    let full_b = color.b;
    let full_g = color.g;
    let full_r = color.r;
    for y in top..bottom {
        let fy = y as f32 + 0.5;
        for x in left..right {
            let fx = x as f32 + 0.5;
            let in_rounded_corner = radius > 0.0
                && ((fx < radius && fy < radius)
                    || (fx > region_width - radius && fy < radius)
                    || (fx < radius && fy > region_height - radius)
                    || (fx > region_width - radius && fy > region_height - radius));
            let alpha = if in_rounded_corner {
                let coverage = rounded_rect_coverage(fx, fy, region_width, region_height, radius);
                if coverage <= 0.0 {
                    continue;
                }
                (255.0 * coverage).round().clamp(0.0, 255.0) as u8
            } else {
                255
            };
            let offset = ((y * surface_width + x) * 4) as usize;
            if alpha == 255 {
                bgra[offset] = full_b;
                bgra[offset + 1] = full_g;
                bgra[offset + 2] = full_r;
                bgra[offset + 3] = 255;
            } else {
                bgra[offset] = ((color.b as u16 * alpha as u16) / 255) as u8;
                bgra[offset + 1] = ((color.g as u16 * alpha as u16) / 255) as u8;
                bgra[offset + 2] = ((color.r as u16 * alpha as u16) / 255) as u8;
                bgra[offset + 3] = alpha;
            }
        }
    }
}

fn mosaic_sample_color(sampled: Color, row: i32, col: i32) -> Color {
    let noise = (((row * 37 + col * 19 + row * col * 7) % 61) - 30) as i16;
    let luma = ((sampled.r as u16 * 30 + sampled.g as u16 * 59 + sampled.b as u16 * 11) / 100)
        as i16
        + noise;
    let quantized = if luma < 76 {
        44
    } else if luma < 144 {
        118
    } else if luma < 210 {
        176
    } else {
        226
    };
    Color::rgb(quantized, quantized, quantized)
}

fn path_surface_for_rect(rect: Rect, stroke_width: f32) -> Rect {
    let pad = stroke_width.max(1.0) / 2.0 + 2.0;
    Rect::new(
        rect.x - pad,
        rect.y - pad,
        rect.width + pad * 2.0,
        rect.height + pad * 2.0,
    )
}

fn path_surface_for_points(points: &[Point], stroke_width: f32) -> Option<Rect> {
    let first = *points.first()?;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x;
    let mut bottom = first.y;
    for point in points.iter().copied().skip(1) {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }
    let pad = stroke_width.max(1.0) / 2.0 + 16.0;
    Some(Rect::new(
        left - pad,
        top - pad,
        (right - left) + pad * 2.0,
        (bottom - top) + pad * 2.0,
    ))
}

unsafe fn draw_tiny_path(
    hdc: HDC,
    surface: Rect,
    path: resvg::tiny_skia::Path,
    stroke_style: StrokeStyle,
) {
    draw_tiny_path_with_cap(
        hdc,
        surface,
        path,
        stroke_style,
        resvg::tiny_skia::LineCap::Round,
    );
}

unsafe fn draw_tiny_path_with_cap(
    hdc: HDC,
    surface: Rect,
    path: resvg::tiny_skia::Path,
    stroke_style: StrokeStyle,
    line_cap: resvg::tiny_skia::LineCap,
) {
    let width = surface.width.round().max(1.0) as u32;
    let height = surface.height.round().max(1.0) as u32;
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(width, height) else {
        return;
    };
    let color = effective_stroke_color(stroke_style);
    let mut paint = resvg::tiny_skia::Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;

    let mut stroke = resvg::tiny_skia::Stroke::default();
    stroke.width = stroke_style.width.max(1.0);
    stroke.line_cap = line_cap;
    stroke.line_join = resvg::tiny_skia::LineJoin::Round;

    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        resvg::tiny_skia::Transform::identity(),
        None,
    );

    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    for (source, dest) in pixmap.data().chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        dest[0] = source[2];
        dest[1] = source[1];
        dest[2] = source[0];
        dest[3] = source[3];
    }
    let _ = alpha_blend_bgra(hdc, surface, width, height, &bgra);
}

unsafe fn draw_stroked_rect(hdc: HDC, rect: Rect, stroke: StrokeStyle) {
    let surface = path_surface_for_rect(rect, stroke.width);
    let local = rect.translate(-surface.x, -surface.y);
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(local.x, local.y);
    builder.line_to(local.right(), local.y);
    builder.line_to(local.right(), local.bottom());
    builder.line_to(local.x, local.bottom());
    builder.close();
    if let Some(path) = builder.finish() {
        draw_tiny_path(hdc, surface, path, stroke);
    }
}

unsafe fn draw_stroked_oval(hdc: HDC, rect: Rect, stroke: StrokeStyle) {
    let surface = path_surface_for_rect(rect, stroke.width);
    let local = rect.translate(-surface.x, -surface.y);
    let Some(oval) = resvg::tiny_skia::Rect::from_xywh(
        local.x,
        local.y,
        local.width.max(1.0),
        local.height.max(1.0),
    ) else {
        return;
    };
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.push_oval(oval);
    if let Some(path) = builder.finish() {
        draw_tiny_path(hdc, surface, path, stroke);
    }
}

unsafe fn draw_line(hdc: HDC, start: Point, end: Point, arrow: bool, stroke: StrokeStyle) {
    let shaft_end = if arrow {
        arrow_shaft_end(start, end, stroke)
    } else {
        end
    };
    let cap = if arrow {
        resvg::tiny_skia::LineCap::Butt
    } else {
        resvg::tiny_skia::LineCap::Round
    };
    draw_tiny_line_with_cap(hdc, start, shaft_end, stroke, cap);
    if arrow {
        draw_arrow_tip(hdc, start, end, stroke);
    }
}

unsafe fn draw_tiny_line_with_cap(
    hdc: HDC,
    start: Point,
    end: Point,
    stroke: StrokeStyle,
    cap: resvg::tiny_skia::LineCap,
) {
    let Some(surface) = path_surface_for_points(&[start, end], stroke.width) else {
        return;
    };
    let local_start = start.translate(-surface.x, -surface.y);
    let local_end = end.translate(-surface.x, -surface.y);
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(local_start.x, local_start.y);
    builder.line_to(local_end.x, local_end.y);
    if let Some(path) = builder.finish() {
        draw_tiny_path_with_cap(hdc, surface, path, stroke, cap);
    }
}

unsafe fn draw_pen_path(hdc: HDC, points: &[Point], stroke: StrokeStyle) {
    draw_pen_path_with_cap(hdc, points, stroke, resvg::tiny_skia::LineCap::Round);
}

unsafe fn draw_pen_path_with_cap(
    hdc: HDC,
    points: &[Point],
    stroke: StrokeStyle,
    cap: resvg::tiny_skia::LineCap,
) {
    if points.len() < 2 {
        return;
    }
    let Some(surface) = path_surface_for_points(points, stroke.width) else {
        return;
    };
    let local_points: Vec<Point> = points
        .iter()
        .map(|point| point.translate(-surface.x, -surface.y))
        .collect();
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(local_points[0].x, local_points[0].y);

    if local_points.len() == 2 {
        builder.line_to(local_points[1].x, local_points[1].y);
    } else {
        for index in 1..local_points.len() - 1 {
            let current = local_points[index];
            let next = local_points[index + 1];
            let midpoint = Point::new((current.x + next.x) / 2.0, (current.y + next.y) / 2.0);
            builder.quad_to(current.x, current.y, midpoint.x, midpoint.y);
        }
        let last = *local_points.last().expect("len checked above");
        builder.line_to(last.x, last.y);
    }

    if let Some(path) = builder.finish() {
        draw_tiny_path_with_cap(hdc, surface, path, stroke, cap);
    }
}

unsafe fn draw_arrow_tip_for_points(hdc: HDC, points: &[Point], stroke: StrokeStyle) {
    let Some(end) = points.last().copied() else {
        return;
    };
    let min_distance = (arrow_tip_size(stroke) * 0.7).max(stroke.width.max(8.0));
    let Some(start) = points.iter().rev().copied().find(|point| {
        ((point.x - end.x).powi(2) + (point.y - end.y).powi(2)).sqrt() > min_distance
    }) else {
        return;
    };
    draw_arrow_tip(hdc, start, end, stroke);
}

fn pen_arrow_shaft_points(points: &[Point], stroke: StrokeStyle) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let trim = (arrow_tip_size(stroke) * 0.72).max(stroke.width * 2.0);
    let mut remaining = trim;
    let mut keep = points.len() - 1;
    for index in (1..points.len()).rev() {
        let a = points[index - 1];
        let b = points[index];
        let segment = distance(a, b);
        if segment >= remaining && segment > 0.1 {
            let t = 1.0 - remaining / segment;
            let shaft_end = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            let mut shaft = points[..index].to_vec();
            shaft.push(shaft_end);
            return shaft;
        }
        remaining -= segment;
        keep = index - 1;
    }
    let mut shaft = points[..=keep].to_vec();
    if shaft.len() < 2 {
        shaft = points[..2].to_vec();
    }
    shaft
}

unsafe fn draw_arrow_tip(hdc: HDC, start: Point, end: Point, stroke: StrokeStyle) {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let size = arrow_tip_size(stroke);
    let spread = 0.48_f32;
    let p1 = Point::new(
        end.x - size * (angle + spread).cos(),
        end.y - size * (angle + spread).sin(),
    );
    let p2 = Point::new(
        end.x - size * (angle - spread).cos(),
        end.y - size * (angle - spread).sin(),
    );
    let left = end.x.min(p1.x).min(p2.x) - 2.0;
    let top = end.y.min(p1.y).min(p2.y) - 2.0;
    let right = end.x.max(p1.x).max(p2.x) + 2.0;
    let bottom = end.y.max(p1.y).max(p2.y) + 2.0;
    let surface = Rect::new(left, top, right - left, bottom - top);
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(end.x, end.y);
    builder.line_to(p1.x, p1.y);
    builder.line_to(p2.x, p2.y);
    builder.close();
    if let Some(path) = builder.finish() {
        draw_filled_tiny_path(hdc, surface, path, effective_stroke_color(stroke));
    }
}

fn arrow_shaft_end(start: Point, end: Point, stroke: StrokeStyle) -> Point {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.1 {
        return end;
    }
    let size = arrow_tip_size(stroke);
    let backoff = (size * 0.55).min(length * 0.5);
    Point::new(end.x - dx / length * backoff, end.y - dy / length * backoff)
}

fn arrow_tip_size(stroke: StrokeStyle) -> f32 {
    (stroke.width * 5.8).clamp(16.0, 64.0)
}

fn distance(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

unsafe fn draw_label(hdc: HDC, x: f32, y: f32, label: &str) {
    draw_label_sized(hdc, x, y, label, 18.0);
}

unsafe fn draw_label_sized(hdc: HDC, x: f32, y: f32, label: &str, font_size: f32) {
    draw_label_sized_color(hdc, x, y, label, font_size, Color::BLACK);
}

unsafe fn draw_label_sized_color(
    hdc: HDC,
    x: f32,
    y: f32,
    label: &str,
    font_size: f32,
    color: Color,
) {
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, crate::util::colorref(color));
    let font = CreateFontW(
        -(font_size.round().max(1.0) as i32),
        0,
        0,
        0,
        400,
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
    let old_font = SelectObject(hdc, font);
    let wide: Vec<u16> = label.encode_utf16().collect();
    let _ = TextOutW(hdc, x.round() as i32, y.round() as i32, &wide);
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

unsafe fn draw_watermark_label_rotated(
    hdc: HDC,
    x: f32,
    y: f32,
    label: &str,
    font_size: f32,
    color: Color,
    degrees: f32,
) {
    if label.is_empty() || color.a == 0 {
        return;
    }
    let text_width = approximate_text_width(label, font_size).max(font_size);
    let text_height = (font_size * 1.55).max(1.0);
    let angle = degrees.to_radians();
    let pad = (font_size * 2.0).ceil().max(8.0);
    let width = (text_width * angle.cos().abs() + text_height * angle.sin().abs() + pad * 2.0)
        .ceil()
        .max(1.0) as u32;
    let height = (text_width * angle.sin().abs() + text_height * angle.cos().abs() + pad * 2.0)
        .ceil()
        .max(1.0) as u32;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: width * height * 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let Ok(bitmap) = CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
        return;
    };
    if bits.is_null() {
        let _ = DeleteObject(bitmap);
        return;
    }
    let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width * height * 4) as usize);
    pixels.fill(0);

    let mem_dc = CreateCompatibleDC(hdc);
    let old_bitmap = SelectObject(mem_dc, bitmap);
    SetBkMode(mem_dc, TRANSPARENT);
    SetTextColor(mem_dc, windows::Win32::Foundation::COLORREF(0x00ff_ffff));
    let escapement = (degrees * 10.0).round() as i32;
    let font = CreateFontW(
        -(font_size.round().max(1.0) as i32),
        0,
        escapement,
        escapement,
        700,
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
    let old_font = SelectObject(mem_dc, font);
    let wide: Vec<u16> = label.encode_utf16().collect();
    let _ = TextOutW(mem_dc, pad.round() as i32, pad.round() as i32, &wide);
    let _ = SelectObject(mem_dc, old_font);
    let _ = DeleteObject(font);

    for pixel in pixels.chunks_exact_mut(4) {
        let coverage = pixel[0].max(pixel[1]).max(pixel[2]);
        if coverage == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = 0;
            continue;
        }
        let alpha = ((coverage as u16 * color.a as u16) / 255) as u8;
        pixel[0] = ((color.b as u16 * alpha as u16) / 255) as u8;
        pixel[1] = ((color.g as u16 * alpha as u16) / 255) as u8;
        pixel[2] = ((color.r as u16 * alpha as u16) / 255) as u8;
        pixel[3] = alpha;
    }

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = AlphaBlend(
        hdc,
        (x - pad).round() as i32,
        (y - pad).round() as i32,
        width as i32,
        height as i32,
        mem_dc,
        0,
        0,
        width as i32,
        height as i32,
        blend,
    );
    let _ = SelectObject(mem_dc, old_bitmap);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem_dc);
}
unsafe fn draw_text_annotation(
    hdc: HDC,
    bounds: Rect,
    text: &str,
    font_size: f32,
    color: Color,
    framed: bool,
    filled: bool,
) {
    if text.is_empty() {
        return;
    }
    let padding = 8.0;
    let draw_bounds = if framed && bounds.is_visible() {
        bounds
    } else {
        let text_width = text
            .lines()
            .map(|line| line.chars().count() as f32 * font_size * 0.68)
            .fold(0.0, f32::max)
            .max(font_size);
        let line_count = text.lines().count().max(1) as f32;
        Rect::new(
            bounds.x - padding,
            bounds.y - font_size * 1.12 - padding,
            text_width + padding * 2.0,
            font_size * 1.55 * line_count + padding * 2.0,
        )
    };
    let text_color = if filled {
        contrast_text_color(color)
    } else {
        color
    };
    if filled {
        fill_rounded_rect_antialias(hdc, draw_bounds, 18.0, color);
    }
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, crate::util::colorref(text_color));
    let font = CreateFontW(
        -(font_size.round().max(1.0) as i32),
        0,
        0,
        0,
        400,
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
    let old_font = SelectObject(hdc, font);
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rect = rect_to_rect(Rect::new(
        draw_bounds.x + padding,
        draw_bounds.y + padding,
        (draw_bounds.width - padding * 2.0).max(1.0),
        (draw_bounds.height - padding * 2.0).max(1.0),
    ));
    let flags = if framed {
        DT_LEFT | DT_WORDBREAK
    } else {
        DT_LEFT
    };
    let _ = DrawTextW(hdc, &mut wide, &mut rect, flags);
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

unsafe fn draw_inline_text_caret(
    hdc: HDC,
    baseline: Point,
    text: &str,
    font_size: f32,
    color: Color,
) {
    if !watermark_caret_visible() {
        return;
    }
    let caret_x = baseline.x + inline_text_width(text, font_size) - font_size * 0.18;
    let caret_top = baseline.y - font_size * 1.08;
    let caret_height = font_size * 1.28;
    let width = (font_size * 0.07).clamp(2.0, 4.0);
    fill_rounded_rect_antialias(
        hdc,
        Rect::new(caret_x, caret_top, width, caret_height),
        width / 2.0,
        color,
    );
}

fn inline_text_width(text: &str, font_size: f32) -> f32 {
    text.lines()
        .map(|line| line.chars().count() as f32 * font_size * 0.68)
        .fold(0.0, f32::max)
}

unsafe fn draw_dotted_rect(hdc: HDC, rect: Rect) {
    let dot = 2.0_f32;
    let gap = 5.0_f32;
    let mut x = rect.x;
    while x <= rect.right() {
        let width = dot.min(rect.right() - x).max(0.0);
        if width > 0.0 {
            let brush = CreateSolidBrush(crate::util::colorref(inverted_pixel_color(
                hdc,
                x + width / 2.0,
                rect.y,
            )));
            FillRect(hdc, &rect_to_rect(Rect::new(x, rect.y, width, dot)), brush);
            let _ = DeleteObject(brush);
            let brush = CreateSolidBrush(crate::util::colorref(inverted_pixel_color(
                hdc,
                x + width / 2.0,
                rect.bottom() - dot,
            )));
            FillRect(
                hdc,
                &rect_to_rect(Rect::new(x, rect.bottom() - dot, width, dot)),
                brush,
            );
            let _ = DeleteObject(brush);
        }
        x += dot + gap;
    }
    let mut y = rect.y;
    while y <= rect.bottom() {
        let height = dot.min(rect.bottom() - y).max(0.0);
        if height > 0.0 {
            let brush = CreateSolidBrush(crate::util::colorref(inverted_pixel_color(
                hdc,
                rect.x,
                y + height / 2.0,
            )));
            FillRect(hdc, &rect_to_rect(Rect::new(rect.x, y, dot, height)), brush);
            let _ = DeleteObject(brush);
            let brush = CreateSolidBrush(crate::util::colorref(inverted_pixel_color(
                hdc,
                rect.right() - dot,
                y + height / 2.0,
            )));
            FillRect(
                hdc,
                &rect_to_rect(Rect::new(rect.right() - dot, y, dot, height)),
                brush,
            );
            let _ = DeleteObject(brush);
        }
        y += dot + gap;
    }
}

unsafe fn inverted_pixel_color(hdc: HDC, x: f32, y: f32) -> Color {
    let pixel = GetPixel(hdc, x.round() as i32, y.round() as i32).0;
    let r = (pixel & 0xff) as u8;
    let g = ((pixel >> 8) & 0xff) as u8;
    let b = ((pixel >> 16) & 0xff) as u8;
    Color::rgb(255 - r, 255 - g, 255 - b)
}

fn resize_tag_for_text(annotation: &mut Annotation, font_size: f32) {
    let AnnotationKind::Tag { label, .. } = &annotation.kind else {
        return;
    };
    if label.is_empty() {
        return;
    }
    let inner_width = (annotation.bounds.width - 16.0).max(font_size);
    let chars_per_line = (inner_width / (font_size * 0.55)).floor().max(1.0);
    let line_count = label
        .lines()
        .map(|line| {
            (line.chars().count() as f32 / chars_per_line)
                .ceil()
                .max(1.0)
        })
        .sum::<f32>()
        .max(1.0);
    let desired_height = 16.0 + line_count * font_size * 1.45;
    if desired_height > annotation.bounds.height {
        annotation.bounds.height = desired_height;
    }
}

unsafe fn draw_tag_annotation(
    hdc: HDC,
    bounds: Rect,
    anchor: Point,
    label: &str,
    frame_color: Color,
    font_size: f32,
    frame_width: f32,
) {
    let frame = tag_frame_for_width(frame_width);
    let pointer_frame = TAG_FRAME;
    let radius = TAG_RADIUS;
    draw_tag_outer_shape(
        hdc,
        bounds,
        anchor,
        frame_color,
        frame,
        pointer_frame,
        radius,
    );
    fill_rounded_rect_antialias(hdc, bounds, radius.max(2.0), Color::WHITE);
    draw_wrapped_text(hdc, bounds, label, font_size, Color::BLACK);
}

fn tag_frame_for_width(width: f32) -> f32 {
    if width >= 6.0 {
        width.clamp(6.0, 28.0)
    } else {
        TAG_FRAME
    }
}

unsafe fn draw_tag_outer_shape(
    hdc: HDC,
    bounds: Rect,
    anchor: Point,
    color: Color,
    frame: f32,
    pointer_frame: f32,
    radius: f32,
) {
    let pointer_box = expand_rect(bounds, pointer_frame);
    if let Some(geom) = tag_corner_connector(pointer_box, anchor, pointer_frame, radius) {
        draw_tag_pointer_shape(hdc, geom, color, pointer_frame.max(radius * 0.35));
    } else {
        let geom = tag_pointer_geometry(pointer_box, anchor, pointer_frame, radius);
        draw_tag_pointer_shape(hdc, geom, color, pointer_frame.max(radius * 0.35));
    }
    fill_rounded_rect_antialias(hdc, expand_rect(bounds, frame), radius + frame, color);
}

fn expand_rect(rect: Rect, amount: f32) -> Rect {
    Rect::new(
        rect.x - amount,
        rect.y - amount,
        rect.width + amount * 2.0,
        rect.height + amount * 2.0,
    )
}

fn tag_corner_connector(
    bounds: Rect,
    anchor: Point,
    frame: f32,
    radius: f32,
) -> Option<TagPointerGeometry> {
    let outside_x = anchor.x < bounds.x || anchor.x > bounds.right();
    let outside_y = anchor.y < bounds.y || anchor.y > bounds.bottom();
    if !outside_x || !outside_y {
        return None;
    }
    let r = radius
        .min(bounds.width / 2.0)
        .min(bounds.height / 2.0)
        .max(0.0);
    let inset = (frame + (frame * 0.35).max(3.0)).max(r * 0.65);
    let center = Point::new(
        if anchor.x < bounds.x {
            bounds.x + inset
        } else {
            bounds.right() - inset
        },
        if anchor.y < bounds.y {
            bounds.y + inset
        } else {
            bounds.bottom() - inset
        },
    );
    Some(tag_pointer_from_center(
        center,
        anchor,
        (frame * 0.77).max(6.0),
        (frame * 0.20).max(2.1),
        frame * 0.72,
    ))
}

#[derive(Clone, Copy)]
struct TagPointerGeometry {
    base1: Point,
    base2: Point,
    tip1: Point,
    tip2: Point,
    anchor: Point,
}

unsafe fn draw_tag_pointer_shape(hdc: HDC, geom: TagPointerGeometry, color: Color, pad: f32) {
    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(geom.base1.x, geom.base1.y);
    builder.line_to(geom.tip1.x, geom.tip1.y);
    builder.quad_to(geom.anchor.x, geom.anchor.y, geom.tip2.x, geom.tip2.y);
    builder.line_to(geom.base2.x, geom.base2.y);
    builder.close();
    if let Some(path) = builder.finish() {
        let min_x = geom
            .base1
            .x
            .min(geom.base2.x)
            .min(geom.tip1.x)
            .min(geom.tip2.x)
            .min(geom.anchor.x);
        let min_y = geom
            .base1
            .y
            .min(geom.base2.y)
            .min(geom.tip1.y)
            .min(geom.tip2.y)
            .min(geom.anchor.y);
        let max_x = geom
            .base1
            .x
            .max(geom.base2.x)
            .max(geom.tip1.x)
            .max(geom.tip2.x)
            .max(geom.anchor.x);
        let max_y = geom
            .base1
            .y
            .max(geom.base2.y)
            .max(geom.tip1.y)
            .max(geom.tip2.y)
            .max(geom.anchor.y);
        let surface = Rect::new(
            min_x - pad,
            min_y - pad,
            (max_x - min_x + pad * 2.0).max(1.0),
            (max_y - min_y + pad * 2.0).max(1.0),
        );
        draw_filled_tiny_path(hdc, surface, path, color);
    }
}

fn tag_pointer_geometry(
    bounds: Rect,
    anchor: Point,
    frame: f32,
    radius: f32,
) -> TagPointerGeometry {
    let edge = if anchor.y < bounds.y {
        2
    } else if anchor.y > bounds.bottom() {
        3
    } else if anchor.x < bounds.x {
        0
    } else if anchor.x > bounds.right() {
        1
    } else {
        3
    };
    let r = radius
        .min(bounds.width / 2.0)
        .min(bounds.height / 2.0)
        .max(0.0);
    let horizontal_half =
        (frame * 0.77).clamp(6.0, ((bounds.width - r * 2.0) / 2.0 - 1.0).max(6.0));
    let vertical_half = (frame * 0.77).clamp(6.0, ((bounds.height - r * 2.0) / 2.0 - 1.0).max(6.0));
    let overlap = (frame * 0.70).max(4.0);
    let round = frame * 0.72;
    match edge {
        0 => {
            let y = anchor.y.clamp(
                bounds.y + r + vertical_half,
                bounds.bottom() - r - vertical_half,
            );
            tag_pointer_from_base(
                Point::new(bounds.x + overlap, y + vertical_half),
                Point::new(bounds.x + overlap, y - vertical_half),
                anchor,
                round,
            )
        }
        1 => {
            let y = anchor.y.clamp(
                bounds.y + r + vertical_half,
                bounds.bottom() - r - vertical_half,
            );
            tag_pointer_from_base(
                Point::new(bounds.right() - overlap, y - vertical_half),
                Point::new(bounds.right() - overlap, y + vertical_half),
                anchor,
                round,
            )
        }
        2 => {
            let x = anchor.x.clamp(
                bounds.x + r + horizontal_half,
                bounds.right() - r - horizontal_half,
            );
            tag_pointer_from_base(
                Point::new(x - horizontal_half, bounds.y + overlap),
                Point::new(x + horizontal_half, bounds.y + overlap),
                anchor,
                round,
            )
        }
        _ => {
            let x = anchor.x.clamp(
                bounds.x + r + horizontal_half,
                bounds.right() - r - horizontal_half,
            );
            tag_pointer_from_base(
                Point::new(x + horizontal_half, bounds.bottom() - overlap),
                Point::new(x - horizontal_half, bounds.bottom() - overlap),
                anchor,
                round,
            )
        }
    }
}

fn tag_pointer_from_center(
    base_center: Point,
    anchor: Point,
    base_half: f32,
    tip_half: f32,
    round: f32,
) -> TagPointerGeometry {
    let vx = anchor.x - base_center.x;
    let vy = anchor.y - base_center.y;
    let len = (vx * vx + vy * vy).sqrt().max(1.0);
    let ux = vx / len;
    let uy = vy / len;
    let px = -uy;
    let py = ux;
    let r = round.min(len * 0.4);
    let base1 = Point::new(
        base_center.x + px * base_half,
        base_center.y + py * base_half,
    );
    let base2 = Point::new(
        base_center.x - px * base_half,
        base_center.y - py * base_half,
    );
    let tip_a = Point::new(
        anchor.x - ux * r + px * tip_half,
        anchor.y - uy * r + py * tip_half,
    );
    let tip_b = Point::new(
        anchor.x - ux * r - px * tip_half,
        anchor.y - uy * r - py * tip_half,
    );
    let same_order = distance_squared(base1, tip_a) + distance_squared(base2, tip_b);
    let swapped_order = distance_squared(base1, tip_b) + distance_squared(base2, tip_a);
    let (tip1, tip2) = if same_order <= swapped_order {
        (tip_a, tip_b)
    } else {
        (tip_b, tip_a)
    };
    TagPointerGeometry {
        base1,
        base2,
        tip1,
        tip2,
        anchor,
    }
}

fn tag_pointer_from_base(
    base1: Point,
    base2: Point,
    anchor: Point,
    round: f32,
) -> TagPointerGeometry {
    let mid = Point::new((base1.x + base2.x) / 2.0, (base1.y + base2.y) / 2.0);
    let vx = anchor.x - mid.x;
    let vy = anchor.y - mid.y;
    let len = (vx * vx + vy * vy).sqrt().max(1.0);
    let ux = vx / len;
    let uy = vy / len;
    let px = -uy;
    let py = ux;
    let r = round.min(len * 0.4);
    let tip_a = Point::new(
        anchor.x - ux * r + px * r * 0.45,
        anchor.y - uy * r + py * r * 0.45,
    );
    let tip_b = Point::new(
        anchor.x - ux * r - px * r * 0.45,
        anchor.y - uy * r - py * r * 0.45,
    );
    let same_order = distance_squared(base1, tip_a) + distance_squared(base2, tip_b);
    let swapped_order = distance_squared(base1, tip_b) + distance_squared(base2, tip_a);
    let (tip1, tip2) = if same_order <= swapped_order {
        (tip_a, tip_b)
    } else {
        (tip_b, tip_a)
    };
    TagPointerGeometry {
        base1,
        base2,
        tip1,
        tip2,
        anchor,
    }
}

fn distance_squared(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

unsafe fn draw_wrapped_text(hdc: HDC, rect: Rect, text: &str, font_size: f32, color: Color) {
    if text.is_empty() {
        return;
    }
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, crate::util::colorref(color));
    let font = CreateFontW(
        -(font_size.round().max(1.0) as i32),
        0,
        0,
        0,
        400,
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
    let old_font = SelectObject(hdc, font);
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut text_rect = rect_to_rect(Rect::new(
        rect.x + 8.0,
        rect.y + 8.0,
        (rect.width - 16.0).max(1.0),
        (rect.height - 16.0).max(1.0),
    ));
    let _ = DrawTextW(hdc, &mut wide, &mut text_rect, DT_LEFT | DT_WORDBREAK);
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

unsafe fn draw_annotations(hdc: HDC, state: &OverlayState) {
    for annotation in &state.document.annotations {
        if !is_watermark_annotation(annotation) {
            draw_annotation(hdc, state, annotation);
        }
    }
}

unsafe fn draw_watermark_annotations(hdc: HDC, state: &OverlayState) {
    for annotation in &state.document.annotations {
        if is_watermark_annotation(annotation) {
            draw_annotation(hdc, state, annotation);
        }
    }
}

unsafe fn draw_native_backed_annotations(hdc: HDC, state: &OverlayState) {
    for annotation in &state.document.annotations {
        if matches!(annotation.kind, AnnotationKind::Mosaic { .. })
            && !is_watermark_annotation(annotation)
        {
            draw_annotation(hdc, state, annotation);
        }
    }
}

fn is_watermark_annotation(annotation: &Annotation) -> bool {
    matches!(annotation.kind, AnnotationKind::Watermark { .. })
}

#[derive(Clone, Copy)]
struct AnnotationRenderSpace {
    origin: Point,
}

impl AnnotationRenderSpace {
    fn overlay(state: &OverlayState) -> Self {
        Self {
            origin: Point::new(state.screen_bounds.x, state.screen_bounds.y),
        }
    }

    fn export(region: Rect) -> Self {
        Self {
            origin: Point::new(region.x, region.y),
        }
    }

    fn rect(self, rect: Rect) -> Rect {
        rect.translate(-self.origin.x, -self.origin.y)
    }

    fn point(self, point: Point) -> Point {
        point.translate(-self.origin.x, -self.origin.y)
    }

    fn points(self, points: &[Point]) -> Vec<Point> {
        points.iter().map(|point| self.point(*point)).collect()
    }
}

unsafe fn draw_watermark_pattern(hdc: HDC, state: &OverlayState, region: Rect) {
    let text = state.watermark_text.trim();
    let date = state.watermark_date_enabled.then(localized_today_string);
    let image = state.watermark_image_bitmap.as_ref();
    if text.is_empty() && date.is_none() && image.is_none() {
        return;
    }
    let color = Color::rgba(
        state.watermark_color.r,
        state.watermark_color.g,
        state.watermark_color.b,
        (255.0 * WATERMARK_OPACITY).round() as u8,
    );
    let font_size = scaled(state, state.font_size).max(scaled(state, 34.0));
    let spread = font_size / scaled(state, 27.0).max(1.0);
    let step_x = scaled(state, 320.0).max(scaled(state, 240.0) * spread);
    let step_y = scaled(state, 200.0).max(scaled(state, 150.0) * spread);
    let mut row = 0;
    let mut y = region.y + scaled(state, 24.0);
    while y < region.bottom() {
        let mut x = region.x
            + if row % 2 == 0 {
                scaled(state, 24.0)
            } else {
                step_x / 2.0
            };
        while x < region.right() {
            let mut text_y = y;
            if let Some(bitmap) = &image {
                draw_watermark_bitmap(hdc, bitmap, x, y - scaled(state, 16.0));
                text_y += scaled(state, 54.0);
            }
            if !text.is_empty() {
                draw_watermark_label_rotated(hdc, x, text_y, text, font_size, color, -20.0);
            }
            if let Some(date) = &date {
                let date_font = font_size * 0.72;
                let text_width = if text.is_empty() {
                    approximate_watermark_text_width(date, date_font)
                } else {
                    approximate_watermark_text_width(text, font_size)
                };
                let date_width = approximate_watermark_text_width(date, date_font);
                draw_watermark_label_rotated(
                    hdc,
                    x + (text_width - date_width) / 2.0,
                    text_y + font_size * 1.02,
                    date,
                    date_font,
                    color,
                    -20.0,
                );
            }
            x += step_x;
        }
        y += step_y;
        row += 1;
    }
}

fn approximate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.56
}

fn approximate_watermark_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.62
}

fn watermark_caret_visible() -> bool {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() % 1000 < 500)
        .unwrap_or(true)
}

struct WatermarkBitmap {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

fn watermark_bitmap_data_url(bitmap: &WatermarkBitmap) -> Option<String> {
    let mut rgba = Vec::with_capacity((bitmap.width * bitmap.height * 4) as usize);
    for px in bitmap.bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, bitmap.width, bitmap.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64_encode(&png_bytes)
    ))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied().unwrap_or(0);
        let b2 = bytes.get(i + 2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}
fn load_watermark_bitmap(path: &PathBuf, target_size: u32) -> Option<WatermarkBitmap> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => load_svg_watermark_bitmap(path, target_size),
        Some("png") => load_png_watermark_bitmap(path, target_size),
        _ => None,
    }
}

fn load_svg_watermark_bitmap(path: &PathBuf, target_size: u32) -> Option<WatermarkBitmap> {
    let svg = std::fs::read_to_string(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).ok()?;
    let tree_size = tree.size();
    let scale = target_size as f32 / tree_size.width().max(tree_size.height()).max(1.0);
    let width = (tree_size.width() * scale).round().max(1.0) as u32;
    let height = (tree_size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(rgba_pixmap_to_watermark(width, height, pixmap.data()))
}

fn load_png_watermark_bitmap(path: &PathBuf, target_size: u32) -> Option<WatermarkBitmap> {
    let file = File::open(path).ok()?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];
    let mut bgra = Vec::with_capacity((info.width * info.height * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => {
            for px in bytes.chunks_exact(4) {
                bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
        png::ColorType::Rgb => {
            for px in bytes.chunks_exact(3) {
                bgra.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for value in bytes {
                bgra.extend_from_slice(&[*value, *value, *value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in bytes.chunks_exact(2) {
                bgra.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(scale_watermark_bitmap(
        WatermarkBitmap {
            width: info.width,
            height: info.height,
            bgra,
        },
        target_size,
    ))
}

fn rgba_pixmap_to_watermark(width: u32, height: u32, rgba: &[u8]) -> WatermarkBitmap {
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    WatermarkBitmap {
        width,
        height,
        bgra,
    }
}

fn scale_watermark_bitmap(bitmap: WatermarkBitmap, target_size: u32) -> WatermarkBitmap {
    let max_dim = bitmap.width.max(bitmap.height).max(1);
    if max_dim == target_size {
        return bitmap;
    }
    let scale = target_size as f32 / max_dim as f32;
    let width = (bitmap.width as f32 * scale).round().max(1.0) as u32;
    let height = (bitmap.height as f32 * scale).round().max(1.0) as u32;
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let sx = ((x as f32 / scale).round() as u32).min(bitmap.width - 1);
            let sy = ((y as f32 / scale).round() as u32).min(bitmap.height - 1);
            let src = ((sy * bitmap.width + sx) * 4) as usize;
            let dst = ((y * width + x) * 4) as usize;
            bgra[dst..dst + 4].copy_from_slice(&bitmap.bgra[src..src + 4]);
        }
    }
    WatermarkBitmap {
        width,
        height,
        bgra,
    }
}

unsafe fn draw_watermark_bitmap(hdc: HDC, bitmap: &WatermarkBitmap, x: f32, y: f32) {
    let rect = Rect::new(
        x - bitmap.width as f32 / 2.0,
        y - bitmap.height as f32 / 2.0,
        bitmap.width as f32,
        bitmap.height as f32,
    );
    let _ = alpha_blend_bgra(hdc, rect, bitmap.width, bitmap.height, &bitmap.bgra);
}

fn faded_watermark_bitmap(bitmap: &WatermarkBitmap, opacity: f32) -> WatermarkBitmap {
    let mut bgra = bitmap.bgra.clone();
    for px in bgra.chunks_exact_mut(4) {
        let alpha = (px[3] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
        px[3] = alpha;
        for channel in 0..3 {
            px[channel] = ((px[channel] as u16 * alpha as u16) / 255) as u8;
        }
    }
    WatermarkBitmap {
        width: bitmap.width,
        height: bitmap.height,
        bgra,
    }
}

#[allow(dead_code)]
fn rotated_watermark_bitmap(
    bitmap: &WatermarkBitmap,
    degrees: f32,
    opacity: f32,
) -> WatermarkBitmap {
    let angle = degrees.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let width = (bitmap.width as f32 * cos.abs() + bitmap.height as f32 * sin.abs())
        .ceil()
        .max(1.0) as u32;
    let height = (bitmap.width as f32 * sin.abs() + bitmap.height as f32 * cos.abs())
        .ceil()
        .max(1.0) as u32;
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    let src_cx = bitmap.width as f32 / 2.0;
    let src_cy = bitmap.height as f32 / 2.0;
    let dst_cx = width as f32 / 2.0;
    let dst_cy = height as f32 / 2.0;
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - dst_cx;
            let dy = y as f32 - dst_cy;
            let sx = dx * cos + dy * sin + src_cx;
            let sy = -dx * sin + dy * cos + src_cy;
            if sx >= 0.0 && sy >= 0.0 && sx < bitmap.width as f32 && sy < bitmap.height as f32 {
                let src = ((sy as u32 * bitmap.width + sx as u32) * 4) as usize;
                let dst = ((y * width + x) * 4) as usize;
                let alpha = (bitmap.bgra[src + 3] as f32 * opacity)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                bgra[dst + 3] = alpha;
                for channel in 0..3 {
                    bgra[dst + channel] =
                        ((bitmap.bgra[src + channel] as u16 * alpha as u16) / 255) as u8;
                }
            }
        }
    }
    WatermarkBitmap {
        width,
        height,
        bgra,
    }
}

fn localized_today_string() -> String {
    unsafe {
        let local_time = GetLocalTime();
        let mut buffer = [0_u16; 128];
        let len = GetDateFormatEx(
            PCWSTR::null(),
            DATE_SHORTDATE,
            Some(&local_time),
            PCWSTR::null(),
            Some(&mut buffer),
            PCWSTR::null(),
        );
        if len > 1 {
            return String::from_utf16_lossy(&buffer[..(len - 1) as usize]);
        }
    }
    "Today".to_string()
}

unsafe fn draw_selected_annotation(hdc: HDC, state: &OverlayState) {
    let Some(selected_id) = state.document.selected_annotation_id else {
        return;
    };
    let Some(annotation) = state.document.annotation(selected_id) else {
        return;
    };
    let bounds = annotation
        .bounds
        .translate(-state.screen_bounds.x, -state.screen_bounds.y);
    let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0000c8ff));

    match &annotation.kind {
        AnnotationKind::Line { start, end }
        | AnnotationKind::Arrow { start, end }
        | AnnotationKind::Highlighter {
            shape: HighlightShape::Rectangle,
            start,
            end,
            ..
        } => {
            for point in [
                state.screen_to_overlay(*start),
                state.screen_to_overlay(*end),
            ] {
                let handle = Rect::new(point.x - 5.0, point.y - 5.0, 10.0, 10.0);
                FillRect(hdc, &rect_to_rect(handle), brush);
            }
        }
        AnnotationKind::Tag { anchor, .. } => {
            FrameRect(hdc, &rect_to_rect(bounds), brush);
            for point in [
                state.screen_to_overlay(*anchor),
                Point::new(bounds.x, bounds.y),
                Point::new(bounds.right(), bounds.y),
                Point::new(bounds.right(), bounds.bottom()),
                Point::new(bounds.x, bounds.bottom()),
            ] {
                let handle = Rect::new(point.x - 4.0, point.y - 4.0, 8.0, 8.0);
                FillRect(hdc, &rect_to_rect(handle), brush);
            }
        }
        AnnotationKind::Text { framed: true, .. } => {
            draw_dotted_rect(hdc, bounds);
            for point in [
                Point::new(bounds.x, bounds.y),
                Point::new(bounds.right(), bounds.y),
                Point::new(bounds.right(), bounds.bottom()),
                Point::new(bounds.x, bounds.bottom()),
            ] {
                let handle = Rect::new(point.x - 4.0, point.y - 4.0, 8.0, 8.0);
                FillRect(hdc, &rect_to_rect(handle), brush);
            }
        }
        AnnotationKind::Text {
            text,
            font_size,
            framed: false,
            filled,
        } => {
            if state.editing_text_id == Some(selected_id) {
                let caret_color = if *filled {
                    contrast_text_color(annotation.stroke.color)
                } else {
                    annotation.stroke.color
                };
                draw_inline_text_caret(
                    hdc,
                    Point::new(bounds.x, bounds.y),
                    text,
                    *font_size,
                    caret_color,
                );
            }
        }
        AnnotationKind::Pen { .. } | AnnotationKind::PenArrow { .. } => {}
        _ => {
            FrameRect(hdc, &rect_to_rect(bounds), brush);
            for point in [
                Point::new(bounds.x, bounds.y),
                Point::new(bounds.right(), bounds.y),
                Point::new(bounds.right(), bounds.bottom()),
                Point::new(bounds.x, bounds.bottom()),
            ] {
                let handle = Rect::new(point.x - 4.0, point.y - 4.0, 8.0, 8.0);
                FillRect(hdc, &rect_to_rect(handle), brush);
            }
        }
    }
    let _ = DeleteObject(brush);
}

unsafe fn draw_drag_preview(hdc: HDC, state: &OverlayState) {
    let Some(DragState::DrawingAnnotation {
        start,
        current,
        points,
    }) = &state.drag
    else {
        return;
    };

    if state.active_tool == ToolKind::Pen {
        let overlay_points: Vec<Point> = points
            .iter()
            .map(|point| state.screen_to_overlay(*point))
            .collect();
        draw_fast_polyline(hdc, &overlay_points, state.current_stroke);
        if state.pen_mode == PenMode::Arrow {
            draw_arrow_tip_for_points(hdc, &overlay_points, state.current_stroke);
        }
        return;
    }

    if matches!(
        state.active_tool,
        ToolKind::Rectangle | ToolKind::Oval | ToolKind::Line | ToolKind::Arrow
    ) {
        draw_fast_geometry_preview(
            hdc,
            state.active_tool,
            state.screen_to_overlay(*start),
            state.screen_to_overlay(*current),
            state.current_stroke,
        );
        return;
    }

    if state.active_tool == ToolKind::Mosaic {
        let bounds = Rect::from_points(*start, *current)
            .translate(-state.screen_bounds.x, -state.screen_bounds.y);
        draw_blur_region(hdc, bounds, HIGHLIGHTER_RADIUS);
        return;
    }

    let annotation = annotation_from_tool(state, *start, *current, points.clone());
    draw_annotation(hdc, state, &annotation);
    if let AnnotationKind::Text { framed: true, .. } = annotation.kind {
        let bounds = annotation
            .bounds
            .translate(-state.screen_bounds.x, -state.screen_bounds.y);
        draw_dotted_rect(hdc, bounds);
    }
}

unsafe fn draw_fast_geometry_preview(
    hdc: HDC,
    tool: ToolKind,
    start: Point,
    current: Point,
    stroke: StrokeStyle,
) {
    let _pen = SelectedPen::new(hdc, stroke.width, stroke.color);
    let _brush = SelectedStockObject::null_brush(hdc);
    match tool {
        ToolKind::Rectangle => {
            let rect = rect_to_rect(Rect::from_points(start, current));
            let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
        ToolKind::Oval => {
            let rect = rect_to_rect(Rect::from_points(start, current));
            let _ = Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
        ToolKind::Line => {
            let _ = MoveToEx(hdc, start.x.round() as i32, start.y.round() as i32, None);
            let _ = LineTo(hdc, current.x.round() as i32, current.y.round() as i32);
        }
        ToolKind::Arrow => {
            let shaft_end = arrow_shaft_end(start, current, stroke);
            let _ = MoveToEx(hdc, start.x.round() as i32, start.y.round() as i32, None);
            let _ = LineTo(hdc, shaft_end.x.round() as i32, shaft_end.y.round() as i32);
            draw_arrow_tip(hdc, start, current, stroke);
        }
        _ => {}
    }
}

unsafe fn draw_fast_polyline(hdc: HDC, points: &[Point], stroke: StrokeStyle) {
    let Some(first) = points.first().copied() else {
        return;
    };
    let _pen = SelectedPen::new(hdc, stroke.width, stroke.color);
    let _ = MoveToEx(hdc, first.x.round() as i32, first.y.round() as i32, None);
    for point in points.iter().copied().skip(1) {
        let _ = LineTo(hdc, point.x.round() as i32, point.y.round() as i32);
    }
}

unsafe fn draw_annotation(hdc: HDC, state: &OverlayState, annotation: &Annotation) {
    draw_annotation_in_space(
        hdc,
        state,
        annotation,
        AnnotationRenderSpace::overlay(state),
    );
}

unsafe fn draw_annotation_in_space(
    hdc: HDC,
    state: &OverlayState,
    annotation: &Annotation,
    space: AnnotationRenderSpace,
) {
    let style = RenderStyle::for_state(state);
    let bounds = space.rect(annotation.bounds);
    match &annotation.kind {
        AnnotationKind::Rectangle => {
            draw_stroked_rect(hdc, bounds, annotation.stroke);
        }
        AnnotationKind::Oval => {
            draw_stroked_oval(hdc, bounds, annotation.stroke);
        }
        AnnotationKind::Line { start, end } => draw_line(
            hdc,
            space.point(*start),
            space.point(*end),
            false,
            annotation.stroke,
        ),
        AnnotationKind::Arrow { start, end } => draw_line(
            hdc,
            space.point(*start),
            space.point(*end),
            true,
            annotation.stroke,
        ),
        AnnotationKind::StepNumber { number } => {
            draw_step_number(hdc, bounds, *number, annotation.stroke.color, style);
        }
        AnnotationKind::Text {
            text,
            font_size,
            framed,
            filled,
        } => {
            draw_text_annotation(
                hdc,
                bounds,
                text,
                *font_size,
                annotation.stroke.color,
                *framed,
                *filled,
            );
        }
        AnnotationKind::Watermark { text, .. } => {
            draw_label_sized_color(
                hdc,
                bounds.x,
                bounds.y,
                text,
                state.font_size,
                annotation.stroke.color,
            );
        }
        AnnotationKind::Tag {
            label,
            anchor,
            font_size,
        } => {
            draw_tag_annotation(
                hdc,
                bounds,
                space.point(*anchor),
                label,
                annotation.stroke.color,
                *font_size,
                annotation.stroke.width,
            );
        }
        AnnotationKind::Mosaic { .. } => {
            draw_blur_region(hdc, bounds, HIGHLIGHTER_RADIUS);
        }
        AnnotationKind::Highlighter {
            shape,
            opacity,
            start,
            end,
        } => {
            draw_highlighter(
                hdc,
                bounds,
                *shape,
                highlighter_color(annotation.stroke.color, *opacity),
                annotation.stroke.width,
                space.point(*start),
                space.point(*end),
            );
        }
        AnnotationKind::Pen { points } => {
            let render_points = space.points(points);
            draw_pen_path(hdc, &render_points, annotation.stroke);
        }
        AnnotationKind::PenArrow { points } => {
            let render_points = space.points(points);
            let shaft_points = pen_arrow_shaft_points(&render_points, annotation.stroke);
            draw_pen_path_with_cap(
                hdc,
                &shaft_points,
                annotation.stroke,
                resvg::tiny_skia::LineCap::Round,
            );
            draw_arrow_tip_for_points(hdc, &render_points, annotation.stroke);
        }
    }
    if let Some(number) = annotation.step_number {
        draw_step_badge(
            hdc,
            auto_step_badge_center(annotation, bounds, space, style),
            number,
            annotation.stroke.color,
            style,
        );
    }
}

unsafe fn draw_step_number(hdc: HDC, bounds: Rect, number: u32, color: Color, style: RenderStyle) {
    draw_step_badge(
        hdc,
        Point::new(
            bounds.center().x,
            bounds.y - style.step_badge_size / 2.0 - 4.0,
        ),
        number,
        color,
        style,
    );
}

fn auto_step_badge_center(
    annotation: &Annotation,
    bounds: Rect,
    space: AnnotationRenderSpace,
    style: RenderStyle,
) -> Point {
    let size = style.step_badge_size;
    match &annotation.kind {
        AnnotationKind::Line { start, end }
        | AnnotationKind::Arrow { start, end }
        | AnnotationKind::Highlighter {
            shape: HighlightShape::Rectangle,
            start,
            end,
            ..
        } => {
            let start = space.point(*start);
            let end = space.point(*end);
            Point::new(
                (start.x + end.x) / 2.0,
                (start.y + end.y) / 2.0 - annotation.stroke.width / 2.0 - size / 2.0 - 6.0,
            )
        }
        AnnotationKind::Pen { points } | AnnotationKind::PenArrow { points } => {
            let render_points = space.points(points);
            let midpoint = polyline_midpoint(&render_points).unwrap_or_else(|| bounds.center());
            Point::new(
                midpoint.x,
                midpoint.y - annotation.stroke.width / 2.0 - size / 2.0 - 3.0,
            )
        }
        _ => Point::new(bounds.center().x, bounds.y - size / 2.0 - 6.0),
    }
}

fn polyline_midpoint(points: &[Point]) -> Option<Point> {
    if points.is_empty() {
        return None;
    }
    if points.len() == 1 {
        return Some(points[0]);
    }
    let total: f32 = points
        .windows(2)
        .map(|pair| ((pair[1].x - pair[0].x).powi(2) + (pair[1].y - pair[0].y).powi(2)).sqrt())
        .sum();
    if total <= 0.0 {
        return Some(points[points.len() / 2]);
    }
    let target = total / 2.0;
    let mut walked = 0.0;
    for pair in points.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let length = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
        if walked + length >= target {
            let t = (target - walked) / length.max(1.0);
            return Some(Point::new(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            ));
        }
        walked += length;
    }
    points.last().copied()
}

unsafe fn draw_step_badge(hdc: HDC, center: Point, number: u32, color: Color, style: RenderStyle) {
    let size = style.step_badge_size;
    let marker = Rect::new(center.x - size / 2.0, center.y - size / 2.0, size, size);
    fill_oval_antialias(hdc, marker, color);
    let label = number.to_string();
    let text_x = marker.x
        + if number < 10 {
            style.step_badge_single_digit_x
        } else {
            style.step_badge_multi_digit_x
        };
    draw_label_sized_color(
        hdc,
        text_x,
        marker.y + style.step_badge_text_y,
        &label,
        style.step_badge_font_size,
        contrast_text_color(color),
    );
}

unsafe fn copy_capture_to_clipboard(state: &OverlayState) -> Result<()> {
    let _ = maybe_request_web_export(state, ExportTarget::Clipboard);
    let Some(bitmap) = render_capture_bitmap(state) else {
        return Ok(());
    };

    OpenClipboard(state.hwnd)?;
    EmptyClipboard()?;
    SetClipboardData(CF_BITMAP_FORMAT, HANDLE(bitmap.0))?;
    CloseClipboard()?;
    Ok(())
}

unsafe fn save_capture_to_file(state: &OverlayState) -> Result<()> {
    let _ = maybe_request_web_export(state, ExportTarget::Save);
    let Some(bitmap) = render_capture_bitmap(state) else {
        return Ok(());
    };
    if let Some(path) = show_save_png_dialog(state.hwnd) {
        let _ = write_png_file(
            bitmap,
            state.document.capture_region.expect("bitmap needs region"),
            &path,
        );
    }
    let _ = DeleteObject(bitmap);
    Ok(())
}

unsafe fn render_capture_bitmap(state: &OverlayState) -> Option<HBITMAP> {
    let Some(region) = state.document.capture_region else {
        return None;
    };

    let screen_dc = GetDC(None);
    let source_dc = CreateCompatibleDC(screen_dc);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let bitmap = CreateCompatibleBitmap(
        screen_dc,
        region.width.round() as i32,
        region.height.round() as i32,
    );
    let _ = SelectObject(source_dc, state.background_bitmap);
    let _ = SelectObject(mem_dc, bitmap);
    let _ = BitBlt(
        mem_dc,
        0,
        0,
        region.width.round() as i32,
        region.height.round() as i32,
        source_dc,
        (region.x - state.screen_bounds.x).round() as i32,
        (region.y - state.screen_bounds.y).round() as i32,
        SRCCOPY,
    );

    draw_export_annotations(mem_dc, state, region);
    draw_watermark_pattern(
        mem_dc,
        state,
        Rect::new(0.0, 0.0, region.width, region.height),
    );
    draw_export_watermark_annotations(mem_dc, state, region);

    let _ = DeleteDC(source_dc);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    Some(bitmap)
}

unsafe fn write_png_file(bitmap: HBITMAP, region: Rect, path: &PathBuf) -> std::io::Result<()> {
    let (width, height, pixels) = bitmap_rgba_pixels(bitmap, region)?;
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    Ok(())
}

unsafe fn bitmap_rgba_pixels(
    bitmap: HBITMAP,
    region: Rect,
) -> std::io::Result<(u32, u32, Vec<u8>)> {
    let width = region.width.round().max(1.0) as i32;
    let height = region.height.round().max(1.0) as i32;
    let stride = width as usize * 4;
    let image_size = stride * height as usize;
    let mut bgra = vec![0_u8; image_size];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: image_size as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let hdc = GetDC(None);
    let lines = GetDIBits(
        hdc,
        bitmap,
        0,
        height as u32,
        Some(bgra.as_mut_ptr().cast()),
        &mut info,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, hdc);
    if lines == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut rgba = bgra;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    Ok((width as u32, height as u32, rgba))
}

unsafe fn show_save_png_dialog(owner: HWND) -> Option<PathBuf> {
    let mut file_buffer = default_png_save_path_wide();
    file_buffer.resize(1024, 0);
    let filter = wide_null_double("PNG Image (*.png)\0*.png\0All Files (*.*)\0*.*\0");
    let title = wide_null("Save screenshot as PNG");
    let default_ext = wide_null("png");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: PCWSTR(default_ext.as_ptr()),
        Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    if !GetSaveFileNameW(&mut dialog).as_bool() {
        return None;
    }
    let len = file_buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(file_buffer.len());
    let mut path = PathBuf::from(String::from_utf16_lossy(&file_buffer[..len]));
    if path.extension().is_none() {
        path.set_extension("png");
    }
    Some(path)
}

unsafe fn show_open_image_dialog(owner: HWND) -> Option<PathBuf> {
    let mut file_buffer = vec![0_u16; 1024];
    let filter = wide_null_double(
        "Images (*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.svg)\0*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.svg\0All Files (*.*)\0*.*\0",
    );
    let title = wide_null("Choose watermark image");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST,
        ..Default::default()
    };
    if !GetOpenFileNameW(&mut dialog).as_bool() {
        return None;
    }
    let len = file_buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(file_buffer.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file_buffer[..len])))
}

fn default_png_save_path_wide() -> Vec<u16> {
    let path = default_save_path();
    wide_null(path.to_string_lossy().as_ref())
}

fn default_save_path() -> PathBuf {
    let dir = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("Pictures")
        .join("ScreenCaptn");
    let _ = fs::create_dir_all(&dir);
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    dir.join(format!("screencaptn-{seconds}.png"))
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_null_double(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    if !wide.ends_with(&[0, 0]) {
        wide.push(0);
        wide.push(0);
    }
    wide
}

unsafe fn draw_export_annotations(hdc: HDC, state: &OverlayState, region: Rect) {
    let space = AnnotationRenderSpace::export(region);
    for annotation in &state.document.annotations {
        if !is_watermark_annotation(annotation) {
            draw_annotation_in_space(hdc, state, annotation, space);
        }
    }
}

unsafe fn draw_export_watermark_annotations(hdc: HDC, state: &OverlayState, region: Rect) {
    let space = AnnotationRenderSpace::export(region);
    for annotation in &state.document.annotations {
        if is_watermark_annotation(annotation) {
            draw_annotation_in_space(hdc, state, annotation, space);
        }
    }
}
