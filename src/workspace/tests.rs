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

#[gpui::test]
fn pseudonymization_mappings_survive_switches_and_end_with_the_tab(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let first_path = dir.path().join("first.md");
    let second_path = dir.path().join("second.md");
    std::fs::write(&first_path, "Alice Alice").unwrap();
    std::fs::write(&second_path, "Bob").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![first_path, second_path], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update(cx, |view, cx| {
        view.pii.reviewing = true;
        let id = view
            .pii
            .review
            .add_manual("Alice Alice", 0..5, Category::Person)
            .unwrap();
        view.pii.review.keep(id, None);
        view.review_changed(cx);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    cx.update(|_, cx| assert!(second.read(cx).pii.review.variants().is_empty()));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first_id, window, cx));
    cx.update(|_, cx| {
        let view = first.read(cx);
        let group = &view.pii.review.variants()[0];
        assert_eq!(
            (group.original.as_ref(), group.replacement.as_str()),
            ("Alice", "PERSON_1")
        );
        assert_eq!(view.pii.review.remaining(), 0);
    });
    let weak = first.downgrade();
    drop(first);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first_id, window, cx));
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "closing a clean tab releases live review and mappings"
    );
}

#[gpui::test]
fn ocr_failure_requires_explicit_native_fallback_and_releases_conversion_slot(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("other.csv");
    std::fs::write(&csv, "name,count\npears,3\n").unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![source, csv], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "missing-models".into(),
            pdfium: "missing-pdfium".into(),
            onnx: "missing-onnx".into(),
        })
    });
    first.update_in(cx, |view, window, cx| view.ocr_action(false, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = first.read(cx);
        assert!(!view.prompting);
        assert!(!view.import.busy());
        assert!(view.import.ocr_required().is_some());
        assert!(view.source_only);
        assert!(view.editor.read(cx).text().is_empty());
        // The OCR runtime is bogus, so recognition fails. Its message varies:
        // a parallel test may hold the process-wide model-work permit, which
        // fails the same way before recognition starts.
        assert!(view.error.is_some(), "OCR failure is reported");
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    second.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!second.read(cx).source_only));
    first.update_in(cx, |view, window, cx| view.ocr_action(true, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_ne!(tabs.read(cx).tabs.active(), first_id);
        assert!(!first.read(cx).source_only);
        assert!(
            first
                .read(cx)
                .session
                .warning
                .as_ref()
                .unwrap()
                .contains("2, 5")
        );
        assert!(!first.read(cx).import.slot_busy());
    });
    second.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(second.read(cx).editor.read(cx).text().contains("pears")));
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
                view.source_only = true;
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

#[gpui::test]
fn ocr_download_retains_card_focus(cx: &mut TestAppContext) {
    download_retains_card_focus(cx, settings::Model::Ocr(settings::OcrModel::V6Small));
}

#[gpui::test]
fn pii_download_retains_card_focus(cx: &mut TestAppContext) {
    download_retains_card_focus(cx, settings::Model::Pii(settings::PiiModel::Fp16));
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
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    for _ in 0..dirty {
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
    }
    assert!(progress.cancel.load(Ordering::Relaxed));
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
}

#[gpui::test]
fn ocr_quit_during_setup_download(cx: &mut TestAppContext) {
    quit_during_setup_download(cx, settings::Model::Ocr(settings::OcrModel::V6Small));
}

#[gpui::test]
fn pii_quit_during_setup_download(cx: &mut TestAppContext) {
    quit_during_setup_download(cx, settings::Model::Pii(settings::PiiModel::Fp16));
}

#[gpui::test]
fn app_commands_work_without_control_focus(cx: &mut TestAppContext) {
    use gpui::Action;
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    for action in [
        Quit.boxed_clone(),
        Settings.boxed_clone(),
        crate::menus::About.boxed_clone(),
        New.boxed_clone(),
        Open.boxed_clone(),
        OpenCase.boxed_clone(),
        ToggleTheme.boxed_clone(),
        ToggleSidebar.boxed_clone(),
    ] {
        assert!(cx.update(|_, cx| cx.is_action_available(action.as_ref())));
    }
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    cx.dispatch_action(Settings);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).settings.read(cx).open));
}

#[gpui::test]
fn app_commands_preserve_confirmation_focus(cx: &mut TestAppContext) {
    use gpui::Action;
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, cx| tabs.install_prompts(cx));
    let _answer = cx.update(|window, cx| {
        window.prompt(
            gpui::PromptLevel::Warning,
            "Keep this confirmation focused",
            None,
            &["Continue", "Cancel"],
            cx,
        )
    });
    cx.run_until_parked();
    draw(cx);
    let focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for action in [
        Quit.boxed_clone(),
        Settings.boxed_clone(),
        crate::menus::About.boxed_clone(),
        New.boxed_clone(),
        Open.boxed_clone(),
        OpenCase.boxed_clone(),
        ToggleTheme.boxed_clone(),
        ToggleSidebar.boxed_clone(),
    ] {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        cx.run_until_parked();
        draw(cx);
        cx.update(|window, cx| {
            assert!(window.has_active_prompt());
            assert_eq!(window.focused(cx), Some(focus.clone()));
            assert_eq!(tabs.read(cx).tabs.len(), 1);
            assert!(!tabs.read(cx).settings.read(cx).open);
        });
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, _| assert!(!window.has_active_prompt()));
}

#[gpui::test]
fn ocr_card_asks_once_offers_retry_and_settings_downloads_do_not_convert(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![source], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Missing;
        cx.notify();
    });
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).import.ocr_required().is_some()));
    assert_eq!(
        mdoc_import::ocr::title(settings::OcrModel::V6Small),
        "English"
    );
    for selector in [
        "ocr-language-hint",
        "conversion-card",
        "Download & recognize",
        "Use native text only",
        "choose-ocr-model",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    for model in settings::OcrModel::ALL {
        view.update(cx, |view, cx| {
            view.pick_recognition(mdoc_import::ocr::languages(model).to_vec(), cx);
        });
        draw(cx);
        let model = settings::Model::Ocr(model);
        let label = format!(
            "{}: {}",
            model.name(),
            crate::models::attribution_label(model)
        );
        let bounds = cx
            .debug_bounds(Box::leak(label.into_boxed_str()))
            .expect("OCR source and license");
        let card = cx.debug_bounds("conversion-card").unwrap();
        assert!(bounds.top() >= card.top() && bounds.bottom() <= card.bottom());
    }
    view.update(cx, |view, cx| {
        view.pick_recognition(
            mdoc_import::ocr::languages(settings::OcrModel::V6Small).to_vec(),
            cx,
        );
    });
    // Setup progress replaces the actions with Cancel.
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Installing;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("cancel-ocr-setup").is_some());
    assert!(cx.debug_bounds("Download & recognize").is_none());
    // Cancel or failure returns to the card with Retry.
    view.update(cx, |view, cx| {
        view.ocr_state = OcrState::Failed("Download cancelled.".into());
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("Retry").is_some());
    // A download finished from Settings only enables one-click Run OCR.
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(
            settings::Model::Ocr(settings::OcrModel::V6Small),
            Ok(Some(ocr::Installed {
                config: settings::OcrConfig::default(),
                models: "models".into(),
                pdfium: "pdfium".into(),
                onnx: "onnx".into(),
            })),
        ))
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(matches!(view.ocr_state, OcrState::Ready(_)));
        assert!(!view.import.busy());
        assert!(view.source_only);
        assert!(view.editor.read(cx).text().is_empty());
        assert!(!panel.read(cx).open);
    });
    assert!(cx.debug_bounds("Run OCR").is_some());
    click_toolbar(cx, "choose-ocr-model");
    cx.update(|_, cx| assert!(panel.read(cx).open));
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

/// ROADMAP 0.2.0 should-fix 1: the Russian OCR fixture gets English &
/// Russian by default and the English one stays English, with the reason
/// on the card, even on a computer with a Russian keyboard.
#[gpui::test]
fn ocr_fixtures_default_to_their_recognition_languages(cx: &mut TestAppContext) {
    use recognition::{ENGLISH, RUSSIAN};
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update(cx, |tabs, _| {
        tabs.input_languages = || vec![ENGLISH, RUSSIAN]
    });
    let russian = open_scan(&tabs, fixtures.join("russian.pdf"), cx);
    assert_eq!(
        recognition(&russian, cx),
        (
            vec![ENGLISH, RUSSIAN],
            "English & Russian: the file name mentions Russian".into(),
            settings::OcrModel::Cyrillic
        )
    );
    for selector in [
        "ocr-language-hint",
        "recognize English",
        "recognize English & Russian",
        "Download & recognize",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    let english = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    assert_ne!(english, russian);
    assert_eq!(
        recognition(&english, cx),
        (
            vec![ENGLISH],
            "English: the file name mentions English".into(),
            settings::OcrModel::V6Small
        )
    );
    assert!(cx.debug_bounds("ocr-language-hint").is_some());
}

#[gpui::test]
fn recognition_defaults_follow_text_file_name_then_keyboard(cx: &mut TestAppContext) {
    use recognition::{ENGLISH, RUSSIAN};
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = tempfile::tempdir().unwrap();
    let raster = fixtures.join("ocr-qualification/english.pdf");
    for name in ["договор.pdf", "scan.pdf", "page.pdf"] {
        std::fs::copy(&raster, dir.path().join(name)).unwrap();
    }
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let cyrillic = open_scan(&tabs, dir.path().join("договор.pdf"), cx);
    assert_eq!(
        recognition(&cyrillic, cx).1,
        "English & Russian: the file name is in Russian"
    );
    // No keyboard, text or file-name sign: the Settings choice (English).
    let plain = open_scan(&tabs, dir.path().join("page.pdf"), cx);
    assert_eq!(
        recognition(&plain, cx).1,
        "English: no sign of Russian in the text, file name or keyboard"
    );
    tabs.update(cx, |tabs, _| {
        tabs.input_languages = || vec![ENGLISH, RUSSIAN]
    });
    let keyboard = open_scan(&tabs, dir.path().join("scan.pdf"), cx);
    assert_eq!(
        recognition(&keyboard, cx),
        (
            vec![ENGLISH, RUSSIAN],
            "English & Russian: this computer has a Russian keyboard".into(),
            settings::OcrModel::Cyrillic
        )
    );
    // English text on the pages that need no OCR outranks the keyboard.
    let partly = open_scan(
        &tabs,
        fixtures.join("import/handmade-partly-scanned.pdf"),
        cx,
    );
    assert_eq!(
        recognition(&partly, cx),
        (
            vec![ENGLISH],
            "English: the text layer has no Russian text".into(),
            settings::OcrModel::V6Small
        )
    );
}

#[gpui::test]
fn picking_recognition_languages_changes_only_this_document(cx: &mut TestAppContext) {
    use recognition::ENGLISH;
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let english = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    let view = open_scan(&tabs, fixtures.join("russian.pdf"), cx);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let installed = |model| ocr::Installed {
        config: settings::OcrConfig {
            model,
            ..Default::default()
        },
        models: "missing-models".into(),
        pdfium: "missing-pdfium".into(),
        onnx: "missing-onnx".into(),
    };
    // English (the Settings model) is ready; English & Russian is not.
    tabs.update(cx, |tabs, cx| {
        tabs.set_ocr(OcrState::Ready(installed(settings::OcrModel::V6Small)), cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("Download & recognize").is_some());
    click_toolbar(cx, "recognize English");
    assert_eq!(
        recognition(&view, cx),
        (
            vec![ENGLISH],
            "English: picked for this document".into(),
            settings::OcrModel::V6Small
        )
    );
    draw(cx);
    assert!(cx.debug_bounds("Run OCR").is_some());
    // Picking the detected list again restores its reason.
    click_toolbar(cx, "recognize English & Russian");
    assert_eq!(
        recognition(&view, cx).1,
        "English & Russian: the file name mentions Russian"
    );
    cx.update(|_, cx| {
        assert_eq!(
            english.read(cx).recognition_model(),
            settings::OcrModel::V6Small
        );
        assert_eq!(
            settings::Preferences::default().ocr.model,
            settings::OcrModel::V6Small
        );
    });
    // A checked English & Russian installation enables Run OCR. Its run
    // fails here (no runtime), and that failure stays with this document.
    panel.update(cx, |panel, _| {
        panel
            .ocr_installations
            .push(installed(settings::OcrModel::Cyrillic))
    });
    draw(cx);
    assert!(cx.debug_bounds("Run OCR").is_some());
    view.update_in(cx, |view, window, cx| view.ocr_action(false, window, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.import.ocr_failure().is_some());
        assert!(matches!(view.ocr_state, OcrState::Ready(_)));
        assert!(matches!(english.read(cx).ocr_state, OcrState::Ready(_)));
        assert!(english.read(cx).import.ocr_failure().is_none());
    });
    assert!(cx.debug_bounds("Retry").is_some());
}

/// A tab set up the Settings model and was closed; Settings then chose
/// another model. The setup's end still clears the shared Installing.
#[gpui::test]
fn ocr_setup_ending_after_its_tab_closed_clears_installing(cx: &mut TestAppContext) {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
    let (tabs, cx) = boot(cx, Session::default());
    let view = open_scan(&tabs, fixtures.join("english.pdf"), cx);
    tabs.update(cx, |tabs, cx| tabs.set_ocr(OcrState::Installing, cx));
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    cx.run_until_parked();
    drop(view);
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    cx.update(|_, cx| {
        let prefs = tabs.read(cx).preferences.clone();
        prefs.borrow_mut().current.as_mut().unwrap().ocr.model = settings::OcrModel::Cyrillic;
    });
    panel.update(cx, |_, cx| {
        cx.emit(settings_ui::Event::Finished(
            settings::Model::Ocr(settings::OcrModel::V6Small),
            Err("Download cancelled.".into()),
        ))
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(matches!(tabs.read(cx).ocr_state, OcrState::Missing)));
}

#[gpui::test]
fn closing_source_rejects_conversion_even_when_view_is_retained(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.csv");
    std::fs::write(&source, "name,count\napples,2\n").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(source, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        view.update(cx, |view, cx| view.import(&Import, window, cx));
        assert!(view.read(cx).import.busy());
        tabs.close_tab(tabs.tabs.active(), window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(view.read(cx).source_only);
        assert!(view.read(cx).editor.read(cx).text().is_empty());
        assert!(!view.read(cx).import.busy());
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
}

#[gpui::test]
fn bulk_open_preserves_edits_deduplicates_and_initializes_only_first_selection(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let markdown = dir.path().join("note.md");
    let csv = dir.path().join("table.csv");
    let unknown = dir.path().join("unknown.bin");
    std::fs::write(&markdown, "# Existing\r\n").unwrap();
    std::fs::write(&csv, "name,count\napples,2\n").unwrap();
    std::fs::write(&unknown, "unknown").unwrap();
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    let original = active(&tabs, cx);
    cx.simulate_input("keep edits");
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| {
        Some(vec![
            pdf.clone(),
            markdown.clone(),
            csv.clone(),
            pdf.clone(),
            unknown,
        ])
    });
    cx.run_until_parked();
    let source = active(&tabs, cx);
    let source_id = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), 4);
        assert!(tabs.tabs[2].view().is_none());
        assert!(tabs.tabs[3].view().is_none());
        assert_eq!(
            tabs.tabs[2].record().markdown,
            session_store::identity(&markdown)
        );
        assert!(tabs.notices.message().unwrap().contains("unknown.bin"));
        assert!(!source.read(cx).source_only);
        assert!(!source.read(cx).dirty(cx));
        assert!(source.read(cx).preview.pdf.is_some());
        assert!(!source.read(cx).editor.read(cx).text().is_empty());
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep edits");
        tabs.tabs.active()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![pdf, markdown], window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 4);
        assert_eq!(tabs.read(cx).tabs.active(), source_id);
        assert!(tabs.read(cx).tabs[2].view().is_none());
    });
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));
}

#[gpui::test]
fn source_only_manifest_restores_multiple_sources_and_missing_source_is_retryable(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("missing.csv");
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let session = Session {
        tabs: vec![pdf, csv.clone()]
            .into_iter()
            .map(|source| TabRecord {
                source_only: true,
                attachment: Some(source),
                preview_visible: true,
                ..TabRecord::default()
            })
            .collect(),
        ..Session::default()
    };
    let manifest = dir.path().join("session.json");
    session_store::save(&manifest, &session).unwrap();
    let (tabs, cx) = boot(cx, session_store::load(&manifest).unwrap());
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        assert!(tabs.read(cx).tabs[1].view().is_none());
        assert_eq!(tabs.read(cx).snapshot(cx).tabs.len(), 2);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).preview.retryable));
    std::fs::write(csv, "name,count\napples,2\n").unwrap();
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.retryable);
        assert!(!view.read(cx).dirty(cx));
    });
}

