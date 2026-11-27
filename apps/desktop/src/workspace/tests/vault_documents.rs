//! Documents live in the Vault: it opens at launch, documents restore by id,
//! and with no key store nothing can be added.
use super::*;
use mdoc_store::{document_id::DocumentId, os_key::KeyError};
use mdoc_vault::{DocumentOrigin, NewDocument, SourceKind, Vault};
use std::path::Path;

fn open_vault(root: &Path) -> Vault {
    Vault::open(root.join("vault"), &[3; 32]).unwrap()
}

/// A Vault holding one Unfiled document, and a session whose tab names it.
fn vault_with_lease(root: &Path) -> (Session, String) {
    let stored = open_vault(root)
        .create_document(NewDocument {
            project: None,
            title: "Lease".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "# Lease\n\nThe tenant pays rent.".into(),
        })
        .unwrap();
    let mut tab = TabRecord {
        caret: 7,
        ..TabRecord::default()
    };
    let id = mdoc_store::document_id::VaultId::parse(&stored.id).unwrap();
    tab.set_document(DocumentId::Vault(id));
    let session = Session {
        tabs: vec![tab],
        ..Session::default()
    };
    (session, stored.markdown)
}

fn text_of(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    active(tabs, cx).read_with(cx, |view, cx| view.editor.read(cx).text().to_string())
}

#[gpui::test]
fn a_restored_tab_opens_its_vault_document_by_id(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, markdown) = vault_with_lease(dir.path());
    let root = dir.path().to_path_buf();
    let (tabs, cx) = boot_home_with(cx, session, move |tabs, _, _| {
        tabs.use_vault(open_vault(&root))
    });
    assert_eq!(text_of(&tabs, cx), markdown);
    active(&tabs, cx).read_with(cx, |view, cx| {
        assert_eq!(view.session.display_name(), "Lease");
        assert_eq!(view.editor.read(cx).cursor(), 7);
        assert!(view.session.document.vault.is_some());
    });
}

#[gpui::test]
fn a_locked_key_keeps_restored_documents_waiting_until_unlock(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, markdown) = vault_with_lease(dir.path());
    let root = dir.path().to_path_buf();
    let (tabs, cx) = boot_home_with(cx, session, move |tabs, _, _| {
        tabs.stage_vault_key(Ok(open_vault(&root)))
    });
    assert_eq!(text_of(&tabs, cx), "");
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let (line, action) = tabs.vault_banner().unwrap();
        assert!(line.contains("locked"), "{line}");
        assert_eq!(action, Some("Unlock…"));
        assert!(tabs.vault_banner().is_some());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.unlock_projects(window, cx));
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(text_of(&tabs, cx), markdown);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.vault.vault().is_some());
        assert!(tabs.vault_banner().is_none());
        // Opening documents read the key once, and showed no notice.
        assert!(tabs.notices.message().is_none());
    });
}

#[gpui::test]
fn a_declined_dialog_offers_try_again_and_keeps_documents_waiting(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, markdown) = vault_with_lease(dir.path());
    let (tabs, cx) = boot_home_with(cx, session, |tabs, _, _| {
        tabs.stage_vault_key(Err(KeyError::KeyDenied))
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.unlock_projects(window, cx));
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let (line, action) = tabs.read(cx).vault_banner().unwrap();
        assert!(line.contains("Keychain"), "{line}");
        assert_eq!(action, Some("Try again…"));
    });
    let root = dir.path().to_path_buf();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.staged_vault = Some(Ok(open_vault(&root)));
        tabs.unlock_projects(window, cx);
    });
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(text_of(&tabs, cx), markdown);
}

