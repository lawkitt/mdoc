//! The sidebar: collapsing, the rail's reveal of the tree, dropped files and row names
//! (`Sidebar`).
use super::*;

#[gpui::test]
fn pdf_preview_refits_when_sidebar_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(false, Some(false));
        cx.notify();
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let settle = |cx: &mut VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    };
    settle(cx);
    let wide = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    cx.dispatch_action(ToggleSidebar);
    settle(cx);
    let narrow = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    assert!(
        narrow < wide - 0.05,
        "PDF must refit when sidebar opens: wide={wide}, narrow={narrow}"
    );
    cx.dispatch_action(ToggleSidebar);
    settle(cx);
    let expanded = cx.update(|_, cx| {
        view.read(cx)
            .preview
            .pdf()
            .unwrap()
            .read(cx)
            .reading_position()
            .1
    });
    assert!((expanded - wide).abs() < 0.01);
}

#[gpui::test]
fn sidebar_default_and_explicit_choice_survive_document_count_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| assert!(tabs.read(cx).sidebar.visible()));
    tabs.update_in(cx, |tabs, window, cx| {
        for _ in 0..3 {
            tabs.new_tab(window, cx);
        }
    });
    cx.run_until_parked();
    click_toolbar(cx, "sidebar-toggle");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, Some(false));
    assert!(!saved.sidebar_visible);
    let restored = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.restore(saved, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(restored.read(cx).sidebar.choice(), Some(false)));
    click_toolbar(cx, "sidebar-toggle");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, Some(true));
    assert!(saved.sidebar_visible);
    // A session saved without an explicit choice opens expanded.
    let legacy = Session {
        sidebar_visible: false,
        sidebar_choice: None,
        ..Session::default()
    };
    let legacy = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut restored = Workspace::empty(window, cx);
            restored.restore(legacy, window, cx);
            restored
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(legacy.read(cx).sidebar.choice(), None));
    let legacy = cx.update(|_, cx| legacy.read(cx).snapshot(cx));
    assert!(legacy.sidebar_choice.is_none());
}

#[gpui::test]
fn keyboard_reveal_escape_restores_the_rail_control(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    collapse(&tabs, cx);
    let opener = cx.update(|_, cx| tabs.read(cx).rail_home_focus.clone());
    reveal_by_keyboard(cx, &opener);
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.sidebar.reveal(), Reveal::Shown);
        assert!(tabs.panel_home_focus.is_focused(window));
        assert_eq!(tabs.sidebar.choice(), Some(false), "a reveal never pins");
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert!(opener.is_focused(window));
    });
    // Escape's target stays put rather than revealing again.
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));
    // The toggle reveals too, at the reveal's own toggle.
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown);
        assert!(tabs.read(cx).panel_toggle_focus.is_focused(window));
    });
}

#[gpui::test]
fn the_rail_holds_no_documents_and_hover_reveals_the_tree(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let [first, second] = open_notes(&tabs, cx, dir.path(), &["a", "b"])[..] else {
        unreachable!()
    };
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first, window, cx));
    collapse(&tabs, cx);
    let row = tree_row(&tabs, second, cx);
    // The rail lists no documents: Search, Home and New only.
    assert!(cx.debug_bounds(row).is_none());
    assert!(cx.debug_bounds("sidebar-new-collapsed").is_some());
    // A pointer passing over the rail does not reveal.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));

    reveal_by_hover(&tabs, cx);
    let overlay = cx.debug_bounds("sidebar-reveal").unwrap();
    assert_eq!(overlay.size.width, px(PANEL_WIDTH));
    let editor_left = cx.update(|_, cx| tabs.read(cx).sidebar_scroll.bounds().left());
    assert!(
        editor_left < px(40.),
        "reveal overlays from the window edge"
    );
    // Rows in the reveal open and keep it open while the pointer is inside.
    click_toolbar(cx, row);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), second);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown);
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(false));
    });
    // Leaving starts a grace period that re-entering cancels.
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    hover(cx, gpui::point(px(100.), px(400.)));
    wait(cx, 400);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 200);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    wait(cx, 200);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert_eq!(tabs.read(cx).tabs.active(), second);
    });
    assert!(cx.debug_bounds("sidebar-reveal").is_none());
    // The reveal's toggle pins the sidebar open.
    reveal_by_hover(&tabs, cx);
    click_toolbar(cx, "sidebar-toggle");
    cx.update(|_, cx| {
        assert!(tabs.read(cx).sidebar.visible());
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(true));
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
}

