//! mdoc patch (ADR 0084): with the menu bar item on, closing mdoc's last
//! window hides it instead (its tabs and undo stay in memory) and mdoc
//! leaves the Dock until a window shows again. GPUI shows a window with
//! `Window::activate_window`, but has no way to hide one or to change the
//! activation policy.

use cocoa::base::{id, nil};
use objc::{class, msg_send, runtime::BOOL, sel, sel_impl};

/// `NSApplicationActivationPolicyRegular` and `…Accessory`.
const REGULAR: isize = 0;
const ACCESSORY: isize = 1;

/// AppKit window and app state change only on the main thread: debug
/// builds check it.
fn debug_assert_main_thread() {
    // SAFETY: `+[NSThread isMainThread]` is safe on any thread.
    let main: BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
    debug_assert!(main != objc::runtime::NO, "AppKit call off the main thread");
}

/// Hide `window` (`orderOut:`) without closing it; `Window::activate_window`
/// shows it again. Main thread only.
pub fn hide_window(window: &gpui::Window) {
    debug_assert_main_thread();
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return;
    };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let view: id = handle.ns_view.as_ptr().cast();
    // SAFETY: main thread; the view belongs to a live window, checked for nil.
    unsafe {
        let native_window: id = msg_send![view, window];
        if !native_window.is_null() {
            let _: () = msg_send![native_window, orderOut: nil];
        }
    }
}

/// Show mdoc in the Dock and the app switcher, or leave both (the menu bar
/// item stays). Main thread only.
pub fn set_dock_visible(visible: bool) {
    debug_assert_main_thread();
    // SAFETY: main thread; NSApp exists once GPUI runs.
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let policy = if visible { REGULAR } else { ACCESSORY };
        let changed: BOOL = msg_send![app, setActivationPolicy: policy];
        debug_assert!(changed != objc::runtime::NO, "activation policy refused");
    }
}
