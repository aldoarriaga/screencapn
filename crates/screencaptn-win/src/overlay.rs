use crate::util::{point_from_lparam, rect_to_rect, SelectedPen, SelectedStockObject};
use screencaptn_core::{
    Annotation, AnnotationId, AnnotationKind, CaptureDocument, Color, HighlightShape, History,
    MosaicMode, Point, Rect, ResizeHandle, StrokeStyle, ToolKind,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{w, Result};
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateSolidBrush, DeleteDC, DeleteObject, Ellipse, FillRect, FrameRect, GetDC, GetDIBits,
    GetMonitorInfoW, InvalidateRect, LineTo, MonitorFromPoint, MoveToEx, Rectangle, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HBRUSH, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows,
    EnumWindows, GetClientRect, GetMessageW, GetShellWindow, GetSystemMetrics, GetWindowLongW,
    GetWindowRect, IsIconic, IsWindowVisible, LoadCursorW, RegisterClassW, SetCursor,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    GWL_EXSTYLE, GWLP_USERDATA, HMENU, IDC_ARROW, IDC_CROSS, IDC_SIZEALL, IDC_SIZENESW,
    IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, WM_CHAR, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

const OVERLAY_CLASS: windows::core::PCWSTR = w!("ScreenCaptnCaptureOverlay");
const CF_BITMAP_FORMAT: u32 = 2;
const HANDLE_RADIUS: f32 = 6.0;
const MIN_REGION_SIZE: f32 = 24.0;
const TOOLBAR_BUTTON: f32 = 30.0;
const TOOLBAR_HEIGHT: f32 = 32.0;
const TOOLBAR_GRIP_WIDTH: f32 = 22.0;
const FRAME_HIT_WIDTH: f32 = 8.0;
const CLICK_DRAG_THRESHOLD: f32 = 5.0;

pub fn open_capture_overlay() -> Result<()> {
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
        let mut state = Box::new(OverlayState::new(
            screen_bounds,
            background_bitmap,
            detected_regions,
        ));
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
        Box::leak(state);

        let _ = ShowWindow(hwnd, SW_SHOW);

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
    document: CaptureDocument,
    history: History<CaptureDocument>,
    active_tool: ToolKind,
    current_stroke: StrokeStyle,
    highlighter_opacity: f32,
    mosaic_brush_size: f32,
    font_size: f32,
    next_step_number: u32,
    editing_text_id: Option<AnnotationId>,
    toolbar_origin: Option<Point>,
    drag: Option<DragState>,
    toolbar_buttons: Vec<ToolbarButton>,
}

impl OverlayState {
    fn new(
        screen_bounds: Rect,
        background_bitmap: HBITMAP,
        detected_regions: Vec<DetectedRegion>,
    ) -> Self {
        Self {
            hwnd: HWND::default(),
            screen_bounds,
            background_bitmap,
            detected_regions,
            hover_region: None,
            document: CaptureDocument::new(),
            history: History::new(100),
            active_tool: ToolKind::Rectangle,
            current_stroke: StrokeStyle::default(),
            highlighter_opacity: 0.45,
            mosaic_brush_size: 16.0,
            font_size: 18.0,
            next_step_number: 1,
            editing_text_id: None,
            toolbar_origin: None,
            drag: None,
            toolbar_buttons: Vec::new(),
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
}

#[derive(Clone, Debug)]
struct DetectedRegion {
    window: Rect,
    client: Option<Rect>,
    sections: Vec<Rect>,
}

#[derive(Clone, Copy)]
struct ToolbarButton {
    rect: Rect,
    tool: ToolbarAction,
}

#[derive(Clone, Copy)]
enum ToolbarAction {
    Grip,
    Tool(ToolKind),
    Undo,
    Copy,
    Save,
    Cancel,
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
}

#[derive(Clone, Copy, Debug)]
enum AnnotationEdit {
    BoxResize(ResizeHandle),
    LineStart,
    LineEnd,
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
        WM_LBUTTONDOWN => {
            let point = point_from_lparam(lparam);
            handle_mouse_down(state, point);
            SetCapture(hwnd);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let point = point_from_lparam(lparam);
            if handle_mouse_move(state, point) {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let point = point_from_lparam(lparam);
            handle_mouse_up(state, point);
            let _ = ReleaseCapture();
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            handle_key_down(state, wparam.0 as u32);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_CHAR => {
            handle_char(state, wparam.0 as u32);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = DeleteObject(state.background_bitmap);
            let _ = Box::from_raw(state_ptr);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn handle_mouse_down(state: &mut OverlayState, point: Point) {
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
            state.editing_text_id = None;
            state.checkpoint();
            state.drag = Some(DragState::ResizingRegion { handle });
            return;
        }

        let screen_point = state.overlay_to_screen(point);
        if let Some(id) = state.document.select_at(screen_point.x, screen_point.y) {
            state.editing_text_id = editable_text_annotation(state, id).then_some(id);
            if let Some(original) = state
                .document
                .annotations
                .iter()
                .find(|annotation| annotation.id == id)
                .cloned()
            {
                state.checkpoint();
                if let Some(edit) = hit_annotation_edit_handle(state, &original, screen_point) {
                    state.drag = Some(DragState::EditingAnnotation { id, edit, original });
                } else {
                    state.drag = Some(DragState::MovingAnnotation {
                        start: screen_point,
                        id,
                        original,
                    });
                }
                return;
            }
        }

        if region_frame_contains(region, point) {
            state.editing_text_id = None;
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
            state.editing_text_id = None;
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

fn editable_text_annotation(state: &OverlayState, id: AnnotationId) -> bool {
    state
        .document
        .annotations
        .iter()
        .find(|annotation| annotation.id == id)
        .is_some_and(|annotation| {
            matches!(
                annotation.kind,
                AnnotationKind::Text { .. } | AnnotationKind::Tag { .. }
            )
        })
}

fn detect_hover_region(state: &OverlayState, screen_point: Point) -> Option<Rect> {
    for detected in &state.detected_regions {
        if !detected.window.contains(screen_point) {
            continue;
        }

        if let Some(section) = best_section_at(detected, screen_point) {
            return Some(section);
        }

        if let Some(client) = detected.client.filter(|client| {
            client.contains(screen_point) && !same_rect(*client, detected.window, 4.0)
        }) {
            return Some(client);
        }

        return Some(detected.window);
    }
    unsafe { monitor_region_at(screen_point) }
}

fn best_section_at(detected: &DetectedRegion, screen_point: Point) -> Option<Rect> {
    detected
        .sections
        .iter()
        .copied()
        .filter(|section| section.contains(screen_point))
        .min_by(|a, b| rect_area(*a).total_cmp(&rect_area(*b)))
}

fn rect_area(rect: Rect) -> f32 {
    rect.width.max(0.0) * rect.height.max(0.0)
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
    let Some(region) = detected_region_from_hwnd(hwnd) else {
        return BOOL(1);
    };
    context.regions.push(region);
    BOOL(1)
}

unsafe fn detected_region_from_hwnd(hwnd: HWND) -> Option<DetectedRegion> {
    if hwnd == GetShellWindow() || IsIconic(hwnd).as_bool() {
        return None;
    }
    if !IsWindowVisible(hwnd).as_bool() {
        return None;
    }
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return None;
    }

    let mut window_rect = RECT::default();
    if GetWindowRect(hwnd, &mut window_rect).is_err() {
        return None;
    }
    let window = rect_from_win32(window_rect);
    if !window.is_visible() {
        return None;
    }

    let client = client_rect_for_window(hwnd, window);
    let mut sections = child_sections_for_window(hwnd, window, client);
    normalize_sections(&mut sections);
    Some(DetectedRegion {
        window,
        client,
        sections,
    })
}

struct ChildSectionContext {
    window: Rect,
    client: Option<Rect>,
    sections: Vec<Rect>,
}

unsafe fn child_sections_for_window(hwnd: HWND, window: Rect, client: Option<Rect>) -> Vec<Rect> {
    let mut context = ChildSectionContext {
        window,
        client,
        sections: Vec::new(),
    };
    let context_ptr = &mut context as *mut ChildSectionContext;
    let _ = EnumChildWindows(
        hwnd,
        Some(enum_child_section),
        LPARAM(context_ptr as isize),
    );
    context.sections
}

fn normalize_sections(sections: &mut Vec<Rect>) {
    sections.sort_by(|a, b| rect_area(*a).total_cmp(&rect_area(*b)));
    sections.dedup_by(|a, b| same_rect(*a, *b, 3.0));
}

unsafe extern "system" fn enum_child_section(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return BOOL(1);
    }

    let section = rect_from_win32(rect);
    let context = &mut *(lparam.0 as *mut ChildSectionContext);
    if section_candidate(section, context) {
        context.sections.push(section);
    }
    BOOL(1)
}

fn section_candidate(section: Rect, context: &ChildSectionContext) -> bool {
    if !section.is_visible() || section.width < 72.0 || section.height < 42.0 {
        return false;
    }
    if !overlaps(section, context.window) {
        return false;
    }
    if same_rect(section, context.window, 4.0) {
        return false;
    }
    if context
        .client
        .is_some_and(|client| same_rect(section, client, 4.0))
    {
        return false;
    }
    true
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
    Some(rect_from_win32(info.rcMonitor))
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

fn handle_mouse_move(state: &mut OverlayState, point: Point) -> bool {
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
            if let Some(annotation) = state
                .document
                .annotations
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            {
                *annotation = original.translated(dx, dy);
                state.document.selected_annotation_id = Some(*id);
            }
            true
        }
        Some(DragState::EditingAnnotation { id, edit, original }) => {
            if let Some(annotation) = state
                .document
                .annotations
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            {
                *annotation = edited_annotation(original, *edit, screen_point);
                state.document.selected_annotation_id = Some(*id);
            }
            true
        }
        Some(DragState::DrawingAnnotation {
            current, points, ..
        }) => {
            *current = screen_point;
            if state.active_tool == ToolKind::Pen || state.active_tool == ToolKind::Mosaic {
                points.push(screen_point);
            }
            true
        }
        None => {
            update_cursor_for_hover(state, point);
            if state.document.capture_region.is_none() {
                let next = detect_hover_region(state, screen_point);
                if state.hover_region != next {
                    state.hover_region = next;
                    return true;
                }
            }
            false
        }
    }
}

fn update_cursor_for_hover(state: &OverlayState, point: Point) {
    unsafe {
        let cursor = if let Some(region) = state.region_overlay() {
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
                    .find(|annotation| annotation.bounds.contains(screen_point))
                {
                    if let Some(edit) = hit_annotation_edit_handle(state, annotation, screen_point)
                    {
                        match edit {
                            AnnotationEdit::BoxResize(handle) => cursor_for_resize_handle(handle),
                            AnnotationEdit::LineStart | AnnotationEdit::LineEnd => IDC_SIZEALL,
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
        AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => {
            if near_point(screen_point, *start, HANDLE_RADIUS + 4.0) {
                Some(AnnotationEdit::LineStart)
            } else if near_point(screen_point, *end, HANDLE_RADIUS + 4.0) {
                Some(AnnotationEdit::LineEnd)
            } else {
                None
            }
        }
        AnnotationKind::Pen { .. } => None,
        _ => annotation
            .bounds
            .hit_resize_handle(screen_point, HANDLE_RADIUS + 4.0)
            .map(AnnotationEdit::BoxResize),
    }
}

fn near_point(point: Point, target: Point, radius: f32) -> bool {
    (point.x - target.x).abs() <= radius && (point.y - target.y).abs() <= radius
}

fn edited_annotation(original: &Annotation, edit: AnnotationEdit, to: Point) -> Annotation {
    let mut next = original.clone();
    match (&mut next.kind, edit) {
        (AnnotationKind::Line { start, .. }, AnnotationEdit::LineStart)
        | (AnnotationKind::Arrow { start, .. }, AnnotationEdit::LineStart) => {
            *start = to;
            next.bounds = line_bounds(&next.kind);
        }
        (AnnotationKind::Line { end, .. }, AnnotationEdit::LineEnd)
        | (AnnotationKind::Arrow { end, .. }, AnnotationEdit::LineEnd) => {
            *end = to;
            next.bounds = line_bounds(&next.kind);
        }
        (_, AnnotationEdit::BoxResize(handle)) => {
            next.bounds = original
                .bounds
                .resize_from_handle(handle, to, MIN_REGION_SIZE);
        }
        _ => {}
    }
    next
}

fn line_bounds(kind: &AnnotationKind) -> Rect {
    match kind {
        AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => {
            Rect::from_points(*start, *end)
        }
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
                }
            } else if rect.is_visible() {
                state.document.set_capture_region(rect);
            }
        }
        DragState::MovingRegion { .. }
        | DragState::MovingToolbar { .. }
        | DragState::ResizingRegion { .. }
        | DragState::MovingAnnotation { .. }
        | DragState::EditingAnnotation { .. } => {}
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
                    ToolKind::Text | ToolKind::Tag | ToolKind::Watermark | ToolKind::Pen
                )
            {
                let annotation = annotation_from_tool(state, start, current, points);
                let is_textual = matches!(
                    annotation.kind,
                    AnnotationKind::Text { .. } | AnnotationKind::Tag { .. }
                );
                let is_step = matches!(annotation.kind, AnnotationKind::StepNumber { .. });
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

fn handle_key_down(state: &mut OverlayState, key: u32) {
    let ctrl_down = unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 };
    if state.editing_text_id.is_some() && !ctrl_down {
        match key {
            0x1B => state.editing_text_id = None,
            key if key == VK_RETURN.0 as u32 => state.editing_text_id = None,
            0x08 => {
                edit_selected_text(state, |text| {
                    text.pop();
                });
            }
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
            }
        }
        0x53 if ctrl_down => {
            let _ = unsafe { save_capture_to_file(state) };
        }
        0x2E => {
            state.checkpoint();
            let _ = state.document.remove_selected();
        }
        _ => {}
    }
}

fn handle_char(state: &mut OverlayState, char_code: u32) {
    if state.editing_text_id.is_none() {
        return;
    }
    if char_code < 32 || char_code == 127 {
        return;
    }
    let Some(ch) = char::from_u32(char_code) else {
        return;
    };
    edit_selected_text(state, |text| text.push(ch));
}

fn edit_selected_text(state: &mut OverlayState, edit: impl FnOnce(&mut String)) {
    let Some(id) = state.editing_text_id else {
        return;
    };
    if let Some(annotation) = state
        .document
        .annotations
        .iter_mut()
        .find(|annotation| annotation.id == id)
    {
        match &mut annotation.kind {
            AnnotationKind::Text { text, .. } => edit(text),
            AnnotationKind::Tag { label, .. } => edit(label),
            _ => {}
        }
    }
}

fn handle_toolbar_action(state: &mut OverlayState, action: ToolbarAction) {
    match action {
        ToolbarAction::Grip => {}
        ToolbarAction::Tool(tool) => state.active_tool = tool,
        ToolbarAction::Undo => {
            if let Some(previous) = state.history.undo(&state.document) {
                state.document = previous;
            }
        }
        ToolbarAction::Copy => {
            let _ = unsafe { copy_capture_to_clipboard(state) };
            unsafe {
                let _ = DestroyWindow(state.hwnd);
            }
        }
        ToolbarAction::Save => {
            let _ = unsafe { save_capture_to_file(state) };
        }
        ToolbarAction::Cancel => unsafe {
            let _ = DestroyWindow(state.hwnd);
        },
    }
}

fn annotation_from_tool(
    state: &OverlayState,
    start: Point,
    end: Point,
    points: Vec<Point>,
) -> Annotation {
    let bounds = Rect::from_points(start, end);
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
        },
        ToolKind::Tag => AnnotationKind::Tag {
            label: String::new(),
            anchor: start,
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
            shape: HighlightShape::RoundedRectangle,
            opacity: state.highlighter_opacity,
        },
        ToolKind::Pen => AnnotationKind::Pen { points },
        ToolKind::Watermark => AnnotationKind::Watermark {
            text: "Screen Captn".to_string(),
            opacity: 0.35,
        },
    };
    Annotation::new(0, kind, bounds, stroke)
}

unsafe fn paint_overlay(state: &mut OverlayState) {
    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = windows::Win32::Graphics::Gdi::BeginPaint(state.hwnd, &mut ps);

    let buffer_dc = CreateCompatibleDC(hdc);
    let buffer_bitmap = CreateCompatibleBitmap(
        hdc,
        state.screen_bounds.width.round() as i32,
        state.screen_bounds.height.round() as i32,
    );
    let old_buffer = SelectObject(buffer_dc, buffer_bitmap);
    paint_overlay_surface(buffer_dc, state);
    let _ = BitBlt(
        hdc,
        0,
        0,
        state.screen_bounds.width.round() as i32,
        state.screen_bounds.height.round() as i32,
        buffer_dc,
        0,
        0,
        SRCCOPY,
    );
    let _ = SelectObject(buffer_dc, old_buffer);
    let _ = DeleteObject(buffer_bitmap);
    let _ = DeleteDC(buffer_dc);

    let _ = windows::Win32::Graphics::Gdi::EndPaint(state.hwnd, &ps);
}

unsafe fn paint_overlay_surface(hdc: HDC, state: &mut OverlayState) {
    paint_background(hdc, state);

    if let Some(region) = state.region_overlay() {
        draw_dim_outside_region(hdc, state.screen_bounds, region);
        let white = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00ffffff));
        FrameRect(hdc, &rect_to_rect(region), white);
        let _ = DeleteObject(white);
        draw_handles(hdc, region);
        draw_annotations(hdc, state);
        draw_selected_annotation(hdc, state);
        draw_drag_preview(hdc, state);
        draw_toolbar(hdc, state, region);
    } else if let Some(DragState::Selecting { start, current }) = state.drag {
        let region = Rect::from_points(start, current);
        let white = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00ffffff));
        FrameRect(hdc, &rect_to_rect(region), white);
        let _ = DeleteObject(white);
    } else if let Some(region) = state
        .hover_region
        .map(|region| region.translate(-state.screen_bounds.x, -state.screen_bounds.y))
    {
        draw_dim_outside_region(hdc, state.screen_bounds, region);
        let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0000c8ff));
        FrameRect(hdc, &rect_to_rect(region), brush);
        let _ = DeleteObject(brush);
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

