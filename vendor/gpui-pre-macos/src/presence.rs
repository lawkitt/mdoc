//! mdoc patch (ADR 0084): with the menu bar item on, closing mdoc's last
//! window hides it instead (its tabs and undo stay in memory) and mdoc
//! leaves the Dock until a window shows again. GPUI shows a window with
//! `Window::activate_window`, but has no way to hide one. The shelf's popup
//! also resizes to its content with its top kept under the menu bar item:
//! GPUI's `Window::resize` keeps the bottom edge instead.

use objc2::{
    class, msg_send,
    runtime::{AnyObject, Bool},
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSUInteger};
use std::ptr;

/// `NSApplicationActivationPolicyRegular` and `…Accessory`.
const REGULAR: isize = 0;
const ACCESSORY: isize = 1;

/// Long enough for AppKit's redraw at the end of this run-loop turn, short
/// enough not to show.
const REDRAW_AFTER: std::time::Duration = std::time::Duration::from_millis(20);

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    static kCAGravityTop: *mut AnyObject;
}

/// AppKit window and app state change only on the main thread: debug
/// builds check it.
fn debug_assert_main_thread() {
    // SAFETY: `+[NSThread isMainThread]` is safe on any thread.
    let main: Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    debug_assert!(main.as_bool(), "AppKit call off the main thread");
}

/// GPUI's view behind `window`, or null.
fn native_view(window: &gpui::Window) -> *mut AnyObject {
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return ptr::null_mut();
    };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return ptr::null_mut();
    };
    handle.ns_view.as_ptr().cast()
}

/// The `NSWindow` behind `window`, or null.
fn native_window(window: &gpui::Window) -> *mut AnyObject {
    let view = native_view(window);
    if view.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the handle's view belongs to a live window on this thread.
    unsafe { msg_send![view, window] }
}

/// Hide `window` (`orderOut:`) without closing it; `Window::activate_window`
/// shows it again. Main thread only.
pub fn hide_window(window: &gpui::Window) {
    debug_assert_main_thread();
    let native_window = native_window(window);
    // SAFETY: main thread; checked for null.
    unsafe {
        if !native_window.is_null() {
            let _: () = msg_send![native_window, orderOut: ptr::null_mut::<AnyObject>()];
        }
    }
}

/// Give `window` a content height of `height` points with its top edge
/// where it is, at once (no animation), never past the bottom of its
/// screen's visible frame. GPUI's view is then given the new size through
/// its own `setFrameSize:`, which resizes its drawable, and the window
/// re-reads its size (`Window::bounds_changed`): GPUI's own resize callback
/// cannot update a window that is being updated, as it is here.
///
/// AppKit then redraws the resized view through `displayLayer:`, presenting
/// inside its own transaction, and that frame does not reach the screen
/// after a programmatic resize like this one (shrinking or growing). So the
/// layer keeps its old picture unscaled at the top, which after a shrink
/// already shows the new layout, and the window draws again once AppKit's
/// redraw is past, on GPUI's own display tick. Main thread only.
pub fn set_window_height_keeping_top(window: &mut gpui::Window, cx: &mut gpui::App, height: f32) {
    if !resize_keeping_top(window, height) {
        return;
    }
    window.bounds_changed(cx);
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(REDRAW_AFTER).await;
        let _ = handle.update(cx, |_, window, _| window.refresh());
    })
    .detach();
}

/// Whether the window changed size.
fn resize_keeping_top(window: &gpui::Window, height: f32) -> bool {
    debug_assert_main_thread();
    let view = native_view(window);
    let native_window = native_window(window);
    if view.is_null() || native_window.is_null() {
        return false;
    }
    // SAFETY: main thread; a live window, its screen checked for null.
    unsafe {
        let frame: NSRect = msg_send![native_window, frame];
        let content: NSRect = msg_send![native_window, contentRectForFrameRect: frame];
        let chrome = frame.size.height - content.size.height;
        let top = frame.origin.y + frame.size.height;
        let mut frame_height = height as f64 + chrome;
        let screen: *mut AnyObject = msg_send![native_window, screen];
        if !screen.is_null() {
            let visible: NSRect = msg_send![screen, visibleFrame];
            frame_height = frame_height.min(top - visible.origin.y);
        }
        let frame_height = frame_height.max(chrome + 1.);
        if (frame_height - frame.size.height).abs() < 0.5 {
            return false;
        }
        let resized = NSRect::new(
            NSPoint::new(frame.origin.x, top - frame_height),
            NSSize::new(frame.size.width, frame_height),
        );
        // GPUI's view learns a new size only through its `setFrameSize:`;
        // autoresizing can change its frame around that, leaving GPUI's
        // drawable and layout at the old size. So the view sits out the
        // window's change and is then sized here, through GPUI's path.
        let mask: NSUInteger = msg_send![view, autoresizingMask];
        let _: () = msg_send![view, setAutoresizingMask: 0 as NSUInteger];
        let _: () = msg_send![native_window, setFrame: resized, display: Bool::NO];
        let content: NSRect = msg_send![native_window, contentRectForFrameRect: resized];
        let _: () = msg_send![view, setFrameOrigin: NSPoint::new(0., 0.)];
        let _: () = msg_send![view, setFrameSize: content.size];
        let _: () = msg_send![view, setAutoresizingMask: mask];
        // A picture of the old height stays pinned to the top, not
        // stretched or squashed, until the next frame replaces it.
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setContentsGravity: kCAGravityTop];
        }
    }
    true
}

/// Show mdoc in the Dock and the app switcher, or leave both (the menu bar
/// item stays). Main thread only.
pub fn set_dock_visible(visible: bool) {
    debug_assert_main_thread();
    // SAFETY: main thread; NSApp exists once GPUI runs.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let policy = if visible { REGULAR } else { ACCESSORY };
        let changed: Bool = msg_send![app, setActivationPolicy: policy];
        debug_assert!(changed.as_bool(), "activation policy refused");
    }
}