#[gpui::test]
fn no_key_store_leaves_documents_unavailable_and_adds_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, _) = vault_with_lease(dir.path());
    let (tabs, cx) = boot_home_with(cx, session, |tabs, window, cx| {
        tabs.vault_opened(
            super::super::vault_state::Opening::Key(KeyError::NoKeyStore),
            window,
            cx,
        )
    });
    active(&tabs, cx).read_with(cx, |view, _| {
        let error = view.error.clone().unwrap();
        assert!(error.contains("can't open"), "{error}");
    });
    cx.update(|_, cx| {
        let (line, action) = tabs.read(cx).vault_banner().unwrap();
        assert!(line.contains("Windows data protection"), "{line}");
        assert_eq!(action, None);
    });
    let count = cx.update(|_, cx| tabs.read(cx).tabs.len());
    let note = dir.path().join("note.md");
    std::fs::write(&note, "# Note").unwrap();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("pasted".into()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![note], window, cx);
        tabs.paste_as_new_tab(window, cx);
    });
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), count);
        let notice = tabs.notices.message().unwrap();
        assert!(notice.starts_with("Documents can't be added."), "{notice}");
    });
    // Home says why in its drop zone.
    tabs.update_in(cx, |tabs, window, cx| tabs.show_home(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("home-drop").is_some());
}

/// Boot on the Vault in `root`, from `session`.
fn boot_on<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
    session: Session,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let root = root.to_path_buf();
    boot_home_with(cx, session, move |tabs, _, _| {
        tabs.use_vault(open_vault(&root))
    })
}

fn documents(root: &Path) -> Vec<mdoc_vault::DocumentSummary> {
    open_vault(root).documents()
}

#[gpui::test]
fn a_new_document_is_created_by_its_first_keystroke_and_saves_itself(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    cx.dispatch_action(New);
    cx.run_until_parked();
    wait(cx, 1000);
    assert!(
        documents(dir.path()).is_empty(),
        "an empty tab is no document"
    );
    cx.simulate_input("Lease notes");
    // Nothing is written while typing goes on.
    wait(cx, 300);
    assert!(documents(dir.path()).is_empty());
    wait(cx, 700);
    let listed = documents(dir.path());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Lease notes");
    assert_eq!(listed[0].origin, DocumentOrigin::New);
    assert_eq!(listed[0].source_kind, SourceKind::Text);
    assert!(listed[0].project.is_none(), "Unfiled");
    // Later typing saves into the same document, and its title follows.
    cx.simulate_input(" for May");
    wait(cx, 700);
    let listed = documents(dir.path());
    assert_eq!(listed.len(), 1);
    let stored = open_vault(dir.path()).document(&listed[0].id).unwrap();
    assert_eq!(stored.markdown, "Lease notes for May");
    assert_eq!(stored.title, "Lease notes for May");
    active(&tabs, cx).read_with(cx, |view, cx| {
        assert_eq!(view.session.display_name(), "Lease notes for May");
        assert!(!view.unsaved(cx));
        assert!(!view.window_title().contains('•'));
    });
}

#[gpui::test]
fn pasted_text_is_an_ordinary_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("# Pasted\n\nText".into()));
    tabs.update_in(cx, |tabs, window, cx| tabs.paste_as_new_tab(window, cx));
    wait(cx, 1000);
    let listed = documents(dir.path());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].origin, DocumentOrigin::Ordinary);
    assert_eq!(listed[0].title, "Pasted");
}

#[gpui::test]
fn a_document_comes_back_after_a_restart_and_nothing_asks(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("First line\nSecond");
    wait(cx, 1000);
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    // The session names the document by id: no title, text or path.
    let json = serde_json::to_string(&session).unwrap();
    let id = documents(dir.path())[0].id.clone();
    assert!(json.contains(&id), "{json}");
    assert!(
        !json.contains("First line") && !json.contains("Second"),
        "{json}"
    );
    assert!(!json.contains(".md"), "{json}");
    // Closing asks nothing.
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());

    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    assert_eq!(text_of(&restored, cx), "First line\nSecond");
    active(&restored, cx).read_with(cx, |view, cx| {
        assert_eq!(view.editor.read(cx).cursor(), "First line\nSecond".len());
        assert_eq!(view.session.display_name(), "First line");
    });
}

