//! A selection copied by hand (#1209, Vault & Projects item 43): marked as
//! a restored reply's copy is unless the core finds it clean, in Edit,
//! Original, Pseudonymized and Side by side; "Replace N and Copy" in the
//! right-click menu while proposals are pending in it. Hand copies are the
//! user's own copies, not exits (`exits.rs`).
use crate::workspace::tests::*;
use crate::{review::CompareMode, review::restored_clipboard::is_restored};
use mdoc_editor::EditorState;
use mdoc_pii::Category::{Organization, Person};
use std::ops::Range;

/// Anna Petrova and Northwind Traders Ltd are replaced; Boris Ivanov is
/// proposed once [`propose_boris`] runs.
const TEXT: &str = "Anna Petrova met Northwind Traders Ltd. Boris Ivanov signed.";

/// An imported document with its first two names replaced.
fn replaced<'a>(
    cx: &'a mut TestAppContext,
    dir: &std::path::Path,
) -> (
    Entity<Workspace>,
    &'a mut VisualTestContext,
    Entity<DocumentView>,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    open_applied(
        &tabs,
        cx,
        dir,
        "contract.md",
        TEXT,
        &[
            ("Anna Petrova", Person),
            ("Northwind Traders Ltd", Organization),
        ],
    );
    let view = active(&tabs, cx);
    (tabs, cx, view)
}

/// Mark "Boris Ivanov" as a proposal, left to review.
fn propose_boris(view: &Entity<DocumentView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Boris Ivanov").unwrap();
        (view.pii.review_mut())
            .add_manual(&text, start..start + "Boris Ivanov".len(), Person)
            .unwrap();
        view.review_changed(cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| view.read(cx).pii.review().remaining()), 1);
}

/// `range` of `words` in `editor`'s text: its first occurrence.
fn find(editor: &Entity<EditorState>, words: &str, cx: &mut VisualTestContext) -> Range<usize> {
    let start = editor.read_with(cx, |e, _| e.text().find(words)).unwrap();
    start..start + words.len()
}

/// Select `words` in `editor` and press ⌘C / Ctrl+C: what the clipboard
/// then holds, and whether it is marked.
fn copy(editor: &Entity<EditorState>, words: &str, cx: &mut VisualTestContext) -> (String, bool) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    let range = find(editor, words, cx);
    let focus = editor.read_with(cx, |e, cx| e.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    editor.update(cx, |e, cx| e.set_selection(range, cx));
    cx.simulate_keystrokes("secondary-c");
    cx.run_until_parked();
    let item = cx.read_from_clipboard().unwrap();
    (item.text().unwrap(), is_restored(&item))
}

