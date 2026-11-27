//! `mdoc --mdoc-companion-probe` (debug builds, Windows; item 17): the
//! native check of the companion window's focus and capture exclusion,
//! run from a terminal with another app in front and `MDOC_DEV_PROFILE`
//! set to a new folder. It opens a window the way the companion opens
//! ([`super::open`]), clicks it with real input (`SendInput`; the pointer
//! goes back where it was), closes it and opens it again, as the tray
//! icon toggles it, and after each step asks Windows:
//!
//! - the foreground window and its keyboard focus are as before, and not
//!   mdoc's; the window is not mdoc's active window;
//! - it is visible, a topmost tool window that never activates
//!   (`WS_EX_NOACTIVATE`), excluded from capture (`WDA_EXCLUDEFROMCAPTURE`),
//!   and answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` (hover
//!   activation);
//! - it is the window at its own centre, above the other app;
//! - a screen capture (GDI) of it shows almost none of its colour, while a
//!   control capture with the exclusion lifted for a moment shows it;
//! - the click reached GPUI;
//! - the tray icon was added and the shell places it.
//!
//! It prints one line per step and exits 1 if any check fails. What only
//! a person can see stays a GUI row: other capture paths (Snipping Tool,
//! a meeting's screen share), hover activation turned on, typing in the
//! other app, and the native tray menu.
#![allow(unsafe_code)] // Win32 calls, each under a `SAFETY:` note.

use gpui::{
    App, AppContext as _, Bounds, Context, MouseButton, Render, Window, WindowHandle, div, point,
    prelude::*, px, size,
};
use std::{cell::Cell, ptr::null_mut, rc::Rc, time::Duration};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
        CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetActiveWindow, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEINPUT, SendInput,
        },
        WindowsAndMessaging::{
            GUITHREADINFO, GWL_EXSTYLE, GetClientRect, GetCursorPos, GetForegroundWindow,
            GetGUIThreadInfo, GetWindowDisplayAffinity, GetWindowLongPtrW,
            GetWindowThreadProcessId, HTCLIENT, IsWindowVisible, MA_NOACTIVATE, SendMessageW,
            SetCursorPos, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
            WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
            WindowFromPoint,
        },
    },
};

/// How long Windows gets to settle after each step.
const SETTLE: Duration = Duration::from_millis(500);
/// The probe's colour, counted in captures.
const COLOUR: u32 = 0xff00ff;

/// The other app, as it was before the window opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Front {
    window: usize,
    focus: usize,
    process: u32,
}

fn front() -> Front {
    // SAFETY: plain queries; `process` and `info` outlive their calls.
    unsafe {
        let window = GetForegroundWindow();
        let mut process = 0;
        let thread = GetWindowThreadProcessId(window, &mut process);
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let focus = if GetGUIThreadInfo(thread, &mut info) != 0 {
            info.hwndFocus as usize
        } else {
            0
        };
        Front {
            window: window as usize,
            focus,
            process,
        }
    }
}

/// The companion as Windows sees it.
#[derive(Clone, Copy, Debug)]
struct Native {
    visible: bool,
    exstyle: u32,
    affinity: u32,
    subclassed: bool,
    /// What it answers `WM_MOUSEACTIVATE` with.
    mouse_activate: isize,
    /// The window at its centre is it.
    on_top: bool,
    /// mdoc's active window is it.
    active: bool,
    front: Front,
    /// Its share of the probe's colour in a capture.
    captured: f32,
}

/// Its client area on screen.
fn screen_rect(hwnd: HWND) -> Option<RECT> {
    let mut client = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: a live window; both outlive the calls.
    let ok =
        unsafe { GetClientRect(hwnd, &mut client) != 0 && ClientToScreen(hwnd, &mut origin) != 0 };
    ok.then_some(RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + client.right,
        bottom: origin.y + client.bottom,
    })
}

