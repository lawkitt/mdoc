//! Staying in the menu bar (ADR 0084): with the item on, closing the window
//! hides it with every tab kept; Quit still asks; off, closing quits.
use super::*;
use crate::menu_bar;

fn install(tabs: &Entity<Workspace>, enabled: bool, cx: &mut VisualTestContext) {
    let handle = cx.window_handle().downcast::<Workspace>().unwrap();
    let _ = tabs;
    cx.cx.update(|cx| menu_bar::install(handle, enabled, cx));
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