#[gpui::test]
fn conversion_stays_in_source_tab_and_ready_ocr_requires_consent(cx: &mut TestAppContext) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import/handmade-partly-scanned.pdf");
    let original = std::fs::read(&source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    view.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: mdoc_store::settings::OcrConfig::default(),
            models: "must-not-load".into(),
            pdfium: "must-not-load".into(),
            onnx: "must-not-load".into(),
        })
    });
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).prompting);
        assert_eq!(view.read(cx).import.ocr_required(), Some(&[2, 5][..]));
        assert!(view.read(cx).source_only);
        assert!(!view.read(cx).import.slot_busy());
        assert!(matches!(view.read(cx).ocr_state, OcrState::Ready(_)));
    });
    view.update_in(cx, |view, window, cx| view.ocr_action(true, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.source_only);
        assert!(view.dirty(cx));
        assert!(view.editor.read(cx).text().contains("Readable page three"));
        assert!(view.session.warning.as_ref().unwrap().contains("2, 5"));
        assert_eq!(view.preview.pdf.as_ref(), Some(&pdf));
        assert!(!view.import.slot_busy());
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx).snapshot(cx).tabs.is_empty(),
            "unsaved conversion is not persisted as a source-only tab"
        );
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), view);
    cx.simulate_input("preserve edits");
    let text = cx.update(|_, cx| view.read(cx).editor.read(cx).text().to_owned());
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), text));
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[gpui::test]
fn disabled_copy_for_ai_says_why(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.simulate_input("Maya Bennett signed.");
    click_toolbar(cx, "header-more");
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).alias_actions_reason(),
            Some("Pseudonymize the document first.")
        );
    });
    // The reason sits under the label, so the row is taller than Copy
    // Markdown's.
    let disabled = cx.debug_bounds("more-copy-for-ai").unwrap();
    let enabled = cx.debug_bounds("more-copy-markdown").unwrap();
    assert!(disabled.size.height > enabled.size.height);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    click_toolbar(cx, "more-copy-for-ai");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("kept")
        );
    });
}

#[gpui::test]
fn copy_markdown_button_is_inert_while_markdown_is_empty(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("kept")
        );
        assert!(view.read(cx).copy_feedback.is_none());
    });
}

#[gpui::test]
fn copy_markdown_button_uses_active_tab_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = active(&tabs, cx);
    cx.simulate_input("first tab");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    cx.simulate_input("second tab");
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("second tab")
        );
        assert!(first.read(cx).copy_feedback.is_none());
        assert!(second.read(cx).copy_feedback.is_some());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(-1, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first tab")
        );
        assert_eq!(second.read(cx).editor.read(cx).text(), "second tab");
    });
}

#[gpui::test]
fn toolbar_settings_opens_and_closes_without_editor_focus(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    // Keep the direct-click targets visible with the model attribution rows.
    // Scrolling keyboard reach is covered separately; below we resize small
    // to verify that the dialog and its footer stay within the viewport.
    cx.simulate_resize(size(px(1100.), px(1200.)));
    let panel = cx.update(|window, cx| {
        window.focus(&tabs.read(cx).focus.clone(), cx);
        tabs.read(cx).settings.clone()
    });
    active(&tabs, cx).update(cx, |view, cx| {
        view.pii.reviewing = true;
        cx.notify();
    });
    click_toolbar(cx, "Settings");
    cx.update(|window, cx| {
        assert!(panel.read(cx).open);
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_some());
    click_toolbar(cx, "settings-advanced");
    click_toolbar(cx, "settings-details-fp16");
    cx.update(|_, cx| {
        assert_eq!(
            panel.read(cx).details,
            Some(settings::Model::Pii(settings::PiiModel::Fp16))
        );
        assert!(panel.read(cx).advanced);
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    assert!(dialog.size.width <= px(640.));
    assert!(dialog.size.height <= px(480.));
    // Done stays in the footer; Save scrolls with the Advanced fields.
    let bounds = cx.debug_bounds("settings-close").unwrap();
    assert!(bounds.top() >= px(0.) && bounds.bottom() <= px(480.));
    cx.dispatch_action(settings_ui::CloseSettings);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open);
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_none());
    // A click inside keeps Settings open; one outside closes it.
    cx.simulate_resize(size(px(1100.), px(760.)));
    click_toolbar(cx, "Settings");
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    cx.simulate_click(dialog.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(panel.read(cx).open));
    cx.simulate_click(
        gpui::point(dialog.left() - px(20.), dialog.center().y),
        Default::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open, "an outside click closes Settings");
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_none());
}

#[gpui::test]
fn plus_keys_zoom_in_like_the_zoom_button(cx: &mut TestAppContext) {
    cx.update(crate::bind_app_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    let mut zoom = cx.update(|_, cx| view.read(cx).zoom);
    // Cmd/Ctrl with =, plus, and plus or = with the Shift a layout needs.
    for keys in [
        "secondary-=",
        "secondary-+",
        "secondary-shift-=",
        "secondary-shift-+",
    ] {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        let next = cx.update(|_, cx| view.read(cx).zoom);
        assert!(
            (next - zoom - crate::document_view::ZOOM_STEP).abs() < 1e-4,
            "{keys}: {zoom} -> {next}"
        );
        zoom = next;
    }
    cx.simulate_keystrokes("secondary-0");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).zoom, 1.));
}

#[gpui::test]
fn toolbar_theme_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    // The sidebar's theme control cycles System → Light → Dark (ADR 0048).
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::System));
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::Light);
        assert_eq!(tabs.read(cx).theme.get(), Theme::Light);
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::Dark);
        assert_eq!(tabs.read(cx).theme.get(), Theme::Dark);
        assert_eq!(tabs.read(cx).snapshot(cx).theme_choice, ThemeChoice::Dark);
    });
}

#[gpui::test]
fn available_update_marks_the_settings_button(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    draw(cx);
    assert!(cx.debug_bounds("Settings").is_some());
    assert!(cx.debug_bounds("update-dot").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            available: Some(crate::updater::test_update("9.0.0")),
            ..Default::default()
        })
    });
    draw(cx);
    let dot = cx
        .debug_bounds("update-dot")
        .expect("an update shows the dot");
    let button = cx.debug_bounds("Settings").unwrap();
    assert!(
        button.contains(&dot.center()),
        "dot={dot:?}, button={button:?}"
    );
}

#[gpui::test]
fn installed_notice_leaves_no_dot_on_the_collapsed_rail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.md");
    std::fs::write(&a, "a").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    collapse(&tabs, cx);
    // Just updated, nothing newer: the "installed" notice isn't an update.
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            updated_to: Some("9.0.0".into()),
            ..Default::default()
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("rail-settings").is_some());
    assert!(cx.debug_bounds("rail-update-dot").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            available: Some(crate::updater::test_update("9.1.0")),
            ..Default::default()
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("rail-update-dot").is_some());
}

#[gpui::test]
fn update_banner_opens_settings_and_dismisses_per_version(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    // A manual check's result raises no banner; an automatic one does.
    let found = crate::updater::UpdateState {
        available: Some(crate::updater::test_update("9.0.0")),
        ..Default::default()
    };
    cx.update(|_, cx| cx.set_global(found.clone()));
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            automatic: true,
            ..found
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("update-dot").is_none(),
        "the banner carries the only dot"
    );
    click_toolbar(cx, "update-banner");
    cx.update(|_, cx| assert!(tabs.read(cx).settings.read(cx).open));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx))
    });
    click_toolbar(cx, "dismiss-update-banner");
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    assert!(
        cx.debug_bounds("update-dot").is_some(),
        "Settings marks the update once the banner is gone"
    );
    cx.update(|_, cx| {
        assert!(
            !tabs.read(cx).settings.read(cx).open,
            "× doesn't open Settings"
        );
        assert_eq!(
            crate::updater::state(cx).dismissed.as_deref(),
            Some("9.0.0")
        );
    });
}

#[gpui::test]
fn toolbar_preview_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(path.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    click_toolbar(cx, "header-source");
    cx.update(|window, cx| {
        assert!(!view.read(cx).preview.visible);
        assert!(
            view.read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    click_toolbar(cx, "header-source");
    cx.update(|_, cx| {
        assert!(view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.pdf.as_ref(), Some(&pdf));
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "header-source");
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.attachment.as_ref(), Some(&path));
    });
}

#[gpui::test]
fn detached_header_routes_preview_and_document_shortcuts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    collapse(&tabs, cx);
    for theme in [Theme::Dark, Theme::Light] {
        tabs.update(cx, |tabs, cx| {
            tabs.theme.set(theme);
            cx.notify();
        });
        click_toolbar(cx, "header-source");
        cx.update(|_, cx| assert!(!view.read(cx).preview.visible));
        click_toolbar(cx, "header-source");
        cx.update(|_, cx| assert!(view.read(cx).preview.visible));
        click_toolbar(cx, "header-more");
        let zoom = cx.update(|_, cx| view.read(cx).zoom);
        cx.dispatch_action(ZoomIn);
        cx.run_until_parked();
        cx.update(|_, cx| assert!(view.read(cx).zoom > zoom));
        cx.dispatch_action(ZoomReset);
        click_toolbar(cx, "more-copy-markdown");
    }
}

#[gpui::test]
fn preview_find_types_through_text_input(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    // The find button's action; Mod+F opens the Markdown find.
    pdf.update_in(cx, |pdf, window, cx| pdf.toggle_search(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| pdf.read(cx).is_search_open()));
    // Composed and non-ASCII text arrives through the input handler, not key
    // codes, so IME and other layouts reach the query (ADR 0057, K7).
    cx.simulate_input("Договор 契約");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "Договор 契約"
    );
    cx.simulate_keystrokes("backspace backspace");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "Договор "
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| pdf.read(cx).is_search_open()));
    assert!(cx.update(|window, cx| pdf.read(cx).focus_handle(cx).is_focused(window)));
}

#[gpui::test]
fn preview_find_composes_japanese(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let mut field = None;
    let (pdf, cx) = cx.add_window_view(|_, cx| {
        let mut pdf = PdfView::new(path, Rc::new(Default::default), Rc::new(|| 1.), cx);
        field = Some(crate::preview::attach_search_field(&mut pdf, cx));
        pdf
    });
    let field = field.unwrap();
    pdf.update_in(cx, |pdf, window, cx| pdf.toggle_search(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|window, cx| field.read(cx).focus_handle(cx).is_focused(window)));
    // What the IME sends: marked kana while composing, then the chosen kanji.
    field.update_in(cx, |field, window, cx| {
        field.replace_and_mark_text_in_range(None, "けいやく", None, window, cx);
        assert_eq!(field.marked_text_range(window, cx), Some(0..4));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "けいやく"
    );
    field.update_in(cx, |field, window, cx| {
        field.replace_text_in_range(None, "契約", window, cx);
        assert_eq!(field.marked_text_range(window, cx), None);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "契約"
    );
}

#[gpui::test]
fn new_switch_reorder_and_close_preserve_live_editor_state(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let first = active(&tabs, cx);
    cx.simulate_input("original **Markdown**");
    cx.run_until_parked();
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.dispatch_action(New);
    cx.run_until_parked();
    let second = active(&tabs, cx);
    assert_ne!(first, second);
    cx.simulate_input("second document");
    cx.run_until_parked();
    let second_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.reorder(first_id, second_id, cx);
        tabs.activate(first_id, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), first);
    cx.update(|_, cx| {
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "original **Markdown**"
        );
        assert!(first.read(cx).dirty(cx));
        assert_eq!(second.read(cx).editor.read(cx).text(), "second document");
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(first.read(cx).editor.read(cx).text().is_empty()));
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
    assert_eq!(active(&tabs, cx), second);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), second);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .editor
                .read(cx)
                .text()
                .is_empty()
        );
    });
}

#[gpui::test]
fn restoration_is_lazy_and_missing_files_remain_retryable(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let missing = dir.path().join("missing.md");
    std::fs::write(&first, "Привет").unwrap();
    let session = Session {
        tabs: vec![
            TabRecord {
                markdown: first.clone(),
                caret: usize::MAX,
                ..TabRecord::default()
            },
            TabRecord {
                markdown: missing.clone(),
                ..TabRecord::default()
            },
        ],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, session);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.tabs[1].view().is_none());
        let view = tabs.active_view().unwrap();
        assert_eq!(view.read(cx).editor.read(cx).cursor(), "Привет".len());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).unavailable));
    std::fs::write(missing, "now present").unwrap();
    cx.dispatch_action(RetryDocument);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).unavailable);
        assert_eq!(view.read(cx).editor.read(cx).text(), "now present");
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(first, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 2));
}

#[gpui::test]
fn paired_preview_hides_reopens_and_failed_replacement_preserves_attachment(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = base.join("tests/fixtures/reference.pdf");
    view.update_in(cx, |view, window, cx| {
        view.open_path(pdf.clone(), window, cx)
    });
    cx.run_until_parked();
    let entity = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    cx.dispatch_action(TogglePreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.attachment.as_ref(), Some(&pdf));
        assert!(view.read(cx).session.document.path.is_none());
    });
    cx.dispatch_action(TogglePreview);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).preview.pdf.as_ref(), Some(&entity)));
    view.update_in(cx, |view, window, cx| {
        view.open_path(base.join("missing.pdf"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).preview.pdf.as_ref(), Some(&entity));
        assert_eq!(view.read(cx).preview.attachment.as_ref(), Some(&pdf));
        assert!(view.read(cx).preview.retryable);
    });
}

#[gpui::test]
fn snapshot_keeps_saved_pairs_and_omits_unsaved_buffers(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("note.md");
    std::fs::write(&note, "saved").unwrap();
    let attachment = dir.path().join("not-loaded.docx");
    let session = Session {
        tabs: vec![TabRecord {
            markdown: note,
            attachment: Some(attachment.clone()),
            preview_visible: false,
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, session);
    cx.simulate_input("dirty text");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("untitled secret");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let snapshot = tabs.read(cx).snapshot(cx);
        assert_eq!(snapshot.tabs.len(), 1);
        assert_eq!(snapshot.tabs[0].attachment.as_ref(), Some(&attachment));
        assert!(!snapshot.tabs[0].preview_visible);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("dirty text"));
        assert!(!serialized.contains("untitled secret"));
        assert!(
            tabs.read(cx).tabs[0]
                .view()
                .unwrap()
                .read(cx)
                .preview
                .pdf
                .is_none()
        );
    });
}

#[gpui::test]
fn quit_cancel_keeps_all_tabs_and_save_as_collision_never_overwrites(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("saved.md");
    std::fs::write(&note, "disk original").unwrap();
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: vec![TabRecord {
                markdown: note.clone(),
                ..TabRecord::default()
            }],
            ..Session::default()
        },
    );
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("unsaved");
    cx.dispatch_action(SaveAs);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(note.clone()));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(note).unwrap(), "disk original");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("other unsaved");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), 3);
        assert!(!tabs.quit_flow.quitting());
        assert_eq!(
            tabs.tabs
                .iter()
                .filter(|tab| tab.view().unwrap().read(cx).dirty(cx))
                .count(),
            2
        );
    });
}

#[gpui::test]
fn queued_docx_jobs_are_serial_and_closed_tabs_cannot_receive_results(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = base.join("tests/fixtures/docx-preview/comments.docx");
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // Admit both jobs in one update before the background executor runs.
    tabs.update_in(cx, |tabs, window, cx| {
        first.update(cx, |view, cx| view.open_docx(source.clone(), window, cx));
        tabs.new_tab(window, cx);
        tabs.active_view()
            .unwrap()
            .update(cx, |view, cx| view.open_docx(source, window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.docx_queue.idle());
        assert!(first.read(cx).preview.pdf.is_some());
        assert!(tabs.active_view().unwrap().read(cx).preview.pdf.is_some());
    });
    let (weak, backing) = cx.update(|_, cx| {
        let view = first.read(cx);
        (
            view.preview.pdf.as_ref().unwrap().downgrade(),
            view.preview.docx.as_ref().unwrap().pdf_path.clone(),
        )
    });
    drop(first);
    tabs.update_in(cx, |tabs, window, cx| tabs.remove(first_id, window, cx));
    cx.run_until_parked();
    assert!(crate::test_support::removed(&backing));
    cx.update(|_, _| {
        assert!(
            weak.upgrade().is_none(),
            "closed PDF entity must be released"
        )
    });
}

#[gpui::test]
fn background_import_finishes_in_its_own_tab_without_stealing_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.csv");
    std::fs::write(&source, "name,value\nalpha,42\n").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("keep original");
    cx.run_until_parked();
    let original = active(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source, window, cx);
    });
    cx.run_until_parked();
    let imported = active(&tabs, cx);
    imported.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.activate(id, window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), id);
        assert!(
            original
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep original");
        let imported = tabs.read(cx).tabs[1].view().unwrap().read(cx);
        assert!(imported.editor.read(cx).text().contains("alpha"));
        assert!(imported.dirty(cx));
        assert!(!imported.import.busy());
    });
}

#[gpui::test]
fn every_menu_item_reaches_a_handler_in_a_document(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| crate::menus::install(cx));
    cx.simulate_input("alpha beta");
    cx.run_until_parked();
    for (name, action) in crate::menus::items() {
        // In the window's focus path, or app-wide (Hide, Minimize).
        let available = cx.update(|window, cx| {
            window.is_action_available(action.as_ref(), cx)
                || cx.is_action_available(action.as_ref())
        });
        assert!(available, "menu item {name} has no handler");
    }
}

#[gpui::test]
fn edit_menu_reaches_the_find_field(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("alpha beta");
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("beta".into()));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Paste);
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), "beta"));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Cut);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), ""));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("beta")
    );
    // The document is untouched.
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "alpha beta"));
}

