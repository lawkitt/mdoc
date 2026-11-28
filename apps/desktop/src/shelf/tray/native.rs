//! The tray icon's Win32 calls (Windows), per the item 4 spike (#1164):
//!
//! - **The owner.** A hidden top-level window of its own class, never
//!   shown, made on GPUI's thread. GPUI's message loop dispatches its
//!   messages here, and this module queues what they ask on GPUI's
//!   foreground executor: nothing here touches the app inside a message.
//!   It is not a message-only window, which would never receive the
//!   shell's `TaskbarCreated` broadcast.
//! - **The icon.** The exe's own (resource 1, `resources/icon.rc`), at the
//!   small icon size, added again when Explorer restarts and deleted at
//!   quit.
//! - **Clicks.** With `NOTIFYICON_VERSION_4`, a select (a click, or Enter
//!   or Space on the icon) toggles; the context menu (a right click,
//!   Shift+F10) offers Open mdoc, Settings and Quit.
//!
//! It also hides and shows the workspace window, which GPUI has no call
//! for on Windows.
#![allow(unsafe_code)] // Win32 calls, each under a `SAFETY:` note.

use super::SelectGuard;
use crate::workspace::Quit;
use gpui::{App, AsyncApp, Bounds, DisplayId, Pixels, Subscription, Window, point, px, size};
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::GetDoubleClickTime,
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETFOCUS,
                NIM_SETVERSION, NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
                NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
                DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, HICON, IDI_APPLICATION,
                IMAGE_ICON, IsWindowVisible, LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_SEPARATOR,
                MF_STRING, PostMessageW, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON,
                SM_CYSMICON, SW_HIDE, SW_SHOW, SetForegroundWindow, ShowWindow, TPM_NONOTIFY,
                TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU,
                WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
    w,
};

/// The icon's id under its owner.
const ICON_ID: u32 = 1;
/// The icon's callback message.
const WM_TRAY: u32 = WM_APP + 17;
/// `NIN_KEYSELECT`: Enter or Space on the icon (`NIN_SELECT | NINF_KEY`).
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const OPEN_MDOC: usize = 1;
const SETTINGS: usize = 2;
const QUIT: usize = 3;
/// The tooltip, at most 127 UTF-16 units.
const TIP: &str = "mdoc";

/// The shell's "taskbar created" message, registered once.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Tray {
    owner: HWND,
    icon: HICON,
    /// The icon is in the notification area now.
    added: bool,
    /// The shell took `NOTIFYICON_VERSION_4`; else it sends the old
    /// mouse messages.
    version_4: bool,
    cx: AsyncApp,
    guard: SelectGuard,
    _quit: Subscription,
}

