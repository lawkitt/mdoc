//! Actual destination, keyboard and pointer dispatch.
use crate::workspace::sidebar::navigation::{Arrival, Stop};
use crate::workspace::tests::*;

fn back_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-["
    } else {
        "alt-left"
    }
}

fn forward_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-]"
    } else {
        "alt-right"
    }
}

/// A Vault document titled "Note", shown in a live view of its own; that
/// view's id.
fn note(w: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) -> u64 {
    let document = (w.vault.vault().unwrap())
        .create_document(mdoc_vault::NewDocument {
            project: None,
            title: "Note".into(),
            source_kind: mdoc_vault::SourceKind::Markdown,
            origin: mdoc_vault::DocumentOrigin::Ordinary,
            markdown: "note".into(),
        })
        .unwrap();
    let id = mdoc_store::document_id::VaultId::parse(&document.id).unwrap();
    w.open_vault_document(id, window, cx);
    w.tabs.active()
}

/// The stop of live view `id`'s document.
fn stop(w: &Workspace, id: u64, cx: &App) -> Stop {
    Stop::Document(w.live_document(w.tabs.get(id).unwrap(), cx).unwrap())
}

/// A Project named `name` in the window's Vault, loaded; its id.
fn add_project(w: &mut Workspace, name: &str) -> String {
    let project = (w.vault.vault().unwrap())
        .create_project(mdoc_vault::NewProject {
            name: name.into(),
            colour: None,
            pinned: false,
        })
        .unwrap();
    let id = project.id.clone();
    w.projects.put(project);
    id
}

#[gpui::test]
fn back_forward_revisits_tabs_home_and_projects_without_key_or_member_reads(
    cx: &mut TestAppContext,
) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    let id = tabs.update(cx, |w, _| add_project(w, "Raman"));
    let first = tabs.update_in(cx, note);
    tabs.update_in(cx, |w, window, cx| w.open_project(&id, window, cx));
    cx.run_until_parked();
    let second = tabs.update_in(cx, note);
    cx.run_until_parked();
    let before = tabs.read_with(cx, |w, _| w.project_ui.member_reads);
    cx.update(|window, cx| {
        let focus = tabs.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    draw(cx);
    cx.simulate_keystrokes(back_key());
    cx.run_until_parked();
    tabs.read_with(cx, |w, _| {
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), id);
        assert_eq!(w.project_ui.member_reads, before);
    });
    cx.simulate_keystrokes(back_key());
    cx.run_until_parked();
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
    // Windows/Linux traversal is chrome-only; tab navigation focused the editor.
    cx.update(|window, cx| {
        let focus = tabs.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    draw(cx);
    cx.simulate_keystrokes(back_key());
    assert!(tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    cx.simulate_keystrokes(forward_key());
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
    tabs.update_in(cx, |w, window, cx| w.navigate(false, window, cx));
    assert_eq!(
        tabs.read_with(cx, |w, _| w
            .project_ui
            .page
            .as_ref()
            .unwrap()
            .id()
            .to_owned()),
        id
    );
    tabs.update_in(cx, |w, window, cx| w.navigate(false, window, cx));
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), second);
    assert_eq!(tabs.read_with(cx, |w, _| w.project_ui.member_reads), before);
}

#[gpui::test]
fn deleted_documents_are_skipped_and_a_new_stop_after_back_discards_forward(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = note(w, window, cx);
        let deleted = note(w, window, cx);
        let last = note(w, window, cx);
        let gone = w.live_document(w.tabs.get(deleted).unwrap(), cx).unwrap();
        w.run_delete_documents(vec![gone], window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        assert_eq!(w.navigation_target(false, cx), Some(&stop(w, last, cx)));
        note(w, window, cx);
        assert_eq!(w.navigation_target(false, cx), None);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        w.navigate(true, window, cx);
        assert!(w.showing_home(cx));
    });
}

