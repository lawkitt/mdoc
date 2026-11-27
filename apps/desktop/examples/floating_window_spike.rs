//! The Windows floating-window spike (Milestone 2 item 4, #1164 decision 18):
//! can GPUI show a small topmost window that takes no keyboard focus, toggled
//! by a tray icon and a system-wide hotkey, and left out of screen capture?
//!
//! Usage: cargo run --example floating_window_spike -- [--control]
//! [--no-exclude] [--at X,Y] [--exit-after SECONDS]
//!
//! It opens a magenta 320x180 GPUI `WindowKind::PopUp` window at X,Y
//! (logical pixels, default 200,200) with `focus: false`, then through the
//! raw HWND adds `WS_EX_NOACTIVATE`, `HWND_TOPMOST` and
//! `WDA_EXCLUDEFROMCAPTURE`, and answers `WM_MOUSEACTIVATE` with
//! `MA_NOACTIVATE` for hover activation. A hidden window of its own registers
//! Ctrl+Alt+Shift+M and a tray icon; both toggle the spike without
//! activating it. `--control` opens a normal window that is activated, as the
//! main window is; `--no-exclude` skips the capture exclusion. Each step
//! prints one line on stdout for the measuring script; it quits after 60 s by
//! default. Shipping code reaches none of this.

fn main() {
    #[cfg(windows)]
    spike::run();
    #[cfg(not(windows))]
    eprintln!("floating_window_spike runs on Windows only");
}

#[cfg(windows)]
// Win32 calls on the spike's own windows, each under a `SAFETY:` note.
#[allow(unsafe_code)]
mod spike {
    use std::{
        ptr::null_mut,
        sync::atomic::{AtomicBool, AtomicIsize, Ordering},
        time::{Duration, Instant},
    };