#[gpui::test]
fn new_sits_on_unfiled_and_a_row_menu_holds_the_reveal(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let [first] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    draw(cx);
    // Expanded: New is on Unfiled's label, above its rows.
    let plus = cx.debug_bounds("tree-new").unwrap();
    let row = tree_row(&tabs, first, cx);
    assert!(plus.bottom() <= cx.debug_bounds(row).unwrap().top());
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| {
        let view = tabs.read(cx).active_view().unwrap();
        assert!(
            view.read(cx).session.document.id().is_none(),
            "a New document"
        );
    });

    // Collapsed: a square on the rail.
    collapse(&tabs, cx);
    let plus = cx.debug_bounds("sidebar-new-collapsed").unwrap();
    assert_eq!(plus.size.width, px(32.));

    // A row's menu keeps the reveal open after the pointer leaves it.
    reveal_by_hover(&tabs, cx);
    let bounds = cx.debug_bounds(row).unwrap();
    cx.simulate_mouse_down(
        bounds.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.simulate_mouse_up(
        bounds.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.run_until_parked();
    draw(cx);
    let item = cx.debug_bounds("context-new-project").unwrap();
    hover(cx, item.center());
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).document_menu.is_none()));
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));
}

#[gpui::test]
fn the_badge_names_the_source_until_it_is_a_vault_document(cx: &mut TestAppContext) {
    let tab = |record: TabRecord| Tab::detached(1, record);
    let docx = PathBuf::from("/contracts/sample.docx");
    let conversion = tab(TabRecord {
        source_only: true,
        attachment: Some(docx.clone()),
        ..TabRecord::default()
    });
    let saved = tab(TabRecord {
        document: mdoc_store::document_id::VaultId::parse(&"5a".repeat(16)),
        ..TabRecord::default()
    });
    cx.update(|cx| {
        assert_eq!(tab_label(&conversion, cx).format, "DOCX");
        assert_eq!(tab_label(&saved, cx).format, "MD");
    });
}

#[gpui::test]
fn keyboard_reveals_the_shown_row_in_a_long_collapsed_tree(cx: &mut TestAppContext) {
    use gpui::InputEvent;
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    let names: Vec<String> = (0..19).map(|i| format!("doc{i:02}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    open_notes(&tabs, cx, dir.path(), &names);
    // The last Unfiled row, shown with Show all, is viewed now.
    tabs.update(cx, |tabs, _| tabs.tree.toggle_show_all());
    let rows = cx.update(|_, cx| tabs.read(cx).tree_keys());
    let [
        ..,
        crate::workspace::sidebar::tree::RowKey::Document(above),
        crate::workspace::sidebar::tree::RowKey::Document(bottom),
        crate::workspace::sidebar::tree::RowKey::ShowAll,
    ] = &rows[..]
    else {
        panic!("Unfiled's last rows before Show fewer: {rows:?}")
    };
    let (above, bottom) = (above.clone(), bottom.clone());
    tabs.update_in(cx, |tabs, window, cx| {
        let id = mdoc_store::document_id::VaultId::parse(&bottom).unwrap();
        tabs.open_vault_document(id, window, cx);
        tabs.sidebar.choose(Some(false));
        cx.notify();
    });
    cx.run_until_parked();
    let last = cx.update(|_, cx| tabs.read(cx).tabs.active());
    draw(cx);
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    let mut reached = false;
    for _ in 0..45 {
        cx.simulate_keystrokes("tab");
        for _ in 0..3 {
            draw(cx);
        }
        reached = tab_row_focused(&tabs, last, cx);
        if reached {
            break;
        }
    }
    assert!(reached, "Tab must reach the shown document's row");
    let bounds = {
        let row = tree_row(&tabs, last, cx);
        cx.debug_bounds(row)
    }
    .unwrap();
    cx.update(|_, cx| {
        let viewport = tabs.read(cx).sidebar_scroll.bounds();
        assert!(
            bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
            "row={bounds:?}, viewport={viewport:?}"
        );
    });
    // ↑ then Enter opens the row above.
    key(cx, "up");
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
            }
            .to_platform_input(),
            cx,
        )
    });
    cx.run_until_parked();
    let shown = cx.update(|_, cx| tabs.read(cx).tabs.active());
    assert_eq!(document_of(&tabs, shown, cx), above);
}