unsafe fn draw_toolbar(hdc: HDC, state: &mut OverlayState, region: Rect) {
    state.toolbar_buttons.clear();
    let actions = [
        ToolbarAction::Grip,
        ToolbarAction::Tool(ToolKind::StepNumber),
        ToolbarAction::Tool(ToolKind::Rectangle),
        ToolbarAction::Tool(ToolKind::Oval),
        ToolbarAction::Tool(ToolKind::Line),
        ToolbarAction::Tool(ToolKind::Arrow),
        ToolbarAction::Tool(ToolKind::Pen),
        ToolbarAction::Tool(ToolKind::Text),
        ToolbarAction::Tool(ToolKind::Tag),
        ToolbarAction::Tool(ToolKind::Mosaic),
        ToolbarAction::Tool(ToolKind::Highlighter),
        ToolbarAction::Tool(ToolKind::Watermark),
        ToolbarAction::Undo,
        ToolbarAction::Copy,
        ToolbarAction::Save,
        ToolbarAction::Cancel,
    ];
    let width = TOOLBAR_GRIP_WIDTH + TOOLBAR_BUTTON * (actions.len() - 1) as f32 + 12.0;
    let total_height = TOOLBAR_HEIGHT + 8.0;
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

    let bg = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00f4f4f4));
    let active = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00ffdca8));
    let border = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00929292));
    let bar = Rect::new(x, y, width, total_height);
    FillRect(hdc, &rect_to_rect(bar), bg);
    FrameRect(hdc, &rect_to_rect(bar), border);
    let black = Color::BLACK;
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, crate::util::colorref(black));

    let mut x_cursor = x + 6.0;
    for action in actions {
        let button_width = if matches!(action, ToolbarAction::Grip) {
            TOOLBAR_GRIP_WIDTH
        } else {
            TOOLBAR_BUTTON
        };
        let rect = Rect::new(x_cursor, y + 4.0, button_width - 3.0, TOOLBAR_HEIGHT - 6.0);
        draw_toolbar_button(hdc, rect, action, state, bg, active, border);
        state
            .toolbar_buttons
            .push(ToolbarButton { rect, tool: action });
        x_cursor += button_width;
    }

    let _ = DeleteObject(bg);
    let _ = DeleteObject(active);
    let _ = DeleteObject(border);
}