#[gpui::test]
fn a_quit_right_after_an_edit_writes_it_and_asks_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, _) = vault_with_lease(dir.path());
    let (tabs, cx) = boot_on(cx, dir.path(), session);
    cx.simulate_input("Signed: ");
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let id = documents(dir.path())[0].id.clone();
    let stored = open_vault(dir.path()).document(&id).unwrap();
    assert!(stored.markdown.contains("Signed: "), "{}", stored.markdown);
    assert_eq!(
        stored.title, "Lease",
        "an ordinary document keeps its title"
    );
}

#[gpui::test]
fn save_writes_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, _) = vault_with_lease(dir.path());
    let (_tabs, cx) = boot_on(cx, dir.path(), session);
    cx.update(|_, cx| crate::bind_app_keys(cx));
    cx.simulate_input("Now ");
    cx.dispatch_action(crate::document::Save);
    cx.run_until_parked();
    let id = documents(dir.path())[0].id.clone();
    assert!(
        open_vault(dir.path())
            .document(&id)
            .unwrap()
            .markdown
            .contains("Now ")
    );
}

#[gpui::test]
fn a_review_comes_back_after_a_restart_and_its_mapping_asks_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, markdown) = vault_with_lease(dir.path());
    let (tabs, cx) = boot_on(cx, dir.path(), session);
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        let start = markdown.find("tenant").unwrap();
        view.pii
            .review_mut()
            .add_manual(
                &markdown,
                start..start + "tenant".len(),
                mdoc_pii::Category::Person,
            )
            .unwrap();
        view.pii.set_reviewing(true);
        view.review_changed(cx);
        view.toggle_replacements_panel(window, cx);
        view.apply_all_pii(&crate::review::PiiApplyAll, window, cx);
    });
    wait(cx, 1000);
    let text = view.read_with(cx, |view, cx| view.editor.read(cx).text().to_string());
    assert!(text.contains("PERSON_1"), "{text}");
    let stored = open_vault(dir.path())
        .document(&documents(dir.path())[0].id)
        .unwrap();
    assert_eq!(stored.markdown, text);
    assert!(stored.review.is_some(), "the review is saved with its text");
    // A Vault document keeps its own mapping: closing asks nothing.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());

    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    assert_eq!(text_of(&restored, cx), text);
    active(&restored, cx).read_with(cx, |view, _| {
        assert_eq!(view.pii.review().applied().len(), 1);
        assert!(view.pii.reviewing());
    });
}

#[gpui::test]
fn new_while_locked_asks_to_unlock_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.stage_vault_key(Ok(open_vault(&root)))
    });
    let count = cx.update(|_, cx| tabs.read(cx).tabs.len());
    cx.dispatch_action(New);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("keychain-explainer").is_some());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), count));
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).vault.vault().is_some()));
}

/// Open `paths` as the user would and let the imports finish.
fn open(tabs: &Entity<Workspace>, paths: Vec<std::path::PathBuf>, cx: &mut VisualTestContext) {
    tabs.update_in(cx, |tabs, window, cx| tabs.open_paths(paths, window, cx));
    cx.run_until_parked();
}

fn original(root: &Path, id: &str) -> Vec<u8> {
    use std::io::Read;
    let mut bytes = Vec::new();
    open_vault(root)
        .open_original(id)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

#[gpui::test]
fn each_opened_file_is_one_unfiled_document_with_its_original(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let fixtures =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let pdf = fixtures.join("reference.pdf");
    let docx = fixtures.join("import/text.docx");
    let note = dir.path().join("Note.md");
    std::fs::write(&note, "# Note\n\nWords.").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![pdf.clone(), docx.clone(), note.clone()], cx);
    let listed = documents(dir.path());
    assert_eq!(listed.len(), 3);
    for (path, kind) in [
        (&pdf, SourceKind::Pdf),
        (&docx, SourceKind::Word),
        (&note, SourceKind::Markdown),
    ] {
        let name = path.file_stem().unwrap().to_string_lossy();
        let summary = listed.iter().find(|d| d.title == name).unwrap();
        assert_eq!(summary.source_kind, kind, "{name}");
        assert_eq!(summary.origin, DocumentOrigin::Ordinary);
        assert!(summary.project.is_none());
        assert_eq!(
            original(dir.path(), &summary.id),
            std::fs::read(path).unwrap()
        );
    }
    // A Markdown file is its own text; the tab is the Vault document.
    let note_id = listed
        .iter()
        .find(|d| d.title == "Note")
        .unwrap()
        .id
        .clone();
    let stored = open_vault(dir.path()).document(&note_id).unwrap();
    assert_eq!(stored.markdown, "# Note\n\nWords.");
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let names: Vec<_> = tabs
            .tabs
            .iter()
            .map(|tab| super::super::tab_label(tab, cx).name)
            .collect();
        assert_eq!(tabs.tabs.len(), 3, "{names:?}");
        for tab in tabs.tabs.iter() {
            assert!(tab.record().document.is_some());
        }
        // The session names them by id alone.
        let json = serde_json::to_string(&tabs.snapshot(cx)).unwrap();
        assert!(
            !json.contains("reference") && !json.contains("Note"),
            "{json}"
        );
    });
}

