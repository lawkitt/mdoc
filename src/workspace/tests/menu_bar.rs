//! Staying in the menu bar (ADR 0084): with the item on, closing the window
//! hides it with every tab kept; Quit still asks; off, closing quits.
use super::*;
use crate::menu_bar;

fn install(tabs: &Entity<Workspace>, enabled: bool, cx: &mut VisualTestContext) {
    install_noting(tabs, enabled, true, cx);
}

fn install_noting(
    tabs: &Entity<Workspace>,
    enabled: bool,
    note_shown: bool,
    cx: &mut VisualTestContext,
) {
    let handle = cx.window_handle().downcast::<Workspace>().unwrap();
    let _ = tabs;
    cx.cx
        .update(|cx| menu_bar::install(handle, enabled, note_shown, cx));
}

#[cfg(target_os = "macos")]
fn note(cx: &mut VisualTestContext) -> Option<gpui::WindowHandle<crate::shelf_ui::NoteView>> {
    cx.update(|_, cx| {
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<crate::shelf_ui::NoteView>())
    })
}

#[cfg(target_os = "macos")]
fn shelf_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        cx.windows()
            .iter()
            .any(|window| window.downcast::<crate::shelf_ui::ShelfView>().is_some())
    })
}

/// A workspace with one unsaved tab.
fn unsaved(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("unsaved text");
    cx.run_until_parked();
    (tabs, cx)
}

#[gpui::test]
fn with_the_item_off_closing_the_window_quits_as_before(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install(&tabs, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    assert!(cx.has_pending_prompt());
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
}

/// Windows has no tray yet (ADR 0084): the setting changes nothing there.
#[cfg(not(target_os = "macos"))]
#[gpui::test]
fn the_setting_does_nothing_outside_macos(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install(&tabs, true, cx);
    assert!(!cx.update(|_, cx| menu_bar::keeps_running(cx)));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn with_the_item_on_closing_hides_the_window_and_keeps_every_tab(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install(&tabs, true, cx);
    assert!(cx.update(|_, cx| menu_bar::keeps_running(cx)));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    // No question and no quit: the tab and its text stay in memory.
    assert!(!cx.has_pending_prompt());
    assert!(cx.update(|_, cx| menu_bar::window_hidden(cx)));
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(!tabs.quit_flow.quitting());
        let view = tabs.active_view().unwrap();
        assert_eq!(view.read(cx).editor.read(cx).text(), "unsaved text");
    });

    // The item opens the shelf; a second click closes it.
    cx.cx.update(menu_bar::item_clicked);
    cx.run_until_parked();
    assert!(shelf_open(cx));
    cx.cx.update(menu_bar::item_clicked);
    cx.run_until_parked();
    assert!(!shelf_open(cx));

    // Activating the window by any path brings mdoc back.
    cx.cx.update(|cx| menu_bar::reveal(cx, |_, _, _| {}));
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn quitting_while_hidden_shows_the_window_before_it_asks(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install(&tabs, true, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| menu_bar::window_hidden(cx)));
    // The shelf's Quit and ⌘Q dispatch Quit.
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn turning_the_setting_off_shows_a_hidden_window_and_closing_quits_again(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install(&tabs, true, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    cx.cx.update(|cx| menu_bar::set_enabled(false, cx));
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
}

/// Review of #264: a case save that fails while quitting asks in a window
/// that shows, even when no tab was at risk.
#[cfg(target_os = "macos")]
#[gpui::test]
fn a_failed_case_save_while_quitting_shows_the_hidden_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, case) = case_quit_fixture(cx, dir.path());
    install(&tabs, true, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| menu_bar::window_hidden(cx)));
    // A case change whose save fails; the quit's final save asks.
    tabs.update(cx, |tabs, cx| {
        tabs.toggle_case_category(&case, mdoc_pii::Category::Person, cx)
    });
    cx.run_until_parked();
    cx.dispatch_action(Quit);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "the save failure asks");
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
}