fn default_toolbar_origin(state: &OverlayState, region: Rect) -> Point {
    let action_count = 1 + ToolKind::ALL.len() + 4;
    let width = TOOLBAR_GRIP_WIDTH + TOOLBAR_BUTTON * (action_count - 1) as f32 + 12.0;
    let height = TOOLBAR_HEIGHT + 8.0;
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

unsafe fn draw_toolbar_button(
    hdc: HDC,
    rect: Rect,
    action: ToolbarAction,
    state: &OverlayState,
    bg: HBRUSH,
    active: HBRUSH,
    border: HBRUSH,
) {
    let selected = matches!(action, ToolbarAction::Tool(tool) if tool == state.active_tool);
    FillRect(hdc, &rect_to_rect(rect), if selected { active } else { bg });
    FrameRect(hdc, &rect_to_rect(rect), border);
    let label = toolbar_label(action);
    draw_label(hdc, rect.x + 5.0, rect.y + 7.0, label);
}

fn toolbar_label(action: ToolbarAction) -> &'static str {
    match action {
        ToolbarAction::Grip => "::",
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

unsafe fn draw_label(hdc: HDC, x: f32, y: f32, label: &str) {
    SetBkMode(hdc, TRANSPARENT);
    let wide: Vec<u16> = label.encode_utf16().collect();
    let _ = TextOutW(hdc, x.round() as i32, y.round() as i32, &wide);
}

unsafe fn draw_annotations(hdc: HDC, state: &OverlayState) {
    for annotation in &state.document.annotations {
        draw_annotation(hdc, state, annotation);
    }
}

unsafe fn draw_selected_annotation(hdc: HDC, state: &OverlayState) {
    let Some(selected_id) = state.document.selected_annotation_id else {
        return;
    };
    let Some(annotation) = state
        .document
        .annotations
        .iter()
        .find(|annotation| annotation.id == selected_id)
    else {
        return;
    };
    let bounds = annotation
        .bounds
        .translate(-state.screen_bounds.x, -state.screen_bounds.y);
    let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0000c8ff));

    match &annotation.kind {
        AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => {
            for point in [
                state.screen_to_overlay(*start),
                state.screen_to_overlay(*end),
            ] {
                let handle = Rect::new(point.x - 5.0, point.y - 5.0, 10.0, 10.0);
                FillRect(hdc, &rect_to_rect(handle), brush);
            }
        }
        AnnotationKind::Pen { .. } => {}
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

    let annotation = annotation_from_tool(state, *start, *current, points.clone());
    draw_annotation(hdc, state, &annotation);
}

unsafe fn draw_annotation(hdc: HDC, state: &OverlayState, annotation: &Annotation) {
    let bounds = annotation
        .bounds
        .translate(-state.screen_bounds.x, -state.screen_bounds.y);
    let _pen = SelectedPen::new(hdc, annotation.stroke.width, annotation.stroke.color);
    match &annotation.kind {
        AnnotationKind::Rectangle => {
            let _brush = SelectedStockObject::null_brush(hdc);
            let _ = Rectangle(
                hdc,
                bounds.x as i32,
                bounds.y as i32,
                bounds.right() as i32,
                bounds.bottom() as i32,
            );
        }
        AnnotationKind::Oval => {
            let _brush = SelectedStockObject::null_brush(hdc);
            let _ = Ellipse(
                hdc,
                bounds.x as i32,
                bounds.y as i32,
                bounds.right() as i32,
                bounds.bottom() as i32,
            );
        }
        AnnotationKind::Line { start, end } => draw_line(
            hdc,
            state.screen_to_overlay(*start),
            state.screen_to_overlay(*end),
            false,
        ),
        AnnotationKind::Arrow { start, end } => draw_line(
            hdc,
            state.screen_to_overlay(*start),
            state.screen_to_overlay(*end),
            true,
        ),
        AnnotationKind::StepNumber { number } => {
            draw_step_number(hdc, bounds, *number);
        }
        AnnotationKind::Text { text, .. } | AnnotationKind::Watermark { text, .. } => {
            draw_label(hdc, bounds.x, bounds.y, text);
        }
        AnnotationKind::Tag { label, anchor } => {
            let _brush = SelectedStockObject::null_brush(hdc);
            let _ = Rectangle(
                hdc,
                bounds.x as i32,
                bounds.y as i32,
                bounds.right() as i32,
                bounds.bottom() as i32,
            );
            draw_line(
                hdc,
                state.screen_to_overlay(*anchor),
                Point::new(bounds.x, bounds.y),
                false,
            );
            draw_label(hdc, bounds.x + 5.0, bounds.y + 5.0, label);
        }
        AnnotationKind::Mosaic { .. } => {
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00808080));
            FillRect(hdc, &rect_to_rect(bounds), brush);
            let _ = DeleteObject(brush);
        }
        AnnotationKind::Highlighter { .. } => {
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0000d6ff));
            FillRect(hdc, &rect_to_rect(bounds), brush);
            let _ = DeleteObject(brush);
        }
        AnnotationKind::Pen { points } => {
            for pair in points.windows(2) {
                draw_line(
                    hdc,
                    state.screen_to_overlay(pair[0]),
                    state.screen_to_overlay(pair[1]),
                    false,
                );
            }
        }
    }
}

