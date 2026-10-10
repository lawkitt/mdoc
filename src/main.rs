//! A file-based Markdown editor with side-by-side PDF and DOCX preview.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod comment_panel;
mod document;
mod document_session;
mod document_view;
mod docx_preview;
mod images;
mod import;
mod import_session;
mod markdown_search;
mod search_session;
use search_session::SearchRefresh;
mod comparison;
mod comparison_ui;
#[cfg(test)]
mod document_view_tests;
mod model_download;
mod model_work;
mod ocr;
#[cfg(test)]
mod perf_tests;
mod pii;
mod preview;
mod session_store;
mod settings;
mod settings_ui;
mod style;
mod ui;
mod workspace;

use document::Document;
use document_view::*;
use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyBinding, Menu, MenuItem, PathPromptOptions,
    PromptLevel, ScrollHandle, Subscription, Window, WindowBounds, WindowOptions, actions, div,
    prelude::*, px, size,
};
use gpui_pdf::PdfView;
use mdoc_editor::{EditorEvent, EditorState, SearchIndex, SearchMatch};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use style::Theme;

actions!(
    mdoc,
    [
        New,
        Open,
        Import,
        SetupOcr,
        Settings,
        RunOcr,
        ExtractNative,
        DismissImportWarning,
        Save,
        SaveAs,
        CopyMarkdown,
        Pseudonymize,
        PiiAddCandidate,
        PiiReviewCandidate,
        PiiNextCandidate,
        PiiPreviousCandidate,
        PiiConfirm,
        PiiApplyAll,
        PiiNextChoice,
        PiiPreviousChoice,
        PiiOpenChoice,
        PiiClosePopup,
        PanelUp,
        PanelDown,
        PanelFirst,
        PanelLast,
        PanelEnterList,
        PanelEscape,
        ApplyThis,
        ApplySame,
        ApplyAll,
        KeepThis,
        KeepSame,
        KeepAll,
        Close,
        ClosePdf,
        RetryPreview,
        ToggleTheme,
        ToggleSidebar,
        NextTab,
        PreviousTab,
        Quit,
        TogglePreview,
        RetryDocument,
        FindMarkdown,
        FindNextMarkdown,
        FindPreviousMarkdown,
        CloseMarkdownSearch,
        ToggleMarkdownMatchCase,
    ]
);

fn main() {
    // The pinned engine chooses providers through this variable. Set it once,
    // before starting GPUI, executors or native worker threads; never mutate it
    // while the application is running. Experimental inference is CPU only.
    unsafe {
        std::env::set_var("GLINER2_DEVICE", "cpu");
    }
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--mdoc-docx-worker") {
        let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
            eprintln!("missing DOCX worker input or output");
            std::process::exit(2);
        };
        let output = PathBuf::from(output);
        if let Err(error) = docx_preview::run_worker(&PathBuf::from(input), &output) {
            let _ = std::fs::write(output.with_extension("error"), error.to_string());
            std::process::exit(1);
        }
        return;
    }
    let initial: Vec<_> = args.into_iter().skip(1).map(PathBuf::from).collect();
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
                let _ = window.update(cx, |workspace, window, cx| {
                    workspace.open_paths(paths, window, cx)
                });
            } else {
                pending_url.borrow_mut().extend(paths);
            }
        }
    });
    application.run(move |cx: &mut App| {
        cx.on_app_quit(|cx| {
            model_work::shutdown();
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
        mdoc_editor::bind_keys(cx);
        markdown_search::bind_keys(cx);
        pii::ui::bind_keys(cx);
        settings_ui::bind_keys(cx);
        ui::bind_keys(cx);
        cx.bind_keys([KeyBinding::new(
            "escape",
            comparison_ui::CloseComparison,
            Some("ModelComparison"),
        )]);
        bind_markdown_search_keys(cx);
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
            KeyBinding::new(&format!("{modifier}-,"), Settings, None),
            KeyBinding::new(&format!("{modifier}-n"), New, None),
            KeyBinding::new(&format!("{modifier}-o"), Open, None),
            KeyBinding::new(&format!("{modifier}-shift-i"), Import, None),
            KeyBinding::new(&format!("{modifier}-s"), Save, None),
            KeyBinding::new(&format!("{modifier}-shift-s"), SaveAs, None),
            KeyBinding::new(&format!("{modifier}-w"), Close, None),
            KeyBinding::new(&format!("{modifier}-q"), Quit, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::action("Convert to Markdown", Import),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::action("Copy Markdown", CopyMarkdown),
                MenuItem::action("Pseudonymize", Pseudonymize),
                MenuItem::action("Settings", Settings),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        }]);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let mut workspace = workspace::Workspace::new(window, cx);
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
        *open_context.borrow_mut() = Some((handle, cx.to_async()));
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
