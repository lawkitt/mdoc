//! The sidebar: collapsing, the rail's reveal, dropped files and row names (`Sidebar`).
use super::*;

#[gpui::test]
fn pdf_preview_refits_when_sidebar_changes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(false, Some(false));
        cx.notify();
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
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
fn keyboard_reveal_escape_restores_rail_control_and_closing_opener_keeps_focus_valid(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let opener = compact_focus(&tabs, cx, first_id);
    reveal_by_keyboard(cx, &opener);
    cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        assert_eq!(tabs.sidebar.reveal(), Reveal::Shown);
        assert!(tabs.sidebar_row_focus.borrow()[&first_id].is_focused(window));
        assert_eq!(tabs.sidebar.choice(), Some(false), "a reveal never pins");
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert!(tabs.read(cx).compact_row_focus.borrow()[&first_id].is_focused(window));
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
    cx.simulate_keystrokes("escape");
    draw(cx);
    reveal_by_keyboard(cx, &opener);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_tab(first_id, window, cx));
    cx.run_until_parked();
    draw(cx);
    cx.update(|window, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert!(tabs.read(cx).sidebar.opener().is_none());
        assert!(
            tabs.read(cx)
                .active_view()
                .unwrap()
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}

#[gpui::test]
fn collapsed_entries_activate_directly_and_hover_reveals_full_list(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let second = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    collapse(&tabs, cx);
    let compact = |id| Box::leak(format!("compact-tab-{id}").into_boxed_str()) as &'static str;
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;

    // A click before hover intent activates without revealing.
    click_toolbar(cx, compact(first));
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), first);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
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
    // Rows in the reveal activate and keep it open while the pointer is inside.
    click_toolbar(cx, row(second));
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
fn new_follows_the_last_document_and_context_close_holds_the_reveal(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;
    let compact = |id| Box::leak(format!("compact-tab-{id}").into_boxed_str()) as &'static str;
    // Expanded: a full-width row directly under the last document.
    let plus = cx.debug_bounds("sidebar-new").unwrap();
    let last = cx.debug_bounds(row(first_id)).unwrap();
    assert!(plus.top() >= last.bottom() && plus.top() < last.bottom() + px(8.));
    assert!(plus.size.width > px(150.));
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
    let second_id = cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        tabs.read(cx).tabs.active()
    });
    assert!(cx.debug_bounds("sidebar-new").unwrap().top() > plus.top());

    // Collapsed: a square below the compact entries.
    collapse(&tabs, cx);
    let plus = cx.debug_bounds("sidebar-new-collapsed").unwrap();
    assert!(plus.top() >= cx.debug_bounds(compact(second_id)).unwrap().bottom());
    assert_eq!(plus.size.width, px(32.));
    cx.simulate_click(plus.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 3));

    // The context menu keeps the reveal open after the pointer leaves it.
    reveal_by_hover(&tabs, cx);
    let first = cx.debug_bounds(row(first_id)).unwrap();
    cx.simulate_mouse_down(first.center(), gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(first.center(), gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);
    let close = cx.debug_bounds("context-close-tab").unwrap();
    hover(cx, close.center());
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.len(), 2);
        assert!(tabs.read(cx).tab_menu.is_none());
    });
    hover(cx, gpui::point(px(850.), px(500.)));
    wait(cx, 1000);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden));
}

#[gpui::test]
fn unsaved_duplicate_names_keep_single_line_rows(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    draw(cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let row = |id| Box::leak(format!("tab-{id}").into_boxed_str()) as &'static str;
    let single = cx.debug_bounds(row(first)).unwrap().size.height;
    let second = tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    draw(cx);
    for id in [first, second] {
        assert_eq!(cx.debug_bounds(row(id)).unwrap().size.height, single);
    }
}

#[test]
fn duplicate_names_show_distinguishing_parent_suffixes() {
    let peers = vec![
        PathBuf::from("/contracts/first/client/sample.md"),
        PathBuf::from("/contracts/second/client/sample.md"),
    ];
    // Shown with the platform's separator.
    let suffix = |a: &str| {
        PathBuf::from(a)
            .join("client")
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(
        disambiguating_parent(&peers[0], &peers),
        Some(suffix("first"))
    );
    assert_eq!(
        disambiguating_parent(&peers[1], &peers),
        Some(suffix("second"))
    );
    // A DOCX's conversion beside an .md of that name: no folder tells them
    // apart, so the row shows the type instead of the full path.
    let same = vec![
        PathBuf::from("/contracts/client/sample.docx"),
        PathBuf::from("/contracts/client/sample.md"),
    ];
    assert_eq!(disambiguating_parent(&same[0], &same), None);
}

#[gpui::test]
fn the_badge_names_the_source_until_saved_as_markdown(cx: &mut TestAppContext) {
    let tab = |record: TabRecord| Tab::detached(1, record);
    let docx = PathBuf::from("/contracts/sample.docx");
    let conversion = tab(TabRecord {
        source_only: true,
        attachment: Some(docx.clone()),
        ..TabRecord::default()
    });
    // Saved As under another name, its DOCX still attached as the source.
    let saved = tab(TabRecord {
        markdown: PathBuf::from("/contracts/sample copy.md"),
        attachment: Some(docx.clone()),
        ..TabRecord::default()
    });
    cx.update(|cx| {
        let label = tab_label(&conversion, cx);
        assert_eq!(
            (label.format.as_str(), label.identity),
            ("DOCX", docx.clone())
        );
        let label = tab_label(&saved, cx);
        assert_eq!(label.format, "MD");
        assert_eq!(label.identity, PathBuf::from("/contracts/sample copy.md"));
        assert_eq!(label.source, Some(docx));
    });
}

#[gpui::test]
fn keyboard_reveals_last_document_in_collapsed_list(cx: &mut TestAppContext) {
    use gpui::InputEvent;
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    let last_id = tabs.update_in(cx, |tabs, window, cx| {
        for _ in 0..19 {
            tabs.new_tab(window, cx);
        }
        tabs.sidebar.choose(Some(false));
        cx.notify();
        tabs.tabs.last().unwrap().id()
    });
    draw(cx);
    let toggle = cx.update(|_, cx| tabs.read(cx).rail_toggle_focus.clone());
    reveal_by_keyboard(cx, &toggle);
    let mut reached = false;
    for _ in 0..45 {
        cx.simulate_keystrokes("tab");
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        reached = cx.update(|window, cx| {
            tabs.read(cx)
                .sidebar_row_focus
                .borrow()
                .get(&last_id)
                .is_some_and(|focus| focus.is_focused(window))
        });
        if reached {
            break;
        }
    }
    assert!(reached, "Tab must reach every document");
    let last = cx
        .debug_bounds(Box::leak(format!("tab-{last_id}").into_boxed_str()))
        .unwrap();
    cx.update(|_, cx| {
        let viewport = tabs.read(cx).sidebar_scroll.bounds();
        assert!(
            last.top() >= viewport.top() && last.bottom() <= viewport.bottom(),
            "last={last:?}, viewport={viewport:?}"
        );
    });
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
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).tabs.active(), last_id);
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
    });
}