    use gpui::{
        App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, MouseButton,
        ParentElement as _, Render, Styled as _, TitlebarOptions, Window, WindowBounds, WindowKind,
        WindowOptions, div, point, px, rgb, size,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                Input::KeyboardAndMouse::{
                    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
                },
                Shell::{
                    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETFOCUS,
                    NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect,
                    Shell_NotifyIconW,
                },
                WindowsAndMessaging::{
                    AppendMenuW, CallWindowProcW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                    DestroyMenu, DestroyWindow, GWL_EXSTYLE, GWLP_WNDPROC, GetCursorPos,
                    GetWindowDisplayAffinity, GetWindowLongPtrW, HWND_TOPMOST, IDI_APPLICATION,
                    IsWindowVisible, LoadIconW, MA_NOACTIVATE, MF_STRING, PostMessageW,
                    RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
                    SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowDisplayAffinity,
                    SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_RIGHTBUTTON, TrackPopupMenu,
                    WDA_EXCLUDEFROMCAPTURE, WM_APP, WM_COMMAND, WM_HOTKEY, WM_LBUTTONUP,
                    WM_MOUSEACTIVATE, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WNDPROC, WS_EX_NOACTIVATE,
                    WS_OVERLAPPED,
                },
            },
        },
        w,
    };

    const HOTKEY_ID: i32 = 1;
    const TRAY_ID: u32 = 1;
    const WM_TRAY: u32 = WM_APP + 1;
    const MENU_TOGGLE: usize = 1;
    const MENU_QUIT: usize = 2;
    /// Ctrl+Alt+Shift+M: a combination no app on the test PC uses.
    const HOTKEY_VK: u32 = 0x4D;

    /// The floating GPUI window, toggled from the hidden window's procedure.
    static SPIKE: AtomicIsize = AtomicIsize::new(0);
    /// The hidden window that owns the hotkey and the tray icon.
    static OWNER: AtomicIsize = AtomicIsize::new(0);
    /// GPUI's window procedure, which `spike_proc` forwards to.
    static GPUI_PROC: AtomicIsize = AtomicIsize::new(0);
    static QUIT: AtomicBool = AtomicBool::new(false);

    struct Options {
        control: bool,
        exclude: bool,
        at: (f32, f32),
        exit_after: Duration,
    }

    impl Options {
        fn parse() -> Self {
            let mut options = Self {
                control: false,
                exclude: true,
                at: (200., 200.),
                exit_after: Duration::from_secs(60),
            };
            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--control" => options.control = true,
                    "--no-exclude" => options.exclude = false,
                    "--at" => {
                        let value = args.next().unwrap_or_default();
                        if let Some((x, y)) = value.split_once(',')
                            && let (Ok(x), Ok(y)) = (x.parse(), y.parse())
                        {
                            options.at = (x, y);
                        }
                    }
                    "--exit-after" => {
                        if let Some(seconds) = args.next().and_then(|s| s.parse().ok()) {
                            options.exit_after = Duration::from_secs(seconds);
                        }
                    }
                    other => eprintln!("ignored argument {other}"),
                }
            }
            options
        }
    }

    struct Spike;

    impl Render for Spike {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_4()
                // Clicks must reach GPUI although the window never activates.
                .on_mouse_down(MouseButton::Left, |_, _, _| report("mouse-down", ""))
                .bg(rgb(0xff00ff))
                .text_color(rgb(0x000000))
                .child("mdoc floating spike")
        }
    }

    fn report(step: &str, detail: impl std::fmt::Display) {
        println!("{step} {detail}");
    }

    fn hwnd_of(window: &Window) -> HWND {
        // `Window` has an inherent `window_handle`: GPUI's own handle.
        match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
            _ => null_mut(),
        }
    }

    pub fn run() {
        let options = Options::parse();
        gpui_platform::application().run(move |cx: &mut App| {
            let (x, y) = options.at;
            let bounds = Bounds::new(point(px(x), px(y)), size(px(320.), px(180.)));
            let window_options = if options.control {
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("FloatSpikeControl".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            } else {
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    focus: false,
                    show: true,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    ..Default::default()
                }
            };
            let control = options.control;
            let exclude = options.exclude;
            let opened = cx.open_window(window_options, move |window, cx| {
                let hwnd = hwnd_of(window);
                SPIKE.store(hwnd as isize, Ordering::SeqCst);
                report("hwnd", format_args!("{:#x}", hwnd as isize));
                if control {
                    // What the main window does when it comes forward.
                    window.activate_window();
                } else {
                    float(hwnd, exclude);
                }
                cx.new(|_| Spike)
            });
            if let Err(error) = opened {
                report("open-failed", error);
                cx.quit();
                return;
            }
            if !control {
                install_owner();
            }
            let deadline = Instant::now() + options.exit_after;
            cx.spawn(async move |cx| {
                while !QUIT.load(Ordering::SeqCst) && Instant::now() < deadline {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
                remove_owner();
                report("quit", "");
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    }

    /// No activation on show or click, above every normal window, and left
    /// out of screen capture.
    fn float(hwnd: HWND, exclude: bool) {
        // SAFETY: `hwnd` is the live GPUI window created on this thread.
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        // SAFETY: as above; only an extended-style bit is added.
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize) };
        // Hover activation ("Activate a window by hovering over it") ignores
        // WS_EX_NOACTIVATE and asks WM_MOUSEACTIVATE, which GPUI answers with
        // MA_ACTIVATE, so `spike_proc` answers it first.
        let spike_proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            spike_proc;
        // SAFETY: as above; `spike_proc` forwards every other message to the
        // procedure it replaces, stored before any message can arrive.
        unsafe {
            let previous = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            GPUI_PROC.store(previous, Ordering::SeqCst);
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, spike_proc as usize as isize);
        }
        // SAFETY: as above; no move, size or activation.
        let topmost = unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        // SAFETY: as above.
        let after = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        report(
            "exstyle",
            format_args!(
                "before={style:#x} after={after:#x} topmost-ok={}",
                topmost != 0
            ),
        );
        if exclude {
            // SAFETY: as above.
            let set = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
            let mut affinity = 0u32;
            // SAFETY: as above; `affinity` outlives the call.
            unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) };
            report(
                "affinity",
                format_args!("set-ok={} value={affinity:#x}", set != 0),
            );
        }
    }

    /// A hidden window on GPUI's thread: GPUI's message loop dispatches its
    /// hotkey and tray messages to `owner_proc`.
    fn install_owner() {
        // SAFETY: a null name asks for this executable's module.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = WNDCLASSW {
            lpfnWndProc: Some(owner_proc),
            hInstance: instance,
            lpszClassName: w!("MdocFloatSpikeOwner"),
            ..Default::default()
        };
        // SAFETY: `class` names a static string and a live procedure.
        unsafe { RegisterClassW(&class) };
        // SAFETY: the class was registered above; the window is never shown.
        let owner = unsafe {
            CreateWindowExW(
                0,
                w!("MdocFloatSpikeOwner"),
                w!("mdoc floating spike owner"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        OWNER.store(owner as isize, Ordering::SeqCst);
        report("owner", format_args!("{:#x}", owner as isize));

        // SAFETY: `owner` is the live window created above.
        let hotkey = unsafe {
            RegisterHotKey(
                owner,
                HOTKEY_ID,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                HOTKEY_VK,
            )
        };
        report("hotkey", format_args!("registered={}", hotkey != 0));

        let mut tip = [0u16; 128];
        for (slot, unit) in tip.iter_mut().zip("mdoc floating spike".encode_utf16()) {
            *slot = unit;
        }
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: owner,
            uID: TRAY_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            // SAFETY: a stock icon; no module is needed.
            hIcon: unsafe { LoadIconW(null_mut(), IDI_APPLICATION) },
            szTip: tip,
            ..Default::default()
        };
        // SAFETY: `data` is fully initialised and outlives the call.
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: owner,
            uID: TRAY_ID,
            ..Default::default()
        };
        let mut rect = RECT::default();
        // SAFETY: both pointers outlive the call.
        let located = unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) };
        report(
            "tray",
            format_args!(
                "added={} rect-hresult={located:#x} rect={},{},{},{}",
                added != 0,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom
            ),
        );
    }

    fn remove_owner() {
        let owner = OWNER.swap(0, Ordering::SeqCst) as HWND;
        if owner.is_null() {
            return;
        }
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: owner,
            uID: TRAY_ID,
            ..Default::default()
        };
        // SAFETY: `owner` is still alive; these undo `install_owner`.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &data);
            UnregisterHotKey(owner, HOTKEY_ID);
            DestroyWindow(owner);
        }
    }

    /// Shows the spike without activating it, or hides it.
    fn toggle(source: &str) {
        let hwnd = SPIKE.load(Ordering::SeqCst) as HWND;
        // SAFETY: the GPUI window lives until the app quits.
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        if visible {
            // SAFETY: as above.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        } else {
            // SAFETY: as above; neither call activates the window.
            unsafe {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        report(
            "toggle",
            format_args!("source={source} visible={}", !visible),
        );
    }

    /// The tray menu: the shell needs the owner in front while it tracks,
    /// and the notification area gets the focus back however it closes.
    fn show_menu(owner: HWND) {
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: owner,
            uID: TRAY_ID,
            ..Default::default()
        };
        // SAFETY: a new menu, destroyed below; `owner` is live; `data`
        // outlives the call that reads it.
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, MENU_TOGGLE, w!("Show or hide"));
            AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit"));
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor);
            SetForegroundWindow(owner);
            TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON,
                cursor.x,
                cursor.y,
                0,
                owner,
                std::ptr::null(),
            );
            PostMessageW(owner, WM_NULL, 0, 0);
            DestroyMenu(menu);
            Shell_NotifyIconW(NIM_SETFOCUS, &data);
        }
    }

    extern "system" fn spike_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_MOUSEACTIVATE {
            // The click still reaches GPUI; only the activation is refused.
            return MA_NOACTIVATE as LRESULT;
        }
        // SAFETY: `GPUI_PROC` holds the procedure `float` replaced, a valid
        // `WNDPROC` for this window for as long as the window lives.
        unsafe {
            let gpui: WNDPROC = std::mem::transmute(GPUI_PROC.load(Ordering::SeqCst));
            CallWindowProcW(gpui, hwnd, msg, wparam, lparam)
        }
    }

    extern "system" fn owner_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_HOTKEY if wparam as i32 == HOTKEY_ID => toggle("hotkey"),
            WM_TRAY => match (lparam & 0xffff) as u32 {
                WM_LBUTTONUP => toggle("tray-click"),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            },
            WM_COMMAND => match wparam & 0xffff {
                MENU_TOGGLE => toggle("tray-menu"),
                MENU_QUIT => QUIT.store(true, Ordering::SeqCst),
                _ => {}
            },
            // SAFETY: forwards this window's own message unchanged.
            _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
        0
    }
}
