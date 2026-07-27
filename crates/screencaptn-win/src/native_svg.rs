use screencaptn_core::{Color, Rect};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HDC,
};

pub fn recolor_svg(svg: &str, color: Color) -> String {
    let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    svg.replace("#4d4d4d", &hex)
        .replace("#4D4D4D", &hex)
        .replace("#b3b3b3", &hex)
        .replace("#B3B3B3", &hex)
        .replace("currentColor", &hex)
}

pub unsafe fn draw_svg(hdc: HDC, svg: &str, rect: Rect) -> std::result::Result<(), ()> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &options).map_err(|_| ())?;
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
    for (source, destination) in pixmap.data().chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        destination[0] = source[2];
        destination[1] = source[1];
        destination[2] = source[0];
        destination[3] = source[3];
    }

    alpha_blend_bgra(hdc, rect, width, height, &bgra)
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
            biSizeImage: width * height * 4,
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

    let memory_dc = CreateCompatibleDC(hdc);
    let previous = SelectObject(memory_dc, bitmap);
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
        memory_dc,
        0,
        0,
        width as i32,
        height as i32,
        blend,
    );
    let _ = SelectObject(memory_dc, previous);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(memory_dc);
    Ok(())
}
