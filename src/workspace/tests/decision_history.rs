//! Decision history at the window boundary (ADR 0079).
use super::*;
use mdoc_pii::decisions::EntryState;

fn lines(view: &DocumentView, cx: &gpui::App) -> Vec<(String, EntryState)> {
    view.pii
        .decisions()
        .rows(&view.editor.read(cx).history_path())
        .map(|(entry, state)| (entry.line(), state))
        .collect()
}

#[gpui::test]
fn decision_history_palette_opens_the_popover(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    key(cx, "secondary-k");
    cx.simulate_input("What you did");
    draw(cx);
    key(cx, "enter");
    assert!(cx.debug_bounds("decision-history").is_some());
    tabs.read_with(cx, |tabs, cx| {
        assert!(tabs.shell.palette.is_none());
        assert!(tabs.active_view().unwrap().read(cx).history.open);
    });
}

#[gpui::test]
fn decision_history_click_tab_enter_travels_and_escape_returns_to_the_clock(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    cx.simulate_resize(size(px(1100.), px(760.)));
    view.update_in(cx, |view, window, cx| {
        view.pii.set_decision_clock(|| "14:02".into());
        let id = view.pii.review().applied()[0].id;
        view.activate_annotation(crate::pii::ui::APPLIED_ID | id, window, cx);
    });
    draw(cx);
    cx.dispatch_action(crate::pii::ui::KeepThis);
    draw(cx);
    view.read_with(cx, |view, cx| assert_eq!(lines(view, cx).len(), 2));
    click_toolbar(cx, "header-history");
    draw(cx);
    key(cx, "tab");
    // First row is Apply. Enter must actually activate its click handler.
    let before = view.read_with(cx, |view, cx| view.editor.read(cx).history_id());
    activate_control(cx);
    view.read_with(cx, |view, cx| {
        assert_ne!(view.editor.read(cx).history_id(), before);
        assert_eq!(view.editor.read(cx).text(), "PERSON_1 met ORG_1.");
        assert_eq!(lines(view, cx)[1].1, EntryState::Undone);
    });
    for keys in ["shift-tab", "tab", "tab", "tab", "shift-tab"] {
        key(cx, keys);
        cx.update(|window, cx| assert!(view.read(cx).history.menu.contains_focused(window, cx)));
    }
    key(cx, "escape");
    assert!(cx.debug_bounds("decision-history").is_none());
    cx.update(|window, cx| assert!(view.read(cx).history.button.is_focused(window)));
}

#[gpui::test]
fn decision_history_case_join_is_one_entry_on_its_undo_step(cx: &mut TestAppContext) {
    use mdoc_pii::Category::Person;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let case = tabs
        .update_in(cx, |tabs, window, cx| {
            tabs.create_case(tabs.tabs.active(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "witness.md",
        "Boris Ivanov wrote to Anna Petrova.",
        &[("Boris Ivanov", Person)],
    );
    let view = active(&tabs, cx);
    let before = view.read_with(cx, |view, cx| view.editor.read(cx).history_id());
    view.update(cx, |view, _| view.pii.set_decision_clock(|| "14:02".into()));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.join_case(second, &case, window, cx)
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_ne!(view.editor.read(cx).history_id(), before);
        assert_eq!(
            lines(view, cx).last().unwrap().0,
            "Joined case “contract” · 14:02"
        );
    });
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.read(cx).history_id(), before);
        assert_eq!(lines(view, cx).last().unwrap().1, EntryState::Undone);
    });
}

#[gpui::test]
fn decision_history_restore_from_home_and_session_snapshot_stays_private(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let source = active(&tabs, cx);
    source.update(cx, |view, _| view.pii.set_decision_clock(|| "14:02".into()));
    view_keep_one(&source, cx);
    source.read_with(cx, |view, cx| {
        assert_eq!(
            lines(view, cx).last().unwrap().0,
            "Kept “Anna Petrova” · 14:02"
        );
    });
    // Return to Apply so both identities are part of the exported restore map.
    // The undone entries still contain the original and its fixed time.
    source.update_in(cx, |view, window, cx| {
        view.travel_to_decision(0, window, cx);
        view.close_pii_popup(&crate::pii::ui::PiiClosePopup, window, cx);
    });
    cx.run_until_parked();
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 owes ORG_1.".into(),
    ));
    click_toolbar(cx, "home-restore");
    let restored = active(&tabs, cx);
    restored.read_with(cx, |view, cx| {
        assert_eq!(
            view.editor.read(cx).text(),
            "Anna Petrova owes Northwind Traders Ltd."
        );
        assert!(
            lines(view, cx)
                .last()
                .unwrap()
                .0
                .starts_with("Restored 2 names · ")
        );
    });
    tabs.read_with(cx, |tabs, cx| {
        let json = serde_json::to_string(&tabs.snapshot(cx)).unwrap();
        for secret in [
            "Anna Petrova",
            "Northwind Traders",
            "Kept",
            "Restored",
            "Applied",
            "14:02",
            "decisions",
        ] {
            assert!(!json.contains(secret), "session contains {secret}: {json}");
        }
    });
}

fn view_keep_one(view: &Entity<DocumentView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        let id = view.pii.review().applied()[0].id;
        view.activate_annotation(crate::pii::ui::APPLIED_ID | id, window, cx);
    });
    draw(cx);
    cx.dispatch_action(crate::pii::ui::KeepThis);
    draw(cx);
    cx.dispatch_action(crate::pii::ui::KeepThis);
    draw(cx);
}