#[gpui::test]
fn retained_search_selection_and_preview_position_survive_switch(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("alpha beta alpha");
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let count = cx.update(|_, cx| first.read(cx).search.matches.len());
    let cursor = cx.update(|_, cx| first.read(cx).editor.read(cx).cursor());
    cx.dispatch_action(New);
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(id, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(first.read(cx).search.open);
        assert_eq!(first.read(cx).search.matches.len(), count);
        assert_eq!(first.read(cx).markdown_search.read(cx).value(), "alpha");
        assert_eq!(first.read(cx).editor.read(cx).cursor(), cursor);
    });
}

#[gpui::test]
fn checkpoint_serializes_latest_state_and_restores_pdf_position(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let note = dir.path().join("note.md");
    std::fs::write(&note, "markdown").unwrap();
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: vec![TabRecord {
                markdown: note,
                attachment: Some(pdf),
                preview_visible: true,
                preview_page: 0,
                preview_zoom: Some(1.5),
                preview_fit: session_store::PreviewFit::Manual,
                ..TabRecord::default()
            }],
            ..Session::default()
        },
    );
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let pdf = view.read(cx).preview.pdf.as_ref().unwrap().read(cx);
        assert_eq!(pdf.reading_position(), (0, 1.5));
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.session_writer.set_path(Some(path.clone()));
        tabs.checkpoint(window, cx);
        tabs.sidebar.restore(false, Some(false));
        tabs.checkpoint(window, cx);
    });
    cx.run_until_parked();
    let saved = session_store::load(&path).unwrap();
    assert!(!saved.sidebar_visible);
    assert_eq!(saved.tabs[0].preview_zoom, Some(1.5));
    assert_eq!(saved.tabs.len(), 1);
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert_eq!(
        session_store::load(&path).unwrap().tabs.len(),
        1,
        "window teardown must not persist an empty session"
    );
}

#[gpui::test]
fn a_closed_tab_s_docx_job_ending_frees_the_worker(cx: &mut TestAppContext) {
    // A closed document can't emit; the window waits on its running jobs.
    let (tabs, cx) = boot(cx, Session::default());
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
        tabs.docx_queue.occupy(id, 4);
        let view = tabs
            .tabs
            .get(id)
            .and_then(|tab| tab.view().cloned())
            .unwrap();
        view.update(cx, |view, cx| {
            let docx = cx
                .background_executor()
                .spawn(async { crate::document_view::Background::Docx { generation: 4 } });
            view.keep_background(docx);
        });
        tabs.remove(id, window, cx);
        assert!(!tabs.docx_queue.idle(), "the job has not ended yet");
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).docx_queue.idle()));
}

#[gpui::test]
fn a_closed_tab_s_import_ending_starts_the_conversion_waiting_for_it(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
    let permit = cx
        .update(|_, cx| import_session::ImportPermit::acquire(&tabs.read(cx).import_busy).unwrap());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![base.join("text.pdf")], window, cx)
    });
    cx.run_until_parked();
    let waiting = active(&tabs, cx);
    cx.update(|_, cx| assert!(waiting.read(cx).import.auto_pending()));
    // Another tab's import holds the slot; the tab closes before it ends.
    let closed = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        let view = tabs
            .tabs
            .get(closed)
            .and_then(|tab| tab.view().cloned())
            .unwrap();
        view.update(cx, |view, cx| {
            let import = cx.background_executor().spawn(async move {
                drop(permit);
                crate::document_view::Background::Import
            });
            view.keep_background(import);
        });
        tabs.remove(closed, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!waiting.read(cx).import.auto_pending());
        assert!(!waiting.read(cx).source_only, "the waiting PDF converted");
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
}

#[gpui::test]
fn closing_queued_conversion_rejects_it_without_affecting_other_tabs(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.docx_queue.occupy(999, 1);
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| {
            view.open_docx("queued.docx".into(), window, cx)
        });
        tabs.event(
            id,
            &TabEvent::Docx(DocxEvent::Queue {
                path: "queued.docx".into(),
                generation: 1,
                cancel: cancelled.clone(),
            }),
            window,
            cx,
        );
        tabs.remove(id, window, cx);
        tabs.docx_finished(999, 1, window, cx);
        assert!(tabs.docx_queue.idle());
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .preview
                .source
                .is_none()
        );
    });
}

#[gpui::test]
#[ignore = "host CPU measurements; run serially on an idle machine"]
fn tabs_host_performance(cx: &mut TestAppContext) {
    use std::time::Instant;
    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }
    fn report(label: &str, values: &mut [f64]) {
        values.sort_by(f64::total_cmp);
        eprintln!(
            "TABS_PERF {label} median_ms={:.3} p95_ms={:.3}",
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1]
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = base.join("tests/fixtures/reference.pdf");
    let docx = base.join("tests/fixtures/docx-preview/comments.docx");
    let mut session = Session::default();
    for index in 0..24 {
        let path = dir.path().join(format!("note-{index}.md"));
        let source = if index % 3 == 0 {
            "# Heading\n\nalpha **beta** words for a long document.\n\n".repeat(2500)
        } else {
            "# Short\n\nhello world\n".repeat(30)
        };
        std::fs::write(&path, source).unwrap();
        session.tabs.push(TabRecord {
            markdown: path,
            attachment: match index % 3 {
                0 => None,
                1 => Some(pdf.clone()),
                _ => Some(docx.clone()),
            },
            preview_visible: index % 3 != 0,
            ..TabRecord::default()
        });
    }
    let start = Instant::now();
    let (tabs, cx) = boot(cx, session);
    cx.simulate_resize(size(px(1280.), px(800.)));
    draw(cx);
    eprintln!(
        "TABS_PERF restore_24_tabs_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
    let ids = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .map(|tab| tab.id())
            .collect::<Vec<_>>()
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx)
                .tabs
                .iter()
                .filter(|tab| tab.view().is_some())
                .count(),
            1
        )
    });
    for id in &ids[..6] {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(*id, window, cx));
        cx.run_until_parked();
        draw(cx);
    }
    let mut switching = Vec::new();
    let mut handlers = Vec::new();
    let mut frames = Vec::new();
    let mut long_frames = Vec::new();
    let mut short_frames = Vec::new();
    for round in 0..24 {
        let id = ids[round % 6];
        let start = Instant::now();
        let handler = tabs.update_in(cx, |tabs, window, cx| {
            let start = Instant::now();
            tabs.activate(id, window, cx);
            start.elapsed().as_secs_f64() * 1000.
        });
        handlers.push(handler);
        switching.push(start.elapsed().as_secs_f64() * 1000.);
        let start = Instant::now();
        draw(cx);
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        frames.push(elapsed);
        if round % 3 == 0 {
            long_frames.push(elapsed);
        } else {
            short_frames.push(elapsed);
        }
    }
    report("switch_handler", &mut handlers);
    report("switch_with_implicit_test_frame", &mut switching);
    report("switch_frame", &mut frames);
    report("long_markdown_frame", &mut long_frames);
    report("short_markdown_with_preview_frame", &mut short_frames);
    // Same document, same viewport, no tab change: distinguish inherited editor
    // layout cost from the sidebar/activation overhead.
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(ids[0], window, cx));
    let mut unchanged = Vec::new();
    for _ in 0..8 {
        let start = Instant::now();
        draw(cx);
        unchanged.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("unchanged_long_markdown_frame", &mut unchanged);
    let view = active(&tabs, cx);
    eprintln!(
        "TABS_PERF unchanged_long_editor_width={:?}",
        view.read_with(cx, |view, cx| view
            .editor
            .read(cx)
            .layout_width()
            .map(f32::from))
    );
    assert!(
        handlers.iter().all(|ms| *ms < 16.7),
        "tab activation alone exceeded one CPU frame"
    );
    let mut snapshots = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        cx.update(|_, cx| {
            let _ = tabs.read(cx).snapshot(cx);
        });
        snapshots.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("metadata_snapshot_24_tabs", &mut snapshots);
    // Repeated DOCX pairs exercise temporary files and retained viewer release.
    eprintln!(
        "TABS_PERF before_close_cycles_rss_kib={:?}",
        crate::perf_tests::rss_kib()
    );
    for cycle in 0..5 {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.open_path(docx.clone(), window, cx)
        });
        cx.run_until_parked();
        let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
        draw(cx);
        let (weak, backing) = cx.update(|_, cx| {
            let view = tabs.read(cx).active_view().unwrap();
            let view = view.read(cx);
            (
                view.preview.pdf.as_ref().unwrap().downgrade(),
                view.preview.docx.as_ref().unwrap().pdf_path.clone(),
            )
        });
        tabs.update_in(cx, |tabs, window, cx| tabs.remove(id, window, cx));
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        assert!(crate::test_support::removed(&backing));
        eprintln!(
            "TABS_PERF close_cycle={cycle} rss_kib={:?}",
            crate::perf_tests::rss_kib()
        );
    }
    eprintln!("TABS_PERF close_cycles=5 all_viewers_and_docx_files_released=true");
}

#[gpui::test]
fn production_shell_queues_startup_opens_and_close_hook_keeps_dirty_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("startup.md");
    std::fs::write(&path, "startup").unwrap();
    cx.update(mdoc_editor::bind_keys);
    cx.update(bind_app_actions);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = Workspace::new(window, cx);
        tabs.open_path(path, window, cx);
        tabs
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), "startup");
        assert!(!view.read(cx).ocr_state.busy());
    });
    cx.simulate_input(" edited");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(view.read(cx).dirty(cx)));
}

#[gpui::test]
fn failed_final_checkpoint_can_cancel_quit_without_losing_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, "preserve").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.session_writer
            .set_path(Some(parent_file.join("session.json")))
    });
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(!tabs.read(cx).quit_flow.finishing());
        assert!(tabs.read(cx).notices.message().is_some());
    });
    assert_eq!(std::fs::read_to_string(parent_file).unwrap(), "preserve");
}

#[gpui::test]
fn pdf_preview_refits_when_sidebar_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(false, Some(false));
        cx.notify();
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let settle = |cx: &mut VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    };
    settle(cx);
    let wide = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf
            .as_ref()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    cx.dispatch_action(ToggleSidebar);
    settle(cx);
    let narrow = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf
            .as_ref()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    assert!(
        narrow < wide - 0.05,
        "PDF must refit when sidebar opens: wide={wide}, narrow={narrow}"
    );
    cx.dispatch_action(ToggleSidebar);
    settle(cx);
    let expanded = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf
            .as_ref()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    assert!((expanded - wide).abs() < 0.01);
}

#[gpui::test]
fn restored_pdf_keeps_auto_fit_and_manual_zoom_remains_manual(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let markdown = dir.path().join("note.md");
    std::fs::write(&markdown, "note").unwrap();
    let attachment = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: vec![TabRecord {
                markdown,
                attachment: Some(attachment),
                preview_visible: true,
                preview_zoom: Some(1.0),
                ..TabRecord::default()
            }],
            ..Session::default()
        },
    );
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    cx.update(|_, cx| {
        assert_eq!(pdf.read(cx).fit_mode(), Some(gpui_pdf::FitMode::Width));
        assert_eq!(
            tabs.read(cx).snapshot(cx).tabs[0].preview_fit,
            session_store::PreviewFit::Width
        );
    });
    pdf.update(cx, |pdf, cx| pdf.set_zoom(1.2, cx));
    cx.dispatch_action(ToggleSidebar);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(pdf.read(cx).reading_position().1, 1.2);
        assert_eq!(pdf.read(cx).fit_mode(), None);
        assert_eq!(
            tabs.read(cx).snapshot(cx).tabs[0].preview_fit,
            session_store::PreviewFit::Manual
        );
    });
}

#[gpui::test]
fn placeholder_replacement_respects_explicit_new_and_survives_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note.md");
    std::fs::write(&file, "note").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let placeholder = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // Rejected selections and canceled pickers cannot consume the placeholder.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.finish_open(vec![(dir.path().join("bad.bin"), false)], None, window, cx)
    });
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.active(), placeholder));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert_ne!(tabs.read(cx).tabs.active(), placeholder);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    let fallback = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // From Home, New turns the placeholder into the tab (ADR 0048); a second
    // New adds another.
    tabs.update_in(cx, |tabs, window, cx| {
        assert_eq!(tabs.new_tab(window, cx), fallback);
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let session = session_store::load(&manifest).unwrap();
    assert_eq!(session.tabs.len(), 2);
    assert!(
        session
            .tabs
            .iter()
            .all(|tab| tab.blank && !tab.blank_disposable)
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(fallback)));
    // Restore both intentional blanks without deduplicating their empty paths.
    tabs.update_in(cx, |tabs, window, cx| {
        for tab in &tabs.tabs {
            if let Some(view) = tab.view() {
                Workspace::release(view, window, cx);
            }
        }
        tabs.tabs.clear();
        tabs.tab_subscriptions.clear();
        tabs.restore(session, window, cx);
        tabs.open_path(file, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));
}

#[gpui::test]
fn edited_then_emptied_placeholder_is_not_disposable(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.simulate_input("temporary text");
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        assert!(view.read(cx).editor.read(cx).text().is_empty());
        assert!(!view.read(cx).blank_disposable);
        assert!(!tabs.read(cx).snapshot(cx).tabs[0].blank_disposable);
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(id)));
}

#[gpui::test]
fn automatic_conversion_is_disposable_until_edited_but_can_be_saved(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.docx");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(!view.read(cx).source_only);
        assert!(view.read(cx).generated_unedited);
        assert!(!view.read(cx).dirty(cx));
        assert!(!view.read(cx).editor.read(cx).text().is_empty());
        assert!(view.read(cx).preview.pdf.is_some());
        let snapshot = tabs.read(cx).snapshot(cx);
        assert!(snapshot.tabs[0].source_only);
        assert_eq!(snapshot.tabs[0].attachment.as_ref(), Some(&source));
    });
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(source, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.simulate_input("edited ");
    cx.run_until_parked();
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("converted.md");
    cx.dispatch_action(Save);
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).dirty(cx));
        assert_eq!(
            std::fs::read_to_string(&saved).unwrap(),
            view.read(cx).editor.read(cx).text()
        );
        // Recents now leads with the saved file (#76).
        let recents = &tabs.read(cx).shell.recents.files;
        assert_eq!(recents[0].path, session_store::identity(&saved));
    });
}

#[gpui::test]
fn automatic_conversions_queue_on_activation_and_closed_waiters_do_not_run(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import");
    let permit = cx
        .update(|_, cx| import_session::ImportPermit::acquire(&tabs.read(cx).import_busy).unwrap());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(
            vec![
                base.join("text.pdf"),
                base.join("text.docx"),
                base.join("handmade-partly-scanned.pdf"),
            ],
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(first.read(cx).import.auto_pending());
        assert!(tabs.read(cx).tabs[1].view().is_none());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let closed = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    let selected = cx.update(|_, cx| tabs.read(cx).tabs.active());
    drop(permit);
    tabs.update_in(cx, |tabs, window, cx| tabs.refresh_import(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), selected);
        assert!(!first.read(cx).source_only);
        assert!(closed.read(cx).editor.read(cx).text().is_empty());
        assert!(closed.read(cx).import.cancelled());
        let view = tabs.read(cx).active_view().unwrap();
        assert_eq!(view.read(cx).import.ocr_required(), Some(&[2, 5][..]));
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn sidebar_default_and_explicit_choice_survive_document_count_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| assert!(tabs.read(cx).sidebar.visible()));
    tabs.update_in(cx, |tabs, window, cx| {
        for _ in 0..3 {
            tabs.new_tab(window, cx);
        }
    });
    cx.run_until_parked();
    click_toolbar(cx, "sidebar-toggle");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, Some(false));
    assert!(!saved.sidebar_visible);
    let restored = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.restore(saved, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(restored.read(cx).sidebar.choice(), Some(false)));
    click_toolbar(cx, "sidebar-toggle");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, Some(true));
    assert!(saved.sidebar_visible);
    // A session saved without an explicit choice opens expanded.
    let legacy = Session {
        sidebar_visible: false,
        sidebar_choice: None,
        ..Session::default()
    };
    let legacy = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.restore(legacy, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(legacy.read(cx).sidebar.choice(), None));
    let legacy = cx.update(|_, cx| legacy.read(cx).snapshot(cx));
    assert!(legacy.sidebar_choice.is_none());
}

#[gpui::test]
fn keyboard_reveal_escape_restores_rail_control_and_closing_opener_keeps_focus_valid(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let opener = compact_focus(&tabs, cx, first_id);
    reveal_by_keyboard(cx, &opener);
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.sidebar.reveal(), Reveal::Shown);
        assert!(tabs.sidebar_row_focus.borrow()[&first_id].is_focused(window));
        assert_eq!(tabs.sidebar.choice(), Some(false), "a reveal never pins");
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert!(tabs.read(cx).compact_row_focus.borrow()[&first_id].is_focused(window));
    });
    // Escape's target stays put rather than revealing again.
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));
    // The toggle reveals too, at the reveal's own toggle.
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown);
        assert!(tabs.read(cx).panel_toggle_focus.is_focused(window));
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    reveal_by_keyboard(cx, &opener);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first_id, window, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert!(tabs.read(cx).sidebar.opener().is_none());
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}

