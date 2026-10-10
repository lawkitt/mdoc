use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn boot(cx: &mut TestAppContext, session: Session) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(mdoc_editor::bind_keys);
    cx.update(ui::bind_keys);
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
        .expect("toolbar button must be rendered");
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn collapse(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar_visible = false;
        tabs.sidebar_choice = Some(false);
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
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
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
    use crate::pii::Category;
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
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
    first.update(cx, |view, cx| {
        view.pii.reviewing = true;
        let id = view
            .pii
            .review
            .add_manual("Alice Alice", 0..5, Category::Person)
            .unwrap();
        view.pii.review.keep(id, None);
        view.sync_pii_theme(cx);
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
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
    first.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: crate::settings::OcrConfig::default(),
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
        assert!(!view.job.busy());
        assert!(view.ocr_required.is_some());
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
        assert_ne!(tabs.read(cx).active, first_id);
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
        assert!(!first.read(cx).import_busy.load(Ordering::Relaxed));
    });
    second.update_in(cx, |view, window, cx| view.import(&Import, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(second.read(cx).editor.read(cx).text().contains("pears")));
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
    cx.update(|_, cx| assert!(view.read(cx).ocr_required.is_some()));
    assert_eq!(settings::OcrModel::V6Small.languages(), "English");
    for selector in [
        "ocr-language-hint",
        "conversion-card",
        "Download & recognize",
        "Use native text only",
        "choose-ocr-model",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
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
        assert!(!view.job.busy());
        assert!(view.source_only);
        assert!(view.editor.read(cx).text().is_empty());
        assert!(!panel.read(cx).open);
    });
    assert!(cx.debug_bounds("Run OCR").is_some());
    click_toolbar(cx, "choose-ocr-model");
    cx.update(|_, cx| assert!(panel.read(cx).open));
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
        assert!(view.read(cx).job.busy());
        tabs.close_tab(tabs.active, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(view.read(cx).source_only);
        assert!(view.read(cx).editor.read(cx).text().is_empty());
        assert!(!view.read(cx).job.busy());
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
        assert!(tabs.tabs[2].view.is_none());
        assert!(tabs.tabs[3].view.is_none());
        assert_eq!(
            tabs.tabs[2].record.markdown,
            session_store::identity(&markdown)
        );
        assert!(tabs.notice.as_ref().unwrap().contains("unknown.bin"));
        assert!(!source.read(cx).source_only);
        assert!(!source.read(cx).dirty(cx));
        assert!(source.read(cx).preview.pdf.is_some());
        assert!(!source.read(cx).editor.read(cx).text().is_empty());
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep edits");
        tabs.active
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![pdf, markdown], window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 4);
        assert_eq!(tabs.read(cx).active, source_id);
        assert!(tabs.read(cx).tabs[2].view.is_none());
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
        assert!(tabs.read(cx).tabs[1].view.is_none());
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
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(source.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf.clone().unwrap());
    view.update(cx, |view, _| {
        view.ocr_state = OcrState::Ready(ocr::Installed {
            config: crate::settings::OcrConfig::default(),
            models: "must-not-load".into(),
            pdfium: "must-not-load".into(),
            onnx: "must-not-load".into(),
        })
    });
    cx.dispatch_action(Import);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).prompting);
        assert_eq!(view.read(cx).ocr_required, Some(vec![2, 5]));
        assert!(view.read(cx).source_only);
        assert!(!view.read(cx).import_busy.load(Ordering::Relaxed));
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
        assert!(!view.import_busy.load(Ordering::Relaxed));
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
fn copy_markdown_button_is_inert_while_markdown_is_empty(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    click_toolbar(cx, "Copy Markdown");
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
    click_toolbar(cx, "Copy Markdown");
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
    click_toolbar(cx, "Copy Markdown");
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
    cx.simulate_resize(size(px(1100.), px(760.)));
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
}

#[gpui::test]
fn toolbar_theme_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let original = cx.update(|_, cx| tabs.read(cx).theme.get());
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme.get(), original.toggle()));
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme.get(), original));
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
            available: Some(crate::updater::UpdateAvailable {
                version: "9.0.0".into(),
                html_url: String::new(),
                notes: String::new(),
            }),
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
    click_toolbar(cx, "Hide original");
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
    click_toolbar(cx, "Show original");
    cx.update(|_, cx| {
        assert!(view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.pdf.as_ref(), Some(&pdf));
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "Hide original");
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.attachment.as_ref(), Some(&path));
    });
}