/// Select `words` in `editor` and right-click it: the host's item under
/// Copy in the menu, as shown.
fn menu_item(
    editor: &Entity<EditorState>,
    words: &str,
    cx: &mut VisualTestContext,
) -> Option<String> {
    let range = find(editor, words, cx);
    let focus = editor.read_with(cx, |e, cx| e.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    editor.update(cx, |e, cx| e.set_selection(range, cx));
    draw(cx);
    let at = editor
        .read_with(cx, |e, _| e.caret_bounds())
        .unwrap()
        .center();
    cx.simulate_mouse_down(at, gpui::MouseButton::Right, gpui::Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    let label = editor.read_with(cx, |e, _| e.menu_copy_action().map(|l| l.to_string()));
    assert_eq!(
        label.is_some(),
        cx.debug_bounds("menu-copy-action").is_some(),
        "the item is drawn when offered"
    );
    label
}

/// Click the menu's "Replace N and Copy".
fn choose_menu_item(cx: &mut VisualTestContext) {
    let at = cx.debug_bounds("menu-copy-action").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
}

fn view_of(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, tab: u64) -> Entity<DocumentView> {
    cx.update(|_, cx| tabs.read(cx).tabs.get(tab).unwrap().view().unwrap().clone())
}

fn main_editor(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Entity<EditorState> {
    cx.update(|_, cx| view.read(cx).editor.clone())
}

/// The Original and Pseudonymized columns of the open view.
fn columns(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
) -> (Entity<EditorState>, Entity<EditorState>) {
    cx.update(|_, cx| view.read(cx).compare.as_ref().unwrap().columns())
}

/// Done when: in Edit, a selection holding a pending name copies marked
/// and offers "Replace 1 and Copy"; a clean one copies plainly and offers
/// nothing. The item replaces the name as one undo step, listed in What
/// you did, and copies the alias, now clean.
#[gpui::test]
fn edit_marks_a_copy_holding_a_name_and_offers_replace_and_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_tabs, cx, view) = replaced(cx, dir.path());
    let editor = main_editor(&view, cx);
    // Replaced and Ready: clean.
    assert_eq!(
        copy(&editor, "PERSON_1 met ORG_1", cx),
        ("PERSON_1 met ORG_1".into(), false)
    );
    assert_eq!(menu_item(&editor, "PERSON_1 met ORG_1", cx), None);

    propose_boris(&view, cx);
    assert!(
        !copy(&editor, "PERSON_1 met ORG_1", cx).1,
        "outside the proposal"
    );
    assert_eq!(
        copy(&editor, "Boris Ivanov signed", cx),
        ("Boris Ivanov signed".into(), true)
    );
    // A selection cutting into the name counts it whole.
    assert_eq!(
        menu_item(&editor, "Ivanov signed", cx).as_deref(),
        Some("Replace 1 and Copy")
    );

    let before = editor.read_with(cx, |e, _| e.history_id());
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    choose_menu_item(cx);
    let item = cx.read_from_clipboard().unwrap();
    assert_eq!(item.text().as_deref(), Some("PERSON_2 signed"));
    assert!(!is_restored(&item), "replaced, the copy is clean");
    cx.update(|_, cx| {
        let view = view.read(cx);
        let text = view.editor.read(cx).text().to_owned();
        assert_eq!(text, "PERSON_1 met ORG_1. PERSON_2 signed.");
        assert_eq!(view.pii.review().remaining(), 0);
        let path = view.editor.read(cx).history_path();
        let (last, _) = view.pii.decisions().rows(&path).last().unwrap();
        assert_eq!(last.label(), "Applied “Boris Ivanov”");
        // Silent, as ⌘C is: no notice under the document.
        assert!(view.clipboard_notice.text().is_none());
    });
    // One undo step.
    editor.update_in(cx, |e, window, cx| e.undo_step(window, cx));
    cx.run_until_parked();
    assert_eq!(editor.read_with(cx, |e, _| e.history_id()), before);
    assert!(editor.read_with(cx, |e, _| e.text().contains("Boris Ivanov")));
}

/// Done when: a known name written out (a Keep) marks the copy, with no
/// item to offer; a document never scanned marks every copy, Copy Markdown
/// included; a clean whole text copies plainly.
#[gpui::test]
fn a_kept_name_and_a_document_never_scanned_copy_marked(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, view) = replaced(cx, dir.path());
    let editor = main_editor(&view, cx);
    let markdown = |cx: &mut VisualTestContext| {
        view.update_in(cx, |view, window, cx| {
            view.copy_markdown(&crate::document::CopyMarkdown, window, cx)
        });
        let item = cx.read_from_clipboard().unwrap();
        (item.text().unwrap(), is_restored(&item))
    };
    assert_eq!(
        markdown(cx),
        ("PERSON_1 met ORG_1. Boris Ivanov signed.".into(), false)
    );
    propose_boris(&view, cx);
    assert!(markdown(cx).1, "a pending name marks the whole text");
    // Keep it: still written out, a name the tab knows.
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let at = text.find("Boris").unwrap();
        let review = view.pii.review_mut();
        let variant = review
            .candidates()
            .iter()
            .find(|c| c.range.start == at)
            .unwrap()
            .variant;
        review.keep(variant, None);
        view.review_changed(cx);
    });
    cx.run_until_parked();
    assert!(copy(&editor, "Boris Ivanov", cx).1);
    assert_eq!(
        menu_item(&editor, "Boris Ivanov", cx),
        None,
        "nothing to replace"
    );

    // A new tab, never scanned.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    let fresh = active(&tabs, cx);
    let typed = main_editor(&fresh, cx);
    typed.update(cx, |e, cx| e.set_text("Call Anna today.", cx));
    cx.run_until_parked();
    assert_eq!(copy(&typed, "today", cx), ("today".into(), true));
    assert_eq!(menu_item(&typed, "today", cx), None);
}

/// Done when: in Original, a copy holding a restored name is marked and a
/// clean stretch is not; the Original offers no "Replace N and Copy", its
/// copy holding the originals whatever is replaced.
#[gpui::test]
fn original_marks_a_copy_holding_a_restored_name(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_tabs, cx, view) = replaced(cx, dir.path());
    view.update_in(cx, |view, window, cx| {
        view.set_view_mode(Some(CompareMode::Original), window, cx)
    });
    cx.run_until_parked();
    let (original, _) = columns(&view, cx);
    assert_eq!(original.read_with(cx, |e, _| e.text().to_owned()), TEXT);
    assert_eq!(
        copy(&original, "Anna Petrova met", cx),
        ("Anna Petrova met".into(), true)
    );
    assert_eq!(copy(&original, " met ", cx), (" met ".into(), false));
    assert!(!copy(&original, "Boris Ivanov", cx).1, "not a name yet");

    propose_boris(&view, cx);
    let (original, _) = columns(&view, cx);
    assert!(copy(&original, "Boris Ivanov", cx).1, "pending");
    assert!(!copy(&original, " met ", cx).1);
    assert_eq!(menu_item(&original, "Boris Ivanov", cx), None);
    assert_eq!(menu_item(&original, "Anna Petrova", cx), None);
}