thread_local! {
    /// The icon, on GPUI's thread, while the item shows.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// The workspace window's HWND.
fn hwnd(window: &Window) -> HWND {
    crate::ai::companion::windows::hwnd(window)
}

/// Hide the workspace `window`: off the screen, the taskbar and Alt+Tab,
/// with everything in it kept.
pub(crate) fn hide_window(window: &Window) {
    let hwnd = hwnd(window);
    if !hwnd.is_null() {
        // SAFETY: GPUI's live window, on its thread.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

/// Show the workspace `window` if hidden; `activate_window` then brings it
/// forward.
pub(crate) fn show_window(window: &Window) {
    let hwnd = hwnd(window);
    // SAFETY: GPUI's live window, on its thread.
    if !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } == 0 {
        // SAFETY: as above.
        unsafe { ShowWindow(hwnd, SW_SHOW) };
    }
}

/// Show the icon, once.
pub(crate) fn show(cx: &mut App) {
    if TRAY.with_borrow(Option::is_some) {
        return;
    }
    let Some(owner) = create_owner() else {
        return;
    };
    // Explorer keeps an icon whose owner ended without deleting it until
    // the pointer passes over it.
    let quit = cx.on_app_quit(|_| {
        TRAY.with_borrow(|tray| {
            if let Some(tray) = tray {
                // SAFETY: the owner is alive; this undoes `add_icon`.
                unsafe { Shell_NotifyIconW(NIM_DELETE, &icon_data(tray.owner)) };
            }
        });
        async {}
    });
    let mut tray = Tray {
        owner,
        icon: load_icon(),
        added: false,
        version_4: false,
        cx: cx.to_async(),
        guard: SelectGuard::default(),
        _quit: quit,
    };
    // Before Explorer's notification area is up this fails, and
    // `TaskbarCreated` adds it later.
    (tray.added, tray.version_4) = add_icon(&tray);
    TRAY.set(Some(tray));
}

/// Whether the icon is in the notification area, so a hidden window can
/// come back through it.
pub(crate) fn usable() -> bool {
    TRAY.with_borrow(|tray| tray.as_ref().is_some_and(|tray| tray.added))
}

/// Remove the icon and its owner.
pub(crate) fn remove() {
    let Some(tray) = TRAY.take() else {
        return;
    };
    // SAFETY: the owner is alive until destroyed here; the icon was loaded
    // by `load_icon` and nothing else holds it.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &icon_data(tray.owner));
        DestroyWindow(tray.owner);
        DestroyIcon(tray.icon);
    }
}

/// The icon's display and bounds in that display's logical pixels: the
/// overflow's chevron while the icon is not pinned, and the pointer if the
/// shell cannot say.
pub(crate) fn anchor(cx: &App) -> Option<(DisplayId, Bounds<Pixels>)> {
    let owner = TRAY.with_borrow(|tray| tray.as_ref().map(|tray| tray.owner))?;
    let rect = icon_rect(owner).or_else(|| {
        let mut cursor = POINT::default();
        // SAFETY: `cursor` outlives the call.
        (unsafe { GetCursorPos(&mut cursor) } != 0).then_some(RECT {
            left: cursor.x,
            top: cursor.y,
            right: cursor.x + 1,
            bottom: cursor.y + 1,
        })
    })?;
    // SAFETY: `rect` outlives the call; the nearest monitor always exists.
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is sized and outlives the call.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    // GPUI's display id on Windows is the monitor's handle.
    let id = DisplayId::new(monitor as u64);
    let display = cx.find_display(id)?;
    let physical = (info.rcMonitor.right - info.rcMonitor.left) as f32;
    let scale = physical / display.bounds().size.width.as_f32();
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    let logical = |value: i32| px(value as f32 / scale);
    Some((
        id,
        Bounds {
            origin: point(logical(rect.left), logical(rect.top)),
            size: size(
                logical(rect.right - rect.left),
                logical(rect.bottom - rect.top),
            ),
        },
    ))
}

/// For the probe: whether the icon was added, and whether the shell
/// places it.
#[cfg(debug_assertions)]
pub(crate) fn icon_status() -> Option<(bool, bool)> {
    TRAY.with_borrow(|tray| {
        let tray = tray.as_ref()?;
        Some((tray.added, icon_rect(tray.owner).is_some()))
    })
}

fn icon_rect(owner: HWND) -> Option<RECT> {
    let identifier = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: owner,
        uID: ICON_ID,
        ..Default::default()
    };
    let mut rect = RECT::default();
    // SAFETY: both outlive the call.
    let found = unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) };
    (found >= 0 && rect.right > rect.left).then_some(rect)
}

fn create_owner() -> Option<HWND> {
    // SAFETY: a null name asks for this executable's module.
    let instance = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(owner_proc),
        hInstance: instance,
        lpszClassName: w!("MdocTray"),
        ..Default::default()
    };
    // SAFETY: a static class name and a live procedure; registering a
    // second time fails harmlessly (the icon came back after removal).
    unsafe { RegisterClassW(&class) };
    // SAFETY: a static, NUL-terminated name.
    let created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(created, Ordering::Relaxed);
    // SAFETY: the class is registered; the window is never shown.
    let owner = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("MdocTray"),
            w!("mdoc"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    (!owner.is_null()).then_some(owner)
}

/// The exe's icon at the small size, or the stock one.
fn load_icon() -> HICON {
    // SAFETY: resource 1 of this executable (`resources/icon.rc`), by
    // `MAKEINTRESOURCE`; the metrics need no state.
    let icon = unsafe {
        LoadImageW(
            GetModuleHandleW(null()),
            std::ptr::without_provenance(1),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
    };
    if icon.is_null() {
        // SAFETY: a stock icon; no module is needed.
        unsafe { LoadIconW(null_mut(), IDI_APPLICATION) }
    } else {
        icon
    }
}

fn icon_data(owner: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: owner,
        uID: ICON_ID,
        ..Default::default()
    }
}

/// Add the icon and ask for version 4 messages: whether it was added, and
/// whether the shell took version 4.
fn add_icon(tray: &Tray) -> (bool, bool) {
    let mut tip = [0u16; 128];
    for (slot, unit) in tip.iter_mut().zip(TIP.encode_utf16()) {
        *slot = unit;
    }
    let mut data = NOTIFYICONDATAW {
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        hIcon: tray.icon,
        szTip: tip,
        ..icon_data(tray.owner)
    };
    // SAFETY: `data` is fully initialised and outlives each call.
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
            return (false, false);
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        (true, Shell_NotifyIconW(NIM_SETVERSION, &data) != 0)
    }
}

