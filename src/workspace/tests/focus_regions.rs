//! Keyboard reach: F6 regions, Tab and Escape across the window (`FocusRegions`).
use super::*;

#[gpui::test]
fn app_commands_work_without_control_focus(cx: &mut TestAppContext) {
    use gpui::Action;
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    for action in [
        Quit.boxed_clone(),
        Settings.boxed_clone(),
        crate::menus::About.boxed_clone(),
        New.boxed_clone(),
        Open.boxed_clone(),
        OpenProject.boxed_clone(),
        ToggleTheme.boxed_clone(),
        ToggleSidebar.boxed_clone(),
    ] {
        assert!(cx.update(|_, cx| cx.is_action_available(action.as_ref())));
    }
    cx.dispatch_action(New);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    cx.dispatch_action(Settings);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).settings.read(cx).open));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        window.blur(cx);
    });
    draw(cx);
    cx.dispatch_action(crate::menus::About);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let settings = tabs.read(cx).settings.read(cx);
        assert!(settings.open);
        assert_eq!(settings.category(), settings_ui::Category::About);
    });
}

#[gpui::test]
fn app_commands_preserve_confirmation_focus(cx: &mut TestAppContext) {
    use gpui::Action;
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, cx| tabs.install_prompts(cx));
    let _answer = cx.update(|window, cx| {
        window.prompt(
            gpui::PromptLevel::Warning,
            "Keep this confirmation focused",
            None,
            &["Continue", "Cancel"],
            cx,
        )
    });
    cx.run_until_parked();
    draw(cx);
    let focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for action in [
        Quit.boxed_clone(),
        Settings.boxed_clone(),
        crate::menus::About.boxed_clone(),
        New.boxed_clone(),
        Open.boxed_clone(),
        OpenProject.boxed_clone(),
        ToggleTheme.boxed_clone(),
        ToggleSidebar.boxed_clone(),
    ] {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        cx.run_until_parked();
        draw(cx);
        cx.update(|window, cx| {
            assert!(window.has_active_prompt());
            assert_eq!(window.focused(cx), Some(focus.clone()));
            assert_eq!(tabs.read(cx).tabs.len(), 1);
            assert!(!tabs.read(cx).settings.read(cx).open);
        });
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, _| assert!(!window.has_active_prompt()));
}

#[gpui::test]
fn f6_cycles_sidebar_toolbar_and_editor_both_ways(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    assert!(editor_focused(&tabs, cx));
    // Editor → sidebar (wrapping) → toolbar → editor.
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    // Backwards: editor → toolbar → sidebar.
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(in_sidebar(&tabs, cx));
}

#[gpui::test]
fn f6_returns_to_the_control_last_focused_in_a_region(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    // Into the toolbar, then Tab to its next control.
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    let first = cx.update(|window, cx| window.focused(cx));
    key(cx, "tab");
    let second = cx.update(|window, cx| window.focused(cx));
    assert_ne!(first, second);
    assert!(!editor_focused(&tabs, cx));
    // On through the editor and the sidebar and back: the same control.
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), second);
}

/// Leave the sidebar from `focus` for the editor, as a keyboard user does.
fn leave_sidebar_from(
    tabs: &Entity<Workspace>,
    focus: gpui::FocusHandle,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    assert!(in_sidebar(tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(tabs, cx));
}

#[gpui::test]
fn f6_into_the_sidebar_lands_on_the_shown_documents_row(cx: &mut TestAppContext) {
    // Not Settings or another control last focused there.
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let [first, active] = open_notes(&tabs, cx, dir.path(), &["a", "b"])[..] else {
        unreachable!()
    };
    draw(cx);
    assert_eq!(cx.update(|_, cx| tabs.read(cx).tabs.active()), active);
    let settings = cx.update(|_, cx| tabs.read(cx).settings_focus.clone());
    // Expanded: after Settings, and after another document's row.
    leave_sidebar_from(&tabs, settings.clone(), cx);
    key(cx, "f6");
    assert!(
        tab_row_focused(&tabs, active, cx),
        "F6 lands on the shown row"
    );
    let other = tree_row_focus(&tabs, first, cx);
    leave_sidebar_from(&tabs, other, cx);
    key(cx, "f6");
    assert!(
        tab_row_focused(&tabs, active, cx),
        "not the row last focused"
    );
    // Backwards from the toolbar too.
    leave_sidebar_from(&tabs, settings.clone(), cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(tab_row_focused(&tabs, active, cx));
    // After opening and closing Settings itself.
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    cx.run_until_parked();
    draw(cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx));
    });
    draw(cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "f6");
    assert!(
        tab_row_focused(&tabs, active, cx),
        "after visiting Settings"
    );
    // Collapsed rail: F6 enters the rail, which has no document rows.
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
    collapse(&tabs, cx);
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    // Expand: F6 lands on the shown row in the tree.
    tabs.update(cx, |tabs, cx| {
        tabs.sidebar.restore(true, Some(true));
        cx.notify();
    });
    draw(cx);
    leave_sidebar_from(&tabs, settings, cx);
    key(cx, "f6");
    assert!(
        tab_row_focused(&tabs, active, cx),
        "across a collapse and expand"
    );
}

