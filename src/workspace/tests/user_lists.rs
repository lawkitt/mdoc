//! The user's lists: a case's from its members' Keeps, the
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
fn a_keep_in_one_project_stays_out_of_another(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met staff at Riverside.",
        &[("Anna Petrova", Person)],
    );
    let first_view = active(&tabs, cx);
    new_project(&tabs, cx, first);
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov visited Riverside.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    new_project(&tabs, cx, second);
    let project_of = |view: &Entity<DocumentView>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).project.link().unwrap().id.clone())
    };
    let (a, b) = (project_of(&first_view, cx), project_of(&second_view, cx));
    assert_ne!(a, b);
    let store = vault(&tabs, cx);

    keep_all(&first_view, "Riverside", cx);
    assert_eq!(store.project(&a).unwrap().lists.never, ["Riverside"]);
    assert!(store.project(&b).unwrap().lists.is_empty());
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
    assert!(store.project(&a).unwrap().lists.is_empty());

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
    assert_eq!(store.project(&a).unwrap().lists.never, ["Riverside"]);
    tabs.update_in(cx, |tabs, window, cx| tabs.open_project(&a, window, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("project-never-remove-0").is_some(),
        "the Project page lists Never"
    );
    assert!(cx.debug_bounds("project-always-remove-0").is_none());
    tabs.update(cx, |tabs, cx| {
        tabs.remove_project_list_entry(&a, false, "riverside", cx)
    });
    cx.run_until_parked();
    assert!(store.project(&a).unwrap().lists.is_empty());
    cx.update(|_, cx| assert!(!first_view.read(cx).scan_lists(cx).never_has("riverside")));
}

#[gpui::test]
fn user_wide_lists_apply_outside_and_inside_projects_and_survive_a_restart(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let (tabs, cx) = boot_on_vault(cx, Session::default(), &root);
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "note.md",
        "The Supreme Court heard Maya Bennett for Acme Ltd.",
        &[("Maya Bennett", Person)],
    );
    let settings = cx.update(|_, cx| tabs.read(cx).settings.clone());
    settings.update(cx, |panel, cx| {
        panel.type_user_list_entry(false, "Supreme Court", cx);
        panel.add_user_list_entry(false, cx);
        panel.type_user_list_entry(true, "Acme Ltd", cx);
        panel.add_user_list_entry(true, cx);
    });
    cx.run_until_parked();
    let saved = vault(&tabs, cx).user_lists().unwrap();
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
    // In a Project: the user-wide lists still apply under the Project's own.
    new_project(&tabs, cx, tab);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_some());
        assert!(view.read(cx).scan_lists(cx).never_has("Supreme Court"));
    });

    // The next launch reads them back from the Vault.
    tabs.update_in(cx, |tabs, window, cx| tabs.quit(window, cx));
    cx.run_until_parked();
    let (restored, cx) = boot_on_vault(&mut cx.cx, Session::default(), &root);
    cx.update(|_, cx| {
        let lists = restored.read(cx).settings.read(cx).user_lists().clone();
        assert!(lists.never_has("Supreme Court"));
        assert!(lists.always_for("Acme Ltd").is_some());
    });
}

#[gpui::test]
fn one_members_undo_leaves_another_members_keep(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met staff at Riverside.",
        &[("Anna Petrova", Person)],
    );
    let first_view = active(&tabs, cx);
    new_project(&tabs, cx, first);
    let case = cx.update(|_, cx| first_view.read(cx).project.link().unwrap().id.clone());
    let second = open_applied(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov visited Riverside.",
        &[("Boris Ivanov", Person)],
    );
    let second_view = active(&tabs, cx);
    move_tab(&tabs, cx, second, Some(&case));
    keep_all(&first_view, "Riverside", cx);
    keep_all(&second_view, "Riverside", cx);
    let store = vault(&tabs, cx);
    assert_eq!(store.project(&case).unwrap().lists.never, ["Riverside"]);
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
    assert_eq!(store.project(&case).unwrap().lists.never, ["Riverside"]);
}

/// Not a name on a saved, replaced name: its case save takes the
/// wording out of the case, alias reserved, and puts it on no list.
#[gpui::test]
fn not_a_name_leaves_the_project_and_its_lists(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let tab = open_applied(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova met staff at Riverside.",
        &[("Anna Petrova", Person)],
    );
    let view = active(&tabs, cx);
    new_project(&tabs, cx, tab);
    settle(cx);
    let id = cx.update(|_, cx| view.read(cx).project.link().unwrap().id.clone());

    let store = vault(&tabs, cx);
    let anna = |store: &mdoc_vault::Vault| {
        store
            .project(&id)
            .unwrap()
            .mapping
            .identities
            .into_iter()
            .find(|i| i.alias == "PERSON_1")
            .unwrap()
            .originals
    };
    assert_eq!(anna(&store), ["Anna Petrova"]);
    // Saved with its alias.
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(Save);
    settle(cx);
    let document = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    assert_eq!(
        store.document(document.as_str()).unwrap().markdown,
        "PERSON_1 met staff at Riverside."
    );

    view.update(cx, |view, cx| {
        let identity = view.pii.review().applied()[0].step.identity;
        view.not_a_name_identity(identity, cx);
    });
    settle(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.editor.read(cx).focus_handle(cx), cx)
    });
    cx.dispatch_action(Save);
    settle(cx);
    assert_eq!(
        store.document(document.as_str()).unwrap().markdown,
        "Anna Petrova met staff at Riverside."
    );
    assert!(anna(&store).is_empty(), "the wording left the Project");
    let case = store.project(&id).unwrap();
    assert!(
        case.mapping
            .reserved_aliases()
            .contains(&"PERSON_1".to_string())
    );
    assert!(case.lists.is_empty(), "no list entry");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).editor.read(cx).text(),
            "Anna Petrova met staff at Riverside."
        )
    });
}