unsafe fn draw_line(hdc: HDC, start: Point, end: Point, arrow: bool) {
    let _ = MoveToEx(hdc, start.x as i32, start.y as i32, None);
    let _ = LineTo(hdc, end.x as i32, end.y as i32);
    if arrow {
        let angle = (end.y - start.y).atan2(end.x - start.x);
        let size = 14.0;
        for offset in [0.55_f32, -0.55_f32] {
            let head = Point::new(
                end.x - size * (angle + offset).cos(),
                end.y - size * (angle + offset).sin(),
            );
            let _ = MoveToEx(hdc, end.x as i32, end.y as i32, None);
            let _ = LineTo(hdc, head.x as i32, head.y as i32);
        }
    }
}

unsafe fn draw_step_number(hdc: HDC, bounds: Rect, number: u32) {
    let size = 24.0_f32;
    let center_x = bounds.center().x;
    let y = bounds.y - size - 4.0;
    let marker = Rect::new(center_x - size / 2.0, y, size, size);
    let _brush = SelectedStockObject::null_brush(hdc);
    let _ = Ellipse(
        hdc,
        marker.x as i32,
        marker.y as i32,
        marker.right() as i32,
        marker.bottom() as i32,
    );
    draw_label(hdc, marker.x + 8.0, marker.y + 5.0, &number.to_string());
}

