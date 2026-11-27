//! The companion's Win32 calls (Windows), per the item 4 spike (#1164).
//! GPUI's `WindowKind::PopUp` is a topmost tool window (off the taskbar
//! and Alt+Tab), and `focus: false` shows it with `SW_SHOWNOACTIVATE`.
//! GPUI has no call for the rest, so `float` makes it, through the HWND
//! and before the first frame:
//!
//! - never activated by a click (`WS_EX_NOACTIVATE`), nor by hover
//!   activation, which asks `WM_MOUSEACTIVATE`: GPUI answers that with
//!   `MA_ACTIVATE`, so a subclass answers `MA_NOACTIVATE` first and passes
//!   every other message on;
//! - left out of screen capture (`WDA_EXCLUDEFROMCAPTURE`; Windows 10 2004
//!   and later, a blank box before that);
//! - placed in screen coordinates (GPUI places a new window in workspace
//!   coordinates, off by a top or left taskbar's thickness).
//!
//! GPUI cannot show a hidden window without activating it, so the
//! companion is never hidden: the tray closes it and opens a new one.
//! The probe (`probe_windows.rs`, debug builds) reads it all back.
#![allow(unsafe_code)] // Win32 calls, each under a `SAFETY:` note.

use gpui::{App, Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ptr::null_mut,
    sync::atomic::{AtomicIsize, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    },
    UI::WindowsAndMessaging::{
        CallWindowProcW, GWL_EXSTYLE, GWLP_WNDPROC, GetClientRect, GetWindowDisplayAffinity,
        GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IsWindow, MA_NOACTIVATE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowDisplayAffinity, SetWindowLongPtrW,
        SetWindowPos, WDA_EXCLUDEFROMCAPTURE, WM_MOUSEACTIVATE, WNDPROC, WS_EX_NOACTIVATE,
    },
};

/// GPUI's window procedure, which `companion_proc` passes messages to:
/// one for every GPUI window, as they share a class.
static GPUI_PROC: AtomicIsize = AtomicIsize::new(0);

/// The HWND behind `window`, or null.
pub(crate) fn hwnd(window: &Window) -> HWND {
    // `Window` has an inherent `window_handle`: GPUI's own handle.
    match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
        _ => null_mut(),
    }
}

/// Whether Windows left `window` out of capture: `float` asked, and an
/// older or refusing system may not have.
pub(crate) fn excluded(window: &Window) -> bool {
    let hwnd = hwnd(window);
    let mut affinity = 0;
    // SAFETY: a live window; `affinity` outlives the call.
    !hwnd.is_null()
        && unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) } != 0
        && affinity == WDA_EXCLUDEFROMCAPTURE
}

/// Whether `hwnd` answers through `companion_proc`.
#[cfg(debug_assertions)]
pub(crate) fn subclassed(hwnd: HWND) -> bool {
    // SAFETY: reads a live window's procedure.
    unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) == companion_proc as *const () as isize }
}

/// Float the new companion `window`, opened at `bounds`: never activated,
/// left out of capture, topmost, at `bounds` on screen. GPUI's thread,
/// inside the window's build.
pub(crate) fn float(window: &Window, bounds: Bounds<Pixels>) {
    let hwnd = hwnd(window);
    if hwnd.is_null() {
        return;
    }
    // SAFETY: `hwnd` is the live GPUI window, on its thread; only an
    // extended-style bit is added.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
    }
    subclass(hwnd);
    let scale = window.scale_factor();
    let mut client = POINT::default();
    let mut outer = RECT::default();
    // SAFETY: as above; both outlive their calls.
    let placed =
        unsafe { ClientToScreen(hwnd, &mut client) != 0 && GetWindowRect(hwnd, &mut outer) != 0 };
    let (dx, dy) = if placed {
        (
            (bounds.origin.x.as_f32() * scale).round() as i32 - client.x,
            (bounds.origin.y.as_f32() * scale).round() as i32 - client.y,
        )
    } else {
        (0, 0)
    };
    // SAFETY: as above; the style change applies with the frame, and
    // nothing activates.
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            outer.left + dx,
            outer.top + dy,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
}

/// Answer `WM_MOUSEACTIVATE` before GPUI does.
fn subclass(hwnd: HWND) {
    let ours = companion_proc as *const () as isize;
    // SAFETY: reads a live window's procedure.
    let current = unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) };
    if current == ours {
        return;
    }
    // Stored before the procedure is replaced, so the first message finds
    // it. Every GPUI window has the same one; any other stays as it is.
    let stored = match GPUI_PROC.compare_exchange(0, current, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => current,
        Err(stored) => stored,
    };
    if stored == current {
        // SAFETY: as above; `companion_proc` passes every other message to
        // the procedure it replaces.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours) };
    }
}

extern "system" fn companion_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        // The click still reaches GPUI; only the activation is refused.
        return MA_NOACTIVATE as LRESULT;
    }
    // SAFETY: `GPUI_PROC` holds the procedure `subclass` replaced, set
    // before it was, and valid for as long as GPUI's windows live.
    unsafe {
        let gpui: WNDPROC = std::mem::transmute(GPUI_PROC.load(Ordering::SeqCst));
        CallWindowProcW(gpui, hwnd, msg, wparam, lparam)
    }
}

/// Make the companion `window` `height` tall (logical pixels), keeping the
/// edge next to the taskbar where it is. The resize runs once the app is
/// free: Windows reports it at once, and GPUI must take it in.
pub(crate) fn set_height(window: &Window, height: f32, cx: &mut App) {
    let hwnd = hwnd(window);
    if hwnd.is_null() {
        return;
    }
    let client_height = (height * window.scale_factor()).round() as i32;
    cx.foreground_executor()
        .spawn(async move { resize(hwnd, client_height) })
        .detach();
}

/// Make `hwnd`'s client area `client_height` device pixels tall.
fn resize(hwnd: HWND, client_height: i32) {
    let mut outer = RECT::default();
    let mut client = RECT::default();
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `IsWindow` first, as the companion may have closed since;
    // the rest outlive their calls.
    let read = unsafe {
        IsWindow(hwnd) != 0
            && GetWindowRect(hwnd, &mut outer) != 0
            && GetClientRect(hwnd, &mut client) != 0
            && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) != 0
    };
    let delta = client_height - (client.bottom - client.top);
    if !read || delta == 0 {
        return;
    }
    let (top, bottom) = crate::shelf::tray::resized(
        outer.top,
        outer.bottom,
        info.rcWork.top,
        info.rcWork.bottom,
        delta,
    );
    // SAFETY: as above; no activation, no change of order.
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            outer.left,
            top,
            outer.right - outer.left,
            bottom - top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}