#[gpui::test]
fn sidebar_context_menu_dismisses_the_rows_tooltip(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [id] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    draw(cx);
    let tip = "tooltip-a";
    let row = {
        let row = tree_row(&tabs, id, cx);
        cx.debug_bounds(row)
    }
    .unwrap();
    let rest = gpui::point(row.left() + px(45.), row.top() + row.size.height / 4.);
    for drift in [px(0.), px(3.)] {
        cx.simulate_mouse_move(rest, None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        cx.run_until_parked();
        draw(cx);
        assert!(cx.debug_bounds(tip).is_some(), "the tooltip shows on hover");
        // A pointer that drifts down and right lands on the tooltip, drawn
        // a pixel past where it showed.
        let click = rest + gpui::point(drift, drift);
        cx.simulate_mouse_move(click, None, gpui::Modifiers::none());
        cx.run_until_parked();
        draw(cx);
        assert!(cx.debug_bounds(tip).is_some(), "drift={drift:?}");
        cx.simulate_mouse_down(click, gpui::MouseButton::Right, Default::default());
        cx.simulate_mouse_up(click, gpui::MouseButton::Right, Default::default());
        cx.run_until_parked();
        draw(cx);
        assert!(cx.update(|_, cx| tabs.read(cx).document_menu.is_some()));
        assert!(
            cx.debug_bounds(tip).is_none(),
            "the menu's opening dismisses the tooltip (drift={drift:?})"
        );
        // Off the menu's corner, back on the row: no tooltip over the menu.
        cx.simulate_mouse_move(
            click - gpui::point(px(2.), px(2.)),
            None,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        cx.run_until_parked();
        draw(cx);
        assert!(
            cx.debug_bounds(tip).is_none(),
            "no tooltip while the menu is open (drift={drift:?})"
        );
        // A click away closes the menu.
        let away = gpui::point(px(850.), px(500.));
        cx.simulate_mouse_move(away, None, gpui::Modifiers::none());
        cx.simulate_mouse_down(away, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_up(away, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        draw(cx);
        assert!(cx.update(|_, cx| tabs.read(cx).document_menu.is_none()));
    }
}

#[gpui::test]
fn a_background_conversion_is_named_as_its_source(cx: &mut TestAppContext) {
    // #311: before activation the second document read
    // `encrypted--errors.md`, a file that doesn't exist.
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/import");
    let docx = fixtures.join("truncated--errors.docx");
    let odt = fixtures.join("encrypted--errors.odt");
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![docx, odt.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let (id, label) = cx.update(|_, cx| {
        let tab = &tabs.read(cx).tabs[1];
        assert!(tab.view().is_none(), "the second document is not shown");
        (tab.id(), tab_label(tab, cx))
    });
    // Named as its document, which takes the file's name.
    assert_eq!(label.name, "encrypted--errors");
    let row = {
        let row = tree_row(&tabs, id, cx);
        cx.debug_bounds(row)
    }
    .unwrap();
    cx.simulate_mouse_move(
        gpui::point(row.left() + px(45.), row.top() + row.size.height / 4.),
        None,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("tooltip-encrypted--errors").is_some(),
        "the tooltip names the document"
    );
}

#[gpui::test]
fn files_dropped_on_the_tree_go_to_unfiled_or_the_project_under_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [c, d] = ["c.md", "d.md"].map(|name| dir.path().join(name));
    for path in [&c, &d] {
        std::fs::write(path, path.display().to_string()).unwrap();
    }
    let (tabs, cx) = boot_home(cx, Session::default());
    let [a, _] = open_notes(&tabs, cx, dir.path(), &["a", "b"])[..] else {
        unreachable!()
    };
    let project = new_project(&tabs, cx, a);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    draw(cx);
    // On the Project's row: imported into it.
    let row = cx
        .debug_bounds(Box::leak(
            format!("tree-project-{project}").into_boxed_str(),
        ))
        .unwrap();
    drag_files(cx, std::slice::from_ref(&c), row.center());
    drop_files(cx, row.center());
    let in_project = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let vault = tabs.read(cx).opened_vault().unwrap();
            let mut titles: Vec<_> = (vault.documents_in(Some(&project)).into_iter())
                .map(|summary| summary.title)
                .collect();
            titles.sort();
            titles
        })
    };
    assert_eq!(in_project(cx), ["a", "c"]);
    // Below the rows: Unfiled.
    let below = gpui::point(px(100.), px(600.));
    drag_files(cx, std::slice::from_ref(&d), below);
    drop_files(cx, below);
    assert_eq!(in_project(cx), ["a", "c"]);
    let unfiled: Vec<String> = cx.update(|_, cx| {
        let vault = tabs.read(cx).opened_vault().unwrap();
        (vault.documents_in(None).into_iter())
            .map(|summary| summary.title)
            .collect()
    });
    assert!(unfiled.contains(&"d".to_string()), "{unfiled:?}");
}

#[gpui::test]
fn sidebar_refuses_files_it_cannot_open(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let unknown = dir.path().join("образец.xyz");
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [a_id] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    draw(cx);
    let row = {
        let row = tree_row(&tabs, a_id, cx);
        cx.debug_bounds(row)
    }
    .unwrap();
    // The tree says the files can't be opened.
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    cx.update(|_, cx| assert!(tabs.read(cx).sidebar.drop_refused()));
    drop_files(cx, row.center());
    assert_eq!(tab_names(&tabs, cx), ["a"]);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).sidebar.drop_refused());
        assert!(
            tabs.read(cx).notices.message().is_none(),
            "nothing to report"
        );
    });

    // With an openable file beside it, the skipped one is named, not its path.
    let b = dir.path().join("b.md");
    std::fs::write(&b, b.display().to_string()).unwrap();
    drag_files(cx, &[b.clone(), unknown.clone()], row.center());
    cx.update(|_, cx| assert!(!tabs.read(cx).sidebar.drop_refused()));
    drop_files(cx, row.center());
    cx.update(|_, cx| {
        let notice = tabs.read(cx).notices.message().unwrap().to_owned();
        assert!(notice.contains("образец.xyz"), "{notice}");
        assert!(!notice.contains(&*dir.path().to_string_lossy()), "{notice}");
    });
    // The notice opened on its own while the document kept focus; Esc
    // there closes it.
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
    key(cx, "escape");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
}