#[gpui::test]
fn new_switch_reorder_and_close_preserve_live_editor_state(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let first = active(&tabs, cx);
    cx.simulate_input("original **Markdown**");
    cx.run_until_parked();
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
    cx.dispatch_action(New);
    cx.run_until_parked();
    let second = active(&tabs, cx);
    assert_ne!(first, second);
    cx.simulate_input("second document");
    cx.run_until_parked();
    let second_id = cx.update(|_, cx| tabs.read(cx).active);
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
        assert!(tabs.tabs[1].view.is_none());
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
                .view
                .as_ref()
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
        assert!(tabs.quitting.is_none());
        assert_eq!(
            tabs.tabs
                .iter()
                .filter(|tab| tab.view.as_ref().unwrap().read(cx).dirty(cx))
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
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
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
        assert!(tabs.docx_running.is_none());
        assert!(tabs.docx_queue.is_empty());
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
    assert!(crate::docx_preview::removed(&backing));
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
    let id = cx.update(|_, cx| tabs.read(cx).active);
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
        assert_eq!(tabs.read(cx).active, id);
        assert!(
            original
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep original");
        let imported = tabs.read(cx).tabs[1].view.as_ref().unwrap().read(cx);
        assert!(imported.editor.read(cx).text().contains("alpha"));
        assert!(imported.dirty(cx));
        assert!(!imported.job.busy());
    });
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
    let id = cx.update(|_, cx| tabs.read(cx).active);
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
        tabs.session_path = Some(path.clone());
        tabs.checkpoint(window, cx);
        tabs.sidebar_visible = false;
        tabs.sidebar_choice = Some(false);
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
fn closing_queued_conversion_rejects_it_without_affecting_other_tabs(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = cx.update(|_, cx| tabs.read(cx).active);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.docx_running = Some((999, 1));
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| {
            view.open_docx("queued.docx".into(), window, cx)
        });
        tabs.event(
            id,
            &TabEvent::Docx {
                path: "queued.docx".into(),
                generation: 1,
                cancel: cancelled.clone(),
            },
            window,
            cx,
        );
        tabs.remove(id, window, cx);
        tabs.docx_finished(999, 1, window, cx);
        assert!(tabs.docx_queue.is_empty());
        assert!(tabs.docx_running.is_none());
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
            .map(|tab| tab.id)
            .collect::<Vec<_>>()
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx)
                .tabs
                .iter()
                .filter(|tab| tab.view.is_some())
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
        let id = cx.update(|_, cx| tabs.read(cx).active);
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
        assert!(crate::docx_preview::removed(&backing));
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
        tabs.session_path = Some(parent_file.join("session.json"))
    });
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(!tabs.read(cx).finishing);
        assert!(tabs.read(cx).notice.is_some());
    });
    assert_eq!(std::fs::read_to_string(parent_file).unwrap(), "preserve");
}

#[gpui::test]
fn pdf_preview_refits_when_sidebar_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar_visible = false;
        tabs.sidebar_choice = Some(false);
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
    let (tabs, cx) = boot(cx, Session::default());
    let placeholder = cx.update(|_, cx| tabs.read(cx).active);
    // Rejected selections and canceled pickers cannot consume the placeholder.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.finish_open(vec![(dir.path().join("bad.bin"), false)], None, window, cx)
    });
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).active, placeholder));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert_ne!(tabs.read(cx).active, placeholder);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.active, window, cx)
    });
    cx.run_until_parked();
    let fallback = cx.update(|_, cx| tabs.read(cx).active);
    tabs.update_in(cx, |tabs, window, cx| {
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
        tabs.close_tab(tabs.active, window, cx)
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.iter().any(|tab| tab.id == fallback)));
    // Restore both intentional blanks without deduplicating their empty paths.
    tabs.update_in(cx, |tabs, window, cx| {
        for tab in &tabs.tabs {
            if let Some(view) = &tab.view {
                Workspace::release(view, window, cx);
            }
        }
        tabs.tabs.clear();
        tabs.restore(session, window, cx);
        tabs.open_path(file, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));
}

#[gpui::test]
fn edited_then_emptied_placeholder_is_not_disposable(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let id = cx.update(|_, cx| tabs.read(cx).active);
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
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.iter().any(|tab| tab.id == id)));
}

