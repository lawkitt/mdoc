use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn boot(cx: &mut TestAppContext, session: Session) -> (Entity<Tabs>, &mut VisualTestContext) {
    cx.update(mdoc_editor::bind_keys);
    cx.update(markdown_search::bind_keys);
    cx.update(bind_markdown_search_keys);
    let (tabs, cx) = cx.add_window_view(|window, cx| {
        let mut tabs = Tabs::empty(window, cx);
        tabs.restore(session, window, cx);
        tabs
    });
    cx.run_until_parked();
    (tabs, cx)
}

fn active(tabs: &Entity<Tabs>, cx: &mut VisualTestContext) -> Entity<Workspace> {
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

#[gpui::test]
fn pseudonymization_mappings_survive_switches_and_end_with_the_tab(cx: &mut TestAppContext) {
    use crate::pseudonymization::Category;
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
        view.pseudonymization.review.open = true;
        let id = view
            .pseudonymization
            .review
            .add_manual("Alice Alice", 0..5, Category::Person)
            .unwrap();
        view.pseudonymization.review.keep(id, None);
        view.sync_pseudonym_theme(cx);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    cx.update(|_, cx| assert!(second.read(cx).pseudonymization.review.groups.is_empty()));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first_id, window, cx));
    cx.update(|_, cx| {
        let view = first.read(cx);
        assert_eq!(
            view.pseudonymization.review.mappings(),
            vec![("Alice".into(), "PERSON_1".into())]
        );
        assert_eq!(view.pseudonymization.review.remaining(), 0);
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
        assert!(view.error.as_ref().unwrap().contains("Local OCR failed"));
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
fn copy_markdown_toolbar_uses_active_tab_without_editor_focus(cx: &mut TestAppContext) {
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
fn toolbar_theme_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let original = cx.update(|_, cx| tabs.read(cx).theme.get());
    click_toolbar(cx, original.toggle_label());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme.get(), original.toggle()));
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, original.toggle().toggle_label());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme.get(), original));
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
    click_toolbar(cx, "Close Preview");
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
    click_toolbar(cx, "Show Preview");
    cx.update(|_, cx| {
        assert!(view.read(cx).preview.visible);
        assert_eq!(view.read(cx).preview.pdf.as_ref(), Some(&pdf));
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "Close Preview");
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
    assert!(!backing.exists());
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
        assert!(!backing.exists());
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
        let mut tabs = Tabs::new(window, cx);
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
        tabs.finish_open(vec![(dir.path().join("bad.bin"), false)], window, cx)
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
                Tabs::release(view, window, cx);
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
fn collapsed_rail_keeps_new_and_tab_controls_and_context_close(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar_visible = false;
        cx.notify();
    });
    let first_id = cx.update(|_, cx| tabs.read(cx).active);
    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    draw(cx);
    let plus = cx.debug_bounds("sidebar-new-collapsed").unwrap();
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 2));
    let first = cx
        .debug_bounds(Box::leak(
            format!("compact-tab-{first_id}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(first.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).active, first_id));
    draw(cx);
    cx.simulate_mouse_down(first.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(first.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);
    let close = cx.debug_bounds("context-close-tab").unwrap();
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(tabs.read(cx).tab_menu.is_none());
    });
    // New remains fixed above the scrollable list even with many tabs.
    tabs.update_in(cx, |tabs, window, cx| {
        for _ in 0..25 {
            tabs.new_tab(window, cx);
        }
    });
    draw(cx);
    assert_eq!(cx.debug_bounds("sidebar-new-collapsed").unwrap(), plus);
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