#[gpui::test]
fn the_refusal_shows_above_a_long_tree(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..40)
        .map(|i| {
            let path = dir.path().join(format!("doc{i:02}.md"));
            std::fs::write(&path, path.display().to_string()).unwrap();
            path
        })
        .collect();
    let unknown = dir.path().join("sample.xyz");
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_paths(paths, window, cx));
    cx.run_until_parked();
    tabs.update(cx, |tabs, cx| {
        tabs.tree.toggle_show_all();
        tabs.sidebar_scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    });
    draw(cx);
    let first = cx.update(|_, cx| tabs.read(cx).tree_keys())[0].clone();
    let crate::workspace::sidebar::tree::RowKey::Document(first) = first else {
        panic!("Unfiled's first row")
    };
    let selector: &'static str = Box::leak(format!("tree-document-{first}").into_boxed_str());
    let row = cx.debug_bounds(selector).unwrap();
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    let viewport = cx.update(|window, _| window.viewport_size());
    let hint = cx.debug_bounds("drop-refused").expect("refusal shown");
    let row_now = cx.debug_bounds(selector).unwrap();
    assert!(hint.bottom() <= viewport.height);
    assert!(hint.bottom() <= row_now.top(), "above the rows");
    drop_files(cx, row.center());
}

#[gpui::test]
fn file_drag_over_collapsed_rail_reveals_the_tree(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let b = dir.path().join("b.md");
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    open_notes(&tabs, cx, dir.path(), &["a"]);
    collapse(&tabs, cx);
    let rail = gpui::point(px(20.), px(400.));
    drag_files(cx, std::slice::from_ref(&b), rail);
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    let list = gpui::point(px(120.), px(400.));
    cx.simulate_event(gpui::FileDropEvent::Pending { position: list });
    draw(cx);
    drop_files(cx, list);
    let mut names = tab_names(&tabs, cx);
    names.sort();
    assert_eq!(names, ["a", "b"]);
    // The reveal behaves as for hover: it closes once the pointer leaves.
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    hover(cx, gpui::point(px(600.), px(400.)));
    wait(cx, 350);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(false));
    });
}

