//! Tabs: opening, switching, reordering and closing them, placeholders and Recents (`TabSet`).
use super::*;

#[gpui::test]
fn pseudonymization_mappings_survive_switches_and_end_with_the_tab(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let first_path = dir.path().join("first.md");
    let second_path = dir.path().join("second.md");
    std::fs::write(&first_path, "Alice Alice").unwrap();
    std::fs::write(&second_path, "Bob").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![first_path, second_path], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update(cx, |view, cx| {
        view.pii.set_reviewing(true);
        let id = view
            .pii
            .review_mut()
            .add_manual("Alice Alice", 0..5, Category::Person)
            .unwrap();
        view.pii.review_mut().keep(id, None);
        view.review_changed(cx);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(1, window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    cx.update(|_, cx| assert!(second.read(cx).pii.review().variants().is_empty()));
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first_id, window, cx));
    cx.update(|_, cx| {
        let view = first.read(cx);
        let group = &view.pii.review().variants()[0];
        assert_eq!(
            (group.original.as_ref(), group.replacement.as_str()),
            ("Alice", "PERSON_1")
        );
        assert_eq!(view.pii.review().remaining(), 0);
    });
    let weak = first.downgrade();
    drop(first);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first_id, window, cx));
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "closing a clean tab releases live review and mappings"
    );
}

#[gpui::test]
fn bulk_open_preserves_edits_deduplicates_and_initializes_only_first_selection(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let markdown = dir.path().join("note.md");
    let csv = dir.path().join("table.csv");
    let unknown = dir.path().join("unknown.bin");
    std::fs::write(&markdown, "# Existing\r\n").unwrap();
    std::fs::write(&csv, "name,count\napples,2\n").unwrap();
    std::fs::write(&unknown, "unknown").unwrap();
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let (tabs, cx) = boot(cx, Session::default());
    let original = active(&tabs, cx);
    cx.simulate_input("keep edits");
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| {
        Some(vec![
            pdf.clone(),
            markdown.clone(),
            csv.clone(),
            pdf.clone(),
            unknown,
        ])
    });
    cx.run_until_parked();
    let source = active(&tabs, cx);
    let source_id = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.tabs.len(), 4);
        assert!(tabs.tabs[2].view().is_none());
        assert!(tabs.tabs[3].view().is_none());
        assert_eq!(
            tabs.tabs[2].record().markdown,
            session_store::identity(&markdown)
        );
        assert!(tabs.notices.message().unwrap().contains("unknown.bin"));
        assert!(!source.read(cx).lifecycle.source_only());
        assert!(!source.read(cx).dirty(cx));
        assert!(source.read(cx).preview.pdf().is_some());
        assert!(!source.read(cx).editor.read(cx).text().is_empty());
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep edits");
        tabs.tabs.active()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![pdf, markdown], window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 4);
        assert_eq!(tabs.read(cx).tabs.active(), source_id);
        assert!(tabs.read(cx).tabs[2].view().is_none());
    });
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));
}

#[gpui::test]
fn new_switch_reorder_and_close_preserve_live_editor_state(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let first = active(&tabs, cx);
    cx.simulate_input("original **Markdown**");
    cx.run_until_parked();
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.dispatch_action(New);
    cx.run_until_parked();
    let second = active(&tabs, cx);
    assert_ne!(first, second);
    cx.simulate_input("second document");
    cx.run_until_parked();
    let second_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.reorder(first_id, second_id, cx);
        tabs.activate(first_id, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), first);
    cx.update(|_, cx| {
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "original **Markdown**"
        );
        assert!(first.read(cx).dirty(cx));
        assert_eq!(second.read(cx).editor.read(cx).text(), "second document");
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(first.read(cx).editor.read(cx).text().is_empty()));
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
    assert_eq!(active(&tabs, cx), second);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), second);
    cx.dispatch_action(Close);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .editor
                .read(cx)
                .text()
                .is_empty()
        );
    });
}

#[gpui::test]
fn retained_search_selection_and_preview_position_survive_switch(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("alpha beta alpha");
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let count = cx.update(|_, cx| first.read(cx).search.matches().len());
    let cursor = cx.update(|_, cx| first.read(cx).editor.read(cx).cursor());
    cx.dispatch_action(New);
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(id, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(first.read(cx).search.open());
        assert_eq!(first.read(cx).search.matches().len(), count);
        assert_eq!(first.read(cx).markdown_search.read(cx).value(), "alpha");
        assert_eq!(first.read(cx).editor.read(cx).cursor(), cursor);
    });
}

