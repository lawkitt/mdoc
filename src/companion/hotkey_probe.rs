//! `mdoc --mdoc-hotkey-probe` (debug builds, macOS): the native check of
//! the companion's hotkey, run from a terminal while another app is in
//! front. It registers ⌥M through the app's own registrar
//! ([`super::hotkey`]), then posts ⌥M twice into the window server's HID
//! stream (`CGEventPost`), as a keyboard would, from outside mdoc's event
//! loop. Each press must reach mdoc and toggle a window opened the way the
//! companion opens ([`super::open`]): open after the first, closed after
//! the second, and after each the frontmost app unchanged and mdoc not
//! active. It prints one line per step and exits 1 if any check fails.
//!
//! Posting needs the Accessibility grant of the app that runs it (the
//! terminal): without it the probe says so and exits 2, before
//! registering anything. It never posts a key unless ⌥M is registered, so
//! nothing is typed into the app in front.
#![allow(unsafe_code)] // The window server's event calls (#897).

use super::{hotkey, probe};
use gpui::{App, Global, WindowHandle};
use mdoc_store::settings::Hotkey;
use std::{cell::Cell, ffi::c_void, rc::Rc};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGEventSourceCreate(state: i32) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(source: *mut c_void, key: u16, down: bool) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
}

/// `kCGEventSourceStateHIDSystemState`.
const HID_STATE: i32 = 1;
/// `kCGHIDEventTap`: where a keyboard's events enter.
const HID_TAP: u32 = 0;
/// `kVK_ANSI_M`.
const KEY_M: u16 = 0x2e;
/// `kCGEventFlagMaskAlternate`: ⌥.
const OPTION: u64 = 0x0008_0000;

/// ⌥M down and up, into the HID stream.
fn post_option_m() {
    // SAFETY: Create-rule CoreGraphics objects, each released once.
    unsafe {
        let source = CGEventSourceCreate(HID_STATE);
        for down in [true, false] {
            let event = CGEventCreateKeyboardEvent(source, KEY_M, down);
            if event.is_null() {
                continue;
            }
            CGEventSetFlags(event, OPTION);
            CGEventPost(HID_TAP, event);
            CFRelease(event);
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
}

/// The window a press toggles, and how many presses reached mdoc.
#[derive(Default)]
struct Toggled {
    window: Option<WindowHandle<probe::Probe>>,
    presses: u32,
}

impl Global for Toggled {}

/// A press: the window opens if closed, and closes if open, as the menu
/// bar's companion toggles.
fn toggle(cx: &mut App) {
    let open = cx.global_mut::<Toggled>().window.take();
    cx.global_mut::<Toggled>().presses += 1;
    match open {
        Some(window) => {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
        None => {
            let window = probe::open_probe(cx, Rc::new(Cell::new(0)));
            cx.global_mut::<Toggled>().window = window;
        }
    }
}

/// Run the probe; the process exits with its result.
pub(crate) fn run() -> ! {
    let own = std::process::id() as i32;
    gpui_platform::application().run(move |cx: &mut App| {
        let before = probe::frontmost();
        if before == Some(own) {
            println!(
                "note: mdoc was frontmost before the probe; run it from a terminal with \
                 another app in front"
            );
        }
        // SAFETY: a plain query of this process's grant.
        if !unsafe { CGPreflightPostEventAccess() } {
            println!(
                "hotkey probe: posting key events needs the Accessibility grant of the app \
                 running the probe; nothing was registered"
            );
            std::process::exit(2);
        }
        cx.set_global(Toggled::default());
        let registrar = Box::new(hotkey::native::Native::new(cx));
        hotkey::use_registrar(registrar, toggle, cx);
        if let Some(why) = hotkey::set(Some(Hotkey::COMPANION), cx) {
            println!("registered: FAILED {why}");
            std::process::exit(1);
        }
        println!("registered: ok {}", Hotkey::COMPANION.label());
        cx.spawn(async move |cx| {
            let settle = cx.background_executor().clone();
            let mut failed = false;
            let mut report = |step: &str, found: Vec<String>, seen: String| {
                failed |= !found.is_empty();
                let verdict = match found.is_empty() {
                    true => "ok".to_string(),
                    false => format!("FAILED {found:?}"),
                };
                println!("{step}: {verdict} {seen}");
            };
            settle.timer(probe::SETTLE).await;
            post_option_m();
            settle.timer(probe::SETTLE).await;
            let (window, presses) = cx.update(|cx| {
                let toggled = cx.global::<Toggled>();
                (toggled.window, toggled.presses)
            });
            let native = window.and_then(|window| probe::inspect_window(window, cx));
            let mut found = probe::failures(native, before, own);
            if presses != 1 {
                found.push(format!("{presses} presses reached mdoc, not 1"));
            }
            report("first press opened it", found, format!("{native:?}"));
            post_option_m();
            settle.timer(probe::SETTLE).await;
            let (window, presses) = cx.update(|cx| {
                let toggled = cx.global::<Toggled>();
                (toggled.window, toggled.presses)
            });
            let frontmost = probe::frontmost();
            let checks = [
                (
                    presses == 2,
                    format!("{presses} presses reached mdoc, not 2"),
                ),
                (window.is_none(), "it is still open".into()),
                (!probe::app_active(), "mdoc became active".into()),
                (
                    frontmost == before && frontmost != Some(own),
                    "the frontmost app changed".into(),
                ),
            ];
            let found = (checks.into_iter())
                .filter(|(ok, _)| !ok)
                .map(|(_, what)| what)
                .collect();
            report(
                "second press closed it",
                found,
                format!("frontmost: {frontmost:?}"),
            );
            cx.update(|cx| hotkey::set(None, cx));
            std::process::exit(i32::from(failed));
        })
        .detach();
    });
    std::process::exit(1)
}