/// A restored source tab reads the same before and after it is first opened:
/// a DOCX that converts on open, and a scan that waits for text recognition.
#[gpui::test]
fn restored_source_tabs_keep_their_names_when_opened(cx: &mut TestAppContext) {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let docx = fixtures.join("import/text.docx");
    let scan = fixtures.join("ocr-qualification/english.pdf");
    let source = |attachment: &PathBuf, name: Option<&str>| TabRecord {
        source_only: true,
        attachment: Some(attachment.clone()),
        name: name.map(str::to_owned),
        ..TabRecord::default()
    };
    let session = Session {
        tabs: vec![
            TabRecord {
                blank: true,
                name: Some("Untitled.md".into()),
                ..TabRecord::default()
            },
            // Saved before records kept the name: named as a conversion.
            source(&docx, None),
            source(&scan, Some("english.pdf")),
        ],
        active: 0,
        ..Session::default()
    };
    let (tabs, cx) = boot(cx, session);
    let names = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let tabs = tabs.read(cx);
            tabs.tabs
                .iter()
                .map(|tab| tab_label(tab, cx).name)
                .collect::<Vec<_>>()
        })
    };
    let before = names(cx);
    assert_eq!(
        before[1..],
        ["text.md".to_string(), "english.pdf".to_string()]
    );
    let ids = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .map(|t| t.id())
            .collect::<Vec<_>>()
    });
    for id in &ids[1..] {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(*id, window, cx));
        cx.run_until_parked();
    }
    // The blank, never typed into, went when it was left.
    let mut after = names(cx);
    let mut before = before[1..].to_vec();
    after.sort();
    before.sort();
    assert_eq!(after, before);
    // The scan still waits for recognition, and its record keeps its name.
    let snapshot = cx.update(|_, cx| tabs.read(cx).snapshot(cx));
    let scan = (snapshot.tabs.iter())
        .find(|tab| tab.attachment.as_ref() == Some(&scan))
        .expect("the scan's record");
    assert!(scan.source_only);
    assert_eq!(scan.name.as_deref(), Some("english.pdf"));
}

/// Drag the expanded sidebar's edge to `x` and let go.
fn drag_sidebar_edge(cx: &mut VisualTestContext, x: f32) {
    draw(cx);
    let grip = cx
        .debug_bounds("sidebar-resize")
        .expect("the expanded sidebar's edge")
        .center();
    cx.simulate_mouse_down(grip, gpui::MouseButton::Left, Default::default());
    let to = gpui::point(px(x), grip.y);
    for position in [grip + gpui::point(px(4.), px(0.)), to, to] {
        cx.simulate_mouse_move(position, Some(gpui::MouseButton::Left), Default::default());
        draw(cx);
    }
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);
}

