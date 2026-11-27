//! The companion's system-wide hotkey ([ADR 0128](../../docs/adr/0128-floating-companion-window.md);
//! Milestone 2 item 15), macOS: ⌥M until Settings › AI connector records
//! another or turns it off. It is registered only while the companion can
//! open, with "AI connector (preview)" and the menu bar item on, so with
//! the preview off nothing of it exists. A press toggles the companion as
//! the item's click does ([`crate::shelf::menu_bar::hotkey_pressed`]); the OS
//! hands it to mdoc without activating it, so the app the user is in keeps
//! focus.
//!
//! A shortcut another app or macOS holds can't be registered: Settings
//! shows why under the row, and the menu bar item still toggles the
//! companion. Registration goes through a [`Registrar`]: `global-hotkey`
//! in the app; tests and headless renders never register with the OS.
//! Windows registers nothing until its companion exists.
// Windows only ever uses the inert registrar.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::ai::text;
use gpui::{App, Global, Keystroke};
use mdoc_store::settings::Hotkey;

/// Registers hotkeys with the OS.
pub(crate) trait Registrar {
    fn register(&mut self, hotkey: Hotkey) -> Result<(), Refused>;
    fn unregister(&mut self, hotkey: Hotkey);
}

/// Why a hotkey couldn't be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// Another app or macOS holds it.
    Taken,
    /// The OS said no for another reason, in its words.
    Failed(String),
}

/// The app's hotkey: what the window wants registered and what is.
struct Hotkeys {
    registrar: Box<dyn Registrar>,
    wanted: Option<Hotkey>,
    registered: Option<Hotkey>,
    /// Settings is recording a new shortcut: the old one must reach it.
    paused: bool,
    /// What a press does: toggle the companion (the probe's own in the
    /// probe).
    pressed: fn(&mut App),
}

impl Global for Hotkeys {}

/// The registrar the app uses: `global-hotkey` on macOS, never in tests or
/// headless renders.
fn default_registrar(cx: &mut App) -> Box<dyn Registrar> {
    #[cfg(all(target_os = "macos", not(test)))]
    if !mdoc_store::app_paths::isolated() {
        return Box::new(native::Native::new(cx));
    }
    let _ = cx;
    Box::new(Inert)
}

/// Registers nothing with the OS, and never refuses.
struct Inert;

impl Registrar for Inert {
    fn register(&mut self, _: Hotkey) -> Result<(), Refused> {
        Ok(())
    }
    fn unregister(&mut self, _: Hotkey) {}
}

fn hotkeys(cx: &mut App) -> &mut Hotkeys {
    if !cx.has_global::<Hotkeys>() {
        let registrar = default_registrar(cx);
        use_registrar(registrar, crate::shelf::menu_bar::hotkey_pressed, cx);
    }
    cx.global_mut::<Hotkeys>()
}

/// Register through `registrar` from now on, a press running `pressed`:
/// tests inject one that never asks the OS, and the probe its own toggle.
pub(crate) fn use_registrar(registrar: Box<dyn Registrar>, pressed: fn(&mut App), cx: &mut App) {
    cx.set_global(Hotkeys {
        registrar,
        wanted: None,
        registered: None,
        paused: false,
        pressed,
    });
}

/// Register `wanted` in place of what is registered, or nothing; why it
/// couldn't be, for Settings. macOS only.
pub(crate) fn set(wanted: Option<Hotkey>, cx: &mut App) -> Option<String> {
    let wanted = wanted.filter(|_| cfg!(target_os = "macos"));
    let hotkeys = hotkeys(cx);
    hotkeys.wanted = wanted;
    hotkeys.apply()
}

/// Settings records a new shortcut while `paused`: the registered one goes
/// until it ends, so pressing it again records it. Why the one wanted
/// couldn't come back, if it couldn't.
pub(crate) fn pause(paused: bool, cx: &mut App) -> Option<String> {
    let hotkeys = hotkeys(cx);
    hotkeys.paused = paused;
    hotkeys.apply()
}

/// The registered hotkey was pressed.
pub(crate) fn pressed(cx: &mut App) {
    let pressed = (cx.try_global::<Hotkeys>())
        .filter(|hotkeys| hotkeys.registered.is_some())
        .map(|hotkeys| hotkeys.pressed);
    if let Some(pressed) = pressed {
        pressed(cx);
    }
}