#[gpui::test]
fn f6_into_the_sidebar_on_home_lands_on_home(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    let (settings, home_row) = cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        (tabs.settings_focus.clone(), tabs.panel_home_focus.clone())
    });
    let in_home = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            tabs.read(cx)
                .focus_regions
                .home()
                .contains_focused(window, cx)
        })
    };
    cx.update(|window, cx| window.focus(&settings, cx));
    draw(cx);
    key(cx, "f6");
    assert!(in_home(&tabs, cx));
    key(cx, "f6");
    assert!(
        cx.update(|window, _| home_row.is_focused(window)),
        "Home's row"
    );
    // Collapsed: Home's rail row opens the reveal onto Home's row.
    key(cx, "f6");
    assert!(in_home(&tabs, cx));
    collapse(&tabs, cx);
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    assert!(cx.update(|window, cx| {
        let tabs = tabs.read(cx);
        home_row.is_focused(window) || tabs.rail_home_focus.is_focused(window)
    }));
}

#[gpui::test]
fn escape_in_the_toolbar_or_sidebar_returns_to_the_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn tab_from_home_enters_its_controls(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    assert!(cx.update(|_, cx| tabs.read(cx).showing_home(cx)));
    tabs.update_in(cx, |tabs, window, cx| window.focus(&tabs.focus, cx));
    draw(cx);
    key(cx, "tab");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .home()
            .contains_focused(window, cx)
    }));
    let home = cx.update(|window, cx| window.focused(cx));
    assert_ne!(
        home,
        cx.update(|_, cx| Some(tabs.read(cx).focus_regions.home().clone()))
    );
    // F6 on Home: sidebar, then Home again.
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .home()
            .contains_focused(window, cx)
    }));
}

/// Home's first Tab stop is Open files…, even with the header's bell
/// showing (U25).
#[gpui::test]
fn home_first_tab_is_open_files_not_the_bell(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "a").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update(cx, |tabs, cx| {
        tabs.notices.post("Saved elsewhere");
        cx.notify();
    });
    draw(cx);
    // The new notice opened the bell's popover; close it and start from
    // the window, as after a click on Home's background.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.title.notices_open.set(false);
        window.focus(&tabs.focus, cx);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_some());
    key(cx, "tab");
    activate_control(cx);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    cx.simulate_path_prompt_response(|_| Some(vec![file.clone()]));
    cx.run_until_parked();
    assert_eq!(tab_names(&tabs, cx), ["a"]);
}

