//! Actual destination, keyboard and pointer dispatch (ADR 0081).
use super::*;
use crate::workspace::navigation::Stop;

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