#[gpui::test]
fn a_converted_pdf_comes_back_from_the_vault_with_its_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let pdf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/reference.pdf");
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![pdf], cx);
    let view = active(&tabs, cx);
    let (text, copy) = view.read_with(cx, |view, cx| {
        assert!(!view.lifecycle.source_only(), "converted on its own");
        (
            view.editor.read(cx).text().to_string(),
            view.preview.source().cloned().unwrap(),
        )
    });
    assert!(!text.is_empty());
    wait(cx, 1000);
    let id = documents(dir.path())[0].id.clone();
    assert_eq!(open_vault(dir.path()).document(&id).unwrap().markdown, text);
    // The viewing copy lives outside the Vault and goes with the tab.
    assert!(copy.exists() && !copy.starts_with(dir.path().join("vault")));
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    assert!(!copy.exists());

    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    let view = active(&restored, cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.read(cx).text(), text);
        assert_eq!(view.session.display_name(), "reference");
        assert!(view.preview.source().is_some_and(|source| source.exists()));
    });
}

fn repeated(cx: &mut TestAppContext, answer: &str) -> (tempfile::TempDir, usize, usize) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("Note.md");
    std::fs::write(&note, "# Note").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![note.clone()], cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let document = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .get(first)
            .unwrap()
            .record()
            .document
            .clone()
    });
    // Still open, it only comes to the front.
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    open(&tabs, vec![note.clone()], cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), first);
    // Closed, opening it again asks.
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first, window, cx));
    cx.run_until_parked();
    open(&tabs, vec![note], cx);
    assert_eq!(
        cx.pending_prompt().unwrap().0,
        "“Note.md” is already in mdoc"
    );
    cx.simulate_prompt_answer(answer);
    cx.run_until_parked();
    let active_is_first = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        tabs.tabs.active_tab().unwrap().record().document == document
    });
    let open = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .filter(|tab| tab.record().document.is_some())
            .count()
    });
    if answer == "Open Existing" {
        assert!(active_is_first, "the existing document comes to the front");
    }
    let count = documents(dir.path()).len();
    (dir, count, open)
}

#[gpui::test]
fn a_repeated_import_opens_the_existing_document(cx: &mut TestAppContext) {
    let (_dir, documents, tabs) = repeated(cx, "Open Existing");
    assert_eq!((documents, tabs), (1, 1));
}

/// Open Existing imports nothing, so Export Markdown still starts where the
/// last import came from, not in the repeat's folder.
#[gpui::test]
fn open_existing_leaves_the_last_folder_where_the_import_was(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    for folder in [&first, &second] {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join("Note.md"), "# Note").unwrap();
    }
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![first.join("Note.md")], cx);
    let imported = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(imported, window, cx));
    cx.run_until_parked();
    open(&tabs, vec![second.join("Note.md")], cx);
    cx.simulate_prompt_answer("Open Existing");
    cx.run_until_parked();
    assert_eq!(documents(dir.path()).len(), 1);
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.dispatch_action(crate::document::ExportMarkdown);
    cx.run_until_parked();
    let mut start = None;
    cx.simulate_new_path_selection(|directory| {
        start = Some(directory.to_path_buf());
        None
    });
    cx.run_until_parked();
    assert_eq!(start, Some(session_store::identity(&first)));
}

