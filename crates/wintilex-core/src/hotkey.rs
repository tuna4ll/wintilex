//! Hotkey bindings, stored and displayed as `"Win+Shift+H"`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub fn is_empty(&self) -> bool {
        !(self.alt || self.control || self.shift || self.win)
    }
}

/// A modifier combination plus one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Binding {
    pub modifiers: Modifiers,
    /// Win32 virtual key code.
    pub key: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("binding is empty")]
    Empty,
    #[error("unknown key `{0}`")]
    UnknownKey(String),
    #[error("binding has no key, only modifiers")]
    NoKey,
}

impl Binding {
    pub fn new(modifiers: Modifiers, key: u32) -> Self {
        Self { modifiers, key }
    }
}

impl FromStr for Binding {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(ParseError::Empty);
        }

        let mut modifiers = Modifiers::default();
        let mut key = None;

        for part in value.split('+') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match part.to_ascii_lowercase().as_str() {
                "alt" | "menu" => modifiers.alt = true,
                "ctrl" | "control" => modifiers.control = true,
                "shift" => modifiers.shift = true,
                "win" | "super" | "meta" | "cmd" => modifiers.win = true,
                _ => {
                    key = Some(
                        virtual_key(part)
                            .ok_or_else(|| ParseError::UnknownKey(part.to_string()))?,
                    )
                }
            }
        }

        match key {
            Some(key) => Ok(Binding { modifiers, key }),
            None => Err(ParseError::NoKey),
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::with_capacity(5);
        if self.modifiers.win {
            parts.push("Win");
        }
        if self.modifiers.control {
            parts.push("Ctrl");
        }
        if self.modifiers.alt {
            parts.push("Alt");
        }
        if self.modifiers.shift {
            parts.push("Shift");
        }
        let name = key_name(self.key);
        parts.push(&name);
        f.write_str(&parts.join("+"))
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Shortcuts that belong to Windows and are worth more than a tiling binding.
///
/// The keyboard hook sees keys before the shell does, so binding one of these
/// would silently take it away: no more task view, no more virtual desktops, no
/// more switching keyboard layout. WinTilex leaves them alone unless the user
/// turns that protection off.
///
/// The directional bindings use the arrow keys rather than `hjkl` precisely so
/// that this list can include `Win+L`: losing the lock screen is not a trade
/// worth making for one focus key.
const RESERVED: &[(Modifiers, u32, &str)] = &[
    (WIN, b'L' as u32, "locking the screen"),
    (WIN, VK_TAB, "task view"),
    (WIN, VK_SPACE, "switching keyboard layout"),
    (WIN_SHIFT, VK_SPACE, "switching keyboard layout"),
    (WIN, b'D' as u32, "show desktop"),
    (WIN, b'G' as u32, "game bar"),
    (WIN, VK_PRINT_SCREEN, "screenshots"),
    (WIN_SHIFT, b'S' as u32, "the snipping tool"),
    (WIN_CTRL, VK_LEFT, "virtual desktops"),
    (WIN_CTRL, VK_RIGHT, "virtual desktops"),
    (WIN_CTRL, b'D' as u32, "virtual desktops"),
    (WIN_CTRL, VK_F4, "virtual desktops"),
    (ALT, VK_TAB, "the window switcher"),
    (ALT_SHIFT, VK_TAB, "the window switcher"),
];

const WIN: Modifiers = Modifiers { alt: false, control: false, shift: false, win: true };
const WIN_SHIFT: Modifiers = Modifiers { alt: false, control: false, shift: true, win: true };
const WIN_CTRL: Modifiers = Modifiers { alt: false, control: true, shift: false, win: true };
const ALT: Modifiers = Modifiers { alt: true, control: false, shift: false, win: false };
const ALT_SHIFT: Modifiers = Modifiers { alt: true, control: false, shift: true, win: false };

const VK_TAB: u32 = 0x09;
const VK_SPACE: u32 = 0x20;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_PRINT_SCREEN: u32 = 0x2C;
const VK_F4: u32 = 0x73;

/// What Windows would lose if WinTilex took this binding, if anything.
pub fn system_reserved(binding: &Binding) -> Option<&'static str> {
    RESERVED
        .iter()
        .find(|(modifiers, key, _)| *modifiers == binding.modifiers && *key == binding.key)
        .map(|(_, _, feature)| *feature)
}

