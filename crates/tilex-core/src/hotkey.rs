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
                        virtual_key(part).ok_or_else(|| ParseError::UnknownKey(part.to_string()))?,
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

    #[test]
    fn rejects_broken_input() {
        assert_eq!("".parse::<Binding>(), Err(ParseError::Empty));
        assert_eq!("Win+Shift".parse::<Binding>(), Err(ParseError::NoKey));
        assert!(matches!("Win+Nope".parse::<Binding>(), Err(ParseError::UnknownKey(_))));
    }
}
