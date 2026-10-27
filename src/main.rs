//! A file-based Markdown editor with side-by-side PDF and DOCX preview.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod comment_panel;
mod comparison;
mod comparison_ui;
mod confirm_dialog;
mod crash;
mod document_view;
mod images;
mod markdown_search;
mod menus;
mod models;
#[cfg(test)]
mod perf_tests;
mod pii;
mod preview;
mod search_session;
mod self_test;
mod settings_ui;
mod single_instance;
mod spelling;
mod style;
#[cfg(test)]
mod test_support;
mod ui;
mod updater;
mod workspace;

use document_view::{
    Save, SaveAs, ToggleCompare, ZoomIn, ZoomOut, ZoomReset, bind_markdown_search_keys,
};
use gpui::{App, Bounds, KeyBinding, WindowBounds, WindowOptions, prelude::*, px, size};
use mdoc_import::docx_preview;
use spelling::{NextMisspelling, PreviousMisspelling};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use workspace::{
    Close, Import, New, NextTab, Open, PasteAiResponse, PreviousTab, PseudonymizeClipboard, Quit,
    Settings,
};

fn main() {
    // The pinned engine chooses providers through this variable. Set it once,
    // before starting GPUI, executors or native worker threads; never mutate it
    // while the application is running. Experimental inference is CPU only.
    unsafe {
        std::env::set_var("GLINER2_DEVICE", "cpu");
    }
    let args = std::env::args_os().collect::<Vec<_>>();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--headless-encode-png")
    {
        #[cfg(all(debug_assertions, feature = "headless-renders", target_os = "windows"))]
        {
            let result = match args.as_slice() {
                [_, _, out] => {
                    workspace::headless_renders::run_png_worker(std::path::Path::new(out))
                }
                _ => Err("usage: mdoc --headless-encode-png <output-directory>".into()),
            };
            match result {
                Ok(()) => return,
                Err(error) => eprintln!("PNG worker failed: {error}"),
            }
        }
        #[cfg(not(all(debug_assertions, feature = "headless-renders", target_os = "windows")))]
        eprintln!("PNG workers require a Windows dev build with --features headless-renders");
        std::process::exit(1);
    }
    if args.get(1).is_some_and(|arg| arg == "--headless-renders") {
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        {
            let result = match args.as_slice() {
                [_, _, out] => workspace::headless_renders::run(std::path::Path::new(out)),
                _ => Err("usage: mdoc --headless-renders <new-output-directory>".into()),
            };
            match result {
                Ok(()) => return,
                Err(error) => eprintln!("headless renders failed: {error}"),
            }
        }
        #[cfg(not(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )))]
        eprintln!(
            "headless renders require a macOS or Windows dev build with --features headless-renders"
        );
        std::process::exit(1);
    }
    if args.get(1).is_some_and(|arg| arg == "--mdoc-docx-worker") {
        let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
            eprintln!("missing DOCX worker input or output");
            std::process::exit(2);
        };
        let output = PathBuf::from(output);
        if docx_preview::run_worker_reporting(&PathBuf::from(input), &output).is_err() {
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).is_some_and(|arg| arg == "--mdoc-self-test") {
        let (Some(pdf), Some(expected)) = (args.get(2), args.get(3)) else {
            eprintln!("usage: mdoc --mdoc-self-test <pdf> <expected.txt>");
            std::process::exit(2);
        };
        match self_test::run(std::path::Path::new(pdf), std::path::Path::new(expected)) {
            Ok(()) => println!("mdoc self-test passed"),
            Err(error) => {
                eprintln!("mdoc self-test failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    // `just smoke-update` (ADR 0040): update this copy in place, headless.
    if args.get(1).is_some_and(|arg| arg == "--mdoc-self-update") {
        match updater::self_update() {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("mdoc self-update failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let initial: Vec<_> = args.into_iter().skip(1).map(PathBuf::from).collect();
    // Windows: a second launch hands its paths to the running mdoc.
    let single_instance::Launch::Primary(later_launches) = single_instance::claim(&initial) else {
        return;
    };
    // The app itself, not its worker modes, records panics (ADR 0047).
    crash::install();
    let application = gpui_platform::application();
    let open_context = Rc::new(RefCell::new(
        None::<(gpui::WindowHandle<workspace::Workspace>, gpui::AsyncApp)>,
    ));
    let pending_url = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    application.on_open_urls({
        let open_context = open_context.clone();
        let pending_url = pending_url.clone();
        move |urls| {
            let paths = urls
                .iter()
                .filter_map(|url| url::Url::parse(url).ok()?.to_file_path().ok())
                .collect();
            if let Some((window, cx)) = open_context.borrow_mut().as_mut() {
                open_paths_from_os(paths, *window, cx);
            } else {
                pending_url.borrow_mut().extend(paths);
            }
        }
    });
    application.run(move |cx: &mut App| {
        cx.on_app_quit(|cx| {
            mdoc_import::model_work::shutdown();
            // Blocking on purpose: GPUI waits only briefly for async quit
            // work, and a staged update must be swapped in before exit.
            updater::apply_at_quit(cx);
            let executor = cx.background_executor().clone();
            async move {
                executor
                    .spawn(async {
                        docx_preview::shutdown_workers();
                    })
                    .await
            }
        })
        .detach();
        updater::init(cx);
        mdoc_editor::bind_keys(cx);
        markdown_search::bind_keys(cx);
        pii::ui::bind_keys(cx);
        workspace::bind_case_keys(cx);
        workspace::bind_shell_keys(cx);
        settings_ui::bind_keys(cx);
        ui::bind_keys(cx);
        gpui_pdf::bind_keys(cx);
        cx.bind_keys([KeyBinding::new(
            "escape",
            comparison_ui::CloseComparison,
            Some("ModelComparison"),
        )]);
        bind_markdown_search_keys(cx);
        bind_app_keys(cx);
        workspace::bind_app_actions(cx);
        menus::install(cx);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // One unified row for traffic lights, sidebar toggle and the
                    // document header (ADR 0048). Linux asks for client-side
                    // decorations and falls back to the compositor's title bar.
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("mdoc".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(
                            px(style::TRAFFIC_LIGHT_X),
                            px(style::TRAFFIC_LIGHT_Y),
                        )),
                    }),
                    window_decorations: Some(gpui::WindowDecorations::Client),
                    window_min_size: Some(size(px(720.), px(480.))),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let mut workspace = workspace::Workspace::new(window, cx);
                        workspace.install_prompts(cx);
                        workspace.open_paths(initial, window, cx);
                        workspace
                    })
                },
            )
            .expect("Could not open the editor window");
        let pending = std::mem::take(&mut *pending_url.borrow_mut());
        if !pending.is_empty() {
            let _ = handle.update(cx, |workspace, window, cx| {
                workspace.open_paths(pending, window, cx)
            });
        }
        let _ = handle.update(cx, |workspace, _, cx| {
            workspace.check_for_updates_at_launch(cx)
        });
        *open_context.borrow_mut() = Some((handle, cx.to_async()));
        if let Some(launches) = later_launches {
            // Polled: the listener is a plain thread, and a launch is rare.
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(200))
                        .await;
                    while let Ok(paths) = launches.try_recv() {
                        let _ = handle.update(cx, |workspace, window, cx| {
                            window.activate_window();
                            workspace.open_paths(paths, window, cx)
                        });
                    }
                }
            })
            .detach();
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                // Restart now (ADR 0040): relaunch after the swap at quit.
                if updater::restart_requested(cx) {
                    cx.restart();
                } else {
                    cx.quit();
                }
            }
        })
        .detach();
        cx.activate(true);
    });
}

