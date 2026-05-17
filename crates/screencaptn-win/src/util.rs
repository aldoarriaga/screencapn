use screencaptn_core::{Color, Point, Rect};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreatePen, DeleteObject, GetStockObject, SelectObject, GET_STOCK_OBJECT_FLAGS, HDC, HGDIOBJ,
    HPEN, NULL_BRUSH, PS_SOLID,
};

pub fn loword(value: usize) -> u16 {
    (value & 0xffff) as u16
}

pub fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xffff) as u16
}

pub fn point_from_lparam(lparam: windows::Win32::Foundation::LPARAM) -> Point {
    let packed = lparam.0 as usize;
    let x = loword(packed) as i16 as f32;
    let y = hiword(packed) as i16 as f32;
    Point::new(x, y)
}

pub fn rect_to_rect(rect: Rect) -> RECT {
    RECT {
        left: rect.x.round() as i32,
        top: rect.y.round() as i32,
        right: rect.right().round() as i32,
        bottom: rect.bottom().round() as i32,
    }
}

pub fn colorref(color: Color) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(
        color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16),
    )
}

pub struct SelectedPen {
    hdc: HDC,
    pen: HPEN,
    old: HGDIOBJ,
}

impl SelectedPen {
    pub unsafe fn new(hdc: HDC, width: f32, color: Color) -> Self {
        let pen = CreatePen(PS_SOLID, width.max(1.0).round() as i32, colorref(color));
        let old = SelectObject(hdc, pen);
        Self { hdc, pen, old }
    }
}

impl Drop for SelectedPen {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.old);
            let _ = DeleteObject(self.pen);
        }
    }
}

pub struct SelectedStockObject {
    hdc: HDC,
    old: HGDIOBJ,
}

impl SelectedStockObject {
    pub unsafe fn null_brush(hdc: HDC) -> Self {
        Self::new(hdc, NULL_BRUSH)
    }

    unsafe fn new(hdc: HDC, stock_object: GET_STOCK_OBJECT_FLAGS) -> Self {
        let object = GetStockObject(stock_object);
        let old = SelectObject(hdc, object);
        Self { hdc, old }
    }
}

impl Drop for SelectedStockObject {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.old);
        }
    }
}
