use screencaptn_core::Rect;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub hotkey: HotkeySettings,
    pub auto_save: AutoSaveSettings,
    pub update_check: UpdateCheckSettings,
    pub aspect_ratio: AspectRatioMode,
    pub locked_regions: Vec<LockedRegion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HotkeySettings {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub key_code: u32,
    pub key_label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AutoSaveSettings {
    pub enabled: bool,
    pub folder: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateCheckSettings {
    pub last_successful_check_unix_seconds: Option<i64>,
    pub retry_after_unix_seconds: Option<i64>,
    pub pending: Option<PendingUpdate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub version: String,
    pub release_notes: Option<ReleaseNotes>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotes {
    pub version: String,
    pub title: String,
    pub highlights: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedRegion {
    pub monitor_id: String,
    pub rect: RectDto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AspectRatioMode {
    Custom,
    Ratio9x16,
    Ratio16x9,
    Ratio1x1,
    Ratio4x5,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectDto {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeySettings::default(),
            auto_save: AutoSaveSettings::default(),
            update_check: UpdateCheckSettings::default(),
            aspect_ratio: AspectRatioMode::Custom,
            locked_regions: Vec::new(),
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            win: false,
            key_code: b'A' as u32,
            key_label: "A".to_string(),
        }
    }
}

impl Default for AutoSaveSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            folder: default_auto_save_folder(),
        }
    }
}

impl Default for AspectRatioMode {
    fn default() -> Self {
        Self::Custom
    }
}

impl AspectRatioMode {
    pub fn next(self) -> Self {
        match self {
            Self::Custom => Self::Ratio9x16,
            Self::Ratio9x16 => Self::Ratio16x9,
            Self::Ratio16x9 => Self::Ratio1x1,
            Self::Ratio1x1 => Self::Ratio4x5,
            Self::Ratio4x5 => Self::Custom,
        }
    }

    pub fn value(self) -> Option<f32> {
        match self {
            Self::Custom => None,
            Self::Ratio9x16 => Some(9.0 / 16.0),
            Self::Ratio16x9 => Some(16.0 / 9.0),
            Self::Ratio1x1 => Some(1.0),
            Self::Ratio4x5 => Some(4.0 / 5.0),
        }
    }
}

impl HotkeySettings {
    pub fn is_valid(&self) -> bool {
        self.key_code != 0 && (self.ctrl || self.shift || self.alt || self.win)
    }

    pub fn display_label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.win {
            parts.push("Win".to_string());
        }
        parts.push(self.key_label.clone());
        parts.join("+")
    }
}

impl AppSettings {
    pub fn locked_region_for_monitor(&self, monitor_id: &str, monitor: Rect) -> Option<Rect> {
        self.locked_regions
            .iter()
            .find(|region| region.monitor_id == monitor_id)
            .map(|region| region.rect.to_rect().translate(monitor.x, monitor.y))
            .and_then(|region| clamp_region_to_monitor(region, monitor))
    }

    pub fn is_region_locked(&self, monitor_id: &str) -> bool {
        self.locked_regions
            .iter()
            .any(|region| region.monitor_id == monitor_id)
    }

    pub fn set_locked_region(&mut self, monitor_id: String, monitor: Rect, region: Rect) {
        let relative = region.translate(-monitor.x, -monitor.y);
        self.locked_regions
            .retain(|existing| existing.monitor_id != monitor_id);
        self.locked_regions.push(LockedRegion {
            monitor_id,
            rect: RectDto::from_rect(relative),
        });
    }

    pub fn remove_locked_region(&mut self, monitor_id: &str) {
        self.locked_regions
            .retain(|existing| existing.monitor_id != monitor_id);
    }
}

impl RectDto {
    pub fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }

    pub fn to_rect(self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

pub fn load_settings() -> AppSettings {
    let Ok(path) = settings_path() else {
        return AppSettings::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<AppSettings>(&contents).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) -> io::Result<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(path, json)
}

pub fn default_auto_save_folder() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("Pictures")
        .join("Screen Cap'n")
}

fn settings_path() -> io::Result<PathBuf> {
    let base = std::env::var_os("APPDATA").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "APPDATA is unavailable; settings cannot be persisted",
        )
    })?;
    Ok(PathBuf::from(base)
        .join("ScreenCaptn")
        .join("settings.json"))
}

fn clamp_region_to_monitor(region: Rect, monitor: Rect) -> Option<Rect> {
    if !region.is_visible() || !monitor.is_visible() {
        return None;
    }
    let width = region.width.min(monitor.width);
    let height = region.height.min(monitor.height);
    let x = region
        .x
        .max(monitor.x)
        .min((monitor.right() - width).max(monitor.x));
    let y = region
        .y
        .max(monitor.y)
        .min((monitor.bottom() - height).max(monitor.y));
    Some(Rect::new(x, y, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_region_is_clamped_after_monitor_shrinks() {
        let monitor = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let mut settings = AppSettings::default();
        settings.locked_regions.push(LockedRegion {
            monitor_id: "display".to_string(),
            rect: RectDto {
                x: 900.0,
                y: 500.0,
                width: 900.0,
                height: 500.0,
            },
        });

        assert_eq!(
            settings.locked_region_for_monitor("display", monitor),
            Some(Rect::new(380.0, 220.0, 900.0, 500.0))
        );
    }

    #[test]
    fn oversized_locked_region_is_reduced_to_monitor() {
        let monitor = Rect::new(-1280.0, 0.0, 1280.0, 720.0);
        let region = clamp_region_to_monitor(Rect::new(-1500.0, -100.0, 2000.0, 1000.0), monitor);

        assert_eq!(region, Some(monitor));
    }

    #[test]
    fn update_settings_default_without_breaking_existing_saved_settings() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"hotkey": {"ctrl": true, "shift": false, "alt": false, "win": false, "keyCode": 65, "keyLabel": "A"}}"#,
        )
        .unwrap();
        assert!(settings.update_check.pending.is_none());
        assert!(settings
            .update_check
            .last_successful_check_unix_seconds
            .is_none());
    }
}