#[gpui::test]
fn collapsed_entries_activate_directly_and_hover_reveals_full_list(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let second = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let compact = |id| Box::leak(format!("compact-tab-{id}").into_boxed_str()) as &'static str;
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;

    // A click before hover intent activates without revealing.
    click_toolbar(cx, compact(first));
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), first);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
    // A pointer passing over the rail does not reveal.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));

    reveal_by_hover(&tabs, cx);
    let overlay = cx.debug_bounds("sidebar-reveal").unwrap();
    assert_eq!(overlay.size.width, px(PANEL_WIDTH));
    let editor_left = cx.update(|_, cx| tabs.read(cx).sidebar_scroll.bounds().left());
    assert!(
        editor_left < px(40.),
        "reveal overlays from the window edge"
    );
    // Rows in the reveal activate and keep it open while the pointer is inside.
    click_toolbar(cx, row(second));
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), second);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown);
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(false));
    });
    // Leaving starts a grace period that re-entering cancels.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    hover(cx, gpui::point(px(100.), px(400.)));
    wait(cx, 400);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    wait(cx, 200);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert_eq!(tabs.read(cx).tabs.active(), second);
    });
    assert!(cx.debug_bounds("sidebar-reveal").is_none());
    // The reveal's toggle pins the sidebar open.
    reveal_by_hover(&tabs, cx);
    click_toolbar(cx, "sidebar-toggle");
    cx.update(|_, cx| {
        assert!(tabs.read(cx).sidebar.visible());
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(true));
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
}

#[gpui::test]
fn menu_routes_save_save_as_and_open_to_the_active_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first_path = dir.path().join("first.md");
    let copy_path = dir.path().join("copy.md");
    let other_path = dir.path().join("other.md");
    std::fs::write(&other_path, "other document").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |v, cx| {
        v.editor.update(cx, |e, cx| e.set_text("first version", cx))
    });
    cx.run_until_parked();
    more(cx, "more-save");
    cx.simulate_new_path_selection(|_| Some(first_path.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(&first_path).unwrap(),
        "first version"
    );
    view.update(cx, |v, cx| {
        v.editor
            .update(cx, |e, cx| e.set_text("updated version", cx))
    });
    cx.run_until_parked();
    more(cx, "more-save");
    assert_eq!(
        std::fs::read_to_string(&first_path).unwrap(),
        "updated version"
    );
    more(cx, "more-save-as");
    cx.simulate_new_path_selection(|_| Some(copy_path.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(&copy_path).unwrap(),
        "updated version"
    );
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).session.document.path.as_ref(),
            Some(&dunce::canonicalize(&copy_path).unwrap())
        )
    });
    // Open has no header button: ⌘O, Home and the palette (ADR 0048).
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![other_path]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let opened = tabs.read(cx).active_view().unwrap();
        assert_ne!(opened.entity_id(), view.entity_id());
        assert_eq!(opened.read(cx).editor.read(cx).text(), "other document");
    });
}

#[gpui::test]
fn document_header_keeps_its_actions_in_a_narrow_window_in_both_themes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna Petrova signed.", cx))
    });
    cx.simulate_resize(size(px(720.), px(480.)));
    for theme in [Theme::Dark, Theme::Light] {
        for expanded in [false, true] {
            for source_only in [false, true] {
                tabs.update(cx, |tabs, cx| {
                    tabs.theme.set(theme);
                    tabs.sidebar.choose(Some(expanded));
                    cx.notify();
                });
                view.update(cx, |view, cx| {
                    view.source_only = source_only;
                    cx.notify();
                });
                for _ in 0..3 {
                    draw(cx);
                    cx.run_until_parked();
                }
                // One header row (ADR 0048): ⋯ and the primary action inside it.
                // Collapsed, it is the title row, level with the traffic lights.
                let header = cx.debug_bounds("document-toolbar").unwrap();
                let row = if expanded {
                    style::HEADER_HEIGHT
                } else {
                    style::CARD_MARGIN + style::HEADER_HEIGHT
                };
                assert_eq!(header.size.height, px(row));
                let primary = if source_only {
                    "primary-convert"
                } else {
                    "Pseudonymize"
                };
                for selector in ["header-more", primary] {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    assert!(
                        header.contains(&bounds.origin) && header.contains(&bounds.bottom_right()),
                        "{selector} {bounds:?} outside {header:?}"
                    );
                    assert!(bounds.right() <= px(720.));
                }
                // Settings and the theme control sit at the sidebar's foot.
                assert_eq!(cx.debug_bounds("theme-toggle").is_some(), expanded);
                assert!(cx.debug_bounds("Copy Markdown").is_none());
            }
        }
    }
}
#[gpui::test]
fn new_follows_the_last_document_and_context_close_holds_the_reveal(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;
    let compact = |id| Box::leak(format!("compact-tab-{id}").into_boxed_str()) as &'static str;
    // Expanded: a full-width row directly under the last document.
    let plus = cx.debug_bounds("sidebar-new").unwrap();
    let last = cx.debug_bounds(row(first_id)).unwrap();
    assert!(plus.top() >= last.bottom() && plus.top() < last.bottom() + px(8.));
    assert!(plus.size.width > px(150.));
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    let second_id = cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        tabs.read(cx).tabs.active()
    });
    assert!(cx.debug_bounds("sidebar-new").unwrap().top() > plus.top());

    // Collapsed: a square below the compact entries.
    collapse(&tabs, cx);
    let plus = cx.debug_bounds("sidebar-new-collapsed").unwrap();
    assert!(plus.top() >= cx.debug_bounds(compact(second_id)).unwrap().bottom());
    assert_eq!(plus.size.width, px(32.));
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));

    // The context menu keeps the reveal open after the pointer leaves it.
    reveal_by_hover(&tabs, cx);
    let first = cx.debug_bounds(row(first_id)).unwrap();
    cx.simulate_mouse_down(first.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(first.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);
    let close = cx.debug_bounds("context-close-tab").unwrap();
    hover(cx, close.center());
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        assert!(tabs.read(cx).tab_menu.is_none());
    });
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));
}

#[gpui::test]
fn markdown_scrollbar_drags_without_changing_preview_scroll(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1200.), px(800.)));
    let view = active(&tabs, cx);
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    view.update_in(cx, |view, window, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("# Heading\n\nA paragraph with words.\n\n".repeat(200), cx)
        });
        view.open_path(pdf, window, cx);
    });
    cx.run_until_parked();
    let draw = |cx: &mut VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    };
    draw(cx);
    let preview = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    let before = cx.update(|_, cx| preview.read(cx).reading_position());
    let thumb = cx
        .debug_bounds("markdown-scrollbar")
        .expect("overflowing Markdown scrollbar");
    cx.simulate_mouse_down(thumb.center(), gpui::MouseButton::Left, Default::default());
    for y in [40., 100.] {
        cx.simulate_mouse_move(
            thumb.center() + gpui::point(px(0.), px(y)),
            Some(gpui::MouseButton::Left),
            Default::default(),
        );
    }
    cx.simulate_mouse_up(
        thumb.center() + gpui::point(px(0.), px(100.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).scroll.offset().y < px(-100.));
        assert_eq!(view.read(cx).scroll.offset().x, px(0.));
        assert_eq!(preview.read(cx).reading_position(), before);
    });
}

#[gpui::test]
fn settings_keyboard_traversal_does_not_expand_and_restores_opener(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    let opener = cx.update(|window, cx| {
        let opener = tabs.read(cx).settings_focus.clone();
        window.focus(&opener, cx);
        opener
    });
    click_toolbar(cx, "Settings");
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    for _ in 0..25 {
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            assert!(gpui::Focusable::focus_handle(panel.read(cx), cx).contains_focused(window, cx));
            assert!(!panel.read(cx).advanced);
        });
    }
    cx.simulate_keystrokes("shift-tab escape");
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open);
        assert!(opener.is_focused(window));
    });
}

#[gpui::test]
fn narrow_original_switch_and_divider_preserve_source_and_session(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |v, cx| {
        v.editor
            .update(cx, |e, cx| e.set_text("# Draft\n\nOriginal words", cx));
        v.source_only = false;
        cx.notify();
    });
    let (source, revision) = cx.update(|_, cx| {
        (
            view.read(cx).editor.read(cx).text().to_owned(),
            view.read(cx).editor.read(cx).revision(),
        )
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let divider = cx.debug_bounds("preview-divider").unwrap();
    cx.simulate_mouse_down(
        divider.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        divider.center() + gpui::point(px(80.), px(0.)),
        Some(gpui::MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(
        divider.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    let ratio = cx.update(|_, cx| view.read(cx).preview.split_ratio.unwrap());
    assert!(ratio > 0.5);
    cx.simulate_resize(size(px(640.), px(480.)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    for theme in [Theme::Dark, Theme::Light] {
        view.update(cx, |v, cx| {
            v.theme.set(theme);
            v.editor.update(cx, |e, cx| {
                e.set_markdown_style(style::markdown_style(theme), cx)
            });
            v.sync_pii_theme(cx);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        for selector in [
            "header-more",
            "header-source",
            "Pseudonymize",
            "Settings",
            "theme-toggle",
            "Markdown",
            "Source",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= px(0.) && bounds.right() <= px(640.));
            assert!(bounds.bottom() <= px(480.));
        }
    }
    assert!(cx.debug_bounds("preview-divider").is_none());
    click_toolbar(cx, "Source");
    cx.update(|_, cx| assert!(view.read(cx).original_selected));
    click_toolbar(cx, "Markdown");
    cx.simulate_resize(size(px(1100.), px(760.)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("preview-divider").is_some());
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.editor.read(cx).text(), source);
        assert_eq!(v.editor.read(cx).revision(), revision);
        assert_eq!(v.preview.split_ratio, Some(ratio));
        assert!(v.preview.visible);
    });
    let snapshot = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    let decoded: Session = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(
        decoded
            .tabs
            .iter()
            .find(|t| t.preview_split.is_some())
            .unwrap()
            .preview_split,
        Some(ratio)
    );
}

#[gpui::test]
fn unsaved_duplicate_names_keep_single_line_rows(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    draw(cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;
    let single = cx.debug_bounds(row(first)).unwrap().size.height;
    let second = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    draw(cx);
    for id in [first, second] {
        assert_eq!(cx.debug_bounds(row(id)).unwrap().size.height, single);
    }
}

#[test]
fn duplicate_names_show_distinguishing_parent_suffixes() {
    let peers = vec![
        PathBuf::from("/contracts/first/client/sample.md"),
        PathBuf::from("/contracts/second/client/sample.md"),
    ];
    // Shown with the platform's separator.
    let suffix = |a: &str| {
        PathBuf::from(a)
            .join("client")
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(
        disambiguating_parent(&peers[0], &peers),
        Some(suffix("first"))
    );
    assert_eq!(
        disambiguating_parent(&peers[1], &peers),
        Some(suffix("second"))
    );
    // A DOCX's conversion beside an .md of that name: no folder tells them
    // apart, so the row shows the type instead of the full path.
    let same = vec![
        PathBuf::from("/contracts/client/sample.docx"),
        PathBuf::from("/contracts/client/sample.md"),
    ];
    assert_eq!(disambiguating_parent(&same[0], &same), None);
}

#[gpui::test]
fn the_badge_names_the_source_until_saved_as_markdown(cx: &mut TestAppContext) {
    let tab = |record: TabRecord| Tab::detached(1, record);
    let docx = PathBuf::from("/contracts/sample.docx");
    let conversion = tab(TabRecord {
        source_only: true,
        attachment: Some(docx.clone()),
        ..TabRecord::default()
    });
    // Saved As under another name, its DOCX still attached as the source.
    let saved = tab(TabRecord {
        markdown: PathBuf::from("/contracts/sample copy.md"),
        attachment: Some(docx.clone()),
        ..TabRecord::default()
    });
    cx.update(|cx| {
        let label = tab_label(&conversion, cx);
        assert_eq!(
            (label.format.as_str(), label.identity),
            ("DOCX", docx.clone())
        );
        let label = tab_label(&saved, cx);
        assert_eq!(label.format, "MD");
        assert_eq!(label.identity, PathBuf::from("/contracts/sample copy.md"));
        assert_eq!(label.source, Some(docx));
    });
}

#[gpui::test]
fn keyboard_reveals_last_document_in_collapsed_list(cx: &mut TestAppContext) {
    use gpui::InputEvent;
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    let last_id = tabs.update_in(cx, |tabs, window, cx| {
        for _ in 0..19 {
            tabs.new_tab(window, cx);
        }
        tabs.sidebar.choose(Some(false));
        cx.notify();
        tabs.tabs.last().unwrap().id()
    });
    draw(cx);
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    let mut reached = false;
    for _ in 0..45 {
        cx.simulate_keystrokes("tab");
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        reached = cx.update(|window, cx| {
            tabs.read(cx)
                .sidebar_row_focus
                .borrow()
                .get(&last_id)
                .is_some_and(|focus| focus.is_focused(window))
        });
        if reached {
            break;
        }
    }
    assert!(reached, "Tab must reach every document");
    let last = cx
        .debug_bounds(Box::leak(format!("tab-{last_id}").into_boxed_str()))
        .unwrap();
    cx.update(|_, cx| {
        let viewport = tabs.read(cx).sidebar_scroll.bounds();
        assert!(
            last.top() >= viewport.top() && last.bottom() <= viewport.bottom(),
            "last={last:?}, viewport={viewport:?}"
        );
    });
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
            }
            .to_platform_input(),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), last_id);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
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

#[gpui::test]
fn home_opens_several_files_and_new_tab_is_a_plain_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    // Launch with nothing to restore shows Home (ADR 0048).
    assert!(cx.debug_bounds("home").is_some());
    assert!(
        cx.debug_bounds("empty-page-local").is_some(),
        "the local note shows"
    );
    click_toolbar(cx, "home-open");
    cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
    cx.run_until_parked();
    draw(cx);
    // The hidden placeholder gives way to the opened files.
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    assert!(cx.debug_bounds("home").is_none());

    // A new tab is a plain editor: no Home, no start page.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("home").is_none());
    assert!(cx.debug_bounds("empty-page").is_none());
    cx.simulate_input("x");
    let typed = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(typed.read(cx).editor.read(cx).text(), "x"));

    // Home stays one click away and leaves the tabs alone.
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home").is_some());
    assert_eq!(tab_names(&tabs, cx).len(), 3);
}

#[gpui::test]
fn cancelled_pseudonymization_setup_does_not_tag_recents(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never-scanned.md");
    std::fs::write(&path, "Alice needs review").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    click_toolbar(cx, "Pseudonymize");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.pii.setup);
        assert!(view.pii.reviewing);
        assert!(!view.pseudonymized());
        assert!(!tabs.read(cx).shell.recents.files[0].pseudonymized);
    });
    click_toolbar(cx, "pii-setup-cancel");
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home-recent-0").is_some());
    cx.update(|_, cx| {
        assert!(!view.read(cx).pii.setup);
        assert!(!view.read(cx).pseudonymized());
        // Both the visible tag and the row's accessibility name use this flag.
        assert!(!tabs.read(cx).shell.recents.files[0].pseudonymized);
    });

    // A completed scan counts even with no candidates, and survives closing review.
    view.update(cx, |view, cx| {
        view.pii.scans.push(settings::PiiConfig::default());
        view.pii.reviewing = false;
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).shell.recents.files[0].pseudonymized));
}

#[gpui::test]
fn recents_stay_capped_and_cleared_when_open_documents_notify(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let mut views = Vec::new();
    for i in 0..13 {
        let path = dir.path().join(format!("{i}.md"));
        std::fs::write(&path, "text").unwrap();
        tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
        cx.run_until_parked();
        views.push(active(&tabs, cx));
    }
    let before = cx.update(|_, cx| tabs.read(cx).shell.recents.clone());
    assert_eq!(before.files.len(), 12);
    for _ in 0..2 {
        for view in &views {
            view.update(cx, |_, cx| cx.notify());
        }
        cx.run_until_parked();
        draw(cx);
        cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.recents, before));
    }
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    tabs.update(cx, |tabs, cx| {
        tabs.settings
            .update(cx, |_, cx| cx.emit(settings_ui::Event::ClearRecents));
    });
    for view in &views {
        view.update(cx, |_, cx| cx.notify());
    }
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).shell.recents.files.is_empty()));
}

#[gpui::test]
fn home_accepts_dropped_files_and_rejects_unsupported(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    let unknown = dir.path().join("unknown.bin");
    for path in [&a, &b, &unknown] {
        std::fs::write(path, "x").unwrap();
    }
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    let center = cx.debug_bounds("home-drop").unwrap().center();

    // A drag anywhere over Home lights the drop zone and counts the files.
    drag_files(cx, std::slice::from_ref(&unknown), center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, Some(0)));
    drop_files(cx, center);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).shell.drop, None);
        assert_eq!(tabs.read(cx).tabs.len(), 1);
    });

    drag_files(cx, &[a.clone(), b.clone(), unknown.clone()], center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, Some(2)));
    drop_files(cx, center);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    cx.update(|_, cx| {
        assert!(
            tabs.read(cx)
                .notices
                .message()
                .unwrap()
                .contains("unknown.bin")
        );
    });

    // A document is no Home drop target.
    assert!(cx.debug_bounds("home-drop").is_none());
    drag_files(cx, std::slice::from_ref(&b), center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, None));
    drop_files(cx, center);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
}