#[gpui::test]
fn the_tree_moves_with_arrows_and_folds_projects(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [a, b, c] = open_notes(&tabs, cx, dir.path(), &["a", "b", "c"])[..] else {
        unreachable!()
    };
    let project = new_project(&tabs, cx, a);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    draw(cx);
    // Projects ▸ "a" (collapsed), then Unfiled: b and c, newest first.
    let rows = cx.update(|_, cx| tabs.read(cx).tree_keys());
    assert_eq!(rows[0], tree::RowKey::Project(project.clone()));
    let (b, c) = if rows[1] == tree::RowKey::Document(document_of(&tabs, b, cx)) {
        (b, c)
    } else {
        (c, b)
    };
    assert_eq!(rows[2], tree::RowKey::Document(document_of(&tabs, c, cx)));
    let focus = tree_row_focus(&tabs, c, cx);
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    key(cx, "up");
    assert!(tab_row_focused(&tabs, b, cx));
    key(cx, "up");
    let project_row = tabs.update(cx, |w, cx| {
        w.tree_focus(&tree::RowKey::Project(project.clone()), cx)
    });
    assert!(cx.update(|window, _| project_row.is_focused(window)));
    key(cx, "up");
    assert!(
        cx.update(|window, _| project_row.is_focused(window)),
        "the first row"
    );
    // → expands the Project, → again enters it, ← goes back, ← folds it.
    key(cx, "right");
    cx.update(|_, cx| assert!(tabs.read(cx).tree.expanded(&project)));
    key(cx, "right");
    assert!(tab_row_focused(&tabs, a, cx));
    key(cx, "left");
    assert!(cx.update(|window, _| project_row.is_focused(window)));
    key(cx, "left");
    cx.update(|_, cx| assert!(!tabs.read(cx).tree.expanded(&project)));
    key(cx, "down");
    assert!(tab_row_focused(&tabs, b, cx));
    // Tab leaves the tree rather than visiting each row.
    key(cx, "tab");
    assert!(!tab_row_focused(&tabs, b, cx) && !tab_row_focused(&tabs, c, cx));
}

#[gpui::test]
fn f6_includes_the_replacements_panel_when_open(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(cx.update(|window, cx| {
        view.read(cx)
            .replacements_focus()
            .contains_focused(window, cx)
    }));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
}

#[gpui::test]
fn f6_stays_inside_settings(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.show_settings(window, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.update(|_, cx| tabs.read(cx).settings.read(cx).open));
    let before = cx.update(|window, cx| window.focused(cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), before);
    key(cx, "shift-f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), before);
}

#[gpui::test]
fn tab_leaves_the_tree_from_any_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let ids = open_notes(&tabs, cx, dir.path(), &["a", "b", "c"]);
    draw(cx);
    let on_row = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            tabs.read(cx)
                .tree_row_focus
                .borrow()
                .values()
                .any(|f| f.is_focused(window))
        })
    };
    // Rows in the tree's order: newest import first, then by title.
    let rows = cx.update(|_, cx| tabs.read(cx).tree_keys());
    let in_order: Vec<u64> = rows
        .iter()
        .map(|key| {
            *ids.iter()
                .find(|id| *key == tree::RowKey::Document(document_of(&tabs, **id, cx)))
                .unwrap()
        })
        .collect();
    let focus = tree_row_focus(&tabs, in_order[2], cx);
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    // Up to the first row, then Tab: out of the tree, not to the next row.
    key(cx, "up");
    key(cx, "up");
    key(cx, "tab");
    assert!(!on_row(&tabs, cx));
    // Shift+Tab comes back to the row the arrows left it on.
    key(cx, "shift-tab");
    assert!(tab_row_focused(&tabs, in_order[0], cx));
}

#[gpui::test]
fn escape_closes_the_more_menu_before_returning_to_the_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    click_toolbar(cx, "header-more");
    let view = active(&tabs, cx);
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    tabs.update_in(cx, |tabs, window, cx| {
        let toolbar = tabs.active_view().unwrap().read(cx).toolbar_region.clone();
        if !toolbar.contains_focused(window, cx) {
            window.focus(&toolbar, cx);
        }
    });
    draw(cx);
    key(cx, "escape");
    assert!(!cx.update(|_, cx| view.read(cx).more_open));
    assert!(!editor_focused(&tabs, cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn escape_and_tab_departures_are_remembered_for_f6(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    key(cx, "tab");
    let second = cx.update(|window, cx| window.focused(cx));
    assert!(!editor_focused(&tabs, cx));
    // Leave with Esc, not F6; F6 back through the sidebar still lands there.
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), second);
}

