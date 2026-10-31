//! mdoc patch (ADR 0084): with the menu bar item on, closing mdoc's last
//! window hides it instead (its tabs and undo stay in memory) and mdoc
//! leaves the Dock until a window shows again. GPUI shows a window with
//! `Window::activate_window`, but has no way to hide one or to change the
//! activation policy. The shelf's popup also resizes to its content with
//! its top kept under the menu bar item: GPUI's `Window::resize` keeps the
//! bottom edge instead.

use cocoa::{
    appkit::{NSScreen, NSWindow},
    base::{NO, id, nil},
    foundation::{NSPoint, NSRect, NSSize, NSUInteger},
};
use objc::{class, msg_send, runtime::BOOL, sel, sel_impl};

/// `NSApplicationActivationPolicyRegular` and `…Accessory`.
const REGULAR: isize = 0;
const ACCESSORY: isize = 1;

/// Long enough for AppKit's redraw at the end of this run-loop turn, short
/// enough not to show.
const REDRAW_AFTER: std::time::Duration = std::time::Duration::from_millis(20);

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    static kCAGravityTop: id;
}

/// AppKit window and app state change only on the main thread: debug
/// builds check it.
fn debug_assert_main_thread() {
    // SAFETY: `+[NSThread isMainThread]` is safe on any thread.
    let main: BOOL = unsafe { msg_send![class!(NSThread), isMainThread] };
    debug_assert!(main != objc::runtime::NO, "AppKit call off the main thread");
}

/// GPUI's view behind `window`, or nil.
fn native_view(window: &gpui::Window) -> id {
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return nil;
    };
    let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return nil;
    };
    handle.ns_view.as_ptr().cast()
}

/// The `NSWindow` behind `window`, or nil.
fn native_window(window: &gpui::Window) -> id {
    let view = native_view(window);
    if view.is_null() {
        return nil;
    }
    // SAFETY: the handle's view belongs to a live window on this thread.
    unsafe { msg_send![view, window] }
}

/// Hide `window` (`orderOut:`) without closing it; `Window::activate_window`
/// shows it again. Main thread only.
pub fn hide_window(window: &gpui::Window) {
    debug_assert_main_thread();
    let native_window = native_window(window);
    // SAFETY: main thread; checked for nil.
    unsafe {
        if !native_window.is_null() {
            let _: () = msg_send![native_window, orderOut: nil];
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
    // SAFETY: main thread; a live window, its screen checked for nil.
    unsafe {
        let frame = NSWindow::frame(native_window);
        let content = native_window.contentRectForFrameRect_(frame);
        let chrome = frame.size.height - content.size.height;
        let top = frame.origin.y + frame.size.height;
        let mut frame_height = height as f64 + chrome;
        let screen = native_window.screen();
        if !screen.is_null() {
            let visible = NSScreen::visibleFrame(screen);
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
        native_window.setFrame_display_(resized, NO);
        let content = native_window.contentRectForFrameRect_(resized);
        let _: () = msg_send![view, setFrameOrigin: NSPoint::new(0., 0.)];
        let _: () = msg_send![view, setFrameSize: content.size];
        let _: () = msg_send![view, setAutoresizingMask: mask];
        // A picture of the old height stays pinned to the top, not
        // stretched or squashed, until the next frame replaces it.
        let layer: id = msg_send![view, layer];
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
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let policy = if visible { REGULAR } else { ACCESSORY };
        let changed: BOOL = msg_send![app, setActivationPolicy: policy];
        debug_assert!(changed != objc::runtime::NO, "activation policy refused");
    }
}
