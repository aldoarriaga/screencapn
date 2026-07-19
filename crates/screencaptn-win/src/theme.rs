use screencaptn_core::Color;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppTheme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct ToolbarPalette {
    pub background: Color,
    pub icon: Color,
    pub icon_background: Color,
    pub selected_icon_background: Color,
    pub border_top: Color,
    pub border_bottom: Color,
    pub divider: Color,
    pub accent: Color,
}

pub fn toolbar_palette(theme: AppTheme) -> ToolbarPalette {
    match theme {
        AppTheme::Light => ToolbarPalette {
            background: Color::rgb(0xf2, 0xf2, 0xf2),
            icon: Color::rgb(0x4d, 0x4d, 0x4d),
            icon_background: Color::rgb(0xf2, 0xf2, 0xf2),
            selected_icon_background: Color::WHITE,
            border_top: Color::WHITE,
            border_bottom: Color::rgb(0xd4, 0xd4, 0xd4),
            divider: Color::rgb(0xb8, 0xb8, 0xb8),
            accent: Color::rgb(0xff, 0x3b, 0x30),
        },
        AppTheme::Dark => ToolbarPalette {
            background: Color::rgb(0x1a, 0x1a, 0x1a),
            icon: Color::rgb(0xb3, 0xb3, 0xb3),
            icon_background: Color::rgb(0x1a, 0x1a, 0x1a),
            selected_icon_background: Color::rgb(0x33, 0x33, 0x33),
            border_top: Color::rgb(0x36, 0x36, 0x36),
            border_bottom: Color::BLACK,
            divider: Color::rgb(0x66, 0x66, 0x66),
            accent: Color::rgb(0xff, 0x3b, 0x30),
        },
    }
}

fn theme_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("ScreenCaptn").join("theme.txt"))
}

pub fn load_theme() -> AppTheme {
    let Some(path) = theme_path() else {
        return AppTheme::Light;
    };
    match fs::read_to_string(path).ok().as_deref().map(str::trim) {
        Some("dark") => AppTheme::Dark,
        _ => AppTheme::Light,
    }
}

pub fn save_theme(theme: AppTheme) {
    let Some(path) = theme_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let value = match theme {
        AppTheme::Light => "light",
        AppTheme::Dark => "dark",
    };
    let _ = fs::write(path, value);
}

pub fn toggled_theme(theme: AppTheme) -> AppTheme {
    match theme {
        AppTheme::Light => AppTheme::Dark,
        AppTheme::Dark => AppTheme::Light,
    }
}
