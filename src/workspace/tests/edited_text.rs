//! An edit's new text is scanned (Vault & Projects item 44), and item 39's
//! desktop part: once typing pauses, the paragraphs edited since they were
//! scanned are scanned as a fresh document is (`pii::ui::edit_scan`). A
//! new name typed into a Ready document becomes a proposal and never
//! leaves as typed; an edit with no new name is Ready again with no click.
use super::*;
use crate::pii::ui::{CopyForAi, TEST_SCAN};
use mdoc_pii::{Category::Person, Detection, Recognizer};

/// Pending autosaves, and the Project writes they start, land.
fn written(cx: &mut VisualTestContext) {
    wait(cx, 800);
    wait(cx, 800);
}

/// The edit scan's pause, its scan and the write after it.
fn paused(cx: &mut VisualTestContext) {
    wait(cx, 1600);
    written(cx);
}

/// What the name finder finds in edited text here: Clara Moss.
fn clara(text: &str) -> Vec<Detection> {
    text.match_indices("Clara Moss")
        .map(|(start, name)| Detection {
            range: start..start + name.len(),
            category: Person,
            score: 0.9,
            recognizer: Recognizer::Model,
        })
        .collect()
}

/// Types `typed` at the end of tab `id`'s text.
fn type_at_end(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, id: u64, typed: &str) {
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(id, window, cx));
    let view = active(tabs, cx);
    view.update_in(cx, |view, window, cx| {
        let end = view.editor.read(cx).text().len();
        view.editor
            .update(cx, |editor, cx| editor.set_cursor(end, cx));
        window.focus(&view.editor.read(cx).focus_handle(cx), cx);
    });
    cx.simulate_input(typed);
}

/// Copy for AI in the editor; what the clipboard holds after, and the
/// refusal's reason if nothing was copied.
fn copy(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> (String, Option<String>) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    view.update_in(cx, |view, window, cx| {
        view.copy_refusal = None;
        view.copy_for_ai(&CopyForAi, window, cx)
    });
    paused(cx);
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    let why = view.read_with(cx, |view, _| {
        (view.copy_refusal.as_ref()).map(|why| why.reason.clone())
    });
    (clipboard, why)
}

fn readiness(
    tabs: &Entity<Workspace>,
    id: u64,
    cx: &mut VisualTestContext,
) -> mdoc_vault::Readiness {
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(id, cx).unwrap());
    vault(tabs, cx).readiness(document.as_str()).unwrap()
}

/// Item 39 (1): a document scanned with nothing found to replace is Ready,
/// and Copy for AI copies it.
#[gpui::test]
fn a_scanned_document_with_nothing_to_replace_is_ready(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The lease ends in May.",
        &[],
    );
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.scan_finding(Vec::new(), cx));
    written(cx);
    assert_eq!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
    // The ⋯ menu's Copy for AI is on, and copies it.
    assert_eq!(
        view.read_with(cx, |view, _| view.copy_for_ai_reason()),
        None
    );
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    cx.update(|window, cx| window.focus(&view.read(cx).editor.read(cx).focus_handle(cx), cx));
    more(cx, "more-copy-for-ai");
    paused(cx);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(copied, "The lease ends in May.");
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert_eq!(copied, "The lease ends in May.");
}

/// Typing in a document with no names, then Undo before the pause: the
/// text is the scanned one again, and Copy for AI copies it rather than
/// waiting on a discovery with nothing to find.
#[gpui::test]
fn undo_before_the_pause_leaves_a_document_with_no_names_ready(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The lease ends in May.",
        &[],
    );
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.scan_finding(Vec::new(), cx));
    written(cx);
    type_at_end(&tabs, cx, id, " x");
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |editor, cx| editor.undo(&mdoc_editor::Undo, window, cx))
    });
    cx.run_until_parked();
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert_eq!(copied, "The lease ends in May.");
}

/// A document with no alias, scanned with nothing found, then given a new
/// name: its Copy for AI, checked though the shelf lists no row for it,
/// refuses until the name is replaced.
#[gpui::test]
fn a_typed_name_in_a_document_with_no_alias_does_not_leave(cx: &mut TestAppContext) {
    *TEST_SCAN.lock().unwrap() = Some(clara);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The lease ends in May.",
        &[],
    );
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.scan_finding(Vec::new(), cx));
    written(cx);
    type_at_end(&tabs, cx, id, " Clara Moss agreed.");
    let (copied, why) = copy(&view, cx);
    assert_eq!(copied, "before");
    assert_eq!(why.as_deref(), Some("1 proposal is left to review."));
    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    written(cx);
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert!(
        copied.ends_with("The lease ends in May. PERSON_1 agreed."),
        "{copied}"
    );
}

/// Item 39 (2): an edit that adds no name is not Ready until its pause
/// scan, then Ready again with no click. A Copy for AI asked at once waits
/// for the scan and copies.
#[gpui::test]
fn an_edit_with_no_new_name_is_ready_again_after_the_pause(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova signed the lease.",
        &[("Anna Petrova", Person)],
    );
    written(cx);
    assert_eq!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
    type_at_end(&tabs, cx, id, " It renews.");
    // The typing's autosave lands before the pause ends.
    wait(cx, 1000);
    assert_ne!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
    let view = active(&tabs, cx);
    assert!(view.read_with(cx, |view, _| view.pii.review().edits_unscanned()));
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert!(
        copied.ends_with("PERSON_1 signed the lease. It renews."),
        "{copied}"
    );
    assert_eq!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
}