#[gpui::test]
fn navigation_tooltips_follow_live_tab_and_project_titles_and_skip_deleted_projects(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let tab = note(w, window, cx);
        let id = add_project(w, "Raman");
        w.open_project(&id, window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Note");
        w.show_home(window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Project “Raman”");
        w.projects.edit(&id, |project| {
            project.name = "Renamed".into();
            Some(())
        });
        assert_eq!(w.navigation_label(true, cx), "Back to Project “Renamed”");
        w.projects.remove(&id);
        w.navigation.forget(&Stop::Project(id.clone()));
        assert_eq!(w.navigation_target(true, cx), Some(&stop(w, tab, cx)));
        w.navigate(true, window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Home");
        assert_eq!(w.navigation_label(false, cx), "Forward to Home");
    });
}

#[cfg(not(target_os = "macos"))]
#[gpui::test]
fn alt_arrows_keep_word_moves_in_editor_and_do_not_navigate_from_text_fields(
    cx: &mut TestAppContext,
) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    cx.simulate_input("one two");
    cx.simulate_keystrokes("alt-left");
    assert_eq!(view.read_with(cx, |v, cx| v.editor.read(cx).cursor()), 4);
    assert!(!tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    cx.simulate_keystrokes("alt-right");
    assert_eq!(view.read_with(cx, |v, cx| v.editor.read(cx).cursor()), 7);
    cx.simulate_keystrokes("secondary-f");
    draw(cx);
    cx.simulate_keystrokes("alt-left");
    assert!(!tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    cx.update(|window, cx| {
        let focus = tabs.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    draw(cx);
    cx.simulate_keystrokes("alt-left");
    assert!(tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    cx.simulate_keystrokes("alt-right");
    assert!(!tabs.read_with(cx, |w, cx| w.showing_home(cx)));
}

#[gpui::test]
fn mouse_buttons_navigate_from_editor_and_text_input_once_per_press(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    let first = tabs.update_in(cx, note);
    tabs.update_in(cx, |w, window, cx| {
        note(w, window, cx);
    });
    draw(cx);
    let point = gpui::point(px(400.), px(240.));
    let back = gpui::MouseButton::Navigate(gpui::NavigationDirection::Back);
    let forward = gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward);
    cx.simulate_mouse_down(point, back, Default::default());
    cx.simulate_mouse_up(point, back, Default::default());
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
    assert!(
        !tabs.read_with(cx, |w, cx| w.showing_home(cx)),
        "mouse-up must not step again to Home"
    );
    cx.simulate_keystrokes("secondary-f");
    draw(cx);
    cx.simulate_mouse_down(point, forward, Default::default());
    assert_ne!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
}

#[gpui::test]
fn title_buttons_disable_at_ends_and_work_with_sidebar_collapsed(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    click_toolbar(cx, "workspace-back");
    assert!(tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    let first = tabs.update_in(cx, note);
    click_toolbar(cx, "workspace-back");
    assert!(tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    click_toolbar(cx, "workspace-back");
    collapse(&tabs, cx);
    click_toolbar(cx, "workspace-forward");
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
    assert!(!tabs.read_with(cx, |w, cx| w.showing_home(cx)));
    click_toolbar(cx, "workspace-forward");
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), first);
}

#[gpui::test]
fn settings_is_not_a_stop_and_navigation_preserves_its_modal_restrictions(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        w.show_settings(window, cx);
        w.navigate(true, window, cx);
        assert!(!w.showing_home(cx));
        w.settings.update(cx, |panel, cx| panel.close(window, cx));
        w.navigate(true, window, cx);
        assert!(w.showing_home(cx));
    });
}

#[gpui::test]
fn navigation_buttons_are_keyboard_reachable_in_both_sidebar_layouts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        note(w, window, cx);
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(1100.), px(760.)));
    for expanded in [false, true] {
        tabs.update_in(cx, |w, window, cx| {
            w.sidebar.restore(expanded, Some(expanded));
            w.focus_active(window, cx);
            cx.notify();
        });
        draw(cx);
        for backwards in [true, false] {
            // F6 leaves the editor or Home for the sidebar, landing on the
            // active tab; Back and Forward sit above the list. The only other
            // region in this fixture is the toolbar; optional panes are hidden.
            key(cx, "f6");
            wait(cx, 250); // settle the rail's keyboard reveal before tabbing
            let target = tabs.read_with(cx, |w, _| {
                if backwards {
                    w.sidebar_controls.back_focus.clone()
                } else {
                    w.sidebar_controls.forward_focus.clone()
                }
            });
            for _ in 0..32 {
                if cx.update(|window, _| target.is_focused(window)) {
                    break;
                }
                key(cx, "shift-tab");
                wait(cx, 250);
            }
            assert!(
                cx.update(|window, _| target.is_focused(window)),
                "{} must be reachable with sidebar expanded={expanded}",
                if backwards { "Back" } else { "Forward" }
            );
            activate_control(cx);
            assert_eq!(tabs.read_with(cx, |w, cx| w.showing_home(cx)), backwards);
        }
    }
}

#[gpui::test]
fn f6_lands_on_the_shown_row_not_back_across_sidebar_layouts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        note(w, window, cx);
    });
    let (back, active) = tabs.read_with(cx, |w, _| {
        (w.sidebar_controls.back_focus.clone(), w.tabs.active())
    });
    for expanded in [false, true] {
        tabs.update_in(cx, |w, window, cx| {
            w.sidebar.restore(expanded, Some(expanded));
            window.focus(&back, cx);
            cx.notify();
        });
        draw(cx);
        // Back is the sidebar's: F6 goes round the other regions first.
        key(cx, "f6");
        for _ in 0..3 {
            if in_sidebar(&tabs, cx) {
                break;
            }
            key(cx, "f6");
        }
        assert!(!cx.update(|window, _| back.is_focused(window)), "not Back");
        if expanded {
            assert!(tab_row_focused(&tabs, active, cx), "the shown row");
        } else {
            // The rail has no document rows: its Home row, which reveals.
            let rail = tabs.read_with(cx, |w, _| {
                (
                    w.sidebar_controls.rail_home_focus.clone(),
                    w.sidebar_controls.panel_home_focus.clone(),
                )
            });
            assert!(cx.update(|window, _| rail.0.is_focused(window) || rail.1.is_focused(window)));
        }
    }
}