#[gpui::test]
fn automatic_conversion_is_disposable_until_edited_but_can_be_saved(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
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
            std::fs::read_to_string(saved).unwrap(),
            view.read(cx).editor.read(cx).text()
        );
    });
}

#[gpui::test]
fn automatic_conversions_queue_on_activation_and_closed_waiters_do_not_run(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
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
        assert!(first.read(cx).auto_convert_pending);
        assert!(tabs.read(cx).tabs[1].view.is_none());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let closed = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.active, window, cx)
    });
    cx.run_until_parked();
    let selected = cx.update(|_, cx| tabs.read(cx).active);
    drop(permit);
    tabs.update_in(cx, |tabs, window, cx| tabs.refresh_import(window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).active, selected);
        assert!(!first.read(cx).source_only);
        assert!(closed.read(cx).editor.read(cx).text().is_empty());
        assert!(closed.read(cx).import_cancel.load(Ordering::Relaxed));
        let view = tabs.read(cx).active_view().unwrap();
        assert_eq!(view.read(cx).ocr_required, Some(vec![2, 5]));
        assert!(!tabs.read(cx).import_busy.load(Ordering::Relaxed));
    });
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn sidebar_default_and_explicit_choice_survive_document_count_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| assert!(tabs.read(cx).sidebar_visible));
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
    cx.update(|_, cx| assert_eq!(restored.read(cx).sidebar_choice, Some(false)));
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
    cx.update(|_, cx| assert_eq!(legacy.read(cx).sidebar_choice, None));
    let legacy = cx.update(|_, cx| legacy.read(cx).snapshot(cx));
    assert!(legacy.sidebar_choice.is_none());
}

#[gpui::test]
fn keyboard_reveal_escape_restores_rail_control_and_closing_opener_keeps_focus_valid(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let opener = compact_focus(&tabs, cx, first_id);
    reveal_by_keyboard(cx, &opener);
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.reveal, Reveal::Shown);
        assert!(tabs.sidebar_row_focus.borrow()[&first_id].is_focused(window));
        assert_eq!(tabs.sidebar_choice, Some(false), "a reveal never pins");
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
        assert!(tabs.read(cx).compact_row_focus.borrow()[&first_id].is_focused(window));
    });
    // Escape's target stays put rather than revealing again.
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Hidden));
    // The toggle reveals too, at the reveal's own toggle.
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).reveal, Reveal::Shown);
        assert!(tabs.read(cx).panel_toggle_focus.is_focused(window));
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    reveal_by_keyboard(cx, &opener);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first_id, window, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
        assert!(tabs.read(cx).reveal_return.is_none());
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
    let first = cx.update(|_, cx| tabs.read(cx).active);
    let second = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let compact = |id| Box::leak(format!("compact-tab-{id}").into_boxed_str()) as &'static str;
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;

    // A click before hover intent activates without revealing.
    click_toolbar(cx, compact(first));
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).active, first);
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
    });
    // A pointer passing over the rail does not reveal.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Hidden));

    reveal_by_hover(&tabs, cx);
    let overlay = cx.debug_bounds("sidebar-reveal").unwrap();
    assert_eq!(overlay.size.width, px(200.));
    let editor_left = cx.update(|_, cx| tabs.read(cx).sidebar_scroll.bounds().left());
    assert!(
        editor_left < px(40.),
        "reveal overlays from the window edge"
    );
    // Rows in the reveal activate and keep it open while the pointer is inside.
    click_toolbar(cx, row(second));
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).active, second);
        assert_eq!(tabs.read(cx).reveal, Reveal::Shown);
        assert_eq!(tabs.read(cx).sidebar_choice, Some(false));
    });
    // Leaving starts a grace period that re-entering cancels.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    hover(cx, gpui::point(px(100.), px(400.)));
    wait(cx, 400);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    wait(cx, 200);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
        assert_eq!(tabs.read(cx).active, second);
    });
    assert!(cx.debug_bounds("sidebar-reveal").is_none());
    // The reveal's toggle pins the sidebar open.
    reveal_by_hover(&tabs, cx);
    click_toolbar(cx, "sidebar-toggle");
    cx.update(|_, cx| {
        assert!(tabs.read(cx).sidebar_visible);
        assert_eq!(tabs.read(cx).sidebar_choice, Some(true));
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
    });
}