/// What is registered now, for tests.
#[cfg(test)]
pub(crate) fn registered(cx: &App) -> Option<Hotkey> {
    cx.try_global::<Hotkeys>()
        .and_then(|hotkeys| hotkeys.registered)
}

impl Hotkeys {
    fn apply(&mut self) -> Option<String> {
        let target = self.wanted.filter(|_| !self.paused);
        if self.registered == target {
            return None;
        }
        if let Some(old) = self.registered.take() {
            self.registrar.unregister(old);
        }
        let hotkey = target?;
        match self.registrar.register(hotkey) {
            Ok(()) => {
                self.registered = Some(hotkey);
                None
            }
            Err(refused) => Some(refused.message(hotkey)),
        }
    }
}

/// Why a key press in Settings' recorder is not a hotkey.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NotAHotkey {
    /// A modifier on its own: the recorder waits for the key.
    Modifier,
    /// Esc: the recorder stops.
    Cancel,
    /// What the recorder says under the row.
    Rejected(String),
}

/// A key press as macOS saw it: the key's place (its virtual key code)
/// and the modifiers held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyDown {
    pub(crate) key: u16,
    pub(crate) control: bool,
    pub(crate) option: bool,
    pub(crate) shift: bool,
    pub(crate) command: bool,
}

/// The offered keys by macOS virtual key code (`kVK_*`), which names a
/// key's place, as the registration does.
const VIRTUAL: [(u16, &str); 68] = [
    (0x00, "KeyA"),
    (0x01, "KeyS"),
    (0x02, "KeyD"),
    (0x03, "KeyF"),
    (0x04, "KeyH"),
    (0x05, "KeyG"),
    (0x06, "KeyZ"),
    (0x07, "KeyX"),
    (0x08, "KeyC"),
    (0x09, "KeyV"),
    (0x0b, "KeyB"),
    (0x0c, "KeyQ"),
    (0x0d, "KeyW"),
    (0x0e, "KeyE"),
    (0x0f, "KeyR"),
    (0x10, "KeyY"),
    (0x11, "KeyT"),
    (0x12, "Digit1"),
    (0x13, "Digit2"),
    (0x14, "Digit3"),
    (0x15, "Digit4"),
    (0x16, "Digit6"),
    (0x17, "Digit5"),
    (0x18, "Equal"),
    (0x19, "Digit9"),
    (0x1a, "Digit7"),
    (0x1b, "Minus"),
    (0x1c, "Digit8"),
    (0x1d, "Digit0"),
    (0x1e, "BracketRight"),
    (0x1f, "KeyO"),
    (0x20, "KeyU"),
    (0x21, "BracketLeft"),
    (0x22, "KeyI"),
    (0x23, "KeyP"),
    (0x25, "KeyL"),
    (0x26, "KeyJ"),
    (0x27, "Quote"),
    (0x28, "KeyK"),
    (0x29, "Semicolon"),
    (0x2a, "Backslash"),
    (0x2b, "Comma"),
    (0x2c, "Slash"),
    (0x2d, "KeyN"),
    (0x2e, "KeyM"),
    (0x2f, "Period"),
    (0x31, "Space"),
    (0x32, "Backquote"),
    (0x40, "F17"),
    (0x4f, "F18"),
    (0x50, "F19"),
    (0x5a, "F20"),
    (0x60, "F5"),
    (0x61, "F6"),
    (0x62, "F7"),
    (0x63, "F3"),
    (0x64, "F8"),
    (0x65, "F9"),
    (0x67, "F11"),
    (0x69, "F13"),
    (0x6a, "F16"),
    (0x6b, "F14"),
    (0x6d, "F10"),
    (0x6f, "F12"),
    (0x71, "F15"),
    (0x76, "F4"),
    (0x78, "F2"),
    (0x7a, "F1"),
];

/// The key press being dispatched, as macOS saw it; `None` in tests and
/// off macOS, where the recorder reads GPUI's keystroke instead.
pub(crate) fn key_down() -> Option<KeyDown> {
    #[cfg(all(target_os = "macos", not(test)))]
    return native::key_down();
    #[cfg(not(all(target_os = "macos", not(test))))]
    None
}