/// Review of #264: a file opened from Finder shows a hidden window.
#[cfg(target_os = "macos")]
#[gpui::test]
fn opening_a_file_from_the_os_shows_the_hidden_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "text").unwrap();
    let (tabs, cx) = unsaved(cx);
    install(&tabs, true, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths_from_os(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    assert!(
        cx.update(|_, cx| tabs
            .read(cx)
            .find_path(&session_store::identity(&path), None, cx))
            .is_some()
    );
}

/// The first-close note (ADR 0084): the first close with the item on shows
/// it under the icon and records it, so it never shows again; Got it
/// closes it and leaves mdoc in the menu bar.
#[cfg(target_os = "macos")]
#[gpui::test]
fn the_first_close_shows_the_note_once(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    assert!(
        cx.update(|_, cx| tabs.read(cx).menu_bar_setting()),
        "on by default"
    );
    assert!(!cx.update(|_, cx| tabs.read(cx).menu_bar_note_shown()));
    install_noting(&tabs, true, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    let shown = note(cx).expect("the note shows under the icon");
    assert!(cx.update(|_, cx| tabs.read(cx).menu_bar_note_shown()));
    shown
        .update(cx, |view, window, cx| view.got_it(window, cx))
        .unwrap();
    cx.run_until_parked();
    assert!(note(cx).is_none());
    assert!(cx.update(|_, cx| menu_bar::keeps_running(cx)));
    assert!(cx.update(|_, cx| menu_bar::window_hidden(cx)));
    // Back and closed again: no note.
    cx.cx.update(|cx| menu_bar::reveal(cx, |_, _, _| {}));
    cx.run_until_parked();
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(note(cx).is_none());
}

/// "Quit when windows close" turns the switch off and runs the ordinary
/// quit: unsaved work shows the window and asks.
#[cfg(target_os = "macos")]
#[gpui::test]
fn quit_when_windows_close_turns_the_switch_off_and_quits(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install_noting(&tabs, true, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    let shown = note(cx).unwrap();
    shown
        .update(cx, |view, window, cx| {
            view.quit_when_windows_close(window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(note(cx).is_none());
    assert!(!cx.update(|_, cx| tabs.read(cx).menu_bar_setting()));
    assert!(!cx.update(|_, cx| menu_bar::keeps_running(cx)));
    assert!(cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    assert!(cx.has_pending_prompt(), "the unsaved tab asks");
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
}

/// Review of #280: Quit when windows close with nothing unsaved, whose
/// final session save fails: the question shows in a window that shows,
/// though the item is gone.
#[cfg(target_os = "macos")]
#[gpui::test]
fn a_failed_save_after_quit_when_windows_close_shows_the_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, "preserve").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    tabs.update(cx, |tabs, _| {
        tabs.session_writer
            .set_path(Some(blocked.join("session.json")))
    });
    install_noting(&tabs, true, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| menu_bar::window_hidden(cx)));
    let shown = note(cx).unwrap();
    shown
        .update(cx, |view, window, cx| {
            view.quit_when_windows_close(window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "the failed save asks");
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
}

/// Review of #280: Quit when windows close whose setting cannot be saved
/// does not quit (the next launch would bring the item back with no note):
/// the window shows and says so.
#[cfg(target_os = "macos")]
#[gpui::test]
fn quit_when_windows_close_that_cannot_save_shows_the_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, "preserve").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    install_noting(&tabs, true, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    let shown = note(cx).unwrap();
    // The settings file cannot be written from here on.
    tabs.update(cx, |tabs, _| {
        tabs.preferences.borrow_mut().path = Some(blocked.join("settings.json"))
    });
    shown
        .update(cx, |view, window, cx| {
            view.quit_when_windows_close(window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| tabs.read(cx).quit_flow.quitting()));
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
    assert!(!cx.update(|_, cx| menu_bar::keeps_running(cx)));
    assert!(cx.update(|_, cx| {
        tabs.read(cx)
            .notices
            .message()
            .is_some_and(|text| text.contains("Could not save"))
    }),);
}

/// Review of #280: the window returning by any path, Settings included,
/// takes the first-close note away.
#[cfg(target_os = "macos")]
#[gpui::test]
fn the_note_goes_when_the_window_returns_by_any_path(cx: &mut TestAppContext) {
    let (tabs, cx) = unsaved(cx);
    install_noting(&tabs, true, false, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(note(cx).is_some());
    // As when ⌘, activates the window directly.
    cx.cx.update(menu_bar::window_activated);
    cx.run_until_parked();
    assert!(note(cx).is_none());
    assert!(!cx.update(|_, cx| menu_bar::window_hidden(cx)));
}