#[gpui::test]
fn file_icons_route_save_save_as_and_open_to_the_active_document(cx: &mut TestAppContext) {
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
    click_toolbar(cx, "Save");
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
    click_toolbar(cx, "Save");
    assert_eq!(
        std::fs::read_to_string(&first_path).unwrap(),
        "updated version"
    );
    click_toolbar(cx, "Save As…");
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
    click_toolbar(cx, "Open…");
    cx.simulate_path_prompt_response(|_| Some(vec![other_path]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let opened = tabs.read(cx).active_view().unwrap();
        assert_ne!(opened.entity_id(), view.entity_id());
        assert_eq!(opened.read(cx).editor.read(cx).text(), "other document");
    });
}

#[gpui::test]
fn main_toolbar_wraps_without_hiding_actions_in_both_themes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    cx.simulate_resize(size(px(640.), px(480.)));
    for theme in [Theme::Dark, Theme::Light] {
        for expanded in [false, true] {
            for source_only in [false, true] {
                tabs.update(cx, |tabs, cx| {
                    tabs.theme.set(theme);
                    tabs.sidebar_choice = Some(expanded);
                    cx.notify();
                });
                view.update(cx, |view, cx| {
                    view.source_only = source_only;
                    cx.notify();
                });
                for _ in 0..3 {
                    cx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear(cx);
                    });
                    cx.run_until_parked();
                }
                let toolbar = cx.debug_bounds("document-toolbar").unwrap();
                for selector in ["Open…", "Settings", "theme-toggle"] {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    assert!(
                        toolbar.contains(&bounds.origin)
                            && toolbar.contains(&bounds.bottom_right())
                    );
                }
                // Copy Markdown floats over the editor, below the toolbar.
                let copy = cx.debug_bounds("Copy Markdown");
                assert_eq!(copy.is_some(), !source_only);
                if let Some(copy) = copy {
                    assert!(copy.top() >= toolbar.bottom() && copy.right() <= px(640.));
                }
                for selector in ["Save", "Save As…", "Pseudonymize"] {
                    let bounds = cx.debug_bounds(selector);
                    assert_eq!(bounds.is_some(), !source_only);
                    if let Some(bounds) = bounds {
                        assert!(
                            toolbar.contains(&bounds.origin)
                                && toolbar.contains(&bounds.bottom_right())
                        );
                    }
                }
                if source_only {
                    assert!(cx.debug_bounds("Convert to Markdown").is_some());
                }
                assert!(cx.debug_bounds("workspace-menu").is_none());
                assert!(cx.debug_bounds("New").is_none());
                assert!(toolbar.bottom() < px(160.));
            }
        }
    }
}

#[gpui::test]
fn new_follows_the_last_document_and_context_close_holds_the_reveal(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
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
        tabs.read(cx).active
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
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        assert!(tabs.read(cx).tab_menu.is_none());
    });
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Hidden));
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
        let opener = tabs
            .read(cx)
            .active_view()
            .unwrap()
            .read(cx)
            .settings_focus
            .clone();
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
            "Open…",
            "Copy Markdown",
            "Settings",
            "Save",
            "Save As…",
            "Pseudonymize",
            "theme-toggle",
            "Markdown",
            "Original",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= px(0.) && bounds.right() <= px(640.));
            assert!(bounds.bottom() <= px(480.));
        }
    }
    assert!(cx.debug_bounds("preview-divider").is_none());
    click_toolbar(cx, "Original");
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
    let first = cx.update(|_, cx| tabs.read(cx).active);
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
    assert_eq!(disambiguating_parent(&peers[0], &peers), suffix("first"));
    assert_eq!(disambiguating_parent(&peers[1], &peers), suffix("second"));
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
        tabs.sidebar_choice = Some(false);
        cx.notify();
        tabs.tabs.last().unwrap().id
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
        assert_eq!(tabs.read(cx).active, last_id);
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
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
                tab.view
                    .as_ref()
                    .map(|view| view.read(cx).display_name())
                    .unwrap_or_else(|| {
                        tab.record
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
fn empty_page_opens_several_files_and_yields_to_typing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    draw(cx);
    assert!(cx.debug_bounds("empty-page").is_some());
    click_toolbar(cx, "empty-page-open");
    cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
    cx.run_until_parked();
    draw(cx);
    // The untouched blank tab gives way to the opened files.
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    assert!(cx.debug_bounds("empty-page").is_none());

    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("empty-page").is_some());
    cx.simulate_input("x");
    draw(cx);
    assert!(cx.debug_bounds("empty-page").is_none());
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert!(cx.debug_bounds("empty-page").is_some());
}

