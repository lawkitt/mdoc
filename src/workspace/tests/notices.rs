//! The header bell: notices, the crash report and update notes (`Notices`).
use super::*;

#[gpui::test]
fn available_update_marks_the_settings_button(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    draw(cx);
    assert!(cx.debug_bounds("Settings").is_some());
    assert!(cx.debug_bounds("update-dot").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            available: Some(crate::updater::test_update("9.0.0")),
            ..Default::default()
        })
    });
    draw(cx);
    let dot = cx
        .debug_bounds("update-dot")
        .expect("an update shows the dot");
    let button = cx.debug_bounds("Settings").unwrap();
    assert!(
        button.contains(&dot.center()),
        "dot={dot:?}, button={button:?}"
    );
}

#[gpui::test]
fn installed_notice_leaves_no_dot_on_the_collapsed_rail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.md");
    std::fs::write(&a, "a").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![a.clone()], window, cx)
    });
    cx.run_until_parked();
    collapse(&tabs, cx);
    // Just updated, nothing newer: the "installed" notice isn't an update.
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            updated_to: Some("9.0.0".into()),
            ..Default::default()
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("rail-settings").is_some());
    assert!(cx.debug_bounds("rail-update-dot").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            available: Some(crate::updater::test_update("9.1.0")),
            ..Default::default()
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("rail-update-dot").is_some());
}

#[gpui::test]
fn update_banner_opens_settings_and_dismisses_per_version(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    // A manual check's result raises no banner; an automatic one does.
    let found = crate::updater::UpdateState {
        available: Some(crate::updater::test_update("9.0.0")),
        ..Default::default()
    };
    cx.update(|_, cx| cx.set_global(found.clone()));
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    cx.update(|_, cx| {
        cx.set_global(crate::updater::UpdateState {
            automatic: true,
            ..found
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("update-dot").is_none(),
        "the banner carries the only dot"
    );
    click_toolbar(cx, "update-banner");
    cx.update(|_, cx| {
        let settings = tabs.read(cx).settings.read(cx);
        assert!(settings.open);
        assert_eq!(settings.category(), settings_ui::Category::General);
    });
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx))
    });
    click_toolbar(cx, "dismiss-update-banner");
    draw(cx);
    assert!(cx.debug_bounds("update-banner").is_none());
    assert!(
        cx.debug_bounds("update-dot").is_some(),
        "Settings marks the update once the banner is gone"
    );
    cx.update(|_, cx| {
        assert!(
            !tabs.read(cx).settings.read(cx).open,
            "× doesn't open Settings"
        );
        assert_eq!(
            crate::updater::state(cx).dismissed.as_deref(),
            Some("9.0.0")
        );
    });
}

#[gpui::test]
fn escape_closes_notices_without_focus_in_an_editor(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| shell::bind_keys(cx));
    let view = active(&tabs, cx);
    // A source-only tab focuses its view, Home the window: neither is an
    // editor, and neither is inside Notices.
    let focus = cx.update(|_, cx| [view.read(cx).focus.clone(), tabs.read(cx).focus.clone()]);
    for handle in focus {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.notices.post("Unsupported files skipped: sample.xyz");
            tabs.title.notices_open.set(true);
            cx.notify();
            window.focus(&handle, cx);
        });
        draw(cx);
        key(cx, "escape");
        cx.update(|_, cx| assert!(!tabs.read(cx).title.notices_open.get()));
    }
}