#[gpui::test]
fn cycle_into_a_group_without_controls_keeps_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    let empty = cx.update(|_, cx| cx.focus_handle());
    cx.update(|window, cx| ui::cycle(window, cx, Some(&empty), false));
    assert!(editor_focused(&tabs, cx));
    cx.update(|window, cx| ui::cycle(window, cx, Some(&empty), true));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn the_more_menu_takes_focus_and_escape_returns_to_its_button(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    click_toolbar(cx, "header-more");
    let view = active(&tabs, cx);
    let in_menu = |view: &Entity<DocumentView>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.read(cx).more_menu_focus.contains_focused(window, cx))
    };
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    assert!(in_menu(&view, cx));
    // Tab moves through the rows and stays in the menu.
    for keys in ["tab", "tab", "shift-tab"] {
        key(cx, keys);
        assert!(in_menu(&view, cx), "{keys}");
        assert!(!cx.update(|window, cx| view.read(cx).more_menu_focus.is_focused(window)));
    }
    key(cx, "escape");
    cx.update(|window, cx| {
        assert!(!view.read(cx).more_open);
        assert!(view.read(cx).more_focus.is_focused(window));
    });
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn escape_in_the_notices_popover_closes_it(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.notices.post("Saved elsewhere");
        tabs.title.notices_open.set(true);
        cx.notify();
        window.focus(&tabs.notices_focus, cx);
    });
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).title.notices_open.get()));
    key(cx, "tab");
    key(cx, "escape");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn the_open_project_list_takes_focus_traps_tab_and_gives_it_back(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let (project, _) = saved_project(&vault(&tabs, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        let project = tabs.opened_vault().unwrap().project(&project).unwrap();
        tabs.projects.put(project);
        tabs.focus_active(window, cx);
    });
    draw(cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.show_project_list(window, cx));
    cx.run_until_parked();
    draw(cx);
    let in_list = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            tabs.read(cx)
                .project_list_focus
                .contains_focused(window, cx)
        })
    };
    assert!(cx.update(|_, cx| tabs.read(cx).project_ui.list_open));
    assert!(in_list(&tabs, cx));
    // Tab and Shift+Tab stay inside, through the Project row and Close.
    for keys in ["tab", "tab", "tab", "shift-tab", "shift-tab"] {
        key(cx, keys);
        assert!(in_list(&tabs, cx), "{keys}");
    }
    key(cx, "escape");
    assert!(!cx.update(|_, cx| tabs.read(cx).project_ui.list_open));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn clicking_more_again_closes_its_menu(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    click_toolbar(cx, "header-more");
    assert!(cx.update(|_, cx| view.read(cx).more_open));
    click_toolbar(cx, "header-more");
    assert!(!cx.update(|_, cx| view.read(cx).more_open));
    draw(cx);
    assert!(cx.update(|window, cx| view.read(cx).more_focus.is_focused(window)));
}

#[gpui::test]
fn closing_notices_with_its_button_returns_to_the_document(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.notices.post("Saved elsewhere");
        tabs.title.notices_open.set(true);
        cx.notify();
        window.focus(&tabs.notices_focus, cx);
    });
    draw(cx);
    click_toolbar(cx, "close-notices");
    cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    assert!(editor_focused(&tabs, cx));
}

#[gpui::test]
fn opening_a_project_with_enter_closes_the_list_and_focuses_its_page(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let (project, _) = saved_project(&vault(&tabs, cx));
    tabs.update_in(cx, |tabs, window, cx| {
        let project = tabs.opened_vault().unwrap().project(&project).unwrap();
        tabs.projects.put(project);
        tabs.show_project_list(window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("open-project-row").is_some());
    key(cx, "tab"); // Close.
    key(cx, "tab"); // The Project's Open button.
    activate_control(cx);
    assert!(!cx.update(|_, cx| tabs.read(cx).project_ui.list_open));
    cx.update(|window, cx| {
        let w = tabs.read(cx);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), project);
        assert!(w.focus_regions.home().contains_focused(window, cx));
        // No document opened; the empty New under the page went.
        assert!(
            w.active_view()
                .is_none_or(|view| view.read(cx).session.document.id().is_none())
        );
        assert_eq!(w.project_ui.member_reads, 0);
    });
}

