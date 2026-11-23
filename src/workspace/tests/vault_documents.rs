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

fn text(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
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
    assert_eq!(text(&tabs, cx), markdown);
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
    assert_eq!(text(&tabs, cx), "");
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
    assert_eq!(text(&tabs, cx), markdown);
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
    assert_eq!(text(&tabs, cx), markdown);
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