/// The hotkey a key press in Settings' recorder asks for. With macOS's
/// own view of the press (`native`), by the key's place, so on any layout
/// it registers the key that was pressed; otherwise by GPUI's name for it
/// on the US layout.
pub(crate) fn recorded(
    keystroke: &Keystroke,
    native: Option<KeyDown>,
) -> Result<Hotkey, NotAHotkey> {
    let held = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    if matches!(key, "shift" | "control" | "alt" | "platform" | "function") {
        return Err(NotAHotkey::Modifier);
    }
    if key == "escape" && !held.modified() {
        return Err(NotAHotkey::Cancel);
    }
    let rejected = |why: &str| Err(NotAHotkey::Rejected(why.to_string()));
    let not_offered = text::NOT_OFFERED;
    let unmodified = text::UNMODIFIED;
    if let Some(down) = native {
        let Some(&(_, code)) = VIRTUAL.iter().find(|(key, _)| *key == down.key) else {
            return rejected(not_offered);
        };
        return match Hotkey::new(down.control, down.option, down.shift, down.command, code) {
            Some(hotkey) => Ok(hotkey),
            None => rejected(unmodified),
        };
    }
    // GPUI names a shifted symbol by itself, with Shift taken off.
    let (code, shifted) = match code_of(key) {
        Some(found) => found,
        None => return rejected(not_offered),
    };
    let shift = held.shift || shifted;
    match Hotkey::new(held.control, held.alt, shift, held.platform, &code) {
        Some(hotkey) => Ok(hotkey),
        None => rejected(unmodified),
    }
}

/// US-layout symbols by key: unshifted, then shifted.
const SYMBOLS: [(&str, char, char); 11] = [
    ("Minus", '-', '_'),
    ("Equal", '=', '+'),
    ("BracketLeft", '[', '{'),
    ("BracketRight", ']', '}'),
    ("Semicolon", ';', ':'),
    ("Quote", '\'', '"'),
    ("Comma", ',', '<'),
    ("Period", '.', '>'),
    ("Slash", '/', '?'),
    ("Backquote", '`', '~'),
    ("Backslash", '\\', '|'),
];

/// Shifted digits on the US layout, from 0.
const SHIFTED_DIGITS: [char; 10] = [')', '!', '@', '#', '$', '%', '^', '&', '*', '('];

/// GPUI's name for a key, as a W3C `code`, and whether it was shifted.
fn code_of(key: &str) -> Option<(String, bool)> {
    if key == "space" {
        return Some(("Space".into(), false));
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=20)
            .contains(&n)
            .then(|| (text::function_key_code(n), false));
    }
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_ascii_alphabetic() {
        return Some((
            text::letter_key_code(c.to_ascii_uppercase()),
            c.is_ascii_uppercase(),
        ));
    }
    if c.is_ascii_digit() {
        return Some((text::digit_key_code(c), false));
    }
    if let Some(digit) = SHIFTED_DIGITS.iter().position(|&s| s == c) {
        return Some((text::digit_key_code(digit), true));
    }
    SYMBOLS.iter().find_map(|&(code, plain, shifted)| {
        (c == plain || c == shifted).then(|| (code.to_string(), c == shifted))
    })
}

/// `hotkey` as `global-hotkey` names it.
#[cfg(target_os = "macos")]
fn to_native(hotkey: Hotkey) -> Result<global_hotkey::hotkey::HotKey, Refused> {
    String::from(hotkey)
        .parse()
        .map_err(|error: global_hotkey::hotkey::HotKeyParseError| {
            Refused::Failed(error.to_string())
        })
}

/// `global-hotkey` on macOS: Carbon's `RegisterEventHotKey`, exclusive, so
/// a shortcut another app holds is refused rather than never firing.
#[cfg(all(target_os = "macos", not(test)))]
#[allow(unsafe_code)] // AppKit's current event, through objc2 (#897).
pub(crate) mod native {
    use super::{KeyDown, Refused, Registrar, to_native};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use mdoc_store::settings::Hotkey;
    use std::cell::RefCell;

    thread_local! {
        /// Where a press goes: the app, on the main thread, where Carbon
        /// delivers it.
        static APP: RefCell<Option<gpui::AsyncApp>> = const { RefCell::new(None) };
    }

