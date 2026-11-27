//! Tabs: opening, switching, reordering and closing them, and placeholders (`LiveViews`).
use super::*;

#[gpui::test]
fn an_unreadable_markdown_file_adds_nothing_and_says_so(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let unreadable = dir.path().join("unreadable.md");
    std::fs::write(&unreadable, [0xff, 0xfe]).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(unreadable, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.tabs.iter().all(|tab| tab.record().document.is_none()));
        let notice = tabs.notices.message().unwrap();
        assert!(notice.contains("unreadable.md"), "{notice}");
        assert!(tabs.opened_vault().unwrap().documents().is_empty());
    });
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).active_view().unwrap().read(cx).display_name(),
            "Untitled.md"
        );
    });
}

#[gpui::test]
fn untitled_numbering_reserves_later_restored_names_before_naming_legacy_blanks(
    cx: &mut TestAppContext,
) {
    // The older manifest's unnamed blank precedes named, lazy records.
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: vec![
                serde_json::from_str(r#"{"blank":true}"#).unwrap(),
                TabRecord {
                    blank: true,
                    name: Some("Untitled 3.md".into()),
                    ..TabRecord::default()
                },
                TabRecord {
                    blank: true,
                    name: Some("Untitled.md".into()),
                    ..TabRecord::default()
                },
            ],
            ..Session::default()
        },
    );
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).active_view().unwrap().read(cx).display_name(),
            "Untitled 2.md"
        );
    });
    tabs.update_in(cx, |tabs, window, cx| {
        let closed = tabs.tabs[1].id();
        tabs.close_tab(closed, window, cx);
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx).active_view().unwrap().read(cx).display_name(),
            "Untitled 3.md"
        );
    });
}

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
    tabs.update_in(cx, |tabs, window, cx| tabs.show_next_recent(window, cx));
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
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
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
        assert!(tabs.tabs[2].record().document.is_some());
        assert!(tabs.notices.message().unwrap().contains("unknown.bin"));
        assert!(!source.read(cx).lifecycle.source_only());
        assert!(source.read(cx).preview.pdf().is_some());
        assert!(!source.read(cx).editor.read(cx).text().is_empty());
        assert_eq!(original.read(cx).editor.read(cx).text(), "keep edits");
        tabs.tabs.active()
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![pdf, markdown], window, cx)
    });
    cx.run_until_parked();
    // Both are open already: the first comes to the front, nothing asks.
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 4);
        assert_eq!(tabs.read(cx).tabs.active(), source_id);
        assert!(tabs.read(cx).tabs[2].view().is_none());
    });
    // Close shows Home; nothing is released or asked.
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        assert!(tabs.read(cx).showing_home(cx));
        assert_eq!(tabs.read(cx).tabs.len(), 4);
    });
}

#[gpui::test]
fn new_switch_and_close_preserve_live_editor_state(cx: &mut TestAppContext) {
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
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first_id, window, cx));
    cx.run_until_parked();
    assert_eq!(active(&tabs, cx), first);
    cx.update(|_, cx| {
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "original **Markdown**"
        );
        assert_eq!(second.read(cx).editor.read(cx).text(), "second document");
    });
    cx.dispatch_action(mdoc_editor::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(first.read(cx).editor.read(cx).text().is_empty()));
    // Close shows Home; both views stay live, with their undo.
    cx.dispatch_action(Close);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        assert!(tabs.read(cx).showing_home(cx));
        assert_eq!(tabs.read(cx).tabs.len(), 2);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first_id, window, cx));
    cx.dispatch_action(mdoc_editor::Redo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "original **Markdown**"
        )
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
    // From Home, New turns the placeholder under Home into the document;
    // typed into, it is saved as an intentional blank until it is one.
    cx.dispatch_action(Close);
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.release_active(window, cx));
    cx.run_until_parked();
    let fallback = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| {
        assert_eq!(tabs.new_tab(window, cx), fallback);
    });
    cx.run_until_parked();
    let manifest = dir.path().join("session.json");
    cx.update(|_, cx| session_store::save(&manifest, &tabs.read(cx).snapshot(cx)).unwrap());
    let session = session_store::load(&manifest).unwrap();
    assert_eq!(session.tabs.len(), 1);
    assert!(session.tabs[0].blank && !session.tabs[0].blank_disposable);
    // A second New leaves the first, which goes: it was never typed into.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!tabs.read(cx).tabs.contains(fallback)));
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
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import/text.pdf");
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
    // Launch with nothing to restore shows Home.
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
    assert_eq!(tab_names(&tabs, cx), ["a", "b"]);
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
fn cancelled_pseudonymization_setup_does_not_count_as_pseudonymized(cx: &mut TestAppContext) {
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
        assert!(!view.pseudonymized() && !view.has_applied_aliases());
    });
    click_toolbar(cx, "pii-setup-cancel");
    click_toolbar(cx, "sidebar-home");
    cx.update(|_, cx| {
        assert!(!view.read(cx).pii.setup());
        assert!(!view.read(cx).pseudonymized());
    });

    // A completed scan with nothing applied applies no alias (#306).
    view.update(cx, |view, cx| {
        view.pii.add_scan(settings::PiiConfig::default());
        view.pii.set_reviewing(false);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(view.read(cx).pseudonymized());
        assert!(!view.read(cx).has_applied_aliases());
    });

    // Applying a replacement does, and survives closing review.
    view.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual("Alice needs review", 0..5, mdoc_pii::Category::Person)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
        view.pii.set_reviewing(false);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(view.read(cx).has_applied_aliases()));
}

