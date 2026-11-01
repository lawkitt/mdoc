//! The user's lists (ADR 0092): a case's from its members' Keeps, the
//! user-wide ones of Settings › Detection, and what each document's scans use.
use super::*;
use mdoc_pii::Category::{Organization, Person};

/// Keep every mention of `wording` in `view` as one decision, as the popup's
/// Keep on the whole name does.
fn keep_all(view: &Entity<DocumentView>, wording: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find(wording).unwrap();
        let review = view.pii.review_mut();
        let variant = review.add_manual(&text, start..start + wording.len(), Organization);
        let variant = variant.unwrap();
        let mentions: Vec<_> = review
            .candidates()
            .iter()
            .filter(|c| c.variant == variant)
            .map(|c| (variant, c.range.clone()))
            .collect();
        review.keep_mentions(mentions);
        view.review_changed(cx);
    });
    settle(cx);
}

/// Past the case save's pause.
fn settle(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn a_keep_in_one_case_stays_out_of_another(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ))
    });
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met staff at Riverside.",
        &[("Anna Petrova", Person)],
    );
    let first_view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov visited Riverside.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(second, window, cx));
    cx.run_until_parked();
    let case_of = |view: &Entity<DocumentView>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).case.link().unwrap().id.clone())
    };
    let (a, b) = (case_of(&first_view, cx), case_of(&second_view, cx));
    assert_ne!(a, b);
    let store = mdoc_store::cases::Store::with_key(dir.path().join("cases"), &[3; 32]);

    keep_all(&first_view, "Riverside", cx);
    assert_eq!(store.load(&a).unwrap().lists.never, ["Riverside"]);
    assert!(store.load(&b).unwrap().lists.is_empty());
    cx.update(|_, cx| {
        assert!(first_view.read(cx).scan_lists(cx).never_has("riverside"));
        assert!(!second_view.read(cx).scan_lists(cx).never_has("riverside"));
    });

    // Proposed again, as undoing the Keep does: off case A's list.
    first_view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let start = text.find("Riverside").unwrap();
        let range = start..start + "Riverside".len();
        view.pii
            .review_mut()
            .propose_kept_mentions(&text, std::slice::from_ref(&range));
        view.review_changed(cx);
    });
    settle(cx);
    assert!(store.load(&a).unwrap().lists.is_empty());

    // Kept again, then removed on the case page.
    first_view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let review = view.pii.review_mut();
        let ranges: Vec<_> = review
            .candidates()
            .iter()
            .filter(|c| &text[c.range.clone()] == "Riverside")
            .map(|c| (c.variant, c.range.clone()))
            .collect();
        review.keep_mentions(ranges);
        view.review_changed(cx);
    });
    settle(cx);
    assert_eq!(store.load(&a).unwrap().lists.never, ["Riverside"]);
    tabs.update_in(cx, |tabs, window, cx| tabs.open_case(&a, window, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("case-never-remove-0").is_some(),
        "the case page lists Never"
    );
    assert!(cx.debug_bounds("case-always-remove-0").is_none());
    tabs.update(cx, |tabs, cx| {
        tabs.remove_case_list_entry(&a, false, "riverside", cx)
    });
    cx.run_until_parked();
    assert!(store.load(&a).unwrap().lists.is_empty());
    cx.update(|_, cx| assert!(!first_view.read(cx).scan_lists(cx).never_has("riverside")));
}

#[gpui::test]
fn user_wide_lists_apply_outside_and_inside_cases_and_survive_a_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let cases = dir.path().join("cases");
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(cases.clone(), &[3; 32]))
    });
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The Supreme Court heard Maya Bennett for Acme Ltd.",
        &[("Maya Bennett", Person)],
    );
    let path = dir.path().join("note.md");
    let settings = cx.update(|_, cx| tabs.read(cx).settings.clone());
    settings.update(cx, |panel, cx| {
        panel.type_user_list_entry(false, "Supreme Court", cx);
        panel.add_user_list_entry(false, cx);
        panel.type_user_list_entry(true, "Acme Ltd", cx);
        panel.add_user_list_entry(true, cx);
    });
    cx.run_until_parked();
    let saved = mdoc_store::cases::Store::with_key(cases.clone(), &[3; 32])
        .load_lists()
        .unwrap();
    assert!(saved.lists.never_has("supreme court"));
    assert_eq!(
        saved.lists.always_for("acme ltd").map(|e| e.category()),
        Some(Organization)
    );
    // Nothing reaches settings.json.
    let json = std::fs::read_to_string(dir.path().join("settings.json")).unwrap_or_default();
    assert!(!json.contains("Supreme"));
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let lists = view.read(cx).scan_lists(cx);
        assert!(lists.never_has("Supreme Court") && lists.always_for("Acme Ltd").is_some());
    });
    // In a case: the user-wide lists still apply under the case's own.
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(tab, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(view.read(cx).case.link().is_some());
        assert!(view.read(cx).scan_lists(cx).never_has("Supreme Court"));
    });

    // The next launch reads them back with the cases.
    let restored = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.use_case_store(mdoc_store::cases::Store::with_key(cases.clone(), &[3; 32]));
            restored.restore(Session::default(), window, cx);
            restored
        })
    });
    restored.update_in(cx, |w, window, cx| {
        w.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let lists = restored.read(cx).settings.read(cx).user_lists().clone();
        assert!(lists.never_has("Supreme Court"));
        assert!(lists.always_for("Acme Ltd").is_some());
    });
}