#[gpui::test]
fn a_repeated_import_can_import_again(cx: &mut TestAppContext) {
    let (dir, documents_now, tabs) = repeated(cx, "Import Again");
    assert_eq!((documents_now, tabs), (2, 1));
    let listed = documents(dir.path());
    assert_eq!(listed[0].source_sha256, listed[1].source_sha256);
}

#[gpui::test]
fn cancelling_a_repeated_import_adds_nothing(cx: &mut TestAppContext) {
    let (_dir, documents, tabs) = repeated(cx, "Cancel");
    assert_eq!((documents, tabs), (1, 0));
}

#[gpui::test]
fn several_repeats_ask_one_question_and_new_files_open_anyway(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["A.md", "B.md", "C.md"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("# {name}")).unwrap();
            path
        })
        .collect();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, paths[..2].to_vec(), cx);
    let ids: Vec<u64> = cx.update(|_, cx| tabs.read(cx).tabs.iter().map(|tab| tab.id()).collect());
    tabs.update_in(cx, |tabs, window, cx| {
        for id in ids {
            tabs.close_tab(id, window, cx);
        }
    });
    cx.run_until_parked();
    open(&tabs, paths.clone(), cx);
    let (question, detail) = cx.pending_prompt().unwrap();
    assert_eq!(question, "2 files are already in mdoc");
    assert!(detail.contains("A.md") && detail.contains("B.md") && !detail.contains("C.md"));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    // Cancel skips only the repeats.
    let titles: Vec<_> = documents(dir.path()).into_iter().map(|d| d.title).collect();
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert!(titles.contains(&"C".to_string()));
}

#[gpui::test]
fn relative_links_in_an_imported_markdown_file_resolve_nowhere(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("Note.md");
    std::fs::write(dir.path().join("beside.md"), "# Beside").unwrap();
    std::fs::write(&note, "[beside](beside.md)").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![note], cx);
    active(&tabs, cx).read_with(cx, |view, _| {
        let directory = view.save_directory();
        assert_eq!(
            mdoc_store::document::local_path("beside.md", &directory),
            None
        );
    });
}

#[gpui::test]
fn a_quit_right_after_typing_a_new_document_restores_it_from_session_json(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("session.json");
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.session_writer.set_path(Some(manifest.clone()))
    });
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.simulate_input("Typed and quit");
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert!(cx.cx.read(|cx| cx.windows().is_empty()));
    let session = session_store::load(&manifest).unwrap();
    assert_eq!(session.tabs.len(), 1, "{session:?}");
    assert!(session.tabs[0].document.is_some());
    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    assert_eq!(text_of(&restored, cx), "Typed and quit");
}

#[gpui::test]
fn a_close_whose_write_fails_keeps_the_tab_and_its_edit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("member.md");
    std::fs::write(&path, "original").unwrap();
    // The Vault stops at the next Document write, as a full disk would.
    let fail = Arc::new(AtomicBool::new(false));
    let root = dir.path().join("vault");
    let hook: mdoc_vault::FaultHook = {
        let fail = fail.clone();
        Arc::new(move |step| {
            if fail.load(Ordering::SeqCst) && step.kind == mdoc_vault::ObjectKind::Document {
                mdoc_vault::Fault::Crash
            } else {
                mdoc_vault::Fault::Continue
            }
        })
    };
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(Vault::open_with_hook(root, &[3; 32], hook).unwrap())
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    fail.store(true, Ordering::SeqCst);
    cx.simulate_input("my edit ");
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(tab, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.tabs.contains(tab), "the tab stays with its edit");
        let notice = tabs.notices.message().unwrap();
        assert!(notice.starts_with("Could not save"), "{notice}");
    });
    assert!(text_of(&tabs, cx).contains("my edit"));
}