/// The share of `rect`'s pixels on screen that are the probe's colour, as
/// a GDI screen capture sees them.
fn captured(rect: RECT) -> f32 {
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return 0.;
    }
    let mut pixels = vec![0u32; (width * height) as usize];
    // SAFETY: the screen DC and the objects made here are released below;
    // `pixels` holds a 32-bit top-down bitmap of `width`×`height`.
    unsafe {
        let screen = GetDC(null_mut());
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap);
        BitBlt(
            memory,
            0,
            0,
            width,
            height,
            screen,
            rect.left,
            rect.top,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(memory, previous);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(null_mut(), screen);
    }
    let close = |pixel: u32| {
        let channel = |shift: u32| ((pixel >> shift) & 0xff) as i32;
        let target = |shift: u32| ((COLOUR >> shift) & 0xff) as i32;
        [0, 8, 16]
            .into_iter()
            .all(|shift| (channel(shift) - target(shift)).abs() < 48)
    };
    pixels.iter().filter(|&&pixel| close(pixel)).count() as f32 / pixels.len() as f32
}

fn inspect(hwnd: HWND) -> Native {
    let rect = screen_rect(hwnd).unwrap_or_default();
    let centre = POINT {
        x: (rect.left + rect.right) / 2,
        y: (rect.top + rect.bottom) / 2,
    };
    let mut affinity = 0;
    // SAFETY: a live window of this thread; `affinity` outlives its call.
    // The message goes straight to the window's procedure, which answers
    // it before GPUI sees it.
    unsafe {
        GetWindowDisplayAffinity(hwnd, &mut affinity);
        Native {
            visible: IsWindowVisible(hwnd) != 0,
            exstyle: GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32,
            affinity,
            subclassed: super::windows::subclassed(hwnd),
            mouse_activate: SendMessageW(
                hwnd,
                WM_MOUSEACTIVATE,
                GetForegroundWindow() as usize,
                (HTCLIENT | (WM_LBUTTONDOWN << 16)) as isize,
            ),
            on_top: WindowFromPoint(centre) == hwnd,
            active: GetActiveWindow() == hwnd,
            front: front(),
            captured: captured(rect),
        }
    }
}

/// A real left click at the window's centre; the pointer goes back.
fn click(hwnd: HWND) {
    let Some(rect) = screen_rect(hwnd) else {
        return;
    };
    let mut before = POINT::default();
    let input = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [input(MOUSEEVENTF_LEFTDOWN), input(MOUSEEVENTF_LEFTUP)];
    // SAFETY: `before` and `inputs` outlive their calls.
    unsafe {
        GetCursorPos(&mut before);
        SetCursorPos((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        );
    }
    // Back once the click is delivered, at the next step.
    CURSOR.set(Some(before));
}

thread_local! {
    static CURSOR: Cell<Option<POINT>> = const { Cell::new(None) };
}

fn restore_cursor() {
    if let Some(point) = CURSOR.take() {
        // SAFETY: no state is needed.
        unsafe { SetCursorPos(point.x, point.y) };
    }
}

/// A stand-in for the companion's content, in the probe's colour, that
/// counts the clicks GPUI delivers to it.
struct Probe {
    clicks: Rc<Cell<u32>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .size_full()
            .bg(gpui::rgb(COLOUR))
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                clicks.set(clicks.get() + 1)
            })
    }
}

/// One step's checks: each failure in words.
fn failures(native: Option<Native>, before: Front, own: u32) -> Vec<String> {
    let Some(native) = native else {
        return vec!["no native window".into()];
    };
    let has = |bit: u32| native.exstyle & bit == bit;
    let checks = [
        (native.visible, "not visible"),
        (has(WS_EX_NOACTIVATE), "no WS_EX_NOACTIVATE"),
        (
            has(WS_EX_TOOLWINDOW),
            "not a tool window (taskbar, Alt+Tab)",
        ),
        (has(WS_EX_TOPMOST), "not topmost"),
        (
            native.affinity == WDA_EXCLUDEFROMCAPTURE,
            "not excluded from capture",
        ),
        (native.subclassed, "not subclassed"),
        (
            native.mouse_activate == MA_NOACTIVATE as isize,
            "WM_MOUSEACTIVATE would activate it",
        ),
        (native.on_top, "not above the other app"),
        (!native.active, "it became mdoc's active window"),
        (
            native.front == before && native.front.process != own,
            "the foreground window or its keyboard focus changed",
        ),
        (native.captured < 0.01, "a screen capture shows it"),
    ];
    (checks.into_iter())
        .filter(|(ok, _)| !ok)
        .map(|(_, what)| what.to_string())
        .collect()
}

