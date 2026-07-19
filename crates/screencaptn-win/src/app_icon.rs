use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

pub unsafe fn load_app_icon(size: u32) -> Option<HICON> {
    let svg = include_str!("../assets/app-icon/screencapn-icon.svg");
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        size as f32 / tree_size.width(),
        size as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: size * size * 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    if bits.is_null() {
        let _ = DeleteObject(color);
        return None;
    }

    let mut bgra = vec![0_u8; (size * size * 4) as usize];
    for (source, dest) in pixmap.data().chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        dest[0] = source[2];
        dest[1] = source[1];
        dest[2] = source[0];
        dest[3] = source[3];
    }
    std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len());

    let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
    if mask.0.is_null() {
        let _ = DeleteObject(color);
        return None;
    }
    let icon = CreateIconIndirect(&ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    })
    .ok();
    let _ = DeleteObject(mask);
    let _ = DeleteObject(color);
    icon
}
