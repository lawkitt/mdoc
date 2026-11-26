//! `mdoc --mdoc-companion-probe` (debug builds, macOS): the native check
//! of the companion window's focus and capture exclusion, run from a
//! terminal while another app is in front. It opens a window the way the
//! companion opens ([`super::open`]), clicks it as AppKit dispatches a
//! click (`-[NSApplication sendEvent:]`), hides it and opens it again, as
//! the menu bar item toggles it, and after each step asks AppKit and the
//! window server: not key, not main, mdoc not active, the frontmost app
//! unchanged, visible above other apps' windows, shared with no capture,
//! kept up while mdoc is in the background, and key only if a view needs
//! it; the click must reach GPUI. It prints one line per step and exits 1
//! if any check fails.
//!
//! What only a person can see stays a GUI row: that a screenshot or a
//! shared screen misses the window, that typing in the other app goes on
//! when it opens and after a click on it, and that the window server does
//! not activate mdoc for a real click (the probe's click starts in the
//! app).
#![allow(unsafe_code)] // AppKit and the window server through objc2 (#897).

use gpui::{
    App, AppContext as _, Bounds, Context, MouseButton, Render, Window, WindowHandle, div, point,
    prelude::*, px, size,
};
use objc2::{
    class,
    encode::{Encode, Encoding},
    msg_send,
    runtime::{AnyObject, Bool},
    sel,
};
use std::{cell::Cell, ffi::c_void, ptr, rc::Rc, time::Duration};

/// How long AppKit gets to settle after each step.
const SETTLE: Duration = Duration::from_millis(500);

/// The companion as AppKit and the window server see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Native {
    key: bool,
    main: bool,
    visible: bool,
    level: isize,
    /// `NSWindow.sharingType` is `NSWindowSharingNone`.
    sharing_none: bool,
    /// The window server's `kCGWindowSharingState` for it: 0 is none.
    server_sharing: Option<u32>,
    hides_on_deactivate: bool,
    becomes_key_only_if_needed: bool,
    /// mdoc is the active app.
    app_active: bool,
    /// The frontmost app's process id.
    frontmost: Option<i32>,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
}

/// `kCGWindowListOptionIncludingWindow`.
const INCLUDING_WINDOW: u32 = 1 << 3;

/// An `NSString` for `text`, autoreleased.
fn ns_string(text: &std::ffi::CStr) -> *mut AnyObject {
    // SAFETY: a NUL-terminated UTF-8 string; the result is autoreleased.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

/// The window server's sharing state for window `number`.
fn server_sharing(number: isize) -> Option<u32> {
    let number = u32::try_from(number).ok()?;
    // SAFETY: a Create-rule CFArray of CFDictionary, released below; both
    // are toll-free bridged to NSArray and NSDictionary.
    unsafe {
        let list = CGWindowListCopyWindowInfo(INCLUDING_WINDOW, number);
        if list.is_null() {
            return None;
        }
        let array = list as *mut AnyObject;
        let count: usize = msg_send![array, count];
        let mut state = None;
        for index in 0..count {
            let info: *mut AnyObject = msg_send![array, objectAtIndex: index];
            let id: *mut AnyObject = msg_send![info, objectForKey: ns_string(c"kCGWindowNumber")];
            if id.is_null() {
                continue;
            }
            let id: u32 = msg_send![id, unsignedIntValue];
            if id != number {
                continue;
            }
            let sharing: *mut AnyObject =
                msg_send![info, objectForKey: ns_string(c"kCGWindowSharingState")];
            if !sharing.is_null() {
                state = Some(msg_send![sharing, unsignedIntValue]);
            }
        }
        CFRelease(list);
        state
    }
}

/// The frontmost app's process id.
fn frontmost() -> Option<i32> {
    // SAFETY: NSWorkspace is safe to ask on the main thread.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        (!app.is_null()).then(|| msg_send![app, processIdentifier])
    }
}

/// Whether mdoc is the active app.
fn app_active() -> bool {
    // SAFETY: NSApp exists once GPUI runs; main thread.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let active: Bool = msg_send![app, isActive];
        active.as_bool()
    }
}

/// `window` as AppKit and the window server see it now. Main thread only.
fn inspect(window: &Window) -> Option<Native> {
    let native = super::native::native_window(window);
    if native.is_null() {
        return None;
    }
    // SAFETY: main thread; a live NSWindow (an NSPanel, checked before
    // asking a panel's property).
    unsafe {
        let key: Bool = msg_send![native, isKeyWindow];
        let main: Bool = msg_send![native, isMainWindow];
        let visible: Bool = msg_send![native, isVisible];
        let level: isize = msg_send![native, level];
        let sharing: usize = msg_send![native, sharingType];
        let number: isize = msg_send![native, windowNumber];
        let hides: Bool = msg_send![native, hidesOnDeactivate];
        let panel: Bool = msg_send![native, respondsToSelector: sel!(becomesKeyOnlyIfNeeded)];
        let only_if_needed = panel.as_bool() && {
            let needed: Bool = msg_send![native, becomesKeyOnlyIfNeeded];
            needed.as_bool()
        };
        Some(Native {
            key: key.as_bool(),
            main: main.as_bool(),
            visible: visible.as_bool(),
            level,
            sharing_none: sharing == super::native::SHARING_NONE,
            server_sharing: server_sharing(number),
            hides_on_deactivate: hides.as_bool(),
            becomes_key_only_if_needed: only_if_needed,
            app_active: app_active(),
            frontmost: frontmost(),
        })
    }
}

