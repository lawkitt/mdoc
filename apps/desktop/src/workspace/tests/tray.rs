//! The tray on Windows (Milestone 2 item 17): only while "AI connector
//! (preview)" is on, with "Keep mdoc in the system tray" on, does the icon
//! show; its click toggles the companion (never the shelf), and closing
//! the window hides it there. With the preview off, Windows is as before.
#![cfg(windows)]

use super::ai_project::{Fixture, boot_fixture, deliver, fixture, set_preview, shared_lease};
use super::*;
use crate::{ai::companion::CompanionView, shelf::menu_bar, shelf::shelf_ui::ShelfView};
use mdoc_mcp::wire::Kind;

/// The tray installed with the switch `on`, and the preview as it is.
fn install(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, on: bool) {
    let handle = cx.window_handle().downcast::<Workspace>().unwrap();
    let preview = cx.update(|_, cx| tabs.read(cx).ai_preview_setting());
    cx.cx.update(|cx| {
        menu_bar::install(handle, on, true, cx);
        menu_bar::set_companion(preview, cx);
    });
    cx.run_until_parked();
}

fn companion(cx: &mut VisualTestContext) -> Option<gpui::WindowHandle<CompanionView>> {
    cx.run_until_parked();
    cx.update(|_, cx| menu_bar::companion(cx))
}

fn shelf_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| (cx.windows().iter()).any(|window| window.downcast::<ShelfView>().is_some()))
}

fn click_icon(cx: &mut VisualTestContext) {
    cx.cx.update(menu_bar::item_clicked);
    cx.run_until_parked();
}

fn keeps_running(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| menu_bar::keeps_running(cx))
}

fn hidden(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| menu_bar::window_hidden(cx))
}

fn boot_off(cx: &mut TestAppContext) -> (Fixture, Entity<Workspace>, &mut VisualTestContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    (fixture, tabs, cx)
}

/// Done when: with the preview off there is no tray: closing quits and a
/// click opens nothing; on, the icon toggles the companion, never the
/// shelf, and closing hides the window; off again, the window comes back
/// and closing quits once more.
#[gpui::test]
fn the_tray_comes_and_goes_with_the_preview(cx: &mut TestAppContext) {
    let (_fixture, tabs, cx) = boot_off(cx);
    install(&tabs, cx, true);
    assert!(!keeps_running(cx));
    click_icon(cx);
    assert!(companion(cx).is_none());
    assert!(!shelf_open(cx));
    set_preview(&tabs, cx, true);
    assert!(keeps_running(cx));
    click_icon(cx);
    assert!(companion(cx).is_some());
    assert!(!shelf_open(cx), "no shelf on Windows");
    click_icon(cx);
    assert!(companion(cx).is_none());
    click_icon(cx);
    assert!(companion(cx).is_some());
    tabs.update_in(cx, |tabs, window, cx| tabs.close_requested(window, cx));
    cx.run_until_parked();
    assert!(hidden(cx), "closing hides the window in the tray");
    assert!(!cx.has_pending_prompt());
    // The preview off: the companion goes and the window shows again.
    set_preview(&tabs, cx, false);
    assert!(companion(cx).is_none());
    assert!(!hidden(cx));
    assert!(!keeps_running(cx));
}

/// Done when: with "Keep mdoc in the system tray" off there is no tray,
/// even with the preview on; on again, there is.
#[gpui::test]
fn the_switch_off_means_no_tray(cx: &mut TestAppContext) {
    let (_fixture, tabs, cx) = boot_off(cx);
    set_preview(&tabs, cx, true);
    install(&tabs, cx, false);
    assert!(!keeps_running(cx));
    click_icon(cx);
    assert!(companion(cx).is_none());
    cx.cx.update(|cx| menu_bar::set_enabled(true, cx));
    assert!(keeps_running(cx));
}

/// Done when: a delivery opens the companion by itself on Windows too,
/// without taking focus.
#[gpui::test]
fn a_delivery_opens_it_without_focus(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx, _notes) = shared_lease(cx, &fixture);
    install(&tabs, cx, true);
    let before = cx.update(|_, cx| cx.active_window());
    deliver(&tabs, cx, &fixture, Kind::Reply);
    let open = companion(cx).expect("a delivery opens it");
    assert_eq!(cx.update(|_, cx| cx.active_window()), before);
    assert!(
        !open
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap()
    );
}

/// Done when: the first-close note and the setting say "system tray".
#[test]
fn the_words_are_windows_words() {
    assert_eq!(crate::shelf::tray::SETTING, "Keep mdoc in the system tray");
    assert_eq!(
        crate::shelf::tray::NOTE_TITLE,
        "mdoc is still in the system tray."
    );
}