#[gpui::test]
fn f6_skips_the_hidden_pane_in_a_narrow_window(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.lifecycle.set_source_only(false);
        view.original_selected = false;
        cx.notify();
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    expand_sidebar(&tabs, cx);
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.toolbar_region, cx)
    });
    key(cx, "f6");
    assert!(editor_focused(&tabs, cx));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    click_toolbar(cx, "Source");
    draw(cx);
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.toolbar_region, cx)
    });
    key(cx, "f6");
    assert!(cx.update(|window, cx| view.read(cx).preview_region.contains_focused(window, cx)));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(cx.update(|window, cx| view.read(cx).preview_region.contains_focused(window, cx)));
    // Back to the toolbar's region, on the pane switch clicked last.
    key(cx, "shift-f6");
    assert!(cx.update(|window, cx| view.read(cx).controls_region.contains_focused(window, cx)));
}

#[gpui::test]
fn ctrl_enter_opens_a_document_rows_menu(cx: &mut TestAppContext) {
    // ⌃↩ needs no fn on MacBook keyboards, unlike ⇧F10.
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(900.), px(700.)));
    let [id] = open_notes(&tabs, cx, dir.path(), &["a"])[..] else {
        unreachable!()
    };
    let document = document_of(&tabs, id, cx);
    let row = tree_row_focus(&tabs, id, cx);
    cx.update(|window, cx| window.focus(&row, cx));
    draw(cx);
    key(cx, "ctrl-enter");
    assert_eq!(
        cx.update(|_, cx| {
            (tabs.read(cx).document_menu.as_ref()).map(|(id, _)| id.as_str().to_string())
        }),
        Some(document)
    );
    key(cx, "escape");
    assert!(cx.update(|_, cx| tabs.read(cx).document_menu.is_none()));
    assert!(cx.update(|window, _| row.is_focused(window)));
}

#[gpui::test]
fn sidebar_project_actions_and_categories_work_without_a_pointer(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let case = new_project(&tabs, cx, tab);
    tabs.update(cx, |tabs, cx| tabs.dismiss_toast(cx));
    draw(cx);
    let row = tabs.update(cx, |w, cx| {
        w.tree_focus(&tree::RowKey::Project(case.clone()), cx)
    });
    cx.update(|window, cx| window.focus(&row, cx));
    key(cx, "shift-f10");
    assert!(cx.update(|_, cx| tabs.read(cx).project_ui.header_menu.is_some()));
    key(cx, "tab"); // Rename
    key(cx, "tab"); // Colour
    key(cx, "tab"); // Categories to detect.
    activate_control(cx);
    cx.update(|window, cx| {
        let w = tabs.read(cx);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), case);
        assert!(w.focus_regions.home().contains_focused(window, cx));
    });
    activate_control(cx);
    assert!(cx.update(|_, cx| {
        tabs.read(cx)
            .projects
            .project(&case)
            .unwrap()
            .skipped()
            .contains(&mdoc_pii::Category::Person)
    }));
    key(cx, "f6");
    assert!(in_sidebar(&tabs, cx));
    key(cx, "shift-f6");
    assert!(cx.update(|window, cx| {
        tabs.read(cx)
            .focus_regions
            .home()
            .contains_focused(window, cx)
    }));
}

