//! Documents live in the Vault: it opens at launch, documents restore by id,
//! and with no key store nothing can be added.
use super::*;
use mdoc_store::{cases::CaseError, document_id::DocumentId};
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
        assert!(view.session.document.path.is_none());
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
        let (line, action) = tabs.case_key_banner().unwrap();
        assert!(line.contains("locked"), "{line}");
        assert_eq!(action, Some("Unlock…"));
        assert!(tabs.case_key_needed());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.unlock_cases(window, cx));
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(text_of(&tabs, cx), markdown);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.vault.vault().is_some());
        assert!(tabs.case_key_banner().is_none());
        // Opening documents read the key once, and asked about no case.
        assert!(tabs.notices.message().is_none());
    });
}

#[gpui::test]
fn a_declined_dialog_offers_try_again_and_keeps_documents_waiting(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (session, markdown) = vault_with_lease(dir.path());
    let (tabs, cx) = boot_home_with(cx, session, |tabs, _, _| {
        tabs.stage_vault_key(Err(CaseError::KeyDenied))
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.unlock_cases(window, cx));
    draw(cx);
    let button = cx.debug_bounds("keychain-continue").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let (line, action) = tabs.read(cx).case_key_banner().unwrap();
        assert!(line.contains("Keychain"), "{line}");
        assert_eq!(action, Some("Try again…"));
    });
    let root = dir.path().to_path_buf();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.staged_vault = Some(Ok(open_vault(&root)));
        tabs.unlock_cases(window, cx);
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
            super::super::vault_state::Opening::Key(CaseError::NoKeyStore),
            window,
            cx,
        )
    });
    active(&tabs, cx).read_with(cx, |view, _| {
        let error = view.error.clone().unwrap();
        assert!(error.contains("can't open"), "{error}");
    });
    cx.update(|_, cx| {
        let (line, action) = tabs.read(cx).case_key_banner().unwrap();
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
    cx.dispatch_action(crate::document_view::Save);
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
        view.apply_all_pii(&crate::pii::ui::PiiApplyAll, window, cx);
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
