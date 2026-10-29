use super::*;
use crate::{
    bind_app_keys,
    document_view::{
        DocumentView, OcrState, Save, SaveAs, TogglePreview, ZoomIn, ZoomReset,
        bind_markdown_search_keys,
    },
    markdown_search,
    pii::ui::{PiiApplyAll, Pseudonymize},
    preview::RetryPreview,
    search_session::FindMarkdown,
    settings_ui,
    style::{self, Theme},
    ui,
    workspace::{
        Close, Import, New, Open, OpenCase, PasteAiResponse, PseudonymizeClipboard, Quit,
        RetryDocument, Settings, ToggleSidebar, ToggleTheme,
    },
};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use gpui_pdf::PdfView;
use mdoc_import::{import_session, ocr, recognition};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::{
    session_store::{self, Session, TabRecord},
    settings,
};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod case_coordinator;
mod case_page;
mod decision_history;
mod default_categories;
mod docx_queue;
mod focus_regions;
mod folder_sheet;
mod import_flow;
mod navigation;
mod notices;
mod quit_flow;
mod restore_in_place;
mod session_writer;
mod shortcuts_sheet;
mod sidebar;
mod tab_set;
mod window_shell;

/// Boot with `session`; an empty session starts from an Untitled tab rather
/// than Home (ADR 0048), as most tests type into it. See [`boot_home`].
fn boot(cx: &mut TestAppContext, session: Session) -> (Entity<Workspace>, &mut VisualTestContext) {
    let untitled = session.tabs.is_empty();
    let (tabs, cx) = boot_home(cx, session);
    if untitled {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.new_tab(window, cx);
        });
        cx.run_until_parked();
    }
    (tabs, cx)
}

/// Boot exactly as launch does: an empty session shows Home.
fn boot_home(
    cx: &mut TestAppContext,
    session: Session,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(mdoc_editor::bind_keys);
    cx.update(ui::bind_keys);
    cx.update(gpui_pdf::bind_keys);
    cx.update(bind_app_actions);
    // Sidebar animations settle at once; timers still need the clock.
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(markdown_search::bind_keys);
    cx.update(bind_markdown_search_keys);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = Workspace::empty(window, cx);
        tabs.restore(session, window, cx);
        tabs
    });
    cx.run_until_parked();
    (tabs, cx)
}

fn active(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<DocumentView> {
    cx.update(|_, cx| tabs.read(cx).active_view().unwrap())
}

fn click_toolbar(cx: &mut VisualTestContext, label: &'static str) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let bounds = cx
        .debug_bounds(label)
        .unwrap_or_else(|| panic!("button {label} must be rendered"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}

/// Choose `item` from the document header's ⋯ menu (ADR 0048).
fn more(cx: &mut VisualTestContext, item: &'static str) {
    click_toolbar(cx, "header-more");
    click_toolbar(cx, item);
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn collapse(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(false, Some(false));
        cx.notify();
    });
    draw(cx);
}

fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(millis));
    cx.run_until_parked();
    draw(cx);
}

fn hover(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(position, None, gpui::Modifiers::none());
    cx.run_until_parked();
    draw(cx);
}

/// Rest the pointer on the collapsed rail until the full list slides out.
fn reveal_by_hover(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    hover(cx, gpui::point(px(20.), px(400.)));
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
}

/// Keyboard focus on a rail control reveals the list at its matching control.
fn reveal_by_keyboard(cx: &mut VisualTestContext, rail: &gpui::FocusHandle) {
    cx.update(|window, cx| window.focus(rail, cx));
    // An unbound key makes the focus keyboard-driven.
    cx.simulate_keystrokes("f19");
    draw(cx);
    cx.run_until_parked();
    draw(cx);
}

fn compact_focus(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    id: u64,
) -> gpui::FocusHandle {
    cx.update(|_, cx| tabs.read(cx).compact_row_focus.borrow()[&id].clone())
}

