use crate::overlay::AppTheme;
use crate::settings::PendingUpdate;
use crate::theme::toolbar_palette;
use crate::util::colorref;
use screencaptn_core::{Color, Point, Rect};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC,
    PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDC_ARROW, MSG, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE,
    WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW, WS_CAPTION,
    WS_OVERLAPPED, WS_SYSMENU,
};

const CLASS_NAME: PCWSTR = w!("ScreenCaptnUpdateWindow");
const WINDOW_WIDTH: i32 = 520;
const WINDOW_HEIGHT: i32 = 350;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateDialogAction {
    UpdateNow,
    Later,
    MoreDetails,
}

struct UpdateWindowState {
    theme: AppTheme,
    pending: PendingUpdate,
    result: Option<UpdateDialogAction>,
    update_rect: Rect,
    later_rect: Rect,
    details_rect: Option<Rect>,
}

pub fn show_update_dialog(
    owner: HWND,
    theme: AppTheme,
    pending: PendingUpdate,
) -> Result<Option<UpdateDialogAction>> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(update_wnd_proc),
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
        let details_rect = pending
            .release_notes
            .as_ref()
            .map(|_| Rect::new(40.0, 274.0, 124.0, 40.0));
        let mut state = Box::new(UpdateWindowState {
            theme,
            pending,
            result: None,
            update_rect: Rect::new(354.0, 274.0, 126.0, 40.0),
            later_rect: Rect::new(236.0, 274.0, 102.0, 40.0),
            details_rect,
        });
        let state_ptr = state.as_mut() as *mut UpdateWindowState;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("Screen Cap'n Update"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            owner,
            HMENU::default(),
            instance,
            Some(state_ptr.cast()),
        )?;
        Box::leak(state);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
            if !IsWindow(hwnd).as_bool() {
                break;
            }
            if update_state(hwnd).is_some_and(|state| state.result.is_some()) {
                break;
            }
        }

        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UpdateWindowState;
        if state_ptr.is_null() {
            return Ok(None);
        }
        let mut state = Box::from_raw(state_ptr);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let result = state.result.take();
        if IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        Ok(result)
    }
}

unsafe extern "system" fn update_wnd_proc(
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
            if let Some(state) = update_state(hwnd) {
                draw_update_window(hdc, state);
            }
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(state) = update_state(hwnd) {
                let point = point_from_lparam(lparam);
                state.result = if state.update_rect.contains(point) {
                    Some(UpdateDialogAction::UpdateNow)
                } else if state.later_rect.contains(point) {
                    Some(UpdateDialogAction::Later)
                } else if state.details_rect.is_some_and(|rect| rect.contains(point)) {
                    Some(UpdateDialogAction::MoreDetails)
                } else {
                    None
                };
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(state) = update_state(hwnd) {
                match wparam.0 as u32 {
                    0x1B => state.result = Some(UpdateDialogAction::Later),
                    0x0D => state.result = Some(UpdateDialogAction::UpdateNow),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(state) = update_state(hwnd) {
                state.result = Some(UpdateDialogAction::Later);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn update_state(hwnd: HWND) -> Option<&'static mut UpdateWindowState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UpdateWindowState;
    (!ptr.is_null()).then(|| &mut *ptr)
}

unsafe fn draw_update_window(hdc: HDC, state: &UpdateWindowState) {
    let palette = toolbar_palette(state.theme);
    fill(
        hdc,
        Rect::new(0.0, 0.0, WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32),
        palette.background,
    );
    draw_text(
        hdc,
        Rect::new(40.0, 32.0, 440.0, 38.0),
        "A Screen Cap'n update is ready",
        24,
        palette.icon,
        true,
        DT_LEFT | DT_SINGLELINE,
    );
    draw_text(
        hdc,
        Rect::new(40.0, 76.0, 440.0, 24.0),
        &format!("Version {} is ready for launch.", state.pending.version),
        14,
        muted_color(state.theme),
        false,
        DT_LEFT | DT_SINGLELINE,
    );

    if let Some(notes) = &state.pending.release_notes {
        draw_text(
            hdc,
            Rect::new(40.0, 118.0, 440.0, 32.0),
            &notes.title,
            16,
            palette.icon,
            true,
            DT_LEFT | DT_SINGLELINE,
        );
        let mut y = 158.0;
        for highlight in &notes.highlights {
            draw_text(
                hdc,
                Rect::new(54.0, y, 416.0, 28.0),
                &format!("- {highlight}"),
                13,
                palette.icon,
                false,
                DT_LEFT | DT_WORDBREAK,
            );
            y += 31.0;
        }
    } else {
        draw_text(
            hdc,
            Rect::new(40.0, 126.0, 420.0, 48.0),
            "A fresh set of improvements is ready. Update now to bring the latest Screen Cap'n aboard.",
            14,
            palette.icon,
            false,
            DT_LEFT | DT_WORDBREAK,
        );
    }

    if let Some(rect) = state.details_rect {
        draw_button(
            hdc,
            rect,
            "More details",
            palette.selected_icon_background,
            palette.icon,
        );
    }
    draw_button(
        hdc,
        state.later_rect,
        "Later",
        palette.selected_icon_background,
        palette.icon,
    );
    draw_button(
        hdc,
        state.update_rect,
        "Update now",
        palette.accent,
        Color::WHITE,
    );
}

unsafe fn draw_button(hdc: HDC, rect: Rect, label: &str, background: Color, foreground: Color) {
    fill(hdc, rect, background);
    draw_text(
        hdc,
        rect,
        label,
        14,
        foreground,
        true,
        windows::Win32::Graphics::Gdi::DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe fn fill(hdc: HDC, rect: Rect, color: Color) {
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

unsafe fn draw_text(
    hdc: HDC,
    rect: Rect,
    text: &str,
    size: i32,
    color: Color,
    bold: bool,
    flags: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let font = CreateFontW(
        -size,
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
    let _ = DrawTextW(hdc, &mut wide, &mut native, flags);
    let _ = SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

fn muted_color(theme: AppTheme) -> Color {
    match theme {
        AppTheme::Light => Color::rgb(0x78, 0x78, 0x78),
        AppTheme::Dark => Color::rgb(0xa6, 0xa6, 0xa6),
    }
}

fn point_from_lparam(lparam: LPARAM) -> Point {
    let packed = lparam.0 as usize;
    Point::new(
        (packed & 0xffff) as i16 as f32,
        ((packed >> 16) & 0xffff) as i16 as f32,
    )
}