/// A new name typed into a Ready document becomes a proposal at the pause:
/// Copy for AI, asked before the scan finished, refuses it rather than
/// copying it as typed, until it is replaced.
#[gpui::test]
fn a_typed_new_name_does_not_leave_unreplaced(cx: &mut TestAppContext) {
    *TEST_SCAN.lock().unwrap() = Some(clara);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova signed the lease.",
        &[("Anna Petrova", Person)],
    );
    written(cx);
    type_at_end(&tabs, cx, id, " Clara Moss agreed.");
    let view = active(&tabs, cx);
    let (copied, why) = copy(&view, cx);
    assert_eq!(copied, "before");
    assert_eq!(why.as_deref(), Some("1 proposal is left to review."));
    assert_ne!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);

    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    written(cx);
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert!(
        copied.ends_with("PERSON_1 signed the lease. PERSON_2 agreed."),
        "{copied}"
    );
    assert!(!copied.contains("Clara"));
}

/// Close tab `id` before its pause ends, as quitting mid-edit would, and
/// open its document again: the new tab's id.
fn reopen_mid_edit(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, id: u64) -> u64 {
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(id, cx).unwrap());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    written(cx);
    let stored = vault(tabs, cx).document(document.as_str()).unwrap();
    assert!(!stored.scanned, "saved mid-edit: not all of it scanned");
    assert!(stored.ready_fingerprint.is_none());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(document, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| tabs.read(cx).tabs.active())
}

/// Q2 (b) of item 44: a document left mid-edit is saved unscanned and
/// reopens owing a scan of all its text, which the pause runs; its typed
/// name is proposed then, and scanned comes back once it is replaced.
#[gpui::test]
fn a_document_left_mid_edit_reopens_owing_a_scan(cx: &mut TestAppContext) {
    *TEST_SCAN.lock().unwrap() = Some(clara);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova signed the lease.",
        &[("Anna Petrova", Person)],
    );
    written(cx);
    type_at_end(&tabs, cx, id, " Clara Moss agreed.");
    wait(cx, 1000);
    let id = reopen_mid_edit(&tabs, cx, id);
    let view = active(&tabs, cx);
    assert!(view.read_with(cx, |view, _| view.pii.review().edits_unscanned()));
    let (copied, why) = copy(&view, cx);
    assert_eq!(copied, "before");
    assert_eq!(why.as_deref(), Some("1 proposal is left to review."));
    view.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    written(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(id, cx).unwrap());
    assert!(
        vault(&tabs, cx)
            .document(document.as_str())
            .unwrap()
            .scanned
    );
    let (copied, why) = copy(&view, cx);
    assert_eq!(why, None);
    assert!(!copied.contains("Clara"), "{copied}");
}

/// With no alias, a record saved mid-edit is not reviewed when it reopens:
/// "scanned" is only a record whose text was all scanned. With no review
/// to restore it waits for Find names, then is Ready again.
#[gpui::test]
fn a_record_saved_mid_edit_is_not_reviewed_until_scanned(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The lease ends in May.",
        &[],
    );
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.scan_finding(Vec::new(), cx));
    written(cx);
    assert_eq!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
    type_at_end(&tabs, cx, id, " It renews.");
    wait(cx, 1000);
    let id = reopen_mid_edit(&tabs, cx, id);
    let reviewed = |cx: &mut VisualTestContext| {
        let view = active(&tabs, cx);
        cx.update(|_, cx| super::super::shelf_source::ready(view.read(cx), cx))
    };
    assert!(!reviewed(cx), "not Ready before its scan");
    paused(cx);
    assert!(!reviewed(cx), "the pause is no Find names");
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| view.scan_finding(Vec::new(), cx));
    written(cx);
    assert!(reviewed(cx));
    assert_eq!(readiness(&tabs, id, cx), mdoc_vault::Readiness::Ready);
}

/// A Project member saved mid-edit and then moved, which clears its review
/// but keeps its aliases, still owes the scan of all its text when it
/// opens: its typed name never leaves as typed.
#[gpui::test]
fn a_document_moved_after_a_save_mid_edit_owes_a_scan(cx: &mut TestAppContext) {
    *TEST_SCAN.lock().unwrap() = Some(clara);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let id = open_applied(
        &tabs,
        cx,
        dir.path(),
        "lease.md",
        "Anna Petrova signed the lease.",
        &[("Anna Petrova", Person)],
    );
    new_project(&tabs, cx, id);
    written(cx);
    type_at_end(&tabs, cx, id, " Clara Moss agreed.");
    wait(cx, 1000);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(id, cx).unwrap());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(id, window, cx));
    written(cx);
    vault(&tabs, cx)
        .move_document(document.as_str(), None)
        .unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_vault_document(document, window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let (copied, why) = copy(&view, cx);
    assert_eq!(copied, "before");
    assert_eq!(why.as_deref(), Some("1 proposal is left to review."));
}