/// Set up the real card with a synthetic running transfer: no model/network access.
fn setup_download_card(
    cx: &mut TestAppContext,
    model: settings::Model,
) -> (
    Entity<Workspace>,
    &'static str,
    &'static str,
    &mut VisualTestContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| bind_app_keys(cx));
    cx.simulate_resize(size(px(1100.), px(760.)));
    let view = active(&tabs, cx);
    let (primary, cancel) = match model {
        settings::Model::Ocr(_) => {
            view.update(cx, |view, cx| {
                view.lifecycle.set_source_only(true);
                view.import.set_source(Some("scan.pdf".into()));
                view.import.require_ocr(Some(vec![1]));
                view.ocr_state = OcrState::Missing;
                cx.notify();
            });
            ("Download & recognize", "cancel-ocr-setup")
        }
        settings::Model::Pii(_) => {
            cx.simulate_input("Alice needs review");
            cx.dispatch_action(Pseudonymize);
            cx.run_until_parked();
            ("pii-setup-primary", "cancel-pii-setup")
        }
    };
    draw(cx);
    assert!(cx.debug_bounds(primary).is_some());
    (tabs, primary, cancel, cx)
}

fn download_retains_card_focus(cx: &mut TestAppContext, model: settings::Model) {
    let (tabs, primary, cancel, cx) = setup_download_card(cx, model);
    // Mouse-down focuses Download without invoking its network-backed click.
    let position = cx.debug_bounds(primary).unwrap().center();
    cx.simulate_mouse_down(position, gpui::MouseButton::Left, Default::default());
    draw(cx);
    let before = cx.update(|window, cx| window.focused(cx).expect("Download takes focus"));
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    panel.update(cx, |panel, cx| panel.simulate_setup(model, cx));
    draw(cx);
    assert!(cx.debug_bounds(primary).is_none());
    assert!(cx.debug_bounds(cancel).is_some());
    cx.update(|window, cx| {
        assert_eq!(
            window.focused(cx),
            Some(before),
            "Cancel retains Download's focus"
        );
    });
    // The retained focus is on Cancel: keyboard activation cancels setup.
    activate_control(cx);
    cx.update(|_, cx| assert!(panel.read(cx).cancelling()));
}

fn quit_during_setup_download(cx: &mut TestAppContext, model: settings::Model) {
    let (tabs, _, cancel, cx) = setup_download_card(cx, model);
    // A separate dirty tab must still get the normal unsaved-changes prompt.
    let setup_tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.simulate_input("Unsaved draft");
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(setup_tab, window, cx));
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let progress = panel.update(cx, |panel, cx| panel.simulate_setup(model, cx));
    draw(cx);
    assert!(cx.debug_bounds(cancel).is_some());
    // Routing must work independently of focus repair (including menu validation).
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    assert!(
        cx.update(|_, cx| cx.is_action_available(&Quit)),
        "Quit menu stays enabled"
    );
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    if matches!(model, settings::Model::Pii(_)) {
        // The setup tab has typed text too: two to ask about, so the quit's
        // summary comes first (ADR 0071).
        cx.simulate_prompt_answer("Review…");
        cx.run_until_parked();
    }
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).quit_flow.quitting());
        assert!(!progress.cancel.load(Ordering::Relaxed));
    });
    let dirty = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .filter(|tab| tab.view().is_some_and(|view| view.read(cx).dirty(cx)))
            .count()
    });
    // The keyboard path also works without focus; Discard completes shutdown.
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    });
    cx.run_until_parked();
    if dirty > 1 {
        // Discard All in the summary drops every tab's changes at once.
        cx.simulate_prompt_answer("Discard All");
        cx.run_until_parked();
    } else {
        assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
        for _ in 0..dirty {
            cx.simulate_prompt_answer("Discard");
            cx.run_until_parked();
        }
    }
    assert!(progress.cancel.load(Ordering::Relaxed));
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

/// Open `path` and return its tab once the OCR card is up.
fn open_scan(
    tabs: &Entity<Workspace>,
    path: PathBuf,
    cx: &mut VisualTestContext,
) -> Entity<DocumentView> {
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let view = active(tabs, cx);
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).import.ocr_required().is_some()));
    view
}

fn recognition(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> (Vec<recognition::Language>, String, settings::OcrModel) {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let choice = view.recognition_choice();
        (choice.languages, choice.reason, view.recognition_model())
    })
}

fn drag_files(cx: &mut VisualTestContext, paths: &[PathBuf], position: gpui::Point<gpui::Pixels>) {
    draw(cx);
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position,
        paths: gpui::ExternalPaths(paths.iter().cloned().collect()),
    });
    cx.simulate_event(gpui::FileDropEvent::Pending { position });
    draw(cx);
}

