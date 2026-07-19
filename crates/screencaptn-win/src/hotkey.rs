use crate::settings::HotkeySettings;

const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_TAB: u32 = 0x09;
const VK_DELETE: u32 = 0x2E;
const VK_SNAPSHOT: u32 = 0x2C;
const VK_F4: u32 = 0x73;
const VK_F10: u32 = 0x79;

pub fn reserved_hotkey_reason(hotkey: &HotkeySettings) -> Option<&'static str> {
    if hotkey.win {
        return Some("Windows-key shortcuts are reserved by Windows.");
    }

    if hotkey.key_code == VK_SNAPSHOT {
        return Some("Print Screen shortcuts are reserved by Windows screenshot tools.");
    }

    if hotkey.ctrl && hotkey.alt && hotkey.key_code == VK_DELETE {
        return Some("Ctrl+Alt+Delete is reserved by Windows.");
    }

    if hotkey.ctrl && hotkey.shift && hotkey.key_code == VK_ESCAPE {
        return Some("Ctrl+Shift+Esc is reserved by Windows.");
    }

    if hotkey.ctrl && hotkey.key_code == VK_ESCAPE {
        return Some("Ctrl+Esc is reserved by Windows.");
    }

    if hotkey.alt {
        return match hotkey.key_code {
            VK_TAB => Some("Alt+Tab is reserved by Windows."),
            VK_ESCAPE => Some("Alt+Esc is reserved by Windows."),
            VK_F4 => Some("Alt+F4 is reserved by Windows."),
            VK_SPACE => Some("Alt+Space is reserved by Windows."),
            _ => None,
        };
    }

    if hotkey.shift && hotkey.key_code == VK_F10 {
        return Some("Shift+F10 is reserved by Windows.");
    }

    None
}