#[gpui::test]
fn sidebar_drop_opens_files_at_the_insertion_line(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b, c, d] = ["a.md", "b.md", "c.md", "d.md"].map(|name| dir.path().join(name));
    for path in [&a, &b, &c, &d] {
        std::fs::write(path, "x").unwrap();
    }
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone(), b.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let b_id = cx.update(|_, cx| tabs.read(cx).tabs[1].id());
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{b_id}").into_boxed_str()))
        .unwrap();
    // The upper half of b inserts before it; already-open a does not move.
    let upper = gpui::point(row.center().x, row.top() + row.size.height / 4.);
    drag_files(cx, &[c.clone(), a.clone()], upper);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(1)));
    drop_files(cx, upper);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "c.md", "b.md"]);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), None));

    // Below the last row the files open at the end.
    let new = cx.debug_bounds("sidebar-new").unwrap();
    let below = gpui::point(new.center().x, new.bottom() + px(40.));
    drag_files(cx, std::slice::from_ref(&d), below);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(3)));
    drop_files(cx, below);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "c.md", "b.md", "d.md"]);
}

#[gpui::test]
fn the_window_title_names_the_document_and_only_mdoc_on_home(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.md");
    std::fs::write(&a, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "a.md — mdoc"));
    cx.dispatch_action(shell::ShowHome);
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
}

#[gpui::test]
fn sidebar_refuses_files_it_cannot_open(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, unknown) = (dir.path().join("a.md"), dir.path().join("образец.xyz"));
    std::fs::write(&a, "x").unwrap();
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let a_id = cx.update(|_, cx| tabs.read(cx).tabs[0].id());
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{a_id}").into_boxed_str()))
        .unwrap();
    // No insertion line: the list says the files can't be opened.
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    cx.update(|_, cx| {
        assert!(tabs.read(cx).sidebar.drop_refused());
        assert_eq!(tabs.read(cx).file_drop_index(), None);
    });
    drop_files(cx, row.center());
    assert_eq!(tab_names(&tabs, cx), ["a.md"]);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).sidebar.drop_refused());
        assert!(
            tabs.read(cx).notices.message().is_none(),
            "nothing to report"
        );
    });

    // With an openable file beside it, the skipped one is named, not its path.
    let b = dir.path().join("b.md");
    std::fs::write(&b, "x").unwrap();
    drag_files(cx, &[b.clone(), unknown.clone()], row.center());
    cx.update(|_, cx| assert!(!tabs.read(cx).sidebar.drop_refused()));
    drop_files(cx, row.center());
    cx.update(|_, cx| {
        let notice = tabs.read(cx).notices.message().unwrap().to_owned();
        assert!(notice.contains("образец.xyz"), "{notice}");
        assert!(!notice.contains(&*dir.path().to_string_lossy()), "{notice}");
    });
    // The notice opened on its own while the document kept focus; Esc
    // there closes it.
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
    key(cx, "escape");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
}

#[gpui::test]
fn escape_closes_notices_without_focus_in_an_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| shell::bind_keys(cx));
    let view = active(&tabs, cx);
    // A source-only tab focuses its view, Home the window: neither is an
    // editor, and neither is inside Notices.
    let focus = cx.update(|_, cx| [view.read(cx).focus.clone(), tabs.read(cx).focus.clone()]);
    for handle in focus {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.notices.post("Unsupported files skipped: sample.xyz");
            tabs.title.notices_open.set(true);
            cx.notify();
            window.focus(&handle, cx);
        });
        draw(cx);
        key(cx, "escape");
        cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    }
}

#[gpui::test]
fn the_refusal_shows_above_a_long_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..40)
        .map(|i| {
            let path = dir.path().join(format!("doc{i:02}.md"));
            std::fs::write(&path, "x").unwrap();
            path
        })
        .collect();
    let unknown = dir.path().join("sample.xyz");
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_paths(paths, window, cx));
    cx.run_until_parked();
    draw(cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs[0].id());
    tabs.update(cx, |tabs, _| {
        tabs.sidebar_scroll.set_offset(gpui::point(px(0.), px(0.)))
    });
    draw(cx);
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{first}").into_boxed_str()))
        .unwrap();
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    let viewport = cx.update(|window, _| window.viewport_size());
    let hint = cx.debug_bounds("drop-refused").expect("refusal shown");
    let row_now = cx
        .debug_bounds(Box::leak(format!("tab-{first}").into_boxed_str()))
        .unwrap();
    assert!(hint.bottom() <= viewport.height);
    assert!(hint.bottom() <= row_now.top(), "above the rows");
    drop_files(cx, row.center());
}

#[gpui::test]
fn file_drag_over_collapsed_rail_reveals_the_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    collapse(&tabs, cx);
    let rail = gpui::point(px(20.), px(400.));
    drag_files(cx, std::slice::from_ref(&b), rail);
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    let list = gpui::point(px(120.), px(400.));
    cx.simulate_event(gpui::FileDropEvent::Pending { position: list });
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(1)));
    drop_files(cx, list);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    // The reveal behaves as for hover: it closes once the pointer leaves.
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    hover(cx, gpui::point(px(600.), px(400.)));
    wait(cx, 350);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(false));
    });
}

#[gpui::test]
fn spelling_menu_item_switches_the_document(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("Frist and secnd words.", cx)
        });
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(!view.read(cx).editor.read(cx).diagnostics().is_empty()));

    // Spelling lives in the header's ⋯ menu (ADR 0048).
    more(cx, "more-spelling");
    cx.update(|_, cx| {
        assert!(view.read(cx).spelling.off_here);
        assert!(!view.read(cx).more_open);
        assert!(view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
    more(cx, "more-spelling");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).spelling.off_here);
        assert!(!view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
}
#[gpui::test]
fn dictionary_view_searches_adds_removes_with_undo_and_stays_virtualized(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let shared = cx.update(|_, cx| panel.read(cx).shared.clone());
    let words: std::collections::BTreeSet<String> =
        (0..300).map(|i| format!("word{i:03}")).collect();
    cx.update(|_, cx| crate::settings_ui::dictionary::set_words(&shared, words, cx));
    click_toolbar(cx, "Settings");
    panel.update_in(cx, |panel, window, cx| panel.open_dictionary(window, cx));
    draw(cx);
    // Only the visible rows exist.
    assert!(cx.debug_bounds("remove-word-0").is_some());
    assert!(cx.debug_bounds("remove-word-250").is_none());

    let query = cx.update(|_, cx| panel.read(cx).dictionary.as_ref().unwrap().query.clone());
    let type_query = |cx: &mut VisualTestContext, text: &str| {
        query.update(cx, |query, cx| {
            query.set_value(text.into(), cx);
            cx.emit(markdown_search::SearchInputEvent::Changed);
        });
        draw(cx);
    };
    type_query(cx, "word29");
    assert!(
        cx.debug_bounds("remove-word-9").is_some(),
        "word290..word299"
    );
    assert!(cx.debug_bounds("remove-word-10").is_none());
    // Not itself a stored word, so it can be added.
    assert!(cx.debug_bounds("add-dictionary-word").is_some());
    type_query(cx, "word290");
    assert!(
        cx.debug_bounds("add-dictionary-word").is_none(),
        "exact match"
    );

    // A new word: Add, then the field clears.
    type_query(cx, "Tebriz");
    click_toolbar(cx, "add-dictionary-word");
    cx.update(|_, cx| {
        assert!(shared.borrow().words.words.contains("Tebriz"));
        assert!(query.read(cx).value().is_empty());
    });

    // Remove the first word, then Undo restores it.
    type_query(cx, "");
    click_toolbar(cx, "remove-word-0");
    cx.update(|_, _| assert!(!shared.borrow().words.words.contains("Tebriz")));
    click_toolbar(cx, "undo-remove-word");
    cx.update(|_, _| assert!(shared.borrow().words.words.contains("Tebriz")));

    // Escape leaves the dictionary first, then Settings.
    cx.dispatch_action(settings_ui::CloseSettings);
    cx.update(|_, cx| {
        assert!(panel.read(cx).dictionary.is_none());
        assert!(panel.read(cx).open);
    });
}

#[gpui::test]
fn ai_reply_round_trip_restores_in_a_new_tab_and_reapplies_aliases(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contract.md");
    let source = "Anna Petrova met Northwind Traders Ltd.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update_in(cx, |view, window, cx| {
        view.pii.reviewing = true;
        view.pii
            .review
            .add_manual(source, 0..12, Category::Person)
            .unwrap();
        view.pii
            .review
            .add_manual(source, 17..38, Category::Organization)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(view.editor.read(cx).text(), "PERSON_1 met ORG_1.");
    });
    cx.run_until_parked();

    // Inspector (ADR 0048, C): Share in the header, Copy for AI in the tab.
    click_toolbar(cx, "primary-share");
    click_toolbar(cx, "share-copy-for-ai");
    cx.update(|_, cx| {
        let copied = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copied.starts_with("Note: names and other identifying details"));
        assert!(copied.contains("such as PERSON_1 or ORG_1."));
        assert!(copied.ends_with("---\n\nPERSON_1 met ORG_1."));
        assert!(!copied.contains("Anna"), "the notice never names originals");
    });

    let reply = "**PERSON_1** owes ORG_1; person 1 and PERSON_7 disagree.";
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    more(cx, "more-paste-reply");
    let restored = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_ne!(tabs.read(cx).tabs.active(), first_id);
        let view = restored.read(cx);
        assert_eq!(
            view.editor.read(cx).text(),
            "**Anna Petrova** owes Northwind Traders Ltd; Anna Petrova and PERSON_7 disagree."
        );
        assert_eq!(view.display_name(), "Restored – contract.md");
        assert!(view.dirty(cx), "a restored reply is unsaved work");
        assert_eq!(view.pii.review.remaining(), 3);
        assert_eq!(view.unresolved_tokens(cx).len(), 1);
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "PERSON_1 met ORG_1.",
            "the source tab is untouched"
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("restore-summary").is_some());

    // Applying the proposals re-pseudonymizes with the source's aliases.
    restored.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            restored.read(cx).editor.read(cx).text(),
            "**PERSON_1** owes ORG_1; PERSON_1 and PERSON_7 disagree."
        );
    });
    // Undo returns to the restored text, then to the reply as pasted.
    for expected in [
        "**Anna Petrova** owes Northwind Traders Ltd; Anna Petrova and PERSON_7 disagree.",
        reply,
    ] {
        cx.update(|window, cx| {
            window.focus(&restored.read(cx).editor.read(cx).focus_handle(cx), cx)
        });
        cx.dispatch_action(mdoc_editor::Undo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(restored.read(cx).editor.read(cx).text(), expected));
    }

    // Unsaved restored replies are not written to the session manifest.
    cx.update(|_, cx| {
        let session = tabs.read(cx).snapshot(cx);
        assert_eq!(session.tabs.len(), 1);
    });
}

#[gpui::test]
fn paste_ai_response_needs_applied_aliases_and_clipboard_text(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    cx.simulate_input("PERSON_1 met ORG_1.");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx);
        assert!(
            view.error
                .as_deref()
                .unwrap()
                .contains("Apply replacements")
        );
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
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
        if view.case.is_some() {
            return;
        }
        view.pii.reviewing = true;
        view.pii
            .review
            .add_manual(source, 0..12, Category::Person)
            .unwrap();
        view.pii
            .review
            .add_manual(source, 17..38, Category::Organization)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    (tabs, cx, path)
}

#[gpui::test]
fn chrome_history_undoes_toolbar_apply_and_redoes_without_editor_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| bind_app_keys(cx));
    let view = active(&tabs, cx);
    let source = "Anna Petrova met Northwind Traders Ltd.";
    let applied = "PERSON_1 met ORG_1.";
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |editor, cx| editor.undo(&mdoc_editor::Undo, window, cx));
    });
    cx.run_until_parked();
    click_toolbar(cx, "primary-apply");
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), applied);
        assert!(
            window
                .context_stack()
                .iter()
                .any(|context| context.contains("UiControl"))
        );
        assert!(
            !view
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    let (undo, redo) = if cfg!(target_os = "macos") {
        ("cmd-z", "cmd-shift-z")
    } else {
        ("ctrl-z", "ctrl-shift-z")
    };
    key(cx, undo);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), source);
        assert_eq!(view.read(cx).pii.review.remaining(), 2);
    });
    key(cx, redo);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), applied);
        assert_eq!(view.read(cx).pii.review.remaining(), 0);
    });
    if !cfg!(target_os = "macos") {
        key(cx, undo);
        key(cx, "ctrl-y");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), applied));
    }
}

#[gpui::test]
fn chrome_history_targets_the_active_tab_and_leaves_home_and_settings_alone(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| bind_app_keys(cx));
    let first = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    second.update(cx, |view, cx| {
        view.editor
            .update(cx, |editor, cx| editor.replace_range(0..0, "second", cx));
    });
    cx.run_until_parked();
    // The sidebar's toggle is outside DocumentView's dispatch subtree.
    click_toolbar(cx, "sidebar-toggle");
    let undo = if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    };
    key(cx, undo);
    cx.update(|_, cx| {
        assert_eq!(second.read(cx).editor.read(cx).text(), "");
        assert_eq!(first.read(cx).editor.read(cx).text(), "PERSON_1 met ORG_1.");
    });
    key(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-shift-z"
        } else {
            "ctrl-shift-z"
        },
    );
    click_toolbar(cx, "sidebar-toggle");
    click_toolbar(cx, "sidebar-home");
    key(cx, undo);
    cx.update(|_, cx| assert_eq!(second.read(cx).editor.read(cx).text(), "second"));
    tabs.update_in(cx, |tabs, window, cx| {
        let id = tabs
            .tabs
            .iter()
            .find(|tab| tab.view() == Some(&second))
            .unwrap()
            .id();
        tabs.activate(id, window, cx);
        tabs.show_settings(window, cx);
    });
    cx.run_until_parked();
    key(cx, undo);
    cx.update(|_, cx| assert_eq!(second.read(cx).editor.read(cx).text(), "second"));
}

#[gpui::test]
fn unsaved_mapping_is_absent_after_session_restore(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    // Saving Markdown retains alias tokens, but does not save their originals.
    view.update(cx, |view, cx| view.write(path, None, cx));
    cx.run_until_parked();
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let json = std::fs::read_to_string(&manifest).unwrap();
    assert!(!json.contains("Anna Petrova"));
    assert!(!json.contains("Northwind Traders"));
    let saved = session_store::load(&manifest).unwrap();
    assert!(saved.tabs[0].case.is_none());
    let restored = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.use_case_store(mdoc_store::cases::Store::with_key(
                dir.path().join("cases"),
                &[3; 32],
            ));
            restored.restore(saved, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    let reopened = active(&restored, cx);
    cx.update(|_, cx| {
        assert_eq!(
            reopened.read(cx).editor.read(cx).text(),
            "PERSON_1 met ORG_1."
        );
        assert!(reopened.read(cx).case.is_none());
        assert!(reopened.read(cx).pii.review.active_identities().is_empty());
    });
}

#[gpui::test]
fn home_clipboard_actions_use_the_active_documents_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let source = active(&tabs, cx);
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Call Anna Petrova.".into()));
    click_toolbar(cx, "home-clipboard");
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).showing_home(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call PERSON_1.")
        );
        assert!(source.read(cx).clipboard_notice.is_some());
    });
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 owes ORG_1.".into(),
    ));
    click_toolbar(cx, "home-restore");
    let restored = active(&tabs, cx);
    assert_ne!(source, restored);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).showing_home(cx));
        assert_eq!(
            restored.read(cx).editor.read(cx).text(),
            "Anna Petrova owes Northwind Traders Ltd."
        );
    });
}

#[gpui::test]
fn a_saved_case_is_encrypted_named_and_listed_in_the_sidebar(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.update(|_, cx| assert!(tabs.read(cx).can_create_case(tab, cx)));
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (cases, _) = store.list();
    assert_eq!(cases.len(), 1);
    let case = &cases[0];
    assert_eq!(case.name, "contract");
    assert_eq!(case.members, [session_store::identity(&path)]);
    assert_eq!(case.identities[0].alias, "PERSON_1");
    assert_eq!(case.identities[0].originals, ["Anna Petrova"]);
    draw(cx);
    assert!(cx.debug_bounds("case-header").is_some());
    cx.update(|_, cx| {
        let session = tabs.read(cx).snapshot(cx);
        assert_eq!(session.tabs[0].case.as_deref(), Some(case.id.as_str()));
        assert!(!tabs.read(cx).can_create_case(tab, cx));
    });

    // Renaming an alias in the tab reaches the case, keeping the old alias.
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let anna = view.pii.review.applied()[0].step.identity;
        view.pii.review.rename_identity(anna, "BUYER").unwrap();
        view.review_changed(cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let saved = store.load(&case.id).unwrap();
    assert_eq!(saved.identities[0].alias, "BUYER");
    assert_eq!(saved.identities[0].previous, ["PERSON_1"]);
}

#[gpui::test]
fn saving_an_unsaved_document_as_a_case_saves_the_markdown_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // As a fresh conversion: Markdown never saved (#75).
    active(&tabs, cx).update(cx, |view, _| view.session.document.path = None);
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    // Cancelling the save creates no case.
    tabs.update_in(cx, |tabs, window, cx| {
        assert!(tabs.create_case(tab, window, cx).is_none())
    });
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(store().list().0.is_empty());
    // Saving it lists the saved file in the new case.
    let saved = dir.path().join("converted.md");
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    let (cases, _) = store().list();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].members, [session_store::identity(&saved)]);
    assert!(saved.exists());
}