/// `CGPoint`, as AppKit passes it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

// SAFETY: laid out as `CGPoint` is, two `f64`s, under its encoding's name.
unsafe impl Encode for Point {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

/// A left click at `at` in `native`'s coordinates, dispatched as AppKit
/// dispatches a real one, where a click makes a window key. Called from
/// the run loop, never inside a GPUI update of that window (GPUI would
/// drop the event). Main thread only; `native` a live window.
fn click(native: *mut AnyObject, at: Point) {
    // SAFETY: main thread; a live NSWindow; the events are autoreleased.
    unsafe {
        let number: isize = msg_send![native, windowNumber];
        let process: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
        let now: f64 = msg_send![process, systemUptime];
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        // NSEventTypeLeftMouseDown, then NSEventTypeLeftMouseUp.
        for kind in [1usize, 2] {
            let event: *mut AnyObject = msg_send![
                class!(NSEvent),
                mouseEventWithType: kind,
                location: at,
                modifierFlags: 0usize,
                timestamp: now,
                windowNumber: number,
                context: ptr::null_mut::<AnyObject>(),
                eventNumber: 0isize,
                clickCount: 1isize,
                pressure: 1.0f32
            ];
            if !event.is_null() {
                let _: () = msg_send![app, sendEvent: event];
            }
        }
    }
}

/// A stand-in for the companion's content that counts the clicks GPUI
/// delivers to it.
struct Probe {
    clicks: Rc<Cell<u32>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0xf4f1ea))
            .text_color(gpui::rgb(0x1f1d1a))
            .child("mdoc companion probe")
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                clicks.set(clicks.get() + 1)
            })
    }
}

/// One step's checks: each failure in words.
fn failures(native: Option<Native>, frontmost_before: Option<i32>, own: i32) -> Vec<String> {
    let Some(native) = native else {
        return vec!["no native window".into()];
    };
    let checks = [
        (!native.key, "became key"),
        (!native.main, "became main"),
        (native.visible, "not visible"),
        // NSNormalWindowLevel is 0.
        (native.level > 0, "not above other apps' windows"),
        (native.sharing_none, "sharingType is not none"),
        (
            native.server_sharing == Some(0),
            "the window server shares it",
        ),
        (
            !native.hides_on_deactivate,
            "it hides while mdoc is inactive",
        ),
        (
            native.becomes_key_only_if_needed,
            "a click would make it key",
        ),
        (!native.app_active, "mdoc became active"),
        (
            native.frontmost == frontmost_before && native.frontmost != Some(own),
            "the frontmost app changed",
        ),
    ];
    (checks.into_iter())
        .filter(|(ok, _)| !ok)
        .map(|(_, what)| what.to_string())
        .collect()
}

fn open_probe(cx: &mut App, clicks: Rc<Cell<u32>>) -> Option<WindowHandle<Probe>> {
    let display = cx.primary_display()?;
    let screen = display.bounds();
    let bounds = Bounds {
        origin: point(screen.size.width - px(380.), px(40.)),
        size: size(px(360.), px(200.)),
    };
    super::open(bounds, display.id(), cx, |_, cx| {
        cx.new(|_| Probe { clicks })
    })
    .ok()
}

fn inspect_window(window: WindowHandle<Probe>, cx: &mut gpui::AsyncApp) -> Option<Native> {
    window
        .update(cx, |_, window, _| inspect(window))
        .ok()
        .flatten()
}

/// Run the probe; the process exits with its result.
pub(crate) fn run() -> ! {
    let own = std::process::id() as i32;
    gpui_platform::application().run(move |cx: &mut App| {
        let before = frontmost();
        if before == Some(own) {
            println!(
                "note: mdoc was frontmost before the window opened; run the probe from a \
                 terminal with another app in front"
            );
        }
        let clicks = Rc::new(Cell::new(0));
        let Some(window) = open_probe(cx, clicks.clone()) else {
            eprintln!("companion probe: the window did not open");
            std::process::exit(1);
        };
        cx.spawn(async move |cx| {
            let mut failed = false;
            let mut report = |step: &str, native: Option<Native>, extra: Option<&str>| {
                let mut found = failures(native, before, own);
                found.extend(extra.map(str::to_string));
                failed |= !found.is_empty();
                let verdict = match found.is_empty() {
                    true => "ok".to_string(),
                    false => format!("FAILED {found:?}"),
                };
                println!("{step}: {verdict} {native:?}");
            };
            let settle = cx.background_executor().clone();
            settle.timer(SETTLE).await;
            report("opened", inspect_window(window, cx), None);
            // In the middle of the window, in its own coordinates.
            let native = window
                .update(cx, |_, window, _| {
                    super::native::native_window(window) as usize
                })
                .unwrap_or(0) as *mut AnyObject;
            if !native.is_null() {
                click(native, Point { x: 180., y: 100. });
            }
            settle.timer(SETTLE).await;
            report(
                "clicked",
                inspect_window(window, cx),
                (clicks.get() == 0).then_some("the click never reached GPUI"),
            );
            // Hidden and shown again, as the menu bar item toggles it.
            let _ = window.update(cx, |_, window, _| window.remove_window());
            settle.timer(SETTLE).await;
            let reopened = cx.update(|cx| open_probe(cx, clicks.clone()));
            settle.timer(SETTLE).await;
            report(
                "reopened",
                reopened.and_then(|window| inspect_window(window, cx)),
                None,
            );
            std::process::exit(i32::from(failed));
        })
        .detach();
    });
    std::process::exit(1)
}
