//! The companion's system-wide hotkey ([ADR 0128](../../docs/adr/0128-floating-companion-window.md);
//! Milestone 2 item 15), macOS: ⌥M until Settings › AI connector records
//! another or turns it off. It is registered only while the companion can
//! open, with "AI connector (preview)" and the menu bar item on, so with
//! the preview off nothing of it exists. A press toggles the companion as
//! the item's click does ([`crate::menu_bar::hotkey_pressed`]); the OS
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

impl Refused {
    /// What Settings › AI connector shows under the row.
    pub(crate) fn message(&self, hotkey: Hotkey) -> String {
        let label = hotkey.label();
        match self {
            Refused::Taken => format!(
                "{label} is in use by another app or by macOS. Record another shortcut; the \
                 menu bar icon still opens the companion."
            ),
            Refused::Failed(why) => {
                format!("Couldn't set {label}: {why}. The menu bar icon still opens the companion.")
            }
        }
    }
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
        use_registrar(registrar, crate::menu_bar::hotkey_pressed, cx);
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

/// The hotkey a key press in Settings' recorder asks for, by the key's
/// place on the US layout, as GPUI names it.
pub(crate) fn recorded(keystroke: &Keystroke) -> Result<Hotkey, NotAHotkey> {
    let held = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    if matches!(key, "shift" | "control" | "alt" | "platform" | "function") {
        return Err(NotAHotkey::Modifier);
    }
    if key == "escape" && !held.modified() {
        return Err(NotAHotkey::Cancel);
    }
    let rejected = |why: &str| Err(NotAHotkey::Rejected(why.to_string()));
    // GPUI names a shifted symbol by itself, with Shift taken off.
    let (code, shifted) = match code_of(key) {
        Some(found) => found,
        None => return rejected("Use a letter, a digit, a symbol, Space or F1–F20."),
    };
    let shift = held.shift || shifted;
    match Hotkey::new(held.control, held.alt, shift, held.platform, &code) {
        Some(hotkey) => Ok(hotkey),
        None => rejected("Hold ⌘, ⌃ or ⌥ with the key, so typing never triggers it."),
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
        return (1..=20).contains(&n).then(|| (format!("F{n}"), false));
    }
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_ascii_alphabetic() {
        return Some((
            format!("Key{}", c.to_ascii_uppercase()),
            c.is_ascii_uppercase(),
        ));
    }
    if c.is_ascii_digit() {
        return Some((format!("Digit{c}"), false));
    }
    if let Some(digit) = SHIFTED_DIGITS.iter().position(|&s| s == c) {
        return Some((format!("Digit{digit}"), true));
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
pub(crate) mod native {
    use super::{Refused, Registrar, to_native};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use mdoc_store::settings::Hotkey;
    use std::cell::RefCell;

    thread_local! {
        /// Where a press goes: the app, on the main thread, where Carbon
        /// delivers it.
        static APP: RefCell<Option<gpui::AsyncApp>> = const { RefCell::new(None) };
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
            crate::menu_bar::hotkey_pressed,
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

    #[test]
    fn the_recorder_takes_a_key_with_command_control_or_option() {
        assert_eq!(recorded(&key("alt-m")), Ok(Hotkey::COMPANION));
        let hotkey = |control, option, shift, command, code| {
            Hotkey::new(control, option, shift, command, code).unwrap()
        };
        assert_eq!(
            recorded(&key("ctrl-shift-space")),
            Ok(hotkey(true, false, true, false, "Space"))
        );
        assert_eq!(
            recorded(&key("cmd-f12")),
            Ok(hotkey(false, false, false, true, "F12"))
        );
        // GPUI names ⌃⇧1 by its symbol, with Shift taken off.
        assert_eq!(
            recorded(&key("ctrl-!")),
            Ok(hotkey(true, false, true, false, "Digit1"))
        );
        assert_eq!(
            recorded(&key("alt-/")),
            Ok(hotkey(false, true, false, false, "Slash"))
        );
        assert_eq!(recorded(&key("escape")), Err(NotAHotkey::Cancel));
        assert_eq!(recorded(&key("shift")), Err(NotAHotkey::Modifier));
        for typed in ["m", "shift-m", "f5", "alt-enter", "cmd-left", "alt-tab"] {
            assert!(
                matches!(recorded(&key(typed)), Err(NotAHotkey::Rejected(_))),
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
                recorded(&Keystroke {
                    modifiers: gpui::Modifiers::alt(),
                    key: typed.clone(),
                    key_char: None,
                }),
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