/// Map a key name to its virtual key code.
pub fn virtual_key(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    if lower.len() == 1 {
        let ch = lower.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_uppercase() as u32);
        }
        if ch.is_ascii_digit() {
            return Some(ch as u32);
        }
    }
    if let Some(number) = lower.strip_prefix('f') {
        if let Ok(index) = number.parse::<u32>() {
            if (1..=24).contains(&index) {
                return Some(0x70 + index - 1);
            }
        }
    }
    Some(match lower.as_str() {
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => 0x1B,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "comma" | "," => 0xBC,
        "period" | "." => 0xBE,
        "slash" | "/" => 0xBF,
        "semicolon" | ";" => 0xBA,
        "quote" | "'" => 0xDE,
        "backtick" | "`" => 0xC0,
        "minus" | "-" => 0xBD,
        "equal" | "=" => 0xBB,
        "leftbracket" | "[" => 0xDB,
        "rightbracket" | "]" => 0xDD,
        "backslash" | "\\" => 0xDC,
        _ => return None,
    })
}

/// Inverse of [`virtual_key`], for round-tripping through the config file.
pub fn key_name(key: u32) -> String {
    match key {
        0x30..=0x39 => (key as u8 as char).to_string(),
        0x41..=0x5A => (key as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", key - 0x70 + 1),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        0x1B => "Escape".into(),
        0x08 => "Backspace".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0xBC => "Comma".into(),
        0xBE => "Period".into(),
        0xBF => "Slash".into(),
        0xBA => "Semicolon".into(),
        0xDE => "Quote".into(),
        0xC0 => "Backtick".into(),
        0xBD => "Minus".into(),
        0xBB => "Equal".into(),
        0xDB => "LeftBracket".into(),
        0xDD => "RightBracket".into(),
        0xDC => "Backslash".into(),
        other => format!("0x{other:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_binding() {
        let binding: Binding = "Win+Shift+H".parse().unwrap();
        assert!(binding.modifiers.win);
        assert!(binding.modifiers.shift);
        assert!(!binding.modifiers.control);
        assert_eq!(binding.key, 'H' as u32);
    }

    #[test]
    fn parsing_is_case_and_alias_insensitive() {
        let a: Binding = "win+ctrl+l".parse().unwrap();
        let b: Binding = "Super + Control + L".parse().unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn round_trips_through_display() {
        for text in ["Win+H", "Win+Ctrl+Alt+Shift+F5", "Win+Left", "Alt+Comma"] {
            let binding: Binding = text.parse().unwrap();
            assert_eq!(binding.to_string().parse::<Binding>().unwrap(), binding);
        }
    }

    #[test]
    fn display_uses_a_stable_modifier_order() {
        let binding: Binding = "shift+alt+ctrl+win+k".parse().unwrap();
        assert_eq!(binding.to_string(), "Win+Ctrl+Alt+Shift+K");
    }

    fn reserved(text: &str) -> Option<&'static str> {
        system_reserved(&text.parse::<Binding>().unwrap())
    }

    #[test]
    fn the_shortcuts_windows_needs_are_protected() {
        assert!(reserved("Win+L").is_some());
        assert!(reserved("Win+Tab").is_some());
        assert!(reserved("Win+Ctrl+Left").is_some());
        assert!(reserved("Win+Ctrl+Right").is_some());
        assert!(reserved("Win+Space").is_some());
        assert!(reserved("Alt+Tab").is_some());
    }

    #[test]
    fn the_directional_keys_stay_available() {
        // Aero Snap is what WinTilex replaces, so the plain arrows are fair game.
        for text in ["Win+Left", "Win+Down", "Win+Up", "Win+Right"] {
            assert_eq!(reserved(text), None, "{text} must stay bindable");
        }
        for text in ["Win+Shift+Left", "Win+Alt+Left", "Win+Alt+Shift+Left", "Win+Alt+Space"] {
            assert_eq!(reserved(text), None, "{text} must stay bindable");
        }
    }

    #[test]
    fn protection_is_exact_about_modifiers() {
        // Win+Ctrl+Left switches virtual desktop; the other two are nothing.
        assert!(reserved("Win+Ctrl+Left").is_some());
        assert_eq!(reserved("Win+Ctrl+Shift+Left"), None);
        assert_eq!(reserved("Win+Alt+Left"), None);

        // Only the bare Win+L locks; a modifier on top does not.
        assert!(reserved("Win+L").is_some());
        assert_eq!(reserved("Win+Shift+L"), None);
        assert_eq!(reserved("Win+Ctrl+L"), None);
    }

    #[test]
    fn rejects_broken_input() {
        assert_eq!("".parse::<Binding>(), Err(ParseError::Empty));
        assert_eq!("Win+Shift".parse::<Binding>(), Err(ParseError::NoKey));
        assert!(matches!("Win+Nope".parse::<Binding>(), Err(ParseError::UnknownKey(_))));
    }
}