    /// AppKit's current event, while it is a key press.
    pub(crate) fn key_down() -> Option<KeyDown> {
        use objc2::{class, msg_send, runtime::AnyObject};
        // SAFETY: main thread (GPUI dispatches keys there); NSApp exists
        // while GPUI runs, and the event is a live autoreleased object.
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let event: *mut AnyObject = msg_send![app, currentEvent];
            if event.is_null() {
                return None;
            }
            // NSEventTypeKeyDown.
            let kind: usize = msg_send![event, type];
            if kind != 10 {
                return None;
            }
            let key: u16 = msg_send![event, keyCode];
            let flags: usize = msg_send![event, modifierFlags];
            Some(KeyDown {
                key,
                shift: flags & (1 << 17) != 0,
                control: flags & (1 << 18) != 0,
                option: flags & (1 << 19) != 0,
                command: flags & (1 << 20) != 0,
            })
        }
    }

    pub(crate) struct Native {
        /// Made at the first registration: it installs Carbon's handler.
        manager: Option<GlobalHotKeyManager>,
    }

    impl Native {
        pub(crate) fn new(cx: &gpui::App) -> Self {
            APP.with(|app| *app.borrow_mut() = Some(cx.to_async()));
            // Set once per process; presses arrive inside an AppKit event,
            // possibly while GPUI is busy, so they queue on its executor.
            GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
                if event.state != HotKeyState::Pressed {
                    return;
                }
                APP.with(|app| {
                    let Some(cx) = app.borrow().clone() else {
                        return;
                    };
                    let executor = cx.foreground_executor().clone();
                    executor
                        .spawn(async move { cx.update(super::pressed) })
                        .detach();
                });
            }));
            Self { manager: None }
        }
    }

    impl Registrar for Native {
        fn register(&mut self, hotkey: Hotkey) -> Result<(), Refused> {
            let native = to_native(hotkey)?;
            let manager = match &mut self.manager {
                Some(manager) => manager,
                manager => manager.insert(
                    GlobalHotKeyManager::new().map_err(|e| Refused::Failed(e.to_string()))?,
                ),
            };
            manager.register(native).map_err(|error| match error {
                global_hotkey::Error::AlreadyRegistered(_) => Refused::Taken,
                error => Refused::Failed(error.to_string()),
            })
        }

        fn unregister(&mut self, hotkey: Hotkey) {
            if let (Some(manager), Ok(native)) = (&self.manager, to_native(hotkey)) {
                let _ = manager.unregister(native);
            }
        }
    }
}

/// A registrar that registers nothing with the OS and holds what it was
/// asked, with shortcuts a test marks taken.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct Recorded(std::rc::Rc<std::cell::RefCell<RecordedState>>);

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordedState {
    pub(crate) registered: Option<Hotkey>,
    pub(crate) taken: Vec<Hotkey>,
    /// Every register and unregister, in order: `true` registers.
    pub(crate) calls: Vec<(bool, Hotkey)>,
}

#[cfg(test)]
impl Recorded {
    /// Install it as the app's registrar, a press toggling the companion.
    pub(crate) fn install(cx: &mut App) -> Self {
        let recorded = Self::default();
        use_registrar(
            Box::new(recorded.clone()),
            crate::shelf::menu_bar::hotkey_pressed,
            cx,
        );
        recorded
    }
    pub(crate) fn state(&self) -> std::cell::Ref<'_, RecordedState> {
        self.0.borrow()
    }
    pub(crate) fn take(&self, hotkey: Hotkey) {
        self.0.borrow_mut().taken.push(hotkey);
    }
}

