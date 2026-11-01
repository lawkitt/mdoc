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
mod menu_bar;
mod menus;
mod models;
#[cfg(test)]
mod perf_tests;
mod pii;
mod preview;
mod restored_clipboard;
mod search_session;
mod self_test;
mod settings_ui;
mod shelf;
mod shelf_ui;
mod shortcuts;
mod single_instance;
mod spelling;
mod style;
#[cfg(any(target_os = "macos", test))]
mod system_quit;
#[cfg(test)]
mod test_support;
mod ui;
mod updater;
mod workspace;

use document_view::bind_markdown_search_keys;
use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use mdoc_import::docx_preview;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

fn main() {
    // The pinned engine chooses providers through this variable. Set it once,
    // before starting GPUI, executors or native worker threads; never mutate it
    // while the application is running. Experimental inference is CPU only.
    unsafe {
        std::env::set_var("GLINER2_DEVICE", "cpu");
    }
    let args = std::env::args_os().collect::<Vec<_>>();
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
    // Launching mdoc again while its window is hidden (ADR 0084) shows it.
    application.on_reopen(|cx| menu_bar::reveal(cx, |_, _, _| {}));
    // macOS: a quit from the Dock, an Apple Event or logout asks first.
    #[cfg(target_os = "macos")]
    let quit_gate = {
        let gate = system_quit::QuitGate::new(gpui_macos::reply_to_should_terminate);
        system_quit::install(&gate, open_context.clone());
        gate
    };
    application.run(move |cx: &mut App| {
        cx.on_app_quit(|cx| {
            mdoc_import::model_work::shutdown();
            // Shelf drags' temporary exports live until quit (ADR 0084).
            shelf_ui::clear_exports(cx);
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
        #[cfg(target_os = "macos")]
        cx.set_global(quit_gate.clone());
        updater::init(cx);
        bind_all_keys(cx);
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
        let (menu_bar, note_shown) = handle
            .read(cx)
            .map(|workspace| {
                (
                    workspace.menu_bar_setting(),
                    workspace.menu_bar_note_shown(),
                )
            })
            .unwrap_or_default();
        menu_bar::install(handle, menu_bar, note_shown, cx);
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
        cx.on_window_closed(move |cx, _| {
            // The shelf or a comparison window may outlive the workspace's:
            // mdoc quits once the workspace window is gone.
            let workspace_open = cx
                .windows()
                .iter()
                .any(|window| window.downcast::<workspace::Workspace>().is_some());
            if !workspace_open {
                menu_bar::close_shelf(cx);
                // A system quit waiting on the quit flow terminates now.
                #[cfg(target_os = "macos")]
                if quit_gate.last_window_closed() {
                    return;
                }
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
                workspace.open_paths_from_os(paths, window, cx)
            });
        })
        .detach();
}

/// Window-wide shortcuts: files, tabs, zoom and spelling, from the
/// shortcuts table (ADR 0073).
pub(crate) fn bind_app_keys(cx: &mut App) {
    cx.bind_keys(shortcuts::bindings(shortcuts::Owner::App));
}

/// Every key binding, in the order GPUI's tie-breaks rely on: between
/// equally deep bindings the later one wins (so the find fields' ⌘↩ beats
/// the global Apply all).
pub(crate) fn bind_all_keys(cx: &mut App) {
    shortcuts::bind_crate(
        cx,
        shortcuts::Owner::Editor,
        mdoc_editor::key_bindings(cfg!(target_os = "macos")),
    );
    markdown_search::bind_keys(cx);
    pii::ui::bind_keys(cx);
    workspace::bind_case_keys(cx);
    shelf_ui::bind_keys(cx);
    workspace::bind_shell_keys(cx);
    settings_ui::bind_keys(cx);
    ui::bind_keys(cx);
    shortcuts::bind_crate(
        cx,
        shortcuts::Owner::Pdf,
        gpui_pdf::key_bindings(cfg!(target_os = "macos")),
    );
    comparison_ui::bind_keys(cx);
    bind_markdown_search_keys(cx);
    bind_app_keys(cx);
    confirm_dialog::bind_keys(cx);
}
