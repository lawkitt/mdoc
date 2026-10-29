//! Actual destination, keyboard and pointer dispatch (ADR 0081).
use super::*;
use crate::workspace::navigation::{Arrival, Stop};

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

#[gpui::test]
fn back_forward_revisits_tabs_home_and_cases_without_key_or_member_reads(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let case = mdoc_store::cases::Case::new("Raman".into(), &[], None);
    let id = case.id.clone();
    let first = tabs.update_in(cx, |w, window, cx| {
        w.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        w.case_coordinator.finish_load(vec![case]);
        w.new_tab(window, cx)
    });
    tabs.update_in(cx, |w, window, cx| w.open_case(&id, window, cx));
    cx.run_until_parked();
    let second = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
    cx.run_until_parked();
    let before = tabs.read_with(cx, |w, _| (w.cases.key_reads, w.cases.member_reads));
    cx.update(|window, cx| {
        let focus = tabs.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    draw(cx);
    cx.simulate_keystrokes(back_key());
    cx.run_until_parked();
    tabs.read_with(cx, |w, _| {
        assert_eq!(w.cases.page.as_ref().unwrap().id(), id);
        assert_eq!((w.cases.key_reads, w.cases.member_reads), before);
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
        tabs.read_with(cx, |w, _| w.cases.page.as_ref().unwrap().id().to_owned()),
        id
    );
    tabs.update_in(cx, |w, window, cx| w.navigate(false, window, cx));
    assert_eq!(tabs.read_with(cx, |w, _| w.tabs.active()), second);
    assert_eq!(
        tabs.read_with(cx, |w, _| (w.cases.key_reads, w.cases.member_reads)),
        before
    );
}

#[gpui::test]
fn back_to_a_locked_cached_case_never_reads_the_injected_store(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let case = mdoc_store::cases::Case::new("Raman".into(), &[], None);
    let (tabs, cx) = boot_home(
        cx,
        Session {
            cases: vec![mdoc_store::cases::CaseSummary::of(&case)],
            ..Session::default()
        },
    );
    tabs.update_in(cx, |w, window, cx| {
        w.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        w.cases.test_may_ask = true;
        w.open_case(&case.id, window, cx);
        w.new_tab(window, cx);
        w.navigate(true, window, cx);
    });
    cx.run_until_parked();
    tabs.read_with(cx, |w, _| {
        assert_eq!(w.cases.page.as_ref().unwrap().id(), case.id);
        assert!(w.case_coordinator.case(&case.id).is_none());
        assert_eq!(w.cases.key_reads, 0);
        assert_eq!(w.cases.member_reads, 0);
        assert!(!w.cases.explainer);
    });
}

#[gpui::test]
fn closed_tabs_are_skipped_and_a_new_stop_after_back_discards_forward(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = w.new_tab(window, cx);
        let closed = w.new_tab(window, cx);
        let last = w.new_tab(window, cx);
        w.remove(closed, window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        assert_eq!(w.navigation_target(false, cx), Some(&Stop::Tab(last)));
        w.new_tab(window, cx);
        assert_eq!(w.navigation_target(false, cx), None);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        w.navigate(true, window, cx);
        assert!(w.showing_home(cx));
    });
}

#[gpui::test]
fn navigation_tooltips_follow_live_tab_and_case_titles_and_skip_deleted_cases(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let tab = w.new_tab(window, cx);
        let case = mdoc_store::cases::Case::new("Raman".into(), &[], None);
        let id = case.id.clone();
        w.case_coordinator.finish_load(vec![case]);
        w.open_case(&id, window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Untitled.md");
        w.show_home(window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Case “Raman”");
        w.case_coordinator.cases_mut()[0].name = "Renamed".into();
        assert_eq!(w.navigation_label(true, cx), "Back to Case “Renamed”");
        w.forget_case(&id, window, cx);
        assert_eq!(w.navigation_target(true, cx), Some(&Stop::Tab(tab)));
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
    let (tabs, cx) = boot(cx, Session::default());
    let first = tabs.read_with(cx, |w, _| w.tabs.active());
    tabs.update_in(cx, |w, window, cx| {
        w.new_tab(window, cx);
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
    let first = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
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
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    for expanded in [false, true] {
        tabs.update_in(cx, |w, window, cx| {
            w.sidebar.restore(expanded, Some(expanded));
            w.focus_active(window, cx);
            cx.notify();
        });
        draw(cx);
        for backwards in [true, false] {
            // F6 leaves the editor or Home for the sidebar. The only other
            // region in this fixture is the toolbar; optional panes are hidden.
            key(cx, "f6");
            wait(cx, 250); // settle the rail's keyboard reveal before tabbing
            let target = tabs.read_with(cx, |w, _| {
                if backwards {
                    w.back_focus.clone()
                } else {
                    w.forward_focus.clone()
                }
            });
            for _ in 0..32 {
                if cx.update(|window, _| target.is_focused(window)) {
                    break;
                }
                key(cx, "tab");
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
fn f6_remembers_navigation_focus_across_sidebar_layouts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let focus = tabs.read_with(cx, |w, _| w.back_focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    for expanded in [false, true] {
        key(cx, "f6"); // toolbar
        key(cx, "f6"); // editor
        tabs.update(cx, |w, cx| {
            w.sidebar.restore(expanded, Some(expanded));
            cx.notify();
        });
        draw(cx);
        key(cx, "f6");
        assert!(
            cx.update(|window, _| focus.is_focused(window)),
            "Back remains the sidebar's remembered control, expanded={expanded}"
        );
    }
}

#[gpui::test]
fn closing_the_active_or_the_last_tab_never_offers_the_page_shown(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = w.new_tab(window, cx);
        let second = w.new_tab(window, cx);
        // Home → first → second; closing second shows first.
        w.close_tab(second, window, cx);
        assert_eq!(w.tabs.active(), first);
        assert_eq!(w.navigation_target(true, cx), Some(&Stop::Home));
        assert_eq!(w.navigation_label(true, cx), "Back to Home");
        assert_eq!(w.navigation_target(false, cx), None);
        // Closing the last tab shows Home: nothing either way.
        w.close_tab(first, window, cx);
        assert!(w.showing_home(cx));
        assert_eq!(w.navigation_target(true, cx), None);
        assert_eq!(w.navigation_target(false, cx), None);
    });
}

#[gpui::test]
fn closing_the_active_tab_after_back_keeps_forward(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |w, window, cx| {
        let first = w.new_tab(window, cx);
        let second = w.new_tab(window, cx);
        let third = w.new_tab(window, cx);
        w.navigate(true, window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), first);
        // The tab after it takes its page and its stop.
        w.close_tab(first, window, cx);
        assert_eq!(w.tabs.active(), second);
        assert_eq!(w.navigation_target(true, cx), Some(&Stop::Home));
        assert_eq!(w.navigation_target(false, cx), Some(&Stop::Tab(third)));
        w.navigate(false, window, cx);
        assert_eq!(w.tabs.active(), third);
    });
}

#[gpui::test]
fn deleting_a_case_from_its_page_shows_home_and_keeps_forward(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    let case = mdoc_store::cases::Case::new("Raman".into(), &[], None);
    let id = case.id.clone();
    let tab = tabs.update_in(cx, |w, window, cx| {
        w.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
        w.case_coordinator.finish_load(vec![case]);
        w.open_case(&id, window, cx);
        let tab = w.new_tab(window, cx);
        w.navigate(true, window, cx);
        assert_eq!(w.cases.page.as_ref().unwrap().id(), id);
        tab
    });
    tabs.update_in(cx, |w, window, cx| w.delete_case(id.clone(), window, cx));
    cx.simulate_prompt_answer("Delete Case");
    cx.run_until_parked();
    tabs.read_with(cx, |w, cx| {
        assert!(w.showing_home(cx));
        // Home took the case page's stop: Back doesn't offer Home again.
        assert_eq!(w.navigation_target(true, cx), None);
        assert_eq!(w.navigation_target(false, cx), Some(&Stop::Tab(tab)));
        assert_eq!(w.navigation_label(false, cx), "Forward to Untitled.md");
    });
}

#[gpui::test]
fn each_case_page_is_its_own_stop(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let raman = mdoc_store::cases::Case::new("Raman".into(), &[], None);
    let petrova = mdoc_store::cases::Case::new("Petrova".into(), &[], None);
    let (first, second) = (raman.id.clone(), petrova.id.clone());
    tabs.update_in(cx, |w, window, cx| {
        w.case_coordinator.finish_load(vec![raman, petrova]);
        w.open_case(&first, window, cx);
        w.open_case(&second, window, cx);
        assert_eq!(w.navigation_label(true, cx), "Back to Case “Raman”");
        w.navigate(true, window, cx);
        assert_eq!(w.cases.page.as_ref().unwrap().id(), first);
        assert_eq!(w.navigation_label(true, cx), "Back to Home");
        assert_eq!(w.navigation_label(false, cx), "Forward to Case “Petrova”");
        w.navigate(false, window, cx);
        assert_eq!(w.cases.page.as_ref().unwrap().id(), second);
        assert_eq!(w.navigation_target(false, cx), None);
    });
}

#[gpui::test]
fn a_restored_workspace_starts_with_no_history(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let records = ["one.md", "two.md"].map(|name| {
        let path = dir.path().join(name);
        std::fs::write(&path, name).unwrap();
        TabRecord {
            markdown: path,
            ..TabRecord::default()
        }
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
fn a_cancelled_close_or_quit_prompt_is_not_a_stop_and_keeps_forward(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("unsaved");
    let dirty = tabs.read_with(cx, |w, _| w.tabs.active());
    let (second, third) = tabs.update_in(cx, |w, window, cx| {
        let second = w.new_tab(window, cx);
        let third = w.new_tab(window, cx);
        w.navigate(true, window, cx);
        (second, third)
    });
    let unchanged = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        tabs.read_with(cx, |w, cx| {
            assert_eq!(w.tabs.active(), dirty, "the prompt's tab stays in front");
            assert_eq!(w.navigation_target(false, cx), Some(&Stop::Tab(third)));
            assert_eq!(w.navigation_target(true, cx), Some(&Stop::Tab(second)));
        });
    };
    tabs.update_in(cx, |w, window, cx| w.close_tab(dirty, window, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    unchanged(&tabs, cx);
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    unchanged(&tabs, cx);
    tabs.update_in(cx, |w, window, cx| {
        w.navigate(true, window, cx);
        assert_eq!(w.tabs.active(), second);
        w.navigate(false, window, cx);
        assert_eq!(w.tabs.active(), third);
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
fn navigating_closes_the_tab_and_case_header_menus(cx: &mut TestAppContext) {
    cx.update(crate::bind_all_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let first = tabs.read_with(cx, |w, _| w.tabs.active());
    let second = tabs.update_in(cx, |w, window, cx| w.new_tab(window, cx));
    draw(cx);
    let row = cx.update(|_, cx| tabs.read(cx).sidebar_row_focus.borrow()[&second].clone());
    cx.update(|window, cx| window.focus(&row, cx));
    key(cx, "shift-f10");
    assert!(tabs.read_with(cx, |w, _| w.tab_menu.is_some()));
    key(cx, back_key());
    tabs.read_with(cx, |w, _| {
        assert_eq!(w.tabs.active(), first);
        assert!(w.tab_menu.is_none());
    });
    tabs.update_in(cx, |w, window, cx| {
        w.cases.header_menu = Some(("case".into(), gpui::point(px(10.), px(10.))));
        w.navigate(false, window, cx);
        assert_eq!(w.tabs.active(), second);
        assert!(w.cases.header_menu.is_none());
    });
}
