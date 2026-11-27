//! The saved tab list: snapshots, checkpoints and restoring them (`SessionWriter`).
use super::*;

#[gpui::test]
fn source_only_manifest_restores_multiple_sources_and_missing_source_is_retryable(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("missing.csv");
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
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
    tabs.update_in(cx, |tabs, window, cx| tabs.show_next_recent(window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert!(view.read(cx).preview.retryable()));
    std::fs::write(csv, "name,count\napples,2\n").unwrap();
    cx.dispatch_action(RetryPreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.retryable());
        assert!(!view.read(cx).unsaved(cx));
    });
}

/// A document of `markdown` in the Vault under `dir`, with `source` as its
/// original when given; a tab record naming it.
fn vault_tab(dir: &std::path::Path, markdown: &str, source: Option<&std::path::Path>) -> TabRecord {
    let vault = mdoc_vault::Vault::open(dir.join("vault"), &[3; 32]).unwrap();
    let new = mdoc_vault::NewDocument {
        project: None,
        title: "note".into(),
        source_kind: mdoc_vault::SourceKind::Pdf,
        origin: mdoc_vault::DocumentOrigin::Ordinary,
        markdown: markdown.into(),
    };
    let stored = match source {
        Some(source) => vault
            .import_document(
                new,
                std::fs::File::open(source).unwrap(),
                mdoc_vault::OriginalInfo {
                    file_name: source.file_name().unwrap().to_string_lossy().into_owned(),
                    ..Default::default()
                },
            )
            .unwrap(),
        None => vault.create_document(new).unwrap(),
    };
    let mut record = TabRecord::default();
    record.set_document(mdoc_store::document_id::DocumentId::Vault(
        mdoc_store::document_id::VaultId::parse(&stored.id).unwrap(),
    ));
    record
}

#[gpui::test]
fn restoration_is_lazy_and_keeps_the_caret(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut first = vault_tab(dir.path(), "Привет", None);
    first.caret = usize::MAX;
    let second = vault_tab(dir.path(), "second", None);
    let session = Session {
        tabs: vec![first, second],
        ..Session::default()
    };
    let (tabs, cx) = boot_on_vault(cx, session, &dir.path().join("vault"));
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.tabs[1].view().is_none());
        let view = tabs.active_view().unwrap();
        assert_eq!(view.read(cx).editor.read(cx).cursor(), "Привет".len());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.show_next_recent(window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "second"));
}

#[gpui::test]
fn snapshot_names_documents_by_id_and_never_their_text(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut record = vault_tab(dir.path(), "saved", None);
    record.preview_visible = false;
    let session = Session {
        tabs: vec![record],
        ..Session::default()
    };
    let (tabs, cx) = boot_on_vault(cx, session, &dir.path().join("vault"));
    cx.simulate_input("dirty text");
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("untitled secret");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let snapshot = tabs.read(cx).snapshot(cx);
        assert!(snapshot.tabs.iter().all(|tab| tab.attachment.is_none()));
        assert!(!snapshot.tabs[0].preview_visible);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("dirty text"));
        assert!(!serialized.contains("untitled secret"));
        assert!(!serialized.contains("saved"));
    });
}

#[gpui::test]
fn checkpoint_serializes_latest_state_and_restores_pdf_position(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    let mut record = vault_tab(dir.path(), "markdown", Some(&pdf));
    record.preview_visible = true;
    record.preview_page = 0;
    record.preview_zoom = Some(1.5);
    record.preview_fit = session_store::PreviewFit::Manual;
    let session = Session {
        tabs: vec![record],
        ..Session::default()
    };
    let (tabs, cx) = boot_on_vault(cx, session, &dir.path().join("vault"));
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
    let attachment =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    let mut record = vault_tab(dir.path(), "note", Some(&attachment));
    record.preview_visible = true;
    record.preview_zoom = Some(1.0);
    let session = Session {
        tabs: vec![record],
        ..Session::default()
    };
    let (tabs, cx) = boot_on_vault(cx, session, &dir.path().join("vault"));
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
