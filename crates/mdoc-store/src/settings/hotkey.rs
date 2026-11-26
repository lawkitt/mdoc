//! The companion's system-wide hotkey as Settings holds it (ADR 0128):
//! modifiers and a key named by its place on the keyboard (the W3C `code`
//! name), which is what the OS registers, whatever the layout.
use serde::{Deserialize, Serialize};

/// The keys a hotkey may use, by `code`, and their US layout labels.
const KEYS: &[(&str, &str)] = &[
    ("KeyA", "A"),
    ("KeyB", "B"),
    ("KeyC", "C"),
    ("KeyD", "D"),
    ("KeyE", "E"),
    ("KeyF", "F"),
    ("KeyG", "G"),
    ("KeyH", "H"),
    ("KeyI", "I"),
    ("KeyJ", "J"),
    ("KeyK", "K"),
    ("KeyL", "L"),
    ("KeyM", "M"),
    ("KeyN", "N"),
    ("KeyO", "O"),
    ("KeyP", "P"),
    ("KeyQ", "Q"),
    ("KeyR", "R"),
    ("KeyS", "S"),
    ("KeyT", "T"),
    ("KeyU", "U"),
    ("KeyV", "V"),
    ("KeyW", "W"),
    ("KeyX", "X"),
    ("KeyY", "Y"),
    ("KeyZ", "Z"),
    ("Digit0", "0"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("F1", "F1"),
    ("F2", "F2"),
    ("F3", "F3"),
    ("F4", "F4"),
    ("F5", "F5"),
    ("F6", "F6"),
    ("F7", "F7"),
    ("F8", "F8"),
    ("F9", "F9"),
    ("F10", "F10"),
    ("F11", "F11"),
    ("F12", "F12"),
    ("F13", "F13"),
    ("F14", "F14"),
    ("F15", "F15"),
    ("F16", "F16"),
    ("F17", "F17"),
    ("F18", "F18"),
    ("F19", "F19"),
    ("F20", "F20"),
    ("Space", "Space"),
    ("Minus", "-"),
    ("Equal", "="),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Semicolon", ";"),
    ("Quote", "'"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
    ("Backquote", "`"),
    ("Backslash", "\\"),
];

/// `KeyM`'s place in [`KEYS`].
const KEY_M: u8 = 12;

/// A system-wide shortcut: at least one of ⌘, ⌃ or ⌥, so it never takes a
/// key the user types, and a key from [`KEYS`]. Stored as `alt+KeyM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hotkey {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
    key: u8,
}

impl Hotkey {
    /// ⌥M: the companion's until Settings changes it.
    pub const COMPANION: Hotkey = Hotkey {
        control: false,
        option: true,
        shift: false,
        command: false,
        key: KEY_M,
    };

    /// The shortcut, or `None` for a key mdoc does not offer or without ⌘,
    /// ⌃ or ⌥.
    pub fn new(
        control: bool,
        option: bool,
        shift: bool,
        command: bool,
        code: &str,
    ) -> Option<Self> {
        let key = KEYS.iter().position(|(name, _)| *name == code)?;
        (control || option || command).then_some(Self {
            control,
            option,
            shift,
            command,
            key: u8::try_from(key).ok()?,
        })
    }

    /// Every key a hotkey may use, by `code`.
    pub fn codes() -> impl Iterator<Item = &'static str> {
        KEYS.iter().map(|(code, _)| *code)
    }

    /// The key's W3C `code` name, such as `KeyM`.
    pub fn code(&self) -> &'static str {
        KEYS[usize::from(self.key)].0
    }

    /// As macOS menus write it: ⌃⌥⇧⌘ in that order, then the key.
    pub fn label(&self) -> String {
        let marks = [
            (self.control, "⌃"),
            (self.option, "⌥"),
            (self.shift, "⇧"),
            (self.command, "⌘"),
        ];
        let mut label: String = (marks.into_iter())
            .filter(|(held, _)| *held)
            .map(|(_, mark)| mark)
            .collect();
        label.push_str(KEYS[usize::from(self.key)].1);
        label
    }
}

impl From<Hotkey> for String {
    fn from(hotkey: Hotkey) -> Self {
        let marks = [
            (hotkey.control, "ctrl+"),
            (hotkey.option, "alt+"),
            (hotkey.shift, "shift+"),
            (hotkey.command, "cmd+"),
        ];
        let mut text: String = (marks.into_iter())
            .filter(|(held, _)| *held)
            .map(|(_, mark)| mark)
            .collect();
        text.push_str(hotkey.code());
        text
    }
}

impl TryFrom<String> for Hotkey {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let mut parts: Vec<&str> = text.split('+').collect();
        let code = parts.pop().unwrap_or_default();
        let (mut control, mut option, mut shift, mut command) = (false, false, false, false);
        for part in parts {
            let held = match part {
                "ctrl" => &mut control,
                "alt" => &mut option,
                "shift" => &mut shift,
                "cmd" => &mut command,
                _ => return Err(format!("not a hotkey: {text}")),
            };
            if std::mem::replace(held, true) {
                return Err(format!("not a hotkey: {text}"));
            }
        }
        Hotkey::new(control, option, shift, command, code).ok_or(format!("not a hotkey: {text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_m_is_the_companions_and_reads_as_macos_writes_it() {
        assert_eq!(Hotkey::COMPANION.code(), "KeyM");
        assert_eq!(Hotkey::COMPANION.label(), "⌥M");
        assert_eq!(String::from(Hotkey::COMPANION), "alt+KeyM");
        let all = Hotkey::new(true, true, true, true, "F5").unwrap();
        assert_eq!(all.label(), "⌃⌥⇧⌘F5");
        assert_eq!(String::from(all), "ctrl+alt+shift+cmd+F5");
        assert_eq!(Hotkey::try_from(String::from(all)), Ok(all));
    }

    #[test]
    fn a_hotkey_needs_command_control_or_option_and_a_key_mdoc_offers() {
        assert_eq!(Hotkey::new(false, false, true, false, "KeyM"), None);
        assert_eq!(Hotkey::new(false, false, false, false, "F5"), None);
        assert_eq!(Hotkey::new(false, true, false, false, "Enter"), None);
        assert!(Hotkey::new(false, false, true, true, "Slash").is_some());
        for text in [
            "KeyM",
            "alt+alt+KeyM",
            "meta+KeyM",
            "alt+",
            "alt+Enter",
            "alt+keym",
        ] {
            assert!(Hotkey::try_from(text.to_string()).is_err(), "{text}");
        }
    }
}