fn saved_sidebar_width(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<u16> {
    cx.update(|_, cx| {
        tabs.read(cx)
            .preferences
            .borrow()
            .current
            .as_ref()
            .and_then(|preferences| preferences.general.sidebar_width)
    })
}

#[gpui::test]
fn dragging_the_sidebar_edge_resizes_it_within_its_range_and_saves_the_width(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    let edge = |cx: &mut VisualTestContext| cx.debug_bounds("sidebar-resize").unwrap().center().x;
    assert_eq!(edge(cx), px(PANEL_WIDTH));
    assert_eq!(saved_sidebar_width(&tabs, cx), None);
    drag_sidebar_edge(cx, 300.);
    assert_eq!(edge(cx), px(300.));
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.sidebar.width(), 300.);
        assert!(!tabs.sidebar.resizing());
        // The list is as wide as the sidebar.
        assert!(tabs.sidebar_scroll.bounds().size.width > px(280.));
    });
    assert_eq!(saved_sidebar_width(&tabs, cx), Some(300));
    // Past either end it stops at the range.
    drag_sidebar_edge(cx, 900.);
    assert_eq!(edge(cx), px(crate::workspace::sidebar::MAX_WIDTH));
    assert_eq!(saved_sidebar_width(&tabs, cx), Some(320));
    drag_sidebar_edge(cx, 20.);
    assert_eq!(edge(cx), px(crate::workspace::sidebar::MIN_WIDTH));
    assert_eq!(saved_sidebar_width(&tabs, cx), Some(160));
    // Collapsing and expanding keeps the dragged width.
    click_toolbar(cx, "sidebar-toggle");
    assert!(cx.debug_bounds("sidebar-resize").is_none());
    click_toolbar(cx, "sidebar-toggle");
    draw(cx);
    assert_eq!(edge(cx), px(crate::workspace::sidebar::MIN_WIDTH));
}

#[gpui::test]
fn a_window_narrower_than_900_collapses_the_sidebar_until_it_widens(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let resize = |cx: &mut VisualTestContext, width: f32| {
        cx.simulate_resize(size(px(width), px(700.)));
        draw(cx);
        cx.update(|_, cx| tabs.read(cx).sidebar.visible())
    };
    assert!(resize(cx, 1000.));
    assert!(!resize(cx, 899.));
    // Collapsed by itself: the rail shows, and nothing is saved as a pick.
    assert!(cx.debug_bounds("sidebar-new-collapsed").is_some());
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, None);
    assert!(resize(cx, 900.));
    assert!(cx.debug_bounds("sidebar-resize").is_some());

    // Keyboard focus in the tree moves to the rail's toggle when the
    // window narrows, and does not reveal the tree.
    let row = cx.update(|_, cx| tabs.read(cx).panel_home_focus.clone());
    cx.update(|window, cx| window.focus(&row, cx));
    assert!(!resize(cx, 800.));
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.rail_toggle_focus.is_focused(window));
        assert_eq!(tabs.sidebar.reveal(), Reveal::Hidden);
    });
    assert!(resize(cx, 1000.));

    // Collapsed by the user, it stays collapsed when the window widens.
    click_toolbar(cx, "sidebar-toggle");
    assert!(!resize(cx, 720.));
    assert!(!resize(cx, 1200.));
    let saved = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    assert_eq!(saved.sidebar_choice, Some(false));

    // Expanded by the user in a narrow window, it stays while the window is
    // narrow; once the window has widened, narrowing collapses it again.
    click_toolbar(cx, "sidebar-toggle");
    assert!(!resize(cx, 720.));
    click_toolbar(cx, "sidebar-toggle");
    draw(cx);
    assert!(cx.update(|_, cx| tabs.read(cx).sidebar.visible()));
    assert!(resize(cx, 800.));
    assert!(resize(cx, 1000.));
    assert!(!resize(cx, 720.));
}