#[gpui::test]
fn a_converted_text_emptied_by_its_user_stays_empty_after_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let docx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/import/text.docx");
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![docx], cx);
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        assert!(!view.editor.read(cx).text().is_empty());
        view.editor
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
    });
    wait(cx, 1000);
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    active(&restored, cx).read_with(cx, |view, cx| {
        assert!(!view.lifecycle.source_only());
        assert!(view.editor.read(cx).text().is_empty());
    });
}

/// Review of #1145 (P2-5): what a conversion warned about and left out
/// is saved with its original and shows again after a restart.
#[gpui::test]
fn a_conversions_warning_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let docx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/import/text.docx");
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![docx], cx);
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        assert!(!view.editor.read(cx).text().is_empty());
        view.session.warning = Some("Some pages had no text.".into());
        view.session.left_out = Some("Left out: 2 images.".into());
        let edited = format!("{} Edited.", view.editor.read(cx).text());
        view.editor
            .update(cx, |editor, cx| editor.set_text(edited, cx));
    });
    wait(cx, 1000);
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    active(&restored, cx).read_with(cx, |view, cx| {
        assert!(view.editor.read(cx).text().ends_with(" Edited."));
        assert_eq!(
            view.session.warning.as_deref(),
            Some("Some pages had no text.")
        );
        assert_eq!(
            view.session.left_out.as_deref(),
            Some("Left out: 2 images.")
        );
    });
}

/// Review of #1145 (P2-6): a converted document whose original is gone
/// still opens its saved Markdown and review; the Source pane says why it
/// has nothing to show.
#[gpui::test]
fn a_missing_original_leaves_the_saved_markdown_open(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let docx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/import/text.docx");
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![docx], cx);
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        let edited = format!("{} Edited.", view.editor.read(cx).text());
        view.editor
            .update(cx, |editor, cx| editor.set_text(edited, cx));
    });
    wait(cx, 1000);
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(tabs.read(cx).tabs.active(), cx));
    let root = vault(&tabs, cx).root().to_path_buf();
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let original = root
        .join("docs")
        .join(id.unwrap().as_str())
        .join("original.enc");
    std::fs::remove_file(&original).unwrap();
    let (restored, cx) = boot_on(&mut cx.cx, dir.path(), session);
    active(&restored, cx).read_with(cx, |view, cx| {
        assert!(!view.lifecycle.unavailable(), "{:?}", view.error);
        assert!(view.editor.read(cx).text().ends_with(" Edited."));
        assert!(view.preview.failed());
        assert!(view.preview.visible());
        let message = view.preview.message().unwrap();
        assert!(
            message.starts_with("The original file is missing"),
            "{message}"
        );
    });
}

/// Review of #1145 (P2-7): a file open in a tab comes to the front again
/// only while its bytes are unchanged; replaced at the same path, it is
/// imported as a new document, never shown as the old import.
#[gpui::test]
fn reopening_a_replaced_file_imports_its_new_bytes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lease.md");
    std::fs::write(&path, "First version.").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![path.clone()], cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_eq!(text_of(&tabs, cx), "First version.");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    // Unchanged: its tab comes to the front, nothing is imported.
    open(&tabs, vec![path.clone()], cx);
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), first);
    let vault = vault(&tabs, cx);
    assert_eq!(vault.documents().len(), 1);
    // Replaced in another app: a new document with the new text.
    std::fs::write(&path, "Second version.").unwrap();
    open(&tabs, vec![path.clone()], cx);
    assert!(!cx.has_pending_prompt(), "new bytes are no repeat");
    let second = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_ne!(second, first);
    assert_eq!(text_of(&tabs, cx), "Second version.");
    assert_eq!(vault.documents().len(), 2);
    // The path now finds the new import.
    open(&tabs, vec![path], cx);
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), second);
    assert_eq!(vault.documents().len(), 2);
}

#[gpui::test]
fn a_second_open_of_a_file_still_importing_asks_as_a_repeat(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("Note.md");
    std::fs::write(&note, "# Note").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![note.clone()], window, cx);
        tabs.open_paths(vec![note.clone()], window, cx);
    });
    cx.run_until_parked();
    // The first admission is open in a tab by now: the second only shows it.
    assert!(!cx.has_pending_prompt());
    assert_eq!(documents(dir.path()).len(), 1);
}

