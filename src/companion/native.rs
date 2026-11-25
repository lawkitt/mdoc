//! The companion's AppKit calls (macOS): GPUI's `WindowKind::PopUp` is a
//! non-activating `NSPanel` above other apps, and `focus: false` orders it
//! front without making it key. GPUI has no call for the rest, so this
//! module makes it:
//!
//! - missing from screenshots and screen sharing (`NSWindowSharingNone`);
//! - stay up while mdoc is not the active app (an `NSPanel` hides on
//!   deactivation by default);
//! - never key on a click (`becomesKeyOnlyIfNeeded`: no GPUI view asks for
//!   the panel to become key), so typing in the app the user is in goes on;
//! - shown only now, after all that, in front of other apps' windows
//!   (`orderFrontRegardless`), as mdoc is in the background.
//!
//! The probe (`probe.rs`, debug builds) reads it all back.
#![allow(unsafe_code)] // AppKit and the window server through objc2 (#897).

use objc2::{
    msg_send,
    runtime::{AnyObject, Bool},
    sel,
};
use std::ptr;

/// `NSWindowSharingNone`.
pub(super) const SHARING_NONE: usize = 0;

/// The `NSWindow` behind `window`, or null.
pub(super) fn native_window(window: &gpui::Window) -> *mut AnyObject {
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return ptr::null_mut();
    };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return ptr::null_mut();
    };
    let view: *mut AnyObject = handle.ns_view.as_ptr().cast();
    // SAFETY: the handle's view belongs to a live window on this thread.
    unsafe { msg_send![view, window] }
}

/// Float `window`, opened with `show: false`, over other apps: excluded
/// from capture, kept up while mdoc is in the background, never key, and
/// only then shown. Main thread only.
pub(crate) fn float(window: &gpui::Window) {
    let native = native_window(window);
    if native.is_null() {
        return;
    }
    // SAFETY: main thread (GPUI updates windows there); a live NSWindow.
    unsafe {
        let _: () = msg_send![native, setSharingType: SHARING_NONE];
        let _: () = msg_send![native, setHidesOnDeactivate: Bool::NO];
        let panel: Bool = msg_send![native, respondsToSelector: sel!(setBecomesKeyOnlyIfNeeded:)];
        if panel.as_bool() {
            let _: () = msg_send![native, setBecomesKeyOnlyIfNeeded: Bool::YES];
        }
        let _: () = msg_send![native, orderFrontRegardless];
    }
}