/// Done when: in Side by side, both columns mark as their text holds, and
/// the Pseudonymized column offers "Replace N and Copy", which replaces in
/// the document and copies its alias.
#[gpui::test]
fn side_by_side_marks_each_column_and_replaces_from_the_pseudonymized(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_tabs, cx, view) = replaced(cx, dir.path());
    propose_boris(&view, cx);
    view.update_in(cx, |view, window, cx| view.toggle_compare(window, cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).compare.as_ref().map(|c| c.mode)),
        Some(CompareMode::Side)
    );
    let (original, current) = columns(&view, cx);
    assert!(copy(&original, "Northwind Traders Ltd", cx).1);
    assert!(!copy(&original, " met ", cx).1);
    assert_eq!(
        copy(&current, "PERSON_1 met ORG_1", cx),
        ("PERSON_1 met ORG_1".into(), false)
    );
    assert_eq!(
        copy(&current, "Boris Ivanov", cx),
        ("Boris Ivanov".into(), true)
    );
    assert_eq!(menu_item(&original, "Boris Ivanov", cx), None);
    assert_eq!(
        menu_item(&current, "Boris Ivanov", cx).as_deref(),
        Some("Replace 1 and Copy")
    );
    choose_menu_item(cx);
    let item = cx.read_from_clipboard().unwrap();
    assert_eq!(item.text().as_deref(), Some("PERSON_2"));
    assert!(!is_restored(&item));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.editor.read(cx).text(),
            "PERSON_1 met ORG_1. PERSON_2 signed."
        );
        assert_eq!(view.pii.review().remaining(), 0);
    });
    // The Original column now restores Boris Ivanov too.
    let (original, current) = columns(&view, cx);
    assert!(copy(&original, "Boris Ivanov", cx).1);
    assert!(!copy(&current, "PERSON_2 signed", cx).1);
}

/// Done when: a menu left open while the selection or the proposals change
/// replaces nothing: the item acts only on what it named (Codex review).
#[gpui::test]
fn a_stale_replace_and_copy_does_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_tabs, cx, view) = replaced(cx, dir.path());
    let editor = main_editor(&view, cx);
    propose_boris(&view, cx);
    let unchanged = |cx: &mut VisualTestContext| {
        choose_menu_item(cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("before")
        );
        assert_eq!(cx.update(|_, cx| view.read(cx).pii.review().remaining()), 1);
    };
    // The selection grew to the whole text after the menu opened.
    assert!(menu_item(&editor, "Boris Ivanov", cx).is_some());
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    let all = editor.read_with(cx, |e, _| 0..e.text().len());
    editor.update(cx, |e, cx| e.set_selection(all, cx));
    draw(cx);
    unchanged(cx);
    // The proposals changed under the open menu: it names one, two wait.
    assert_eq!(
        menu_item(&editor, "Boris Ivanov signed", cx).as_deref(),
        Some("Replace 1 and Copy")
    );
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("signed").unwrap();
        (view.pii.review_mut())
            .add_manual(&text, start..start + "signed".len(), Person)
            .unwrap();
        view.review_changed(cx);
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
    choose_menu_item(cx);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("before")
    );
}
/// Done when: a wording one member adds to a Project identity reaches the
/// other open members at once, so a hand copy of it there is marked (and
/// the shelf counts it as written out). Codex review, at the hub's request.
#[gpui::test]
fn a_wording_another_member_adds_marks_the_copy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met Bob. Annushka agreed.",
        &[("Anna Petrova", Person)],
    );
    let project = new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Anna Petrova signed. Annushka paid.",
        &[("Anna Petrova", Person)],
    );
    move_tab(&tabs, cx, second, Some(&project));
    let other = view_of(&tabs, cx, second);
    let other_editor = main_editor(&other, cx);
    assert!(
        !copy(&other_editor, "Annushka paid", cx).1,
        "not a name yet"
    );

    // The first member marks "Annushka" as PERSON_1 and replaces it.
    let view = view_of(&tabs, cx, first);
    view.update_in(cx, |view, window, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Annushka").unwrap();
        let review = view.pii.review_mut();
        review
            .add_manual(&text, start..start + "Annushka".len(), Person)
            .unwrap();
        let variant = (review.candidates().iter())
            .find(|c| c.range.start == start)
            .unwrap()
            .variant;
        let person = review.identity_by_alias("PERSON_1").unwrap();
        review.assign_variant(variant, person).unwrap();
        view.review_changed(cx);
        view.apply_all_pii(&PiiApplyAll, window, cx);
    });
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| tabs.project_changed(first, cx));
    cx.run_until_parked();
    let originals = cx.update(|_, cx| {
        let project = tabs.read(cx).projects.project(&project).cloned().unwrap();
        let identity = (project.mapping.exported().into_iter())
            .find(|i| i.alias == "PERSON_1")
            .unwrap();
        identity.originals
    });
    assert!(
        originals.iter().any(|o| &**o == "Annushka"),
        "{originals:?}"
    );
    let (text, marked) = copy(&other_editor, "Annushka paid", cx);
    assert_eq!(text, "Annushka paid");
    assert!(marked, "a name its Project knows now");
}