unsafe fn copy_capture_to_clipboard(state: &OverlayState) -> Result<()> {
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
    let Some(bitmap) = render_capture_bitmap(state) else {
        return Ok(());
    };
    if let Some(path) = default_save_path() {
        let _ = write_bitmap_file(
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

    let _ = DeleteDC(source_dc);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    Some(bitmap)
}

unsafe fn write_bitmap_file(bitmap: HBITMAP, region: Rect, path: &PathBuf) -> std::io::Result<()> {
    let width = region.width.round().max(1.0) as i32;
    let height = region.height.round().max(1.0) as i32;
    let stride = width as usize * 4;
    let image_size = stride * height as usize;
    let mut pixels = vec![0_u8; image_size];
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
        Some(pixels.as_mut_ptr().cast()),
        &mut info,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, hdc);
    if lines == 0 {
        return Ok(());
    }

    let mut file = File::create(path)?;
    let pixel_offset = 14_u32 + std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    let file_size = pixel_offset + image_size as u32;
    file.write_all(b"BM")?;
    file.write_all(&file_size.to_le_bytes())?;
    file.write_all(&0_u16.to_le_bytes())?;
    file.write_all(&0_u16.to_le_bytes())?;
    file.write_all(&pixel_offset.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biSize.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biWidth.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biHeight.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biPlanes.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biBitCount.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biCompression.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biSizeImage.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biXPelsPerMeter.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biYPelsPerMeter.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biClrUsed.to_le_bytes())?;
    file.write_all(&info.bmiHeader.biClrImportant.to_le_bytes())?;
    file.write_all(&pixels)?;
    Ok(())
}

fn default_save_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    let dir = PathBuf::from(home).join("Pictures").join("ScreenCaptn");
    let _ = fs::create_dir_all(&dir);
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(dir.join(format!("screencaptn-{seconds}.bmp")))
}