/// Tab past the toolbar (and the view switcher, part of its region) enters
/// the editor, and from the Replacements panel's last control moves on to the
/// next region; Shift+Tab from a region's first control goes back.
/// Both sidebar layouts.
#[gpui::test]
fn tab_moves_between_regions_at_their_edges(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    let in_panel = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.read(cx)
                .replacements_focus()
                .contains_focused(window, cx)
        })
    };
    // Tab until focus leaves `inside`, with a bound against loops.
    let tab_out = |cx: &mut VisualTestContext, inside: &dyn Fn(&mut VisualTestContext) -> bool| {
        for _ in 0..60 {
            key(cx, "tab");
            if !inside(cx) {
                return;
            }
        }
        panic!("Tab never left the region");
    };
    for layout in ["expanded", "rail"] {
        if layout == "rail" {
            cx.dispatch_action(ToggleSidebar);
            cx.run_until_parked();
        }
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.focus_regions.forget();
            tabs.focus_active(window, cx);
        });
        draw(cx);
        // Toolbar: Shift+Tab from its first control goes to the sidebar;
        // Tab past its last control enters the editor.
        key(cx, "shift-f6");
        assert!(in_toolbar(&tabs, cx), "{layout}");
        key(cx, "shift-tab");
        assert!(
            in_sidebar(&tabs, cx),
            "{layout}: Shift+Tab from the toolbar"
        );
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        assert!(in_toolbar(&tabs, cx), "{layout}");
        // The view switcher counts as the toolbar's region.
        let toolbar_or_switcher = |cx: &mut VisualTestContext| {
            in_toolbar(&tabs, cx)
                || cx
                    .update(|window, cx| view.read(cx).switcher_region.contains_focused(window, cx))
        };
        let mut switcher = false;
        for _ in 0..60 {
            key(cx, "tab");
            if !toolbar_or_switcher(cx) {
                break;
            }
            switcher |= !in_toolbar(&tabs, cx);
        }
        assert!(switcher, "{layout}: Tab reaches the view switcher");
        assert!(editor_focused(&tabs, cx), "{layout}: Tab past the toolbar");
        // Panel: Shift+Tab from its first control returns to the editor;
        // Tab past its last control moves on, wrapping to the sidebar.
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        assert!(in_panel(cx), "{layout}");
        key(cx, "shift-tab");
        assert!(
            editor_focused(&tabs, cx),
            "{layout}: Shift+Tab from the panel"
        );
        tabs.update_in(cx, |tabs, _, _| tabs.focus_regions.forget());
        key(cx, "f6");
        tab_out(cx, &in_panel);
        assert!(in_sidebar(&tabs, cx), "{layout}: Tab past the panel");
        // And back: Shift+Tab from the sidebar's first control. F6 lands on
        // the active tab, so Shift+Tab up from there.
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.focus_regions.forget();
            tabs.focus_active(window, cx);
        });
        key(cx, "shift-f6");
        key(cx, "shift-f6");
        assert!(in_sidebar(&tabs, cx), "{layout}");
        for _ in 0..60 {
            key(cx, "shift-tab");
            if !in_sidebar(&tabs, cx) {
                break;
            }
        }
        assert!(in_panel(cx), "{layout}: Shift+Tab from the sidebar");
    }
}

/// The find bar and the view switcher belong to the toolbar's region: Tab
/// from the header reaches them before the editor, Esc returns to the editor
/// from them, and F6 comes back to the control last focused there.
#[gpui::test]
fn toolbar_members_take_part_in_tab_esc_and_f6(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna wrote to Bob.", cx));
        view.toggle_replacements_panel(window, cx);
    });
    cx.run_until_parked();
    let member = |cx: &mut VisualTestContext, which: &str| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            match which {
                "find" => view.find_region.contains_focused(window, cx),
                _ => view.switcher_region.contains_focused(window, cx),
            }
        })
    };
    // Esc and F6 from a view switcher control.
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    key(cx, "shift-f6");
    for _ in 0..60 {
        if member(cx, "switcher") {
            break;
        }
        key(cx, "tab");
    }
    assert!(member(cx, "switcher"));
    let control = cx.update(|window, cx| window.focused(cx));
    key(cx, "escape");
    assert!(editor_focused(&tabs, cx), "Esc from the view switcher");
    key(cx, "shift-f6");
    assert_eq!(cx.update(|window, cx| window.focused(cx)), control);
    // With find open, Tab from the header passes through it first.
    view.update_in(cx, |view, window, cx| {
        view.open_markdown_search(true, window, cx)
    });
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.focus_regions.forget();
        tabs.focus_active(window, cx);
    });
    draw(cx);
    key(cx, "shift-f6");
    assert!(in_toolbar(&tabs, cx));
    let mut found = false;
    for _ in 0..60 {
        key(cx, "tab");
        found |= member(cx, "find");
        if editor_focused(&tabs, cx) {
            break;
        }
    }
    assert!(found, "Tab reaches the find bar");
    assert!(editor_focused(&tabs, cx), "and then the editor");
}
