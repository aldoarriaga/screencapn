use crate::overlay::AppTheme;
use std::fs;
use std::path::PathBuf;

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