#[gpui::test]
fn placeholder_replacement_respects_explicit_new_and_survives_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note.md");
    std::fs::write(&file, "note").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let placeholder = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // Rejected selections and canceled pickers cannot consume the placeholder.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.finish_open(vec![(dir.path().join("bad.bin"), false)], None, window, cx)
    });
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.active(), placeholder));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 1);
        assert_ne!(tabs.read(cx).tabs.active(), placeholder);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    let fallback = cx.update(|_, cx| tabs.read(cx).tabs.active());
    // From Home, New turns the placeholder into the tab (ADR 0048); a second
    // New adds another.
    tabs.update_in(cx, |tabs, window, cx| {
        assert_eq!(tabs.new_tab(window, cx), fallback);
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let session = session_store::load(&manifest).unwrap();
    assert_eq!(session.tabs.len(), 2);
    assert!(
        session
            .tabs
            .iter()
            .all(|tab| tab.blank && !tab.blank_disposable)
    );
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.close_tab(tabs.tabs.active(), window, cx)
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(file.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(fallback)));
    // Restore both intentional blanks without deduplicating their empty paths.
    tabs.update_in(cx, |tabs, window, cx| {
        for tab in &tabs.tabs {
            if let Some(view) = tab.view() {
                Workspace::release(view, window, cx);
            }
        }
        tabs.tabs.clear();
        tabs.tab_subscriptions.clear();
        tabs.restore(session, window, cx);
        tabs.open_path(file, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));
}

#[gpui::test]
fn edited_then_emptied_placeholder_is_not_disposable(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    cx.simulate_input("temporary text");
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        assert!(view.read(cx).editor.read(cx).text().is_empty());
        assert!(!view.read(cx).lifecycle.blank_disposable());
        assert!(!tabs.read(cx).snapshot(cx).tabs[0].blank_disposable);
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import/text.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).tabs.contains(id)));
}

#[gpui::test]
fn home_opens_several_files_and_new_tab_is_a_plain_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    // Launch with nothing to restore shows Home (ADR 0048).
    assert!(cx.debug_bounds("home").is_some());
    assert!(
        cx.debug_bounds("empty-page-local").is_some(),
        "the local note shows"
    );
    click_toolbar(cx, "home-open");
    cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
    cx.run_until_parked();
    draw(cx);
    // The hidden placeholder gives way to the opened files.
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    assert!(cx.debug_bounds("home").is_none());

    // A new tab is a plain editor: no Home, no start page.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("home").is_none());
    assert!(cx.debug_bounds("empty-page").is_none());
    cx.simulate_input("x");
    let typed = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(typed.read(cx).editor.read(cx).text(), "x"));

    // Home stays one click away and leaves the tabs alone.
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home").is_some());
    assert_eq!(tab_names(&tabs, cx).len(), 3);
}

#[gpui::test]
fn cancelled_pseudonymization_setup_does_not_tag_recents(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never-scanned.md");
    std::fs::write(&path, "Alice needs review").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    click_toolbar(cx, "Pseudonymize");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.pii.setup());
        assert!(view.pii.reviewing());
        assert!(!view.pseudonymized());
        assert!(!tabs.read(cx).shell.recents.files[0].pseudonymized);
    });
    click_toolbar(cx, "pii-setup-cancel");
    click_toolbar(cx, "sidebar-home");
    assert!(cx.debug_bounds("home-recent-0").is_some());
    cx.update(|_, cx| {
        assert!(!view.read(cx).pii.setup());
        assert!(!view.read(cx).pseudonymized());
        // Both the visible tag and the row's accessibility name use this flag.
        assert!(!tabs.read(cx).shell.recents.files[0].pseudonymized);
    });

    // A completed scan counts even with no candidates, and survives closing review.
    view.update(cx, |view, cx| {
        view.pii.add_scan(settings::PiiConfig::default());
        view.pii.set_reviewing(false);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).shell.recents.files[0].pseudonymized));
}

#[gpui::test]
fn recents_stay_capped_and_cleared_when_open_documents_notify(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let mut views = Vec::new();
    for i in 0..13 {
        let path = dir.path().join(format!("{i}.md"));
        std::fs::write(&path, "text").unwrap();
        tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
        cx.run_until_parked();
        views.push(active(&tabs, cx));
    }
    let before = cx.update(|_, cx| tabs.read(cx).shell.recents.clone());
    assert_eq!(before.files.len(), 12);
    for _ in 0..2 {
        for view in &views {
            view.update(cx, |_, cx| cx.notify());
        }
        cx.run_until_parked();
        draw(cx);
        cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.recents, before));
    }
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    tabs.update(cx, |tabs, cx| {
        tabs.settings
            .update(cx, |_, cx| cx.emit(settings_ui::Event::ClearRecents));
    });
    for view in &views {
        view.update(cx, |_, cx| cx.notify());
    }
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).shell.recents.files.is_empty()));
}

#[gpui::test]
fn the_window_title_names_the_document_and_only_mdoc_on_home(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.md");
    std::fs::write(&a, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "a.md — mdoc"));
    cx.dispatch_action(shell::ShowHome);
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
}
