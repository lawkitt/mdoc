//! Document notes and Promote to Document through the window (#1091
//! Decision 3): notes save as the text does, come back after a restart and
//! never leave for an AI; a saved reply's page makes a new document of it,
//! opens it and keeps the reply.
use super::*;
use crate::{document_view::Inspector, workspace::PasteAiResponse};
use mdoc_pii::Category::{Organization, Person};
use mdoc_store::document_id::VaultId;

const NOTES: &str = "Call Anna Petrova about the deposit before Friday.";

/// Paste `reply` with Paste AI Reply in the active document; the saved
/// reply's id, shown on its page.
fn paste_reply(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, reply: &str) -> String {
    let view = active(tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| Some(tabs.read(cx).reply_page.as_ref()?.id().to_string()))
        .expect("Paste AI Reply shows the saved reply")
}

/// The Vault document the active tab shows.
fn active_document(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> VaultId {
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        tabs.tab_document(tabs.tabs.active(), cx).unwrap()
    })
}

/// Promote to Document on the Reply page, as a click does; the document
/// it opened.
fn promote(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> VaultId {
    let before = vault(tabs, cx).documents().len();
    click_toolbar(cx, "reply-page-promote");
    cx.run_until_parked();
    assert_eq!(vault(tabs, cx).documents().len(), before + 1);
    cx.update(|_, cx| assert!(tabs.read(cx).reply_page.is_none(), "the document opened"));
    active_document(tabs, cx)
}

/// Notes typed in the Inspector's Notes tab are written once typing pauses,
/// come back after a restart, and are never in Copy for AI or the shelf.
#[gpui::test]
fn notes_save_reload_and_never_leave_for_an_ai(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    cx.simulate_resize(size(px(1400.), px(850.)));
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova leased to Northwind Ltd.",
        &[("Anna Petrova", Person), ("Northwind Ltd", Organization)],
    );
    let document = active_document(&tabs, cx);
    // Keyboard: More › Notes opens the tab and puts the caret in it.
    more(cx, "more-notes");
    let view = active(&tabs, cx);
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.inspector, Inspector::Notes);
        let editor = view.notes.editor().expect("the notes editor");
        assert!(editor.read(cx).focus_handle(cx).is_focused(window));
    });
    draw(cx);
    assert!(cx.debug_bounds("notes-box").is_some());
    assert!(cx.debug_bounds("notes-private").is_some());
    cx.simulate_input(NOTES);
    wait(cx, 800);
    assert_eq!(
        vault(&tabs, cx).document(document.as_str()).unwrap().notes,
        NOTES
    );

    // Never in what leaves for an AI.
    let copy = cx.update(|_, cx| view.read(cx).text_for_ai(cx));
    assert!(copy.contains("PERSON_1"), "{copy}");
    assert!(!copy.contains("deposit"), "{copy}");
    let outbound = vault(&tabs, cx).text_for_ai(document.as_str()).unwrap();
    assert!(!outbound.contains("deposit"));
    let rows = cx.update(|_, cx| tabs.read(cx).shelf_documents(cx));
    assert!(!rows.is_empty());
    for row in &rows {
        let shown = [
            row.title.clone(),
            row.text_for_ai().unwrap_or("").to_string(),
        ]
        .into_iter()
        .chain(row.preview.iter().map(|line| line.text.clone()));
        for text in shown {
            assert!(!text.contains("deposit"), "the shelf shows {text:?}");
        }
    }

    // After a restart.
    let session = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (tabs, cx) = boot_on_vault(&mut cx.cx, session, &root);
    let _ = tab;
    let view = active(&tabs, cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).notes.text().to_string()),
        NOTES
    );
    view.update_in(cx, |view, window, cx| view.show_notes(window, cx));
    draw(cx);
    cx.update(|_, cx| {
        let editor = view.read(cx).notes.editor().unwrap();
        assert_eq!(editor.read(cx).text(), NOTES);
    });
}

/// A quit right after typing notes writes them: the writer waits for them
/// as for the text.
#[gpui::test]
fn a_quit_right_after_typing_notes_writes_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let document = active_document(&tabs, cx);
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.show_notes(window, cx);
        view.type_notes("Ask about the deposit.", window, cx);
    });
    let vault = vault(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    assert_eq!(
        vault.document(document.as_str()).unwrap().notes,
        "Ask about the deposit."
    );
}

/// With no notes, the tab says what it is for; a document not in the Vault
/// yet keeps none.
#[gpui::test]
fn empty_notes_show_a_hint(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| view.show_notes(window, cx));
    draw(cx);
    assert!(cx.debug_bounds("notes-box").is_some());
    cx.update(|_, cx| {
        let editor = view.read(cx).notes.editor().unwrap().read(cx);
        // The editor's placeholder, EMPTY_HINT, shows over it (renders).
        assert!(editor.text().is_empty());
    });
    // Back to Review by its tab.
    click_toolbar(cx, "inspector-review");
    cx.update(|_, cx| assert_eq!(view.read(cx).inspector, Inspector::Review));
}