#[test]
fn tab_names_keep_their_extension_when_truncated() {
    assert_eq!(
        split_extension("Settlement agreement draft 2.docx"),
        ("Settlement agreement draft 2", ".docx")
    );
    assert_eq!(split_extension("Notes.md"), ("Notes", ".md"));
    assert_eq!(split_extension("Untitled"), ("Untitled", ""));
    // A leading dot, a trailing dot, a long or spaced last part: no extension.
    assert_eq!(split_extension(".env"), (".env", ""));
    assert_eq!(split_extension("Draft."), ("Draft.", ""));
    assert_eq!(
        split_extension("v1.final version"),
        ("v1.final version", "")
    );
    assert_eq!(split_extension("Brief.markdown"), ("Brief.markdown", ""));
    assert_eq!(split_extension("Отчёт.pdf"), ("Отчёт", ".pdf"));
}

#[test]
fn a_cut_name_keeps_its_start_and_gives_its_end_the_rest() {
    // Ten-point characters: "Settlement draft 2" is 180 wide.
    let stem = "Settlement draft 2";
    let x = |index: usize| index as f32 * 10.;
    let width = x(stem.len());
    let cut = |room| {
        let (front, back) = middle_cut(stem, x, width, room);
        format!("{}…{}", &stem[..front], &stem[back..])
    };
    assert_eq!(cut(90.), "Settle…t 2");
    // Slack is under one character: the end takes what the start left.
    assert_eq!(cut(95.), "Settle…t 2");
    assert_eq!(cut(100.), "Settle…ft 2");
    assert_eq!(cut(0.), "…");
    assert_eq!(cut(-20.), "…");
    // Multibyte characters are cut at their boundaries.
    let stem = "Отчёт за квартал";
    let x = |index: usize| stem[..index].chars().count() as f32 * 10.;
    let (front, back) = middle_cut(stem, x, x(stem.len()), 60.);
    assert_eq!(format!("{}…{}", &stem[..front], &stem[back..]), "Отчё…ал");
}

/// While the rail's reveal slides, the document beside it reuses its last
/// frame even when the workspace redraws; the slide's end draws it afresh.
/// With reduced motion nothing slides, so nothing is reused.
#[gpui::test]
fn the_document_waits_out_the_reveal_slide_and_redraws_after(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    collapse(&tabs, cx);
    let renders = || crate::document::RENDERS.with(|renders| renders.get());
    // A frame as GPUI draws one, without `refresh`, which skips every cache.
    let frame = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    // A redraw of the workspace alone, as the sidebar's own changes cause.
    let workspace_changes = |cx: &mut VisualTestContext| {
        tabs.update(cx, |_, cx| cx.notify());
        frame(cx);
    };

    cx.update(|_, cx| cx.set_reduce_motion(false));
    tabs.update(cx, |tabs, cx| tabs.show_reveal(false, cx));
    frame(cx);
    let before = renders();
    workspace_changes(cx);
    workspace_changes(cx);
    assert_eq!(renders(), before, "the slide reuses the document's frame");
    cx.executor().advance_clock(REVEAL_SLIDE);
    cx.run_until_parked();
    frame(cx);
    assert!(
        renders() > before,
        "the slide's end draws the document afresh"
    );
    let after = renders();
    workspace_changes(cx);
    assert!(renders() > after, "outside a slide the document redraws");

    cx.update(|_, cx| cx.set_reduce_motion(true));
    tabs.update_in(cx, |tabs, window, cx| tabs.dismiss_reveal(window, cx));
    frame(cx);
    tabs.update(cx, |tabs, cx| tabs.show_reveal(false, cx));
    frame(cx);
    let shown = renders();
    workspace_changes(cx);
    assert!(renders() > shown, "reduced motion never reuses a frame");
}