fn open_paths_from_os(
    paths: Vec<PathBuf>,
    handle: gpui::WindowHandle<workspace::Workspace>,
    cx: &mut gpui::AsyncApp,
) {
    // A native callback can re-enter GPUI while the app is borrowed. Queue
    // the paths on its foreground executor so that update cannot drop them.
    let mut cx = cx.clone();
    cx.foreground_executor()
        .clone()
        .spawn(async move {
            let _ = handle.update(&mut cx, |workspace, window, cx| {
                workspace.open_paths(paths, window, cx)
            });
        })
        .detach();
}

/// Window-wide shortcuts: files, tabs, zoom and spelling.
pub(crate) fn bind_app_keys(cx: &mut App) {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        // Chrome falls back to the active document; text fields keep their keys.
        KeyBinding::new(
            &format!("{modifier}-z"),
            mdoc_editor::Undo,
            Some("Workspace && !Editor && !TextInput"),
        ),
        KeyBinding::new(
            &format!("{modifier}-shift-z"),
            mdoc_editor::Redo,
            Some("Workspace && !Editor && !TextInput"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-y",
            mdoc_editor::Redo,
            Some("Workspace && !Editor && !TextInput"),
        ),
        KeyBinding::new(&format!("{modifier}-,"), Settings, None),
        KeyBinding::new(&format!("{modifier}-n"), New, None),
        KeyBinding::new(&format!("{modifier}-o"), Open, None),
        KeyBinding::new(&format!("{modifier}-shift-i"), Import, None),
        KeyBinding::new(&format!("{modifier}-s"), Save, None),
        KeyBinding::new(&format!("{modifier}-shift-s"), SaveAs, None),
        KeyBinding::new(&format!("{modifier}-alt-shift-v"), PasteAiResponse, None),
        KeyBinding::new(&format!("{modifier}-\\"), ToggleCompare, None),
        KeyBinding::new(
            &format!("{modifier}-alt-shift-c"),
            PseudonymizeClipboard,
            None,
        ),
        KeyBinding::new(&format!("{modifier}-w"), Close, None),
        KeyBinding::new(&format!("{modifier}-="), ZoomIn, None),
        KeyBinding::new(&format!("{modifier}-+"), ZoomIn, None),
        // Layouts where plus needs Shift (US: Shift + =) report the Shift.
        KeyBinding::new(&format!("{modifier}-shift-="), ZoomIn, None),
        KeyBinding::new(&format!("{modifier}-shift-+"), ZoomIn, None),
        KeyBinding::new(&format!("{modifier}--"), ZoomOut, None),
        KeyBinding::new(&format!("{modifier}-0"), ZoomReset, None),
        KeyBinding::new(&format!("{modifier}-q"), Quit, None),
        KeyBinding::new(&format!("{modifier}-;"), NextMisspelling, None),
        KeyBinding::new(&format!("{modifier}-shift-;"), PreviousMisspelling, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        // The macOS convention for View › Toggle Sidebar.
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-cmd-s", workspace::ToggleSidebar, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
    ]);
}
