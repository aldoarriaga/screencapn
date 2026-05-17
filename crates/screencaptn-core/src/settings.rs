#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub hotkey: Hotkey,
    pub confirm_copies_to_clipboard: bool,
    pub portable_mode: bool,
    pub launch_at_startup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Hotkey::default(),
            confirm_copies_to_clipboard: true,
            portable_mode: false,
            launch_at_startup: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: char,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            key: 'A',
        }
    }
}