/// Review of #1122: a Project delete with Move to Unfiled moves an
/// unconverted member as it is: it opens from its source and converts,
/// though the move wrote it again.
#[gpui::test]
fn an_unconverted_member_moved_to_unfiled_by_a_delete_still_converts(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let pdf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/reference.pdf");
    let (project, id) = {
        let vault = open_vault(dir.path());
        let project = vault
            .create_project(mdoc_vault::NewProject {
                name: "Lease".into(),
                ..mdoc_vault::NewProject::default()
            })
            .unwrap();
        let new = NewDocument {
            project: Some(project.id.clone()),
            title: "reference".into(),
            source_kind: SourceKind::Pdf,
            origin: DocumentOrigin::Ordinary,
            markdown: String::new(),
        };
        let info = mdoc_vault::OriginalInfo {
            file_name: "reference.pdf".into(),
            ..mdoc_vault::OriginalInfo::default()
        };
        let source = std::fs::File::open(&pdf).unwrap();
        let document = vault.import_document(new, source, info).unwrap();
        (project.id, document.id)
    };
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_project(project.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Move to Unfiled");
    cx.run_until_parked();
    let moved = open_vault(dir.path()).document(&id).unwrap();
    assert_eq!(moved.project, None);
    assert!(moved.revision > 1, "the move wrote it");
    assert!(moved.unconverted && moved.markdown.is_empty());

    let vault_id = mdoc_store::document_id::VaultId::parse(&id).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(vault_id, window, cx)
    });
    cx.run_until_parked();
    let text = active(&tabs, cx).read_with(cx, |view, cx| {
        assert!(!view.lifecycle.source_only(), "converted from its source");
        view.editor.read(cx).text().to_string()
    });
    assert!(!text.is_empty());
    wait(cx, 1000);
    let converted = open_vault(dir.path()).document(&id).unwrap();
    assert_eq!(converted.markdown, text);
    assert!(!converted.unconverted);
}

/// Review of #1126: a desktop save replaces the review, so the counts a
/// phone saved with the old one go, and lists count it again.
#[gpui::test]
fn a_desktop_save_drops_the_counts_of_the_review_it_replaces(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("Note.md");
    std::fs::write(&note, "# Note").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![note], cx);
    wait(cx, 1000);
    let id = documents(dir.path())[0].id.clone();
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    let counts = mdoc_vault::ReviewCounts {
        proposed: 3,
        replaced: 1,
        kept: 0,
    };
    (vault.update_document(&id, |d| d.counts = Some(counts))).unwrap();
    active(&tabs, cx).update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text(String::from("# Note, edited"), cx)
        });
        view.save_now(cx);
    });
    wait(cx, 1000);
    let saved = open_vault(dir.path()).document(&id).unwrap();
    assert_eq!(saved.markdown, "# Note, edited");
    assert_eq!(saved.counts, None);
}

/// Review of #1145, round 2 (P2): Import Again of a Markdown file changed
/// while the question was up stores the Markdown and the original from the
/// same bytes, the ones admission read.
#[gpui::test]
fn import_again_keeps_one_snapshot_of_a_changed_markdown_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("First.md"), dir.path().join("Second.md"));
    std::fs::write(&first, "# Note").unwrap();
    std::fs::write(&second, "# Note").unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![first], cx);
    open(&tabs, vec![second.clone()], cx);
    assert!(cx.has_pending_prompt());
    // Another editor saves the file while the question shows.
    std::fs::write(&second, "# Note, changed").unwrap();
    cx.simulate_prompt_answer("Import Again");
    cx.run_until_parked();
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    let summaries = vault.documents();
    assert_eq!(summaries.len(), 2);
    let read = mdoc_vault::source_sha256(&b"# Note"[..]).unwrap();
    for summary in summaries {
        let document = vault.document(&summary.id).unwrap();
        assert_eq!(document.markdown, "# Note");
        assert_eq!(document.original.unwrap().sha256, read, "the same bytes");
    }
}