#[gpui::test]
fn shortcuts_work_right_after_share_save_as_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| crate::bind_app_keys(cx));
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.inspector_share = true;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    draw(cx);
    let button = cx.debug_bounds("share-save-case").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(view.read(cx).case.is_some()));
    // #79: Mod+Q reaches the workspace without clicking the editor first.
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.quit_flow.quitting() || tabs.quit_flow.finishing());
    });
}

#[gpui::test]
fn a_long_case_name_keeps_the_categories_popup_close_button_inside(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        let case = &mut tabs.case_coordinator.cases_mut()[0];
        case.name = "Заявка_на_приемку_первой_партии_10000_единиц_и_ещё_немного".into();
        tabs.cases.categories_open = Some(case.id.clone());
        cx.notify();
    });
    draw(cx);
    let panel = cx.debug_bounds("case-categories").unwrap();
    let close = cx.debug_bounds("close-case-categories").unwrap();
    assert!(
        close.right() <= panel.right() && close.left() >= panel.left(),
        "× {close:?} must stay inside the popup {panel:?}"
    );
}

#[gpui::test]
fn reopening_a_case_document_restores_its_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let path = dir.path().join("contract.md");
    std::fs::write(&path, "PERSON_1 met ORG_1; PERSON_9 did not.").unwrap();
    let case = mdoc_store::cases::Case::new(
        "Petrova".into(),
        &[
            mdoc_pii::restore::ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: vec![],
                category: mdoc_pii::Category::Person,
                custom: false,
                originals: vec!["Anna Petrova".into()],
            },
            mdoc_pii::restore::ExportedIdentity {
                alias: "ORG_1".into(),
                previous: vec![],
                category: mdoc_pii::Category::Organization,
                custom: false,
                originals: vec!["Northwind Traders Ltd".into()],
            },
        ],
        Some(session_store::identity(&path)),
    );
    store.save(&case).unwrap();
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.case.as_ref().map(|l| l.name.as_str()), Some("Petrova"));
        let applied: Vec<_> = view
            .pii
            .review
            .applied()
            .iter()
            .map(|a| a.step.before.to_string())
            .collect();
        assert_eq!(applied, ["Anna Petrova", "Northwind Traders Ltd"]);
        assert!(
            view.has_applied_aliases(),
            "Copy for AI and Paste AI Response apply"
        );
    });

    // Deleting the case wipes it; the tab keeps its live mapping.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.forget_case(&case.id, window, cx)
    });
    cx.run_until_parked();
    assert!(store.load(&case.id).is_err());
    cx.update(|_, cx| {
        assert!(view.read(cx).case.is_none());
        assert_eq!(view.read(cx).pii.review.applied().len(), 2);
        assert!(
            tabs.read(cx)
                .snapshot(cx)
                .tabs
                .iter()
                .all(|t| t.case.is_none())
        );
    });
}

#[gpui::test]
fn closing_an_unsaved_mapping_offers_to_save_it_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    // The tab was edited, so the ordinary unsaved-changes question follows.
    if cx.has_pending_prompt() {
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
    }
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    assert_eq!(store.list().0.len(), 1);
}

#[gpui::test]
fn closing_an_unsaved_conversion_as_a_case_saves_it_then_closes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    active(&tabs, cx).update(cx, |view, _| view.session.document.path = None);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save as Case");
    cx.run_until_parked();
    let saved = dir.path().join("converted.md");
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(tab)));
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    let (cases, _) = store.list();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].members, [session_store::identity(&saved)]);
}

#[gpui::test]
fn replies_join_the_source_case_and_save_as_moves_the_member(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let source = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(source, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());

    // Save As: the case now names only the new path.
    let moved = dir.path().join("moved.md");
    std::fs::copy(&path, &moved).unwrap();
    let view = active(&tabs, cx);
    view.update(cx, |view, _| {
        view.session.document.path = Some(session_store::identity(&moved))
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.event(source, &TabEvent::Save(SaveEvent::Saved), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).case_coordinator.cases()[0].members,
            [session_store::identity(&moved)]
        );
    });

    // A reply pasted from a case tab joins the case, right after its source.
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(source, window, cx));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 agrees.".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    let reply = active(&tabs, cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(
            tabs.tabs[1].id(),
            tabs.tabs.active(),
            "the reply follows its source"
        );
        let link = reply.read(cx).case.clone().unwrap();
        assert_eq!(link.id, case);
        assert_eq!(
            reply.read(cx).editor.read(cx).text(),
            "Anna Petrova agrees."
        );
    });
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
        view.pii.reviewing = true;
        for (original, category) in names {
            let start = text.find(original).unwrap();
            view.pii
                .review
                .add_manual(text, start..start + original.len(), *category)
                .unwrap();
        }
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| tabs.read(cx).tabs.active())
}

#[gpui::test]
fn joining_a_case_renumbers_proposes_and_follows_renames(cx: &mut TestAppContext) {
    use mdoc_pii::Category::{Organization, Person};
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_1 wrote to Anna Petrova."
        )
    });

    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = second_view.read(cx);
        assert_eq!(view.case.as_ref().unwrap().id, case);
        assert_eq!(
            view.editor.read(cx).text(),
            "PERSON_2 wrote to Anna Petrova.",
            "Boris takes the next free number"
        );
        assert_eq!(view.pii.review.remaining(), 1, "Anna is proposed");
        let tabs = tabs.read(cx);
        assert!(
            tabs.notices
                .message()
                .unwrap()
                .contains("1 alias renumbered")
        );
        let case = &tabs.case_coordinator.cases()[0];
        assert!(case.identities.iter().any(|i| i.alias == "PERSON_2"));
    });
    second_view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to PERSON_1."
        )
    });

    // A rename in the first document reaches the second as its own edit.
    let first_view = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(first)
            .and_then(|t| t.view().cloned())
            .unwrap()
    });
    first_view.update(cx, |view, cx| {
        let anna = view.pii.review.identity_by_alias("PERSON_1").unwrap();
        view.retag_identities(&[(anna, "BUYER".into(), Person, true)], cx)
            .unwrap();
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            first_view.read(cx).editor.read(cx).text(),
            "BUYER met ORG_1."
        );
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to BUYER."
        );
    });
    // Undo in the second document is its own step.
    cx.update(|window, cx| {
        window.focus(&second_view.read(cx).editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            second_view.read(cx).editor.read(cx).text(),
            "PERSON_2 wrote to PERSON_1."
        )
    });

    // Folding the case is remembered in the session.
    tabs.update(cx, |tabs, _| {
        tabs.cases.collapsed.insert(case.clone());
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).snapshot(cx).collapsed_cases,
            std::slice::from_ref(&case)
        )
    });
}

#[gpui::test]
fn a_term_added_to_the_case_is_proposed_in_every_member(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova hired Project Bluebird.",
        &[("Anna Petrova", Person)],
    );
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        tabs.add_case_term(first, "Project Bluebird", cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let case = &tabs.case_coordinator.cases()[0];
        assert!(
            case.identities
                .iter()
                .any(|i| i.originals == ["Project Bluebird"])
        );
        let view = view.read(cx);
        assert_eq!(view.pii.review.remaining(), 1, "the new term is proposed");
        assert_eq!(view.case_names.len(), 2);
    });
}

#[gpui::test]
fn a_dropped_folder_opens_its_files_as_one_case(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Petrova v Northwind");
    std::fs::create_dir_all(folder.join("drafts")).unwrap();
    std::fs::write(folder.join("b.md"), "Second").unwrap();
    std::fs::write(folder.join("a.md"), "First").unwrap();
    std::fs::write(folder.join("notes.bin"), [0u8, 1, 2]).unwrap();
    std::fs::write(folder.join(".draft.md"), "Hidden").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        tabs.open_paths(vec![folder.clone()], window, cx)
    });
    cx.run_until_parked();
    // Members join when their Markdown loads; activate the second too.
    let ids: Vec<_> = cx.update(|_, cx| tabs.read(cx).tabs.iter().map(|t| t.id()).collect());
    for id in &ids {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(*id, window, cx));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.case_coordinator.cases().len(), 1);
        let case = &tabs.case_coordinator.cases()[0];
        assert_eq!(case.name, "Petrova v Northwind");
        let names: Vec<_> = tabs
            .tabs
            .iter()
            .filter_map(|t| t.view())
            .map(|v| {
                let v = v.read(cx);
                assert_eq!(v.case.as_ref().unwrap().id, case.id);
                v.display_name()
            })
            .collect();
        assert_eq!(
            names,
            ["a.md", "b.md"],
            "sorted by name; unsupported skipped"
        );
        assert_eq!(case.members.len(), 2, "members are recorded as they join");
        let notice = tabs.notices.message().unwrap();
        assert!(notice.contains("1 subfolder") && notice.contains("1 hidden file"));
    });
}

#[gpui::test]
fn clipboard_names_and_case_categories_follow_the_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Call Anna Petrova at Northwind Traders Ltd today.".into(),
    ));
    view.update_in(cx, |view, window, cx| {
        view.pseudonymize_clipboard(&PseudonymizeClipboard, window, cx)
    });
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call PERSON_1 at ORG_1 today.")
        );
        assert!(
            view.read(cx)
                .clipboard_notice
                .as_deref()
                .unwrap()
                .contains("Replaced 2 names")
        );
    });

    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    let case = cx.update(|_, cx| tabs.read(cx).case_coordinator.cases()[0].id.clone());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Address, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .case_skip
                .contains(&mdoc_pii::Category::Address)
        );
        assert_eq!(tabs.read(cx).case_coordinator.cases()[0].skip, ["ADDRESS"]);
    });
    // Leaving the case ends its category switches for this tab.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.remove_from_case(tab, window, cx)
    });
    cx.update(|_, cx| assert!(view.read(cx).case_skip.is_empty()));
    // Before/after shows originals beside aliases.
    view.update_in(cx, |view, window, cx| view.toggle_compare(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("compare-view").is_some());
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

#[gpui::test]
fn saving_a_case_explains_the_keychain_before_asking(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    // Not now cancels: nothing is saved and the key is not read.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        assert!(tabs.create_case(tab, window, cx).is_none());
    });
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.cases.test_open.is_some() && !tabs.cases.explainer);
    });
    assert!(store().list().0.is_empty());
    // Continue reads the key, then saves the case it was opened for.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_none());
    assert_eq!(store().list().0.len(), 1);
    // The key is open now: later case actions do not explain again.
    cx.update(|_, cx| assert!(tabs.read(cx).case_key_banner().is_none()));
}

#[gpui::test]
fn case_members_restored_at_launch_wait_for_unlock(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        // The first attempt is declined, the second allowed.
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    // Nothing asked at launch: the member opened without its mapping, kept
    // its case link, and the bar offers Unlock.
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).case.is_none());
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.locked() && !tabs.cases.explainer);
        assert!(tabs.cases.test_open.is_some());
        assert_eq!(
            tabs.snapshot(cx).tabs.last().unwrap().case.as_deref(),
            Some(case.id.as_str())
        );
    });
    let unlock = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    // Declined: still locked, and the bar offers Try again.
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.denied());
        assert_eq!(tabs.case_key_banner().map(|b| b.1), Some("Try again…"));
        assert_eq!(
            tabs.snapshot(cx).tabs.last().unwrap().case.as_deref(),
            Some(case.id.as_str())
        );
    });
    tabs.update(cx, |tabs, _| {
        tabs.cases.test_open = Some(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
    });
    // The declined action waits: the notices popover reopens on Try again.
    assert!(cx.debug_bounds("notices-popover").is_some());
    let unlock = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).case.as_ref().map(|l| l.name.as_str()),
            Some("Petrova")
        );
        assert!(tabs.read(cx).case_key_banner().is_none());
    });
    assert!(cx.debug_bounds("unlock-cases").is_none());
}

#[gpui::test]
fn try_again_after_a_declined_keychain_dialog_finishes_the_action(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(store().list().0.is_empty());
    // Try again → Continue saves the case the user asked for.
    tabs.update(cx, |tabs, _| tabs.cases.test_open = Some(Ok(store())));
    let retry = cx.debug_bounds("unlock-cases").unwrap();
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(store().list().0.len(), 1);
    cx.update(|_, cx| assert!(tabs.read(cx).case_key_banner().is_none()));
}

#[gpui::test]
fn a_second_case_action_waits_with_the_first_for_the_keychain(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let store = || mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(store()));
        tabs.create_case(tab, window, cx);
        // Open Case… while the save waits, and the save requested again.
        tabs.show_case_list(window, cx);
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    // Both ran, in order, and the repeated save made no second case.
    assert_eq!(store().list().0.len(), 1);
    cx.update(|_, cx| assert!(tabs.read(cx).cases.list_open));
}

#[gpui::test]
fn the_keychain_explainer_is_modal_and_returns_keyboard_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let view = active(&tabs, cx);
    let before = view.update_in(cx, |view, window, cx| {
        let focus = view.editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        focus
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.create_case(tab, window, cx);
    });
    draw(cx);
    // Focus starts on Continue; Tab and Shift-Tab alternate between both
    // buttons, never sticking on Not now (dry-run H18 / A4).
    let continue_focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| tabs.read(cx).explainer_focus.is_focused(window))
    };
    assert!(continue_focused(cx), "Continue starts focused");
    for (keys, on_continue) in [
        ("tab", false),
        ("tab", true),
        ("tab", false),
        ("shift-tab", true),
        ("shift-tab", false),
        ("shift-tab", true),
    ] {
        key(cx, keys);
        assert_eq!(continue_focused(cx), on_continue, "{keys}");
    }
    // Tab and Shift-Tab stay on the dialog's buttons.
    for keys in [
        "tab",
        "tab",
        "tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "f6",
        "shift-f6",
    ] {
        cx.simulate_keystrokes(keys);
        draw(cx);
        tabs.update_in(cx, |tabs, window, cx| {
            assert!(tabs.explainer_group.contains_focused(window, cx), "{keys}");
        });
    }
    // Window shortcuts and clicks stop at the dialog: no tab opens or closes.
    let open = cx.update(|_, cx| tabs.read(cx).tabs.len());
    let new_tab = cx
        .debug_bounds("sidebar-new")
        .expect("the sidebar's New tab button");
    cx.simulate_click(new_tab.center(), Default::default());
    cx.dispatch_action(Close);
    cx.dispatch_action(New);
    draw(cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), open);
        assert!(tabs.cases.explainer);
    });
    // Esc closes it and hands focus back to the editor.
    cx.simulate_keystrokes("escape");
    draw(cx);
    tabs.update_in(cx, |tabs, window, _| {
        assert!(!tabs.cases.explainer);
        assert!(before.is_focused(window));
    });
}

#[gpui::test]
fn locked_members_load_when_the_key_opens_without_a_dialog_later(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    let session = Session {
        tabs: vec![TabRecord {
            markdown: session_store::identity(&path),
            case: Some(case.id.clone()),
            ..TabRecord::default()
        }],
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.restore(session, window, cx);
        let restored = tabs.tabs.last().unwrap().id();
        tabs.activate(restored, window, cx);
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(tabs.read(cx).case_coordinator.locked() && view.read(cx).case.is_none())
    });
    // The key became readable without a dialog (e.g. allowed elsewhere):
    // Open Case… opens it quietly and the locked member gets its mapping.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.cases.test_may_ask = false;
        tabs.show_case_list(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(!tabs.case_coordinator.locked() && tabs.cases.list_open);
        assert_eq!(
            view.read(cx).case.as_ref().map(|l| l.name.as_str()),
            Some("Petrova")
        );
    });
}

#[gpui::test]
fn a_member_opened_by_hand_offers_unlock_and_gets_its_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, path) = saved_case(dir.path());
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        // Opened from disk: no session record names its case.
        tabs.open_paths(vec![path.clone()], window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).case.is_none()));
    // Nothing marks it as a member yet, so the notice waits behind the bell
    // (ADR 0043); the bell still offers Unlock.
    assert!(cx.debug_bounds("notices-popover").is_none());
    click_toolbar(cx, "header-notices");
    let unlock = cx
        .debug_bounds("unlock-cases")
        .expect("locked cases offer Unlock even without a tagged tab");
    cx.simulate_click(unlock.center(), Default::default());
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).case.as_ref().map(|l| l.name.as_str()),
            Some("Petrova")
        );
    });
}

#[gpui::test]
fn a_crash_report_is_previewed_before_it_is_copied(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, cx| {
        tabs.notices.load_crash(Some(
            "mdoc 0.1 — macos aarch64\nPanic at <path>/a.rs:1:1".into(),
        ));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("crash-bar").is_some());
    assert!(
        cx.debug_bounds("crash-report").is_none(),
        "nothing shown until asked"
    );
    assert!(cx.debug_bounds("crash-copy").is_none());
    click_toolbar(cx, "crash-preview");
    draw(cx);
    assert!(cx.debug_bounds("crash-report").is_some());
    click_toolbar(cx, "crash-copy");
    draw(cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        copied.as_deref(),
        Some("mdoc 0.1 — macos aarch64\nPanic at <path>/a.rs:1:1")
    );
    assert!(cx.debug_bounds("crash-bar").is_none());
    tabs.read_with(cx, |tabs, _| assert!(tabs.notices.crash().is_none()));
}