#[gpui::test]
fn a_crash_report_is_previewed_before_it_is_copied(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, cx| {
        tabs.notices.load_crash(Some(
            "mdoc 0.1 — macos aarch64\nPanic at <path>/a.rs:1:1".into(),
        ));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("crash-bar").is_some());
    assert!(
        cx.debug_bounds("crash-report").is_none(),
        "nothing shown until asked"
    );
    assert!(cx.debug_bounds("crash-copy").is_none());
    click_toolbar(cx, "crash-preview");
    draw(cx);
    assert!(cx.debug_bounds("crash-report").is_some());
    click_toolbar(cx, "crash-copy");
    draw(cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        copied.as_deref(),
        Some("mdoc 0.1 — macos aarch64\nPanic at <path>/a.rs:1:1")
    );
    assert!(cx.debug_bounds("crash-bar").is_none());
    tabs.read_with(cx, |tabs, _| assert!(tabs.notices.crash().is_none()));
}

#[gpui::test]
fn notices_wait_behind_the_header_bell(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_none());
    // A new notice opens the popover once (ADR 0048).
    tabs.update(cx, |tabs, cx| {
        tabs.notices.post("Could not restore one tab.");
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_some());
    assert!(cx.debug_bounds("notices-popover").is_some());
    // A click elsewhere closes it; the bell stays and reopens it.
    cx.simulate_click(gpui::point(px(500.), px(500.)), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    assert!(cx.debug_bounds("header-notices").is_some());
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // The bell closes its open popover rather than reopening it.
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // The close button closes it too; the notice keeps the bell.
    click_toolbar(cx, "close-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    assert!(cx.debug_bounds("header-notices").is_some());
    cx.update(|_, cx| assert_eq!(tabs.read(cx).title.notices.get(), 1));
    click_toolbar(cx, "header-notices");
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    // Dismissing the last notice removes the bell.
    click_toolbar(cx, "dismiss-session-notice");
    draw(cx);
    assert!(cx.debug_bounds("header-notices").is_none());
    assert!(cx.debug_bounds("notices-popover").is_none());
}

#[gpui::test]
fn locked_cases_notice_opens_only_when_a_locked_case_is_needed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.md");
    std::fs::write(&plain, "Nothing to restore here.").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.open_paths(vec![plain], window, cx);
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    // A plain document: the notice waits behind the bell (ADR 0043).
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.case_coordinator.locked());
        assert_eq!(tabs.title.notices.get(), 1);
        assert!(!tabs.title.notices_open.get());
    });
    assert!(cx.debug_bounds("header-notices").is_some());
    assert!(cx.debug_bounds("notices-popover").is_none());
    // Pasting an AI reply into a tab without a mapping needs the locked
    // cases: the notice opens.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    cx.dispatch_action(crate::workspace::PasteAiResponse);
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_some());
    assert!(cx.debug_bounds("unlock-cases").is_some());
}

#[gpui::test]
fn restore_from_home_shows_unlock_over_an_earlier_notice(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update(cx, |tabs, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        // An earlier notice the user closed without dismissing.
        tabs.notices.post("The clipboard has no text.");
        cx.notify();
    });
    draw(cx);
    tabs.update(cx, |tabs, cx| {
        tabs.title.notices_open.set(false);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("notices-popover").is_none());
    tabs.update_in(cx, |_, window, cx| {
        window.dispatch_action(Box::new(crate::workspace::PasteAiResponse), cx)
    });
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).case_coordinator.locked()));
    assert!(cx.debug_bounds("notices-popover").is_some());
    assert!(cx.debug_bounds("unlock-cases").is_some());
}

#[gpui::test]
fn restore_from_a_blank_tab_looks_for_locked_cases(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.stage_case_key(Err(mdoc_store::cases::CaseError::KeyDenied));
        tabs.new_tab(window, cx);
    });
    cx.simulate_input("PERSON_1 wrote back.");
    draw(cx);
    cx.update(|_, cx| {
        assert!(
            !tabs.read(cx).case_coordinator.locked(),
            "nothing looked yet"
        )
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1 replied.".into()));
    cx.dispatch_action(crate::workspace::PasteAiResponse);
    draw(cx);
    cx.update(|_, cx| assert!(tabs.read(cx).case_coordinator.locked()));
    assert!(cx.debug_bounds("unlock-cases").is_some());
}