fn drop_files(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(gpui::FileDropEvent::Pending { position });
    cx.simulate_event(gpui::FileDropEvent::Submit { position });
    cx.run_until_parked();
    draw(cx);
}

fn tab_names(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .map(|tab| {
                tab.view()
                    .map(|view| view.read(cx).display_name())
                    .unwrap_or_else(|| {
                        tab.record()
                            .markdown
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into()
                    })
            })
            .collect()
    })
}

/// A workspace with one saved document whose names are replaced, and an
/// isolated case store.
fn applied_document<'a>(
    cx: &'a mut TestAppContext,
    dir: &std::path::Path,
    session: Session,
) -> (
    Entity<Workspace>,
    &'a mut VisualTestContext,
    std::path::PathBuf,
) {
    use mdoc_pii::Category;
    let path = dir.join("contract.md");
    let source = "Anna Petrova met Northwind Traders Ltd.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot_home(cx, session);
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.join("cases"),
            &[3; 32],
        ));
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if view.case.link().is_some() {
            return;
        }
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual(source, 0..12, Category::Person)
            .unwrap();
        view.pii
            .review_mut()
            .add_manual(source, 17..38, Category::Organization)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    (tabs, cx, path)
}

/// Open `text` at `name` in `dir`, mark `names` and apply them.
fn open_applied(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    dir: &std::path::Path,
    name: &str,
    text: &str,
    names: &[(&str, mdoc_pii::Category)],
) -> u64 {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let view = active(tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        for (original, category) in names {
            let start = text.find(original).unwrap();
            view.pii
                .review_mut()
                .add_manual(text, start..start + original.len(), *category)
                .unwrap();
        }
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| tabs.read(cx).tabs.active())
}

/// A case to restore, saved in an isolated store under `dir`.
fn saved_case(dir: &std::path::Path) -> (mdoc_store::cases::Case, std::path::PathBuf) {
    let store = mdoc_store::cases::Store::with_key(dir.join("cases"), &[3; 32]);
    let path = dir.join("contract.md");
    std::fs::write(&path, "PERSON_1 met Bob.").unwrap();
    let case = mdoc_store::cases::Case::new(
        "Petrova".into(),
        &[mdoc_pii::restore::ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: vec![],
            category: mdoc_pii::Category::Person,
            custom: false,
            originals: vec!["Anna Petrova".into()],
        }],
        Some(session_store::identity(&path)),
    );
    store.save(&case).unwrap();
    (case, path)
}

fn case_quit_fixture<'a>(
    cx: &'a mut TestAppContext,
    dir: &std::path::Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext, String) {
    let (tabs, cx, path) = applied_document(cx, dir, Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.write(path, None, cx));
    let id = tabs
        .update_in(cx, |tabs, window, cx| {
            tabs.create_case(tabs.tabs.active(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    // A regular file at the store directory gives a real, portable IO failure.
    let blocked = dir.join("blocked-store");
    std::fs::write(&blocked, "keep this file").unwrap();
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(blocked, &[3; 32]));
    });
    (tabs, cx, id)
}

// Keyboard reach (ADR 0062).

fn key(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
    draw(cx);
}

/// GPUI activates click controls on release; simulate_keystrokes sends only
/// key-down events and therefore cannot prove keyboard button activation.
fn activate_control(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        use gpui::InputEvent;
        let keystroke = gpui::Keystroke::parse("enter").unwrap();
        window.dispatch_event(
            gpui::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(gpui::KeyUpEvent { keystroke }.to_platform_input(), cx);
    });
    cx.run_until_parked();
    draw(cx);
}

fn editor_focused(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let view = active(tabs, cx);
    cx.update(|window, cx| {
        view.read(cx)
            .editor
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    })
}

fn in_toolbar(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let view = active(tabs, cx);
    cx.update(|window, cx| view.read(cx).toolbar_region.contains_focused(window, cx))
}

fn in_sidebar(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .sidebar()
            .contains_focused(window, cx)
    })
}

/// Whether keyboard focus is on `id`'s sidebar row in either layout: the
/// expanded list (or the rail's reveal of it) or the collapsed rail.
fn tab_row_focused(tabs: &Entity<Workspace>, id: u64, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        [&tabs.sidebar_row_focus, &tabs.compact_row_focus]
            .iter()
            .any(|rows| rows.borrow().get(&id).is_some_and(|f| f.is_focused(window)))
    })
}