/// Export Project… writes a document's notes beside it.
#[gpui::test]
fn export_project_writes_the_notes_file(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home_with(cx, Session::default(), |tabs, _, _| tabs.use_temp_vault());
    let vault = vault(&tabs, cx);
    let project = vault
        .create_project(mdoc_vault::NewProject {
            name: "Lease".into(),
            ..Default::default()
        })
        .unwrap();
    let document = vault
        .create_document(mdoc_vault::NewDocument {
            project: Some(project.id.clone()),
            title: "Letter".into(),
            source_kind: mdoc_vault::SourceKind::Text,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: "Dear PERSON_1".into(),
        })
        .unwrap();
    vault.set_notes(&document.id, NOTES.into()).unwrap();
    let picked = tempfile::tempdir().unwrap();
    let id = project.id.clone();
    tabs.update_in(cx, |tabs, window, cx| tabs.export_project(id, window, cx));
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    cx.run_until_parked();
    let folder = picked.path().join("Lease");
    let read = |name: &str| std::fs::read_to_string(folder.join(name)).unwrap();
    assert_eq!(read("Letter.md"), "Dear PERSON_1");
    assert_eq!(read("Letter.notes.md"), NOTES);
}

/// Promote to Document on a Project's reply: a new document in the
/// Project with the reply's restored text, opened, not Ready until
/// reviewed, its names marked again; the reply stays, and the document's
/// "From AI reply" opens it.
#[gpui::test]
fn promote_creates_opens_and_keeps_the_reply(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let project = new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    let reply = paste_reply(&tabs, cx, "PERSON_1 thanks ORG_1.");
    let tabs_before = cx.update(|_, cx| tabs.read(cx).tabs.iter().count());
    let document = promote(&tabs, cx);
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).tabs.iter().count()),
        tabs_before + 1
    );
    let vault = vault(&tabs, cx);
    let stored = vault.document(document.as_str()).unwrap();
    assert_eq!(stored.project.as_deref(), Some(project.as_str()));
    assert_eq!(
        stored.markdown,
        "Anna Petrova thanks Northwind Traders Ltd."
    );
    assert_eq!(stored.from_reply.as_deref(), Some(reply.as_str()));
    assert_eq!(stored.source_kind, mdoc_vault::SourceKind::Markdown);
    assert!(vault.reply(&reply).is_ok(), "the reply is kept");
    assert!(matches!(
        vault.readiness(document.as_str()),
        Ok(mdoc_vault::Readiness::NotReviewed)
    ));
    // Review marks the Project's names again, under their aliases.
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.editor.read(cx).text(),
            "Anna Petrova thanks Northwind Traders Ltd."
        );
        let review = view.pii.review();
        let marked: Vec<(String, String)> = (review.variants().iter())
            .filter(|v| !v.mentions.is_empty())
            .map(|v| (v.original.to_string(), v.replacement.clone()))
            .collect();
        assert!(
            marked.contains(&("Anna Petrova".into(), "PERSON_1".into())),
            "{marked:?}"
        );
        assert!(review.remaining() > 0, "proposed, not yet applied");
    });
    // "From AI reply" goes back to the reply.
    draw(cx);
    click_toolbar(cx, "from-reply");
    cx.update(|_, cx| {
        let page = tabs.read(cx).reply_page.as_ref().expect("the Reply page");
        assert_eq!(page.id(), reply);
    });
    // Deleted, the line stays without a way to open it.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.delete_saved_reply(&reply, window, cx)
    });
    cx.run_until_parked();
    let view = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        let tab = tabs.find_document(&DocumentId::Vault(document.clone()), None, cx);
        tabs.tabs.get(tab.unwrap()).unwrap().view().unwrap().clone()
    });
    cx.update(|_, cx| {
        let from = view.read(cx).from_reply.clone().unwrap();
        assert!(from.gone);
    });
}

/// An Unfiled document's reply promotes into Unfiled, with the names the
/// reply used as its own, so review marks them again.
#[gpui::test]
fn an_unfiled_reply_promotes_into_unfiled_and_review_marks_its_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "Anna Petrova called Boris Ivanov.",
        &[("Anna Petrova", Person), ("Boris Ivanov", Person)],
    );
    let source = active_document(&tabs, cx);
    let reply = paste_reply(&tabs, cx, "PERSON_2 called PERSON_1 back.");
    let document = promote(&tabs, cx);
    assert_ne!(document, source);
    let vault = vault(&tabs, cx);
    let stored = vault.document(document.as_str()).unwrap();
    assert_eq!(stored.project, None);
    assert_eq!(stored.markdown, "Boris Ivanov called Anna Petrova back.");
    assert_eq!(stored.from_reply.as_deref(), Some(reply.as_str()));
    assert!(vault.reply(&reply).is_ok());
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let review = view.read(cx).pii.review();
        let marked: Vec<(String, String)> = (review.variants().iter())
            .filter(|v| !v.mentions.is_empty())
            .map(|v| (v.original.to_string(), v.replacement.clone()))
            .collect();
        assert!(
            marked.contains(&("Anna Petrova".into(), "PERSON_1".into())),
            "{marked:?}"
        );
        assert!(
            marked.contains(&("Boris Ivanov".into(), "PERSON_2".into())),
            "{marked:?}"
        );
    });
}