/// Review of #1145, round 2 (P2): a converted file changed while the
/// repeated-import question was up is admitted again as it is now, so a
/// change that makes it a repeat of another document asks again.
#[gpui::test]
fn import_again_admits_a_changed_converted_file_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let fixtures =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let (a, c) = (
        std::fs::read(fixtures.join("import/text.docx")).unwrap(),
        std::fs::read(fixtures.join("import/handmade-hidden.docx")).unwrap(),
    );
    let paths: Vec<_> = ["A.docx", "B.docx", "C.docx"]
        .iter()
        .map(|name| dir.path().join(name))
        .collect();
    std::fs::write(&paths[0], &a).unwrap();
    std::fs::write(&paths[1], &a).unwrap();
    std::fs::write(&paths[2], &c).unwrap();
    let (tabs, cx) = boot_on(cx, dir.path(), Session::default());
    open(&tabs, vec![paths[0].clone(), paths[2].clone()], cx);
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    assert_eq!(vault.documents().len(), 2);
    open(&tabs, vec![paths[1].clone()], cx);
    assert_eq!(
        cx.pending_prompt().unwrap().0,
        "“B.docx” is already in mdoc"
    );
    // Saved meanwhile with C's bytes: a repeat of C now, not of A.
    std::fs::write(&paths[1], &c).unwrap();
    cx.simulate_prompt_answer("Import Again");
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "asked again, for the bytes it has now"
    );
    cx.simulate_prompt_answer("Open Existing");
    cx.run_until_parked();
    assert_eq!(vault.documents().len(), 2, "nothing imported unasked");
}

/// Review of #1152 (P2): a converted file changed while the repeat question
/// was up is checked before its record is written, so no document of the
/// changed bytes is ever published, not even for a moment.
#[gpui::test]
fn a_changed_converted_file_never_publishes_a_document(cx: &mut TestAppContext) {
    use std::sync::{Mutex, atomic::Ordering};
    let dir = tempfile::tempdir().unwrap();
    let fixtures =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let (a, c) = (
        std::fs::read(fixtures.join("import/text.docx")).unwrap(),
        std::fs::read(fixtures.join("import/handmade-hidden.docx")).unwrap(),
    );
    let (first, second) = (dir.path().join("A.docx"), dir.path().join("B.docx"));
    std::fs::write(&first, &a).unwrap();
    std::fs::write(&second, &a).unwrap();
    // The Documents whose records are written while armed.
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let written = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let root = dir.path().join("vault");
    let hook: mdoc_vault::FaultHook = {
        let (armed, written) = (armed.clone(), written.clone());
        Arc::new(move |step| {
            if armed.load(Ordering::SeqCst)
                && step.kind == mdoc_vault::ObjectKind::Document
                && step.stage == mdoc_vault::Stage::BeforeRename
            {
                written.lock().unwrap().insert(step.id.to_string());
            }
            mdoc_vault::Fault::Continue
        })
    };
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(Vault::open_with_hook(root, &[3; 32], hook).unwrap())
    });
    open(&tabs, vec![first], cx);
    open(&tabs, vec![second.clone()], cx);
    assert!(cx.has_pending_prompt());
    // Saved meanwhile with other bytes.
    std::fs::write(&second, &c).unwrap();
    armed.store(true, Ordering::SeqCst);
    cx.simulate_prompt_answer("Import Again");
    cx.run_until_parked();
    // Admitted again as new bytes, a new document; none was written for
    // the bytes it was asked about and then taken back.
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    let documents = vault.documents();
    assert_eq!(documents.len(), 2);
    for id in written.lock().unwrap().iter() {
        assert!(
            documents.iter().any(|d| &d.id == id),
            "{id} was published, then deleted"
        );
    }
    let sha = mdoc_vault::source_sha256(&c[..]).unwrap();
    assert!(
        documents
            .iter()
            .any(|d| d.source_sha256.as_deref() == Some(sha.as_str()))
    );
}