#[gpui::test]
fn command_palette_filters_and_runs_actions(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    // ⌘K opens it from the document (ADR 0048).
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_input("home");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    assert!(cx.debug_bounds("home").is_some());
    // From Home, Esc closes it again.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    cx.update(|_, cx| assert!(tabs.read(cx).shell.palette.is_none()));
    // View actions without a menu bar on Windows are in the palette too.
    let visible = tabs.read_with(cx, |tabs, _| tabs.sidebar.visible());
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("toggle sidebar");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    tabs.read_with(cx, |tabs, _| assert_eq!(tabs.sidebar.visible(), !visible));
}

#[gpui::test]
fn a_docx_source_notes_its_approximate_preview_without_a_notice(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(800.)));
    let docx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx-preview/comments.docx");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(docx, window, cx));
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    // The expected limitation sits by the Source name; the bell stays for
    // real notices (README media decision 9; ADR 0048 names the pane Source).
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        let pdf = view.read(cx).preview.pdf.clone().unwrap();
        let pdf = pdf.read(cx);
        assert_eq!(pdf.display_name(), Some("Source · comments.docx"));
        assert_eq!(
            pdf.source_note().map(|note| note.as_ref()),
            Some(crate::preview::DOCX_NOTE)
        );
    });
    assert!(cx.debug_bounds("header-notices").is_none());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).title.notices.get(), 0));
}

#[gpui::test]
fn notices_wait_behind_the_header_bell(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_none());
    // A new notice opens the popover once (ADR 0048).
    tabs.update(cx, |tabs, cx| {
        tabs.notices.post("Could not restore one tab.");
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_some());
    assert!(cx.debug_bounds("notices-popover").is_some());
    // A click elsewhere closes it; the bell stays and reopens it.
    cx.simulate_click(gpui::point(px(500.), px(500.)), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    assert!(cx.debug_bounds("header-notices").is_some());
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // The bell closes its open popover rather than reopening it.
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // The close button closes it too; the notice keeps the bell.
    click_toolbar(cx, "close-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    assert!(cx.debug_bounds("header-notices").is_some());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).title.notices.get(), 1));
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // Dismissing the last notice removes the bell.
    click_toolbar(cx, "dismiss-session-notice");
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_none());
    assert!(cx.debug_bounds("notices-popover").is_none());
}

#[gpui::test]
fn locked_cases_notice_opens_only_when_a_locked_case_is_needed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.md");
    std::fs::write(&plain, "Nothing to restore here.").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.open_paths(vec![plain], window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    // A plain document: the notice waits behind the bell (ADR 0043).
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.locked());
        assert_eq!(tabs.title.notices.get(), 1);
        assert!(!tabs.title.notices_open.get());
    });
    assert!(cx.debug_bounds("header-notices").is_some());
    assert!(cx.debug_bounds("notices-popover").is_none());
    // Pasting an AI reply into a tab without a mapping needs the locked
    // cases: the notice opens.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    cx.dispatch_action(crate::workspace::PasteAiResponse);
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    assert!(cx.debug_bounds("unlock-cases").is_some());
}

#[gpui::test]
fn restore_from_home_shows_unlock_over_an_earlier_notice(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update(cx, |tabs, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        // An earlier notice the user closed without dismissing.
        tabs.notices.post("The clipboard has no text.");
        cx.notify();
    });
    draw(cx);
    tabs.update(cx, |tabs, cx| {
        tabs.title.notices_open.set(false);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    tabs.update_in(cx, |_, window, cx| {
        window.dispatch_action(Box::new(crate::workspace::PasteAiResponse), cx)
    });
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).case_coordinator.locked()));
    assert!(cx.debug_bounds("notices-popover").is_some());
    assert!(cx.debug_bounds("unlock-cases").is_some());
}

#[gpui::test]
fn restore_from_a_blank_tab_looks_for_locked_cases(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.new_tab(window, cx);
    });
    cx.simulate_input("PERSON_1 wrote back.");
    draw(cx);
    cx.update(|_, cx| {
        assert!(
            !tabs.read(cx).case_coordinator.locked(),
            "nothing looked yet"
        )
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    cx.dispatch_action(crate::workspace::PasteAiResponse);
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).case_coordinator.locked()));
    assert!(cx.debug_bounds("unlock-cases").is_some());
}

/// Public synthetic input through the real editor/review/case host. Timings are
/// observations until release/native budgets are qualified (ADR 0049).
#[gpui::test]
#[ignore = "review/diagnostics/compare/case CPU matrix; run serially on an idle machine"]
fn review_host_performance(cx: &mut TestAppContext) {
    use crate::perf_tests::{draw, report};
    use mdoc_pii::Category;
    use std::time::Instant;
    let stress = std::env::var("MDOC_PERF_STRESS").is_ok_and(|v| v == "1");
    let (entities, mentions) = if stress { (64, 20_000) } else { (16, 160) };
    let names: Vec<_> = (0..entities).map(|i| format!("ClientName{i:04}")).collect();
    let wordings: Vec<_> = names
        .iter()
        .map(|n| (n.as_str(), Category::Person))
        .collect();
    let mut text: String = (0..mentions)
        .map(|i| {
            format!(
                "{} agrees to the agreemnt. Проверить условия.\n\n",
                names[i % entities]
            )
        })
        .collect();
    if stress {
        text.push_str(&" ".repeat(2_097_152_usize.saturating_sub(text.len())));
    }
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
    });
    // Layout-only baseline: the same text without review state, with the
    // Replacements panel open so the editor width matches the review phases.
    let plain = dir.path().join("plain.md");
    std::fs::write(&plain, &text).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![plain], window, cx)
    });
    cx.run_until_parked();
    let plain_view = active(&tabs, cx);
    plain_view.update_in(cx, |view, window, cx| {
        view.inspector_share = false;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        draw(cx);
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let plain_width =
        plain_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from));
    eprintln!(
        "REVIEW_PERF phase=plain_layout bytes={} editor_width={plain_width:?}",
        text.len()
    );
    report("review_plain_layout", &mut samples);
    plain_view.read_with(cx, |view, _| {
        assert!(view.pii.review.candidates().is_empty() && view.pii.review.applied().is_empty());
    });
    let first = open_applied(&tabs, cx, dir.path(), "first.md", &text, &wordings);
    let first_view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    let case = tabs.read_with(cx, |tabs, _| tabs.case_coordinator.cases()[0].id.clone());
    for name in ["second.md", "third.md"] {
        let member = open_applied(&tabs, cx, dir.path(), name, &text, &wordings);
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.join_case(member, &case, window, cx)
        });
        cx.run_until_parked();
    }
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first, window, cx));
    for phase in ["proposed_panel", "applied_panel", "before_after"] {
        first_view.update_in(cx, |view, window, cx| {
            match phase {
                "proposed_panel" => view
                    .editor
                    .update(cx, |editor, cx| editor.undo_step(window, cx)),
                "applied_panel" => view.apply_all_pii(&PiiApplyAll, window, cx),
                _ => view.toggle_compare(window, cx),
            }
            view.inspector_share = false;
            if !view.replacements_open() {
                view.toggle_replacements_panel(window, cx);
            }
        });
        cx.run_until_parked();
        draw(cx);
        draw(cx);
        let mut samples = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            draw(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        eprintln!(
            "REVIEW_PERF phase={phase} bytes={} entities={entities} mentions={mentions} editor_width={:?}",
            text.len(),
            first_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from))
        );
        report(&format!("review_{phase}"), &mut samples);
        assert_eq!(
            first_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from)),
            plain_width,
            "review phases and the layout baseline share one editor width"
        );
        first_view.read_with(cx, |view, _| {
            assert_eq!(
                view.pii.review.candidates().len() + view.pii.review.applied().len(),
                mentions
            );
            if phase == "proposed_panel" {
                assert_eq!(view.pii.review.candidates().len(), mentions);
            } else {
                assert_eq!(view.pii.review.applied().len(), mentions);
            }
            assert_eq!(view.compare.is_some(), phase == "before_after");
            assert!(
                !view.inspector_share,
                "measure the Review tab, not the post-Apply Share tab"
            );
        });
    }
    let start = Instant::now();
    first_view.update_in(cx, |view, window, cx| {
        view.close_compare(window, cx);
        let id = view.pii.review.identity_by_alias("PERSON_1").unwrap();
        view.retag_identities(&[(id, "CLIENT".into(), Category::Person, true)], cx)
            .unwrap();
    });
    tabs.update(cx, |tabs, cx| tabs.case_changed(first, cx));
    cx.run_until_parked();
    draw(cx);
    eprintln!(
        "REVIEW_PERF case_fanout_3_members_ms={:.3} rss_kib={:?}",
        start.elapsed().as_secs_f64() * 1000.,
        crate::perf_tests::rss_kib()
    );
    tabs.read_with(cx, |tabs, cx| {
        let members: Vec<_> = tabs
            .tabs
            .iter()
            .filter_map(|tab| tab.view())
            .filter(|view| {
                view.read(cx)
                    .case
                    .as_ref()
                    .is_some_and(|link| link.id == case)
            })
            .collect();
        assert_eq!(members.len(), 3);
        for view in members {
            assert!(
                view.read(cx)
                    .pii
                    .review
                    .identity_by_alias("CLIENT")
                    .is_some()
            );
        }
    });
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

#[gpui::test]
fn case_quit_failure_can_cancel_without_losing_mappings(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    tabs.read_with(cx, |tabs, cx| {
        assert!(!tabs.quit_flow.finishing() && !tabs.quit_flow.quitting());
        assert_eq!(tabs.case_coordinator.cases()[0].id, case);
        assert_eq!(
            tabs.active_view()
                .unwrap()
                .read(cx)
                .pii
                .review
                .applied()
                .len(),
            2
        );
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("blocked-store")).unwrap(),
        "keep this file"
    );
    // Cancelling retains the pending write; the next quit persists it.
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let store = mdoc_store::cases::Store::with_key(dir.path().join("blocked-store"), &[3; 32]);
    assert!(!store.load(&case).unwrap().skip.is_empty());
}

#[gpui::test]
fn case_quit_retry_saves_the_latest_pending_revision(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    // A newer mapping policy queued while recovery is open must win.
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Organization, cx)
    });
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let saved = mdoc_store::cases::Store::with_key(dir.path().join("blocked-store"), &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 2);
}

#[gpui::test]
fn case_quit_can_explicitly_discard_a_failed_final_save(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Quit without saving mappings");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("blocked-store")).unwrap(),
        "keep this file"
    );
}

#[gpui::test]
fn case_quit_store_open_failure_retains_work_for_retry(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::NoKeyStore));
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("retry-store"),
            &[3; 32],
        ))
    });
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let saved = mdoc_store::cases::Store::with_key(dir.path().join("retry-store"), &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 1);
}

#[gpui::test]
fn abandoned_quit_does_not_block_later_case_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    let blocked = dir.path().join("blocked-store");
    std::fs::remove_file(&blocked).unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, "preserve").unwrap();
    // Waiting on final case saves when the tab-list write fails; Cancel
    // abandons the quit.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.session_writer
            .set_path(Some(parent_file.join("session.json")));
        tabs.quit_flow.settle([]);
        tabs.set_case_quit_window(Some(window.window_handle()));
        tabs.checkpoint(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    tabs.read_with(cx, |tabs, _| {
        assert!(!tabs.quit_flow.finishing());
        assert!(!tabs.case_quit_pending());
    });
    // An ordinary failure afterwards is a notice, not a quit prompt.
    std::fs::write(&blocked, "keep this file").unwrap();
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    tabs.read_with(cx, |tabs, _| {
        assert!(
            tabs.notices
                .message()
                .is_some_and(|n| n.contains("Could not save the case"))
        );
        assert!(!tabs.case_quit_pending());
    });
    // The queue still drains once the store is writable again.
    std::fs::remove_file(&blocked).unwrap();
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Organization, cx)
    });
    cx.run_until_parked();
    let saved = mdoc_store::cases::Store::with_key(blocked, &[3; 32])
        .load(&case)
        .unwrap();
    assert_eq!(saved.skip.len(), 2);
}

#[gpui::test]
fn failed_write_of_a_removed_case_keeps_the_queue_moving(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, removed) = case_quit_fixture(cx, dir.path());
    let other = mdoc_store::cases::Case::new("Other".into(), &[], None);
    let good = dir.path().join("good-store");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.case_coordinator.cases_mut().push(other.clone());
        // The removed case's write is in flight against the blocked store;
        // the other case queues behind it and targets a writable one.
        tabs.toggle_case_category(&removed, mdoc_pii::Category::Person, cx);
        tabs.use_case_store(mdoc_store::cases::Store::with_key(good.clone(), &[3; 32]));
        tabs.toggle_case_category(&other.id, mdoc_pii::Category::Person, cx);
        tabs.forget_case(&removed, window, cx);
    });
    cx.run_until_parked();
    let saved = mdoc_store::cases::Store::with_key(good, &[3; 32])
        .load(&other.id)
        .unwrap();
    assert_eq!(saved.skip.len(), 1);
}

#[gpui::test]
fn case_quit_checks_edits_made_while_final_saves_are_pending(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            let end = editor.text().len();
            editor.replace_range(end..end, " New edit.", cx);
        });
    });
    cx.run_until_parked();
    std::fs::remove_file(dir.path().join("blocked-store")).unwrap();
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "the new Markdown edit needs its own save decision"
    );
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    assert!(
        std::fs::read_to_string(dir.path().join("contract.md"))
            .unwrap()
            .ends_with(" New edit.")
    );
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

#[gpui::test]
fn f6_cycles_sidebar_toolbar_and_editor_both_ways(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    assert!(editor_focused(&tabs, cx));
    // Editor → sidebar (wrapping) → toolbar → editor.
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    // Backwards: editor → toolbar → sidebar.
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(in_sidebar(&tabs, cx));
}

#[gpui::test]
fn f6_returns_to_the_control_last_focused_in_a_region(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    // Into the sidebar, then Tab to its second control.
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    let first = cx.update(|window, cx| window.focused(cx));
    key(cx, "tab");
    let second = cx.update(|window, cx| window.focused(cx));
    assert_ne!(first, second);
    assert!(in_sidebar(&tabs, cx));
    // On through the toolbar to the editor and back: the same control.
    key(cx, "f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), second);
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

#[gpui::test]
fn f6_into_the_collapsed_rail_returns_to_the_last_focused_tab(cx: &mut TestAppContext) {
    // ADR 0062: the rail lands on the tab that last had focus, not Search.
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    draw(cx);
    // Expanded sidebar: focus the first tab's row, then leave by F6.
    tabs.update_in(cx, |tabs, window, cx| {
        let row = tabs.sidebar_row_focus.borrow()[&first].clone();
        window.focus(&row, cx);
    });
    draw(cx);
    key(cx, "f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    // Collapse: F6 comes back to that tab in the rail.
    collapse(&tabs, cx);
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    assert!(
        tab_row_focused(&tabs, first, cx),
        "the rail lands on the tab"
    );
    // Out and back again, still in the rail.
    key(cx, "f6");
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(tab_row_focused(&tabs, first, cx), "and again after leaving");
    // Expand: F6 lands on the same tab in the full list.
    key(cx, "f6");
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(true, Some(true));
        cx.notify();
    });
    draw(cx);
    key(cx, "f6");
    cx.update(|window, cx| {
        assert!(tabs.read(cx).sidebar_row_focus.borrow()[&first].is_focused(window));
    });
}

#[gpui::test]
fn escape_in_the_toolbar_or_sidebar_returns_to_the_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn tab_from_home_enters_its_controls(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    assert!(cx.update(|_, cx| tabs.read(cx).showing_home(cx)));
    tabs.update_in(cx, |tabs, window, cx| window.focus(&tabs.focus, cx));
    draw(cx);
    key(cx, "tab");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .home()
            .contains_focused(window, cx)
    }));
    let home = cx.update(|window, cx| window.focused(cx));
    assert_ne!(
        home,
        cx.update(|_, cx| Some(tabs.read(cx).focus_regions.home().clone()))
    );
    // F6 on Home: sidebar, then Home again.
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .home()
            .contains_focused(window, cx)
    }));
}

#[gpui::test]
fn sidebar_tab_list_moves_with_arrows_and_closes_with_backspace(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    let listed = cx.update(|_, cx| tabs.read(cx).listed_tabs(cx));
    assert_eq!(listed.len(), 3);
    let row = |tabs: &Entity<Workspace>, id: u64, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            tabs.read(cx)
                .sidebar_row_focus
                .borrow()
                .get(&id)
                .is_some_and(|f| f.is_focused(window))
        })
    };
    // F6 into the sidebar lands on the list's one Tab stop: the active tab.
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    let active_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_eq!(active_id, listed[2]);
    tabs.update_in(cx, |tabs, window, cx| {
        let focus = tabs
            .sidebar_row_focus
            .borrow()
            .get(&active_id)
            .cloned()
            .unwrap();
        window.focus(&focus, cx);
    });
    draw(cx);
    key(cx, "up");
    assert!(row(&tabs, listed[1], cx));
    key(cx, "up");
    assert!(row(&tabs, listed[0], cx));
    key(cx, "up");
    assert!(row(&tabs, listed[0], cx));
    key(cx, "down");
    assert!(row(&tabs, listed[1], cx));
    // ⌫ closes the focused tab; focus moves to its neighbour.
    key(cx, "backspace");
    let now = cx.update(|_, cx| tabs.read(cx).listed_tabs(cx));
    assert_eq!(now, vec![listed[0], listed[2]]);
    assert!(row(&tabs, listed[2], cx));
    // Tab leaves the list rather than visiting each row.
    key(cx, "tab");
    assert!(!row(&tabs, listed[0], cx) && !row(&tabs, listed[2], cx));
}