unsafe fn draw_export_annotations(hdc: HDC, state: &OverlayState, region: Rect) {
    for annotation in &state.document.annotations {
        let translated = annotation.bounds.translate(-region.x, -region.y);
        let _pen = SelectedPen::new(hdc, annotation.stroke.width, annotation.stroke.color);
        match &annotation.kind {
            AnnotationKind::Rectangle => {
                let _brush = SelectedStockObject::null_brush(hdc);
                let _ = Rectangle(
                    hdc,
                    translated.x as i32,
                    translated.y as i32,
                    translated.right() as i32,
                    translated.bottom() as i32,
                );
            }
            AnnotationKind::Oval => {
                let _brush = SelectedStockObject::null_brush(hdc);
                let _ = Ellipse(
                    hdc,
                    translated.x as i32,
                    translated.y as i32,
                    translated.right() as i32,
                    translated.bottom() as i32,
                );
            }
            AnnotationKind::Line { start, end } => draw_line(
                hdc,
                start.translate(-region.x, -region.y),
                end.translate(-region.x, -region.y),
                false,
            ),
            AnnotationKind::Arrow { start, end } => draw_line(
                hdc,
                start.translate(-region.x, -region.y),
                end.translate(-region.x, -region.y),
                true,
            ),
            AnnotationKind::StepNumber { number } => {
                draw_step_number(hdc, translated, *number);
            }
            AnnotationKind::Text { text, .. } | AnnotationKind::Watermark { text, .. } => {
                draw_label(hdc, translated.x, translated.y, text);
            }
            AnnotationKind::Tag { label, anchor } => {
                let _brush = SelectedStockObject::null_brush(hdc);
                let _ = Rectangle(
                    hdc,
                    translated.x as i32,
                    translated.y as i32,
                    translated.right() as i32,
                    translated.bottom() as i32,
                );
                draw_line(
                    hdc,
                    anchor.translate(-region.x, -region.y),
                    Point::new(translated.x, translated.y),
                    false,
                );
                draw_label(hdc, translated.x + 5.0, translated.y + 5.0, label);
            }
            AnnotationKind::Mosaic { .. } => {
                let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00808080));
                FillRect(hdc, &rect_to_rect(translated), brush);
                let _ = DeleteObject(brush);
            }
            AnnotationKind::Highlighter { .. } => {
                let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0000d6ff));
                FillRect(hdc, &rect_to_rect(translated), brush);
                let _ = DeleteObject(brush);
            }
            AnnotationKind::Pen { points } => {
                for pair in points.windows(2) {
                    draw_line(
                        hdc,
                        pair[0].translate(-region.x, -region.y),
                        pair[1].translate(-region.x, -region.y),
                        false,
                    );
                }
            }
        }
    }
}