#[gpui::test]
fn empty_page_accepts_dropped_files_and_rejects_unsupported(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    let unknown = dir.path().join("unknown.bin");
    for path in [&a, &b, &unknown] {
        std::fs::write(path, "x").unwrap();
    }
    let (tabs, cx) = boot(cx, Session::default());
    draw(cx);
    let center = cx.debug_bounds("empty-page").unwrap().center();
    let view = active(&tabs, cx);

    drag_files(cx, std::slice::from_ref(&unknown), center);
    cx.update(|_, cx| assert_eq!(view.read(cx).file_drag, Some(false)));
    drop_files(cx, center);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).file_drag, None);
        assert_eq!(tabs.read(cx).tabs.len(), 1);
    });

    drag_files(cx, &[a.clone(), b.clone(), unknown.clone()], center);
    cx.update(|_, cx| assert_eq!(view.read(cx).file_drag, Some(true)));
    drop_files(cx, center);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    cx.update(|_, cx| {
        assert!(
            tabs.read(cx)
                .notice
                .as_ref()
                .unwrap()
                .contains("unknown.bin")
        );
    });

    // A non-empty document is no drop target.
    assert!(cx.debug_bounds("empty-page").is_none());
    drag_files(cx, std::slice::from_ref(&b), center);
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).file_drag, None));
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
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone(), b.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let b_id = cx.update(|_, cx| tabs.read(cx).tabs[1].id);
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
fn file_drag_over_collapsed_rail_reveals_the_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    collapse(&tabs, cx);
    let rail = gpui::point(px(20.), px(400.));
    drag_files(cx, std::slice::from_ref(&b), rail);
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    let list = gpui::point(px(120.), px(400.));
    cx.simulate_event(gpui::FileDropEvent::Pending { position: list });
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(1)));
    drop_files(cx, list);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    // The reveal behaves as for hover: it closes once the pointer leaves.
    cx.update(|_, cx| assert_eq!(tabs.read(cx).reveal, Reveal::Shown));
    hover(cx, gpui::point(px(600.), px(400.)));
    wait(cx, 350);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).reveal, Reveal::Hidden);
        assert_eq!(tabs.read(cx).sidebar_choice, Some(false));
    });
}

#[gpui::test]
fn spelling_icon_badge_menu_switches_document_and_turns_off(cx: &mut TestAppContext) {
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
    assert!(
        cx.debug_bounds("spelling-badge").is_some(),
        "badge counts flags"
    );

    click_toolbar(cx, "spelling-indicator");
    cx.update(|_, cx| assert!(view.read(cx).spelling.menu_open));
    click_toolbar(cx, "spelling-document");
    cx.update(|_, cx| {
        assert!(view.read(cx).spelling.off_here);
        assert!(!view.read(cx).spelling.menu_open);
        assert!(view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
    draw(cx);
    assert!(cx.debug_bounds("spelling-badge").is_none());

    // A second click on the icon closes an open menu.
    click_toolbar(cx, "spelling-indicator");
    click_toolbar(cx, "spelling-indicator");
    cx.update(|_, cx| assert!(!view.read(cx).spelling.menu_open));

    // Turn off saves the global switch; the icon stays, faded, without a badge.
    click_toolbar(cx, "spelling-indicator");
    click_toolbar(cx, "spelling-document");
    click_toolbar(cx, "spelling-indicator");
    click_toolbar(cx, "spelling-off");
    cx.update(|_, cx| {
        let prefs = view.read(cx).preferences.borrow().snapshot().unwrap();
        assert!(!prefs.spelling.enabled);
        assert!(view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
    draw(cx);
    assert!(cx.debug_bounds("spelling-indicator").is_some());
    assert!(cx.debug_bounds("spelling-badge").is_none());

    // Turn on restores checking and the badge.
    click_toolbar(cx, "spelling-indicator");
    click_toolbar(cx, "spelling-on");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let prefs = view.read(cx).preferences.borrow().snapshot().unwrap();
        assert!(prefs.spelling.enabled);
    });
    draw(cx);
    assert!(cx.debug_bounds("spelling-badge").is_some());
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
    cx.update(|_, cx| settings::Store::set_words(&shared, words, cx));
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
