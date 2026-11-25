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

/// Copy for AI in the editor; what the clipboard holds after, and why
/// nothing was copied if so.
fn copy(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> (String, Option<String>) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    view.update_in(cx, |view, window, cx| {
        view.error = None;
        view.copy_for_ai(&CopyForAi, window, cx)
    });
    paused(cx);
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    (clipboard, view.read_with(cx, |view, _| view.error.clone()))
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
    assert_eq!(
        why.as_deref(),
        Some("Not copied: 1 proposal is left to review.")
    );
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
    assert_eq!(
        why.as_deref(),
        Some("Not copied: 1 proposal is left to review.")
    );
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