#[gpui::test]
fn the_window_title_names_the_document_and_only_mdoc_on_home(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.md");
    std::fs::write(&a, a.display().to_string()).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "a — mdoc"));
    cx.dispatch_action(shell::ShowHome);
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).window_title, "mdoc"));
}

/// Done when (#1160): the Window menu lists each live view by title;
/// choosing one brings the window forward and shows it; the list follows a
/// new view and a title that changes, and stays through Home. Nothing
/// moves under the palette, nor for a view no longer live.
#[gpui::test]
fn the_window_menu_lists_the_live_views(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_on_vault(cx, Session::default(), dir.path());
    let mut ids = Vec::new();
    for text in ["Lease notes", "Letter"] {
        cx.dispatch_action(New);
        cx.run_until_parked();
        cx.simulate_input(text);
        wait(cx, 1000);
        ids.push(cx.update(|_, cx| tabs.read(cx).tabs.active()));
    }
    draw(cx);
    assert_eq!(
        window_menu_views(cx),
        [("Lease notes".into(), ids[0]), ("Letter".into(), ids[1])]
    );
    // Every live view's row reaches its handler.
    let views = cx.update(|_, cx| tabs.read(cx).live_view_menu.clone());
    let rows = (crate::commands::menus::items_with(&views).into_iter())
        .filter(|(_, action)| action.as_any().is::<crate::commands::menus::ShowLiveView>());
    for (name, action) in rows {
        let available = cx.update(|window, cx| {
            window.is_action_available(action.as_ref(), cx)
                || cx.is_action_available(action.as_ref())
        });
        assert!(available, "menu item {name} has no handler");
    }
    // Choosing the first, from behind, brings the window forward to it.
    cx.deactivate_window();
    cx.dispatch_action(crate::commands::menus::ShowLiveView(ids[0]));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), ids[0]);
    assert!(cx.update(|window, _| window.is_window_active()));
    // Switching leaves the list as it was.
    assert_eq!(
        cx.update(|_, cx| tabs.read(cx).live_view_menu.clone()),
        views
    );
    // Its title follows its first line.
    cx.simulate_input(" for May");
    wait(cx, 1000);
    draw(cx);
    assert_eq!(
        window_menu_views(cx)[0],
        ("Lease notes for May".into(), ids[0])
    );
    // The palette open: nothing moves under it.
    cx.dispatch_action(OpenPalette);
    cx.run_until_parked();
    cx.dispatch_action(crate::commands::menus::ShowLiveView(ids[1]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.shell.palette.is_some());
        assert_eq!(tabs.tabs.active(), ids[0]);
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.close_palette(window, cx));
    // Home: the views stay live and listed.
    cx.dispatch_action(shell::ShowHome);
    cx.run_until_parked();
    draw(cx);
    assert_eq!(window_menu_views(cx).len(), 2);
    // One no longer live does nothing.
    cx.dispatch_action(crate::commands::menus::ShowLiveView(u64::MAX));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).showing_overview(cx)));
}

#[test]
fn a_dev_profile_window_says_so_in_its_title() {
    assert_eq!(window_title("mdoc".into(), true), "mdoc — dev profile");
    assert_eq!(
        window_title("a — mdoc".into(), true),
        "a — mdoc — dev profile"
    );
    assert_eq!(window_title("a — mdoc".into(), false), "a — mdoc");
}

#[gpui::test]
fn untitled_save_as_starts_in_the_last_used_folder(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "notes").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(file, window, cx));
    cx.run_until_parked();
    // A new document has no folder of its own: the folder of the last file
    // imported this run, not the process's working directory (#180).
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.dispatch_action(ExportMarkdown);
    cx.run_until_parked();
    let mut start = None;
    cx.simulate_new_path_selection(|directory| {
        start = Some(directory.to_path_buf());
        None
    });
    cx.run_until_parked();
    assert_eq!(start, Some(session_store::identity(dir.path())));
}