#[gpui::test]
fn f6_includes_the_replacements_panel_when_open(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(cx.update(|window, cx| {
        view.read(cx)
            .replacements_focus()
            .contains_focused(window, cx)
    }));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
}

#[gpui::test]
fn f6_stays_inside_settings(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.update(|_, cx| tabs.read(cx).settings.read(cx).open));
    let before = cx.update(|window, cx| window.focused(cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), before);
    key(cx, "shift-f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), before);
}

#[gpui::test]
fn tab_leaves_the_sidebar_list_from_any_row(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    let listed = cx.update(|_, cx| tabs.read(cx).listed_tabs(cx));
    let on_row = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            tabs.read(cx)
                .sidebar_row_focus
                .borrow()
                .values()
                .any(|f| f.is_focused(window))
        })
    };
    tabs.update_in(cx, |tabs, window, cx| {
        let focus = tabs
            .sidebar_row_focus
            .borrow()
            .get(&listed[2])
            .cloned()
            .unwrap();
        window.focus(&focus, cx);
    });
    draw(cx);
    // Up to the first row, then Tab: out of the list, not to the next row.
    key(cx, "up");
    key(cx, "up");
    key(cx, "tab");
    assert!(!on_row(&tabs, cx));
    // Shift+Tab comes back to the row the arrows left it on.
    key(cx, "shift-tab");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .sidebar_row_focus
            .borrow()
            .get(&listed[0])
            .is_some_and(|f| f.is_focused(window))
    }));
}

#[gpui::test]
fn escape_closes_the_more_menu_before_returning_to_the_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    click_toolbar(cx, "header-more");
    let view = active(&tabs, cx);
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    tabs.update_in(cx, |tabs, window, cx| {
        let toolbar = tabs.active_view().unwrap().read(cx).toolbar_region.clone();
        if !toolbar.contains_focused(window, cx) {
            window.focus(&toolbar, cx);
        }
    });
    draw(cx);
    key(cx, "escape");
    assert!(!cx.update(|_, cx| view.read(cx).more_open));
    assert!(!editor_focused(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn escape_and_tab_departures_are_remembered_for_f6(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "tab");
    let second = cx.update(|window, cx| window.focused(cx));
    assert!(in_sidebar(&tabs, cx));
    // Leave with Esc, not F6; F6 back from the toolbar still lands there.
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), second);
}

#[gpui::test]
fn cycle_into_a_group_without_controls_keeps_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    let empty = cx.update(|_, cx| cx.focus_handle());
    cx.update(|window, cx| ui::cycle(window, cx, Some(&empty), false));
    assert!(editor_focused(&tabs, cx));
    cx.update(|window, cx| ui::cycle(window, cx, Some(&empty), true));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn the_more_menu_takes_focus_and_escape_returns_to_its_button(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    click_toolbar(cx, "header-more");
    let view = active(&tabs, cx);
    let in_menu = |view: &Entity<DocumentView>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.read(cx).more_menu_focus.contains_focused(window, cx))
    };
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    assert!(in_menu(&view, cx));
    // Tab moves through the rows and stays in the menu.
    for keys in ["tab", "tab", "shift-tab"] {
        key(cx, keys);
        assert!(in_menu(&view, cx), "{keys}");
        assert!(!cx.update(|window, cx| view.read(cx).more_menu_focus.is_focused(window)));
    }
    key(cx, "escape");
    cx.update(|window, cx| {
        assert!(!view.read(cx).more_open);
        assert!(view.read(cx).more_focus.is_focused(window));
    });
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn escape_in_the_notices_popover_closes_it(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.notices.post("Saved elsewhere");
        tabs.title.notices_open.set(true);
        cx.notify();
        window.focus(&tabs.notices_focus, cx);
    });
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).title.notices_open.get()));
    key(cx, "tab");
    key(cx, "escape");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn the_open_case_list_takes_focus_traps_tab_and_gives_it_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, _) = saved_case(dir.path());
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.cases.test_may_ask = false;
        tabs.case_coordinator.cases_mut().push(case);
        tabs.focus_active(window, cx);
    });
    draw(cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.show_case_list(window, cx));
    cx.run_until_parked();
    draw(cx);
    let in_list = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| tabs.read(cx).case_list_focus.contains_focused(window, cx))
    };
    assert!(cx.update(|_, cx| tabs.read(cx).cases.list_open));
    assert!(in_list(&tabs, cx));
    // Tab and Shift+Tab stay inside, through the case row and Close.
    for keys in ["tab", "tab", "tab", "shift-tab", "shift-tab"] {
        key(cx, keys);
        assert!(in_list(&tabs, cx), "{keys}");
    }
    key(cx, "escape");
    assert!(!cx.update(|_, cx| tabs.read(cx).cases.list_open));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn clicking_more_again_closes_its_menu(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    click_toolbar(cx, "header-more");
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    click_toolbar(cx, "header-more");
    assert!(!cx.update(|_, cx| view.read(cx).more_open));
    draw(cx);
    assert!(cx.update(|window, cx| view.read(cx).more_focus.is_focused(window)));
}

#[gpui::test]
fn closing_notices_with_its_button_returns_to_the_document(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.notices.post("Saved elsewhere");
        tabs.title.notices_open.set(true);
        cx.notify();
        window.focus(&tabs.notices_focus, cx);
    });
    draw(cx);
    click_toolbar(cx, "close-notices");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn opening_a_case_with_enter_closes_the_list_and_focuses_its_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (case, path) = saved_case(dir.path());
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Ok(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        )));
        tabs.cases.test_may_ask = false;
        tabs.case_coordinator.cases_mut().push(case);
        tabs.show_case_list(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("open-case-row").is_some());
    key(cx, "tab"); // Close.
    key(cx, "tab"); // The case's Open button.
    activate_control(cx);
    assert!(!cx.update(|_, cx| tabs.read(cx).cases.list_open));
    let view = active(&tabs, cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).session.document.path.clone()),
        Some(session_store::identity(&path))
    );
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn f6_skips_the_hidden_pane_in_a_narrow_window(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.source_only = false;
        view.original_selected = false;
        cx.notify();
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.toolbar_region, cx)
    });
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    click_toolbar(cx, "Source");
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.toolbar_region, cx)
    });
    key(cx, "f6");
    assert!(cx.update(|window, cx| view.read(cx).preview_region.contains_focused(window, cx)));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(cx.update(|window, cx| view.read(cx).preview_region.contains_focused(window, cx)));
    // Back to the toolbar's region, on the pane switch clicked last.
    key(cx, "shift-f6");
    assert!(cx.update(|window, cx| view.read(cx).controls_region.contains_focused(window, cx)));
}

#[gpui::test]
fn sidebar_case_actions_and_categories_work_without_a_pointer(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, path) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.write(path, None, cx));
    let case = tabs
        .update_in(cx, |tabs, window, cx| {
            tabs.create_case(tabs.tabs.active(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    draw(cx);
    let row = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        tabs.sidebar_row_focus.borrow()[&tabs.tabs.active()].clone()
    });
    cx.update(|window, cx| window.focus(&row, cx));
    key(cx, "shift-f10");
    assert!(cx.update(|_, cx| tabs.read(cx).tab_menu.is_some()));
    key(cx, "tab"); // Categories to detect.
    activate_control(cx);
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).cases.categories_open.clone()),
        Some(case.clone())
    );
    for keys in ["tab", "tab", "shift-tab", "f6", "shift-f6"] {
        key(cx, keys);
        assert!(
            cx.update(|window, cx| tabs
                .read(cx)
                .case_categories_focus
                .contains_focused(window, cx)),
            "{keys}"
        );
    }
    key(cx, "escape");
    assert!(cx.update(|_, cx| tabs.read(cx).cases.categories_open.is_none()));
    assert!(cx.update(|window, _| row.is_focused(window)));
    collapse(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let compact = compact_focus(&tabs, cx, id);
    cx.update(|window, cx| window.focus(&compact, cx));
    key(cx, "shift-f10");
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).tab_menu.map(|(id, _)| id)),
        Some(id)
    );
    key(cx, "escape");
    assert!(in_sidebar(&tabs, cx));
}

/// Tab past the toolbar (and the view switcher, part of its region) enters
/// the editor, and from the Replacements panel's last control moves on to the
/// next region; Shift+Tab from a region's first control goes back (ADR 0062).
/// Both sidebar layouts.
#[gpui::test]
fn tab_moves_between_regions_at_their_edges(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    let in_panel = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.read(cx)
                .replacements_focus()
                .contains_focused(window, cx)
        })
    };
    // Tab until focus leaves `inside`, with a bound against loops.
    let tab_out = |cx: &mut VisualTestContext, inside: &dyn Fn(&mut VisualTestContext) -> bool| {
        for _ in 0..60 {
            key(cx, "tab");
            if !inside(cx) {
                return;
            }
        }
        panic!("Tab never left the region");
    };
    for layout in ["expanded", "rail"] {
        if layout == "rail" {
            cx.dispatch_action(ToggleSidebar);
            cx.run_until_parked();
        }
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.focus_regions.forget();
            tabs.focus_active(window, cx);
        });
        draw(cx);
        // Toolbar: Shift+Tab from its first control goes to the sidebar;
        // Tab past its last control enters the editor.
        key(cx, "shift-f6");
        assert!(in_toolbar(&tabs, cx), "{layout}");
        key(cx, "shift-tab");
        assert!(
            in_sidebar(&tabs, cx),
            "{layout}: Shift+Tab from the toolbar"
        );
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        assert!(in_toolbar(&tabs, cx), "{layout}");
        // The view switcher counts as the toolbar's region.
        let toolbar_or_switcher = |cx: &mut VisualTestContext| {
            in_toolbar(&tabs, cx)
                || cx
                    .update(|window, cx| view.read(cx).switcher_region.contains_focused(window, cx))
        };
        let mut switcher = false;
        for _ in 0..60 {
            key(cx, "tab");
            if !toolbar_or_switcher(cx) {
                break;
            }
            switcher |= !in_toolbar(&tabs, cx);
        }
        assert!(switcher, "{layout}: Tab reaches the view switcher");
        assert!(editor_focused(&tabs, cx), "{layout}: Tab past the toolbar");
        // Panel: Shift+Tab from its first control returns to the editor;
        // Tab past its last control moves on, wrapping to the sidebar.
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        assert!(in_panel(cx), "{layout}");
        key(cx, "shift-tab");
        assert!(
            editor_focused(&tabs, cx),
            "{layout}: Shift+Tab from the panel"
        );
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        tab_out(cx, &in_panel);
        assert!(in_sidebar(&tabs, cx), "{layout}: Tab past the panel");
        // And back: Shift+Tab from the sidebar's first control.
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.focus_regions.forget();
            tabs.focus_active(window, cx);
        });
        key(cx, "shift-f6");
        key(cx, "shift-f6");
        assert!(in_sidebar(&tabs, cx), "{layout}");
        key(cx, "shift-tab");
        assert!(in_panel(cx), "{layout}: Shift+Tab from the sidebar");
    }
}

/// The find bar and the view switcher belong to the toolbar's region: Tab
/// from the header reaches them before the editor, Esc returns to the editor
/// from them, and F6 comes back to the control last focused there (ADR 0062).
#[gpui::test]
fn toolbar_members_take_part_in_tab_esc_and_f6(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    let member = |cx: &mut VisualTestContext, which: &str| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            match which {
                "find" => view.find_region.contains_focused(window, cx),
                _ => view.switcher_region.contains_focused(window, cx),
            }
        })
    };
    // Esc and F6 from a view switcher control.
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "shift-f6");
    for _ in 0..60 {
        if member(cx, "switcher") {
            break;
        }
        key(cx, "tab");
    }
    assert!(member(cx, "switcher"));
    let control = cx.update(|window, cx| window.focused(cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx), "Esc from the view switcher");
    key(cx, "shift-f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), control);
    // With find open, Tab from the header passes through it first.
    view.update_in(cx, |view, window, cx| {
        view.open_markdown_search(true, window, cx)
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.focus_regions.forget();
        tabs.focus_active(window, cx);
    });
    draw(cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    let mut found = false;
    for _ in 0..60 {
        key(cx, "tab");
        found |= member(cx, "find");
        if editor_focused(&tabs, cx) {
            break;
        }
    }
    assert!(found, "Tab reaches the find bar");
    assert!(editor_focused(&tabs, cx), "and then the editor");
}

/// Each Home Recents row is a Tab stop named for screen readers, and Enter
/// opens it (ADR 0062, dry-run H18 / A4).
#[gpui::test]
fn home_recents_rows_are_reachable_by_tab_and_open_with_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..3)
        .map(|i| {
            let path = dir.path().join(format!("recent-{i}.md"));
            std::fs::write(&path, format!("Recent {i}")).unwrap();
            path
        })
        .collect();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.shell.recents.files = paths
            .iter()
            .map(|path| mdoc_store::recents::Recent {
                path: path.clone(),
                opened: 0,
                pseudonymized: false,
            })
            .collect();
        window.focus(&tabs.focus, cx);
        cx.notify();
    });
    draw(cx);
    let rows: Vec<_> = cx.update(|_, cx| tabs.read(cx).shell.recent_focus.borrow()[..3].to_vec());
    // Walk Home's Tab stops, noting which rows take focus.
    let mut reached = [false; 3];
    for _ in 0..40 {
        key(cx, "tab");
        for (i, row) in rows.iter().enumerate() {
            reached[i] |= cx.update(|window, _| row.is_focused(window));
        }
        if reached.iter().all(|r| *r) {
            break;
        }
    }
    assert_eq!(reached, [true; 3], "Tab reaches every Recents row");
    // The third row opens with Enter.
    assert!(cx.update(|window, _| rows[2].is_focused(window)));
    activate_control(cx);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| !tabs.read(cx).showing_home(cx)));
    assert_eq!(
        tab_names(&tabs, cx).last().map(String::as_str),
        Some("recent-2.md")
    );
}

/// The Replacements panel's header toggle expands or collapses every
/// identity, by pointer or keyboard, and its name follows the state; the
/// palette has both commands while the panel is open.
#[gpui::test]
fn expand_and_collapse_all_identities(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        // Apply all leaves the inspector on Share; the list is on Review.
        view.inspector_share = false;
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            (view.expanded_identities(), view.all_identities_expanded(cx))
        })
    };
    assert_eq!(state(cx), (0, false));
    // Pointer: expand all, then collapse all.
    click_toolbar(cx, "replacement-expand-all");
    assert_eq!(state(cx), (2, true));
    click_toolbar(cx, "replacement-expand-all");
    assert_eq!(state(cx), (0, false));
    // Keyboard: Tab through the panel to the toggle, then Enter.
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.replacements_focus(), cx)
    });
    draw(cx);
    let toggle = cx.update(|_, cx| view.read(cx).expand_all_focus());
    let mut on_toggle = false;
    for _ in 0..30 {
        key(cx, "tab");
        on_toggle = cx.update(|window, _| toggle.is_focused(window));
        if on_toggle {
            break;
        }
    }
    assert!(on_toggle, "Tab reaches the toggle");
    activate_control(cx);
    assert_eq!(state(cx), (2, true));
    // Palette: both commands, run with focus in the sidebar, outside the
    // document's own action handlers.
    for (query, expanded) in [("collapse all identities", 0), ("expand all identities", 2)] {
        tabs.update_in(cx, |tabs, window, cx| {
            let sidebar = tabs.focus_regions.sidebar().clone();
            window.focus(&sidebar, cx);
        });
        draw(cx);
        cx.simulate_keystrokes("secondary-k");
        draw(cx);
        cx.simulate_input(query);
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(state(cx).0, expanded, "{query}");
    }
}

/// The palette lists the document's headings; choosing one moves the caret
/// to its line and scrolls it into view, leaving Undo as it was.
#[gpui::test]
fn palette_goes_to_a_heading(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(600.)));
    let view = active(&tabs, cx);
    let filler = "Paragraph text.\n\n".repeat(60);
    let text = format!("# Overview\n\n{filler}## Termination clause\n\n{filler}## Fees\n");
    view.update_in(cx, |view, _, cx| {
        view.editor.update(cx, |e, cx| e.set_text(&text, cx));
        view.editor.update(cx, |e, cx| e.set_cursor(text.len(), cx));
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    // One typed character is the Undo step to keep.
    cx.simulate_input("x");
    draw(cx);
    let rows = cx.update(|_, cx| tabs.read(cx).palette_section("Go to heading", cx));
    assert_eq!(rows, ["Overview", "Termination clause", "Fees"]);
    // A narrow window showing Source switches back to the Markdown pane.
    view.update(cx, |view, _| view.original_selected = true);
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("termination");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    draw(cx);
    let target = text.find("## Termination").unwrap();
    assert!(!cx.update(|_, cx| view.read(cx).original_selected));
    assert!(editor_focused(&tabs, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).cursor(), target);
        let caret = view.editor.read(cx).caret_bounds().unwrap();
        let viewport = view.scroll.bounds();
        assert!(
            caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(),
            "heading {caret:?} outside {viewport:?}"
        );
    });
    // Undo still removes the typed character and nothing else.
    cx.dispatch_action(mdoc_editor::Undo);
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), text));
}
