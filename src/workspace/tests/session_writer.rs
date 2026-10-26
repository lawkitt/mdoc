//! The saved tab list: snapshots, checkpoints and restoring them (`SessionWriter`).
use super::*;

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
    cx.update(|_, cx| assert!(view.read(cx).preview.retryable()));
    std::fs::write(csv, "name,count\napples,2\n").unwrap();
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.retryable());
        assert!(!view.read(cx).dirty(cx));
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
    cx.update(|_, cx| assert!(view.read(cx).lifecycle.unavailable()));
    std::fs::write(missing, "now present").unwrap();
    cx.dispatch_action(RetryDocument);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).lifecycle.unavailable());
        assert_eq!(view.read(cx).editor.read(cx).text(), "now present");
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(first, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 2));
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
                .pdf()
                .is_none()
        );
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
        let pdf = view.read(cx).preview.pdf().unwrap().read(cx);
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
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
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