/// Run `then` in the app, after the message that asked for it.
fn queue(then: impl FnOnce(&mut App) + 'static) {
    let Some(cx) = TRAY.with_borrow(|tray| tray.as_ref().map(|tray| tray.cx.clone())) else {
        return;
    };
    cx.foreground_executor()
        .clone()
        .spawn(async move {
            cx.update(then);
        })
        .detach();
}

fn selected() {
    // SAFETY: no state is needed.
    let within = Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() }));
    let accepted = TRAY.with_borrow_mut(|tray| {
        tray.as_mut()
            .is_some_and(|tray| tray.guard.accept(Instant::now(), within))
    });
    if accepted {
        queue(crate::shelf::menu_bar::item_clicked);
    }
}

/// Where the menu opens: at the pointer when it is on the icon (a right
/// click), else at the icon (Shift+F10), else at the pointer. The shell
/// gives no place with its context menu message.
fn menu_point(owner: HWND) -> POINT {
    let mut cursor = POINT::default();
    // SAFETY: `cursor` outlives the call.
    unsafe { GetCursorPos(&mut cursor) };
    match icon_rect(owner) {
        Some(rect)
            if !(rect.left..rect.right).contains(&cursor.x)
                || !(rect.top..rect.bottom).contains(&cursor.y) =>
        {
            POINT {
                x: (rect.left + rect.right) / 2,
                y: (rect.top + rect.bottom) / 2,
            }
        }
        _ => cursor,
    }
}

/// The icon's menu: the shell needs the owner in front while it tracks,
/// and the notification area gets the focus back however it closes.
fn show_menu(owner: HWND) {
    let at = menu_point(owner);
    // The rows' words, as `w!` would spell them: UTF-16 with a NUL.
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (open_mdoc, settings, quit) = (
        wide(crate::shelf::text::OPEN_MDOC),
        wide(crate::shelf::text::SETTINGS),
        wide(crate::shelf::text::QUIT),
    );
    // SAFETY: a new menu, destroyed below; `owner` is live, and the rows'
    // words outlive the calls that copy them.
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, OPEN_MDOC, open_mdoc.as_ptr());
        AppendMenuW(menu, MF_STRING, SETTINGS, settings.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, QUIT, quit.as_ptr());
        SetForegroundWindow(owner);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            0,
            owner,
            null(),
        );
        PostMessageW(owner, WM_NULL, 0, 0);
        DestroyMenu(menu);
        Shell_NotifyIconW(NIM_SETFOCUS, &icon_data(owner));
        chosen
    };
    match usize::try_from(chosen).unwrap_or(0) {
        OPEN_MDOC => queue(|cx| {
            crate::shelf::menu_bar::reveal(cx, |workspace, window, cx| {
                workspace.home_from_shelf(window, cx)
            })
        }),
        SETTINGS => queue(|cx| {
            crate::shelf::menu_bar::reveal(cx, |workspace, window, cx| {
                workspace.settings_from_shelf(window, cx)
            })
        }),
        // The quit flow asks about unsaved work, showing the window to.
        QUIT => queue(|cx| cx.dispatch_action(&Quit)),
        _ => {}
    }
}

/// Explorer restarted: the icon goes back. If it cannot, a window hidden
/// in the tray shows, as nothing else could bring it back.
fn taskbar_created() {
    let added = TRAY.with_borrow_mut(|tray| {
        let tray = tray.as_mut()?;
        (tray.added, tray.version_4) = add_icon(tray);
        Some(tray.added)
    });
    if added == Some(false) {
        queue(|cx| {
            if crate::shelf::menu_bar::window_hidden(cx) {
                crate::shelf::menu_bar::reveal(cx, |_, _, _| {});
            }
        });
    }
}

extern "system" fn owner_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_TRAY {
        let version_4 = TRAY.with_borrow(|tray| tray.as_ref().is_some_and(|tray| tray.version_4));
        // Version 4: the event in the low word; before it, the mouse
        // message itself.
        match ((lparam as u32) & 0xffff, version_4) {
            (NIN_SELECT | NIN_KEYSELECT, true) | (WM_LBUTTONUP, false) => selected(),
            (WM_CONTEXTMENU, true) | (WM_RBUTTONUP, false) => show_menu(hwnd),
            _ => {}
        }
        return 0;
    }
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        taskbar_created();
        return 0;
    }
    // SAFETY: forwards this window's own message unchanged.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