fn open_probe(cx: &mut App, clicks: Rc<Cell<u32>>) -> Option<WindowHandle<Probe>> {
    let display = cx.primary_display()?;
    let work = display.visible_bounds();
    let bounds = Bounds {
        origin: point(
            work.origin.x + work.size.width - px(380.),
            work.origin.y + work.size.height - px(220.),
        ),
        size: size(px(360.), px(200.)),
    };
    super::open(bounds, display.id(), cx, |_, cx| {
        cx.new(|_| Probe { clicks })
    })
    .ok()
}

fn hwnd_of(window: WindowHandle<Probe>, cx: &mut gpui::AsyncApp) -> HWND {
    window
        .update(cx, |_, window, _| super::windows::hwnd(window) as usize)
        .unwrap_or(0) as HWND
}

/// Run the probe; the process exits with its result.
pub(crate) fn run() -> ! {
    let own = std::process::id();
    gpui_platform::application().run(move |cx: &mut App| {
        let before = front();
        if before.process == own {
            println!(
                "note: mdoc was in front before the window opened; run the probe from a \
                 terminal with another app in front"
            );
        }
        // Its one window closes on the way to reopening: that is no quit.
        cx.set_quit_mode(gpui::QuitMode::Explicit);
        crate::shelf::tray::show(cx);
        let clicks = Rc::new(Cell::new(0));
        let Some(window) = open_probe(cx, clicks.clone()) else {
            eprintln!("companion probe: the window did not open");
            std::process::exit(1);
        };
        cx.spawn(async move |cx| {
            let mut failed = false;
            let mut report = |step: &str, native: Option<Native>, extra: Vec<&str>| {
                let mut found = failures(native, before, own);
                found.extend(extra.into_iter().map(str::to_string));
                failed |= !found.is_empty();
                let verdict = match found.is_empty() {
                    true => "ok".to_string(),
                    false => format!("FAILED {found:?}"),
                };
                println!("{step}: {verdict} {native:?}");
            };
            let settle = cx.background_executor().clone();
            settle.timer(SETTLE).await;
            let hwnd = hwnd_of(window, cx);
            let opened = (!hwnd.is_null()).then(|| inspect(hwnd));
            // The control: with the exclusion lifted, the capture sees it.
            // SAFETY: the live window, on this thread.
            unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) };
            settle.timer(SETTLE).await;
            let control = screen_rect(hwnd).map(captured).unwrap_or(0.);
            // SAFETY: as above.
            unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
            settle.timer(SETTLE).await;
            println!(
                "control capture without the exclusion: {:.1} %",
                control * 100.
            );
            report(
                "opened",
                opened,
                (control < 0.5)
                    .then_some("the control capture missed it: the capture check proves nothing")
                    .into_iter()
                    .collect(),
            );
            click(hwnd);
            settle.timer(SETTLE).await;
            restore_cursor();
            report(
                "clicked",
                Some(inspect(hwnd)),
                (clicks.get() == 0)
                    .then_some("the click never reached GPUI")
                    .into_iter()
                    .collect(),
            );
            // Closed and opened again, as the tray icon toggles it.
            let _ = window.update(cx, |_, window, _| window.remove_window());
            settle.timer(SETTLE).await;
            let reopened = cx.update(|cx| open_probe(cx, clicks.clone()));
            settle.timer(SETTLE).await;
            let reopened = reopened
                .map(|window| hwnd_of(window, cx))
                .filter(|hwnd| !hwnd.is_null())
                .map(inspect);
            report("reopened", reopened, Vec::new());
            let icon = crate::shelf::tray::icon_status();
            println!("tray icon (added, placed): {icon:?}");
            if icon != Some((true, true)) {
                println!("tray: FAILED");
                failed = true;
            }
            cx.update(|_| crate::shelf::tray::remove());
            std::process::exit(i32::from(failed));
        })
        .detach();
    });
    std::process::exit(1)
}