#[cfg(test)]
impl Registrar for Recorded {
    fn register(&mut self, hotkey: Hotkey) -> Result<(), Refused> {
        let mut state = self.0.borrow_mut();
        state.calls.push((true, hotkey));
        if state.taken.contains(&hotkey) {
            return Err(Refused::Taken);
        }
        state.registered = Some(hotkey);
        Ok(())
    }
    fn unregister(&mut self, hotkey: Hotkey) {
        let mut state = self.0.borrow_mut();
        state.calls.push((false, hotkey));
        if state.registered == Some(hotkey) {
            state.registered = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &'static str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    fn recorded(native: Option<KeyDown>, keystroke: &Keystroke) -> Result<Hotkey, NotAHotkey> {
        super::recorded(keystroke, native)
    }

    /// Review of #1268: on German QWERTZ the key in the US `Y` place types
    /// `z`; macOS's key code still names the place, which is what gets
    /// registered, so the shortcut recorded is the one pressed.
    #[test]
    fn the_recorder_registers_the_key_pressed_whatever_the_layout() {
        let down = KeyDown {
            key: 0x10,
            control: true,
            option: false,
            shift: false,
            command: false,
        };
        assert_eq!(
            recorded(Some(down), &key("ctrl-z")),
            Ok(Hotkey::new(true, false, false, false, "KeyY").unwrap())
        );
        let unmodified = KeyDown {
            control: false,
            ..down
        };
        assert!(matches!(
            recorded(Some(unmodified), &key("z")),
            Err(NotAHotkey::Rejected(_))
        ));
        // Return is not offered; Esc stops whatever the code.
        let enter = KeyDown { key: 0x24, ..down };
        assert!(matches!(
            recorded(Some(enter), &key("ctrl-enter")),
            Err(NotAHotkey::Rejected(_))
        ));
        assert_eq!(
            recorded(
                Some(KeyDown {
                    key: 0x35,
                    control: false,
                    ..down
                }),
                &key("escape")
            ),
            Err(NotAHotkey::Cancel)
        );
        // Every offered key has exactly one place.
        for code in Hotkey::codes() {
            let places = VIRTUAL.iter().filter(|(_, named)| *named == code).count();
            assert_eq!(places, 1, "{code}");
        }
    }

    #[test]
    fn the_recorder_takes_a_key_with_command_control_or_option() {
        assert_eq!(recorded(None, &key("alt-m")), Ok(Hotkey::COMPANION));
        let hotkey = |control, option, shift, command, code| {
            Hotkey::new(control, option, shift, command, code).unwrap()
        };
        assert_eq!(
            recorded(None, &key("ctrl-shift-space")),
            Ok(hotkey(true, false, true, false, "Space"))
        );
        assert_eq!(
            recorded(None, &key("cmd-f12")),
            Ok(hotkey(false, false, false, true, "F12"))
        );
        // GPUI names ⌃⇧1 by its symbol, with Shift taken off.
        assert_eq!(
            recorded(None, &key("ctrl-!")),
            Ok(hotkey(true, false, true, false, "Digit1"))
        );
        assert_eq!(
            recorded(None, &key("alt-/")),
            Ok(hotkey(false, true, false, false, "Slash"))
        );
        assert_eq!(recorded(None, &key("escape")), Err(NotAHotkey::Cancel));
        assert_eq!(recorded(None, &key("shift")), Err(NotAHotkey::Modifier));
        for typed in ["m", "shift-m", "f5", "alt-enter", "cmd-left", "alt-tab"] {
            assert!(
                matches!(recorded(None, &key(typed)), Err(NotAHotkey::Rejected(_))),
                "{typed}"
            );
        }
    }

    /// Every key the setting offers is one `global-hotkey` registers, and
    /// one the recorder can take.
    #[cfg(target_os = "macos")]
    #[test]
    fn every_offered_key_registers_and_records() {
        for code in Hotkey::codes() {
            let hotkey = Hotkey::new(false, true, false, false, code).unwrap();
            let native = to_native(hotkey).unwrap_or_else(|_| panic!("{code}"));
            assert_eq!(native.key.to_string(), code);
            assert_eq!(native.mods, global_hotkey::hotkey::Modifiers::ALT, "{code}");
            let typed = match hotkey.label().trim_start_matches('⌥') {
                "Space" => "space".to_string(),
                label => label.to_lowercase(),
            };
            assert_eq!(
                recorded(
                    None,
                    &Keystroke {
                        modifiers: gpui::Modifiers::alt(),
                        key: typed.clone(),
                        key_char: None,
                    }
                ),
                Ok(hotkey),
                "{typed}"
            );
        }
    }

    #[test]
    fn a_taken_shortcut_says_the_menu_bar_icon_still_opens_the_companion() {
        assert_eq!(
            Refused::Taken.message(Hotkey::COMPANION),
            "⌥M is in use by another app or by macOS. Record another shortcut; the menu bar \
             icon still opens the companion."
        );
    }
}