#[gpui::test]
fn sidebar_drop_opens_files_at_the_insertion_line(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b, c, d] = ["a.md", "b.md", "c.md", "d.md"].map(|name| dir.path().join(name));
    for path in [&a, &b, &c, &d] {
        std::fs::write(path, "x").unwrap();
    }
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone(), b.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let b_id = cx.update(|_, cx| tabs.read(cx).tabs[1].id());
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{b_id}").into_boxed_str()))
        .unwrap();
    // The upper half of b inserts before it; already-open a does not move.
    let upper = gpui::point(row.center().x, row.top() + row.size.height / 4.);
    drag_files(cx, &[c.clone(), a.clone()], upper);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(1)));
    drop_files(cx, upper);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "c.md", "b.md"]);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), None));

    // Below the last row the files open at the end.
    let new = cx.debug_bounds("sidebar-new").unwrap();
    let below = gpui::point(new.center().x, new.bottom() + px(40.));
    drag_files(cx, std::slice::from_ref(&d), below);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(3)));
    drop_files(cx, below);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "c.md", "b.md", "d.md"]);
}

#[gpui::test]
fn sidebar_refuses_files_it_cannot_open(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, unknown) = (dir.path().join("a.md"), dir.path().join("образец.xyz"));
    std::fs::write(&a, "x").unwrap();
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let a_id = cx.update(|_, cx| tabs.read(cx).tabs[0].id());
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{a_id}").into_boxed_str()))
        .unwrap();
    // No insertion line: the list says the files can't be opened.
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    cx.update(|_, cx| {
        assert!(tabs.read(cx).sidebar.drop_refused());
        assert_eq!(tabs.read(cx).file_drop_index(), None);
    });
    drop_files(cx, row.center());
    assert_eq!(tab_names(&tabs, cx), ["a.md"]);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).sidebar.drop_refused());
        assert!(
            tabs.read(cx).notices.message().is_none(),
            "nothing to report"
        );
    });

    // With an openable file beside it, the skipped one is named, not its path.
    let b = dir.path().join("b.md");
    std::fs::write(&b, "x").unwrap();
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
fn the_refusal_shows_above_a_long_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..40)
        .map(|i| {
            let path = dir.path().join(format!("doc{i:02}.md"));
            std::fs::write(&path, "x").unwrap();
            path
        })
        .collect();
    let unknown = dir.path().join("sample.xyz");
    std::fs::write(&unknown, "x").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.open_paths(paths, window, cx));
    cx.run_until_parked();
    draw(cx);
    let first = cx.update(|_, cx| tabs.read(cx).tabs[0].id());
    tabs.update(cx, |tabs, _| {
        tabs.sidebar_scroll.set_offset(gpui::point(px(0.), px(0.)))
    });
    draw(cx);
    let row = cx
        .debug_bounds(Box::leak(format!("tab-{first}").into_boxed_str()))
        .unwrap();
    drag_files(cx, std::slice::from_ref(&unknown), row.center());
    let viewport = cx.update(|window, _| window.viewport_size());
    let hint = cx.debug_bounds("drop-refused").expect("refusal shown");
    let row_now = cx
        .debug_bounds(Box::leak(format!("tab-{first}").into_boxed_str()))
        .unwrap();
    assert!(hint.bottom() <= viewport.height);
    assert!(hint.bottom() <= row_now.top(), "above the rows");
    drop_files(cx, row.center());
}

#[gpui::test]
fn file_drag_over_collapsed_rail_reveals_the_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    collapse(&tabs, cx);
    let rail = gpui::point(px(20.), px(400.));
    drag_files(cx, std::slice::from_ref(&b), rail);
    wait(cx, 250);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    let list = gpui::point(px(120.), px(400.));
    cx.simulate_event(gpui::FileDropEvent::Pending { position: list });
    draw(cx);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).file_drop_index(), Some(1)));
    drop_files(cx, list);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    // The reveal behaves as for hover: it closes once the pointer leaves.
    cx.update(|_, cx| assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Shown));
    hover(cx, gpui::point(px(600.), px(400.)));
    wait(cx, 350);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).sidebar.reveal(), Reveal::Hidden);
        assert_eq!(tabs.read(cx).sidebar.choice(), Some(false));
    });
}