#[gpui::test]
fn deleting_the_shown_document_after_back_shows_home_and_keeps_forward(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = note(w, window, cx);
        note(w, window, cx);
        let third = note(w, window, cx);
        w.navigate(true, window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        // Home takes its page and its stop.
        let gone = w.live_document(w.tabs.get(first).unwrap(), cx).unwrap();
        w.run_delete_documents(vec![gone], window, cx);
        assert!(w.showing_home(cx));
        assert_eq!(w.navigation_target(true, cx), None);
        w.navigate(false, window, cx);
        w.navigate(false, window, cx);
        assert_eq!(w.tabs.active(), third);
    });
}

#[gpui::test]
fn deleting_a_project_from_its_page_shows_home_and_keeps_forward(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let tab = tabs.update_in(cx, |w, window, cx| {
        let id = add_project(w, "Raman");
        w.open_project(&id, window, cx);
        let tab = note(w, window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), id);
        tab
    });
    let id = tabs.read_with(cx, |w, _| {
        w.project_ui.page.as_ref().unwrap().id().to_string()
    });
    tabs.update_in(cx, |w, window, cx| w.delete_project(id.clone(), window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Delete");
    cx.run_until_parked();
    tabs.read_with(cx, |w, cx| {
        assert!(w.showing_home(cx));
        // Home took the Project page's stop: Back doesn't offer Home again.
        assert_eq!(w.navigation_target(true, cx), None);
        assert_eq!(w.navigation_target(false, cx), Some(&stop(w, tab, cx)));
        assert_eq!(w.navigation_label(false, cx), "Forward to Note");
    });
}

#[gpui::test]
fn each_project_page_is_its_own_stop(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = add_project(w, "Raman");
        let second = add_project(w, "Petrova");
        w.open_project(&first, window, cx);
        w.open_project(&second, window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Project “Raman”");
        w.navigate(true, window, cx);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), first);
        assert_eq!(w.navigation_label(true, cx), "Back to Home");
        assert_eq!(
            w.navigation_label(false, cx),
            "Forward to Project “Petrova”"
        );
        w.navigate(false, window, cx);
        assert_eq!(w.project_ui.page.as_ref().unwrap().id(), second);
        assert_eq!(w.navigation_target(false, cx), None);
    });
}

#[gpui::test]
fn a_restored_workspace_starts_with_no_history(cx: &mut TestAppContext) {
    let records = ["One.md", "Two.md"].map(|name| TabRecord {
        blank: true,
        name: Some(name.into()),
        ..TabRecord::default()
    });
    let (tabs, cx) = boot(
        cx,
        Session {
            tabs: records.to_vec(),
            active: 1,
            ..Session::default()
        },
    );
    tabs.read_with(cx, |w, cx| {
        assert_eq!(w.tabs.len(), 2);
        assert_eq!(w.navigation_target(true, cx), None);
        assert_eq!(w.navigation_target(false, cx), None);
        assert_eq!(w.navigation_label(true, cx), "Back");
        assert_eq!(w.navigation_label(false, cx), "Forward");
    });
}

#[gpui::test]
fn buttons_and_actions_share_one_liveness_rule(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        // A stop on the tab now under Home: the button is off, and the
        // action must not move either.
        let placeholder = w.tabs.active();
        w.navigation = Default::default();
        w.navigation.arrive(Stop::Tab(placeholder), Arrival::Visit);
        w.navigation.arrive(Stop::Home, Arrival::Visit);
        assert_eq!(w.navigation_target(true, cx), None);
        assert_eq!(w.navigation_label(true, cx), "Back");
        w.navigate(true, window, cx);
        assert_eq!(w.navigation.current(), Some(&Stop::Home));
        assert_eq!(w.navigation_target(false, cx), None);
    });
}

#[gpui::test]
fn navigating_closes_the_document_and_project_menus(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let [first, second] = open_notes(&tabs, cx, dir.path(), &["a", "b"])[..] else {
        unreachable!()
    };
    draw(cx);
    let row = tree_row_focus(&tabs, second, cx);
    cx.update(|window, cx| window.focus(&row, cx));
    key(cx, "shift-f10");
    assert!(tabs.read_with(cx, |w, _| w.sidebar_controls.document_menu.is_some()));
    key(cx, back_key());
    tabs.read_with(cx, |w, _| {
        assert_eq!(w.tabs.active(), first);
        assert!(w.sidebar_controls.document_menu.is_none());
    });
    tabs.update_in(cx, |w, window, cx| {
        w.project_ui.header_menu = Some(("lease".into(), gpui::point(px(10.), px(10.))));
        w.navigate(false, window, cx);
        assert_eq!(w.tabs.active(), second);
        assert!(w.project_ui.header_menu.is_none());
    });
}
