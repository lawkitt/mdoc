//! The companion's hotkey (Milestone 2 item 15), macOS: ⌥M registered
//! while the companion can open, changed, turned off or refused as
//! Settings says, and a press toggling the companion as the menu bar item
//! does. Registration goes through a registrar that never asks the OS.
#![cfg(target_os = "macos")]

use super::ai_project::{boot_fixture, fixture, set_preview};
use super::*;
use crate::{
    companion::hotkey::{self, Recorded},
    menu_bar,
};
use mdoc_store::settings::Hotkey;

/// The menu bar item on, as at launch, and the test's registrar.
fn launch(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Recorded {
    let handle = cx.window_handle().downcast::<Workspace>().unwrap();
    let recorded = cx.cx.update(|cx| {
        let recorded = Recorded::install(cx);
        menu_bar::install(handle, true, true, cx);
        recorded
    });
    tabs.update(cx, |tabs, cx| tabs.refresh_hotkey(cx));
    recorded
}

/// Settings saved `change`, as its Applied publishes it.
fn apply(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut settings::Preferences),
) {
    tabs.update(cx, |tabs, cx| {
        change(tabs.preferences.borrow_mut().current.as_mut().unwrap());
        tabs.settings
            .update(cx, |_, cx| cx.emit(settings_ui::Event::Applied));
    });
    cx.run_until_parked();
}

fn registered(cx: &mut VisualTestContext) -> Option<Hotkey> {
    cx.update(|_, cx| hotkey::registered(cx))
}

fn failed(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| tabs.read(cx).settings.read(cx).hotkey_failed_shown())
}

fn companion_open(cx: &mut VisualTestContext) -> bool {
    cx.run_until_parked();
    cx.update(|_, cx| menu_bar::companion(cx).is_some())
}

fn press(cx: &mut VisualTestContext) {
    cx.cx.update(hotkey::pressed);
    cx.run_until_parked();
}

/// Done when: with the preview off nothing is registered at all; on, ⌥M
/// is, until the preview or the menu bar item goes.
#[gpui::test]
fn option_m_is_registered_only_while_the_preview_is_on(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let recorded = launch(&tabs, cx);
    assert_eq!(registered(cx), None);
    assert!(recorded.state().calls.is_empty(), "the OS was never asked");
    set_preview(&tabs, cx, true);
    assert_eq!(registered(cx), Some(Hotkey::COMPANION));
    assert_eq!(recorded.state().registered, Some(Hotkey::COMPANION));
    // Without the menu bar item the companion can't open: nothing holds ⌥M.
    apply(&tabs, cx, |p| p.general.menu_bar = false);
    assert_eq!(recorded.state().registered, None);
    apply(&tabs, cx, |p| p.general.menu_bar = true);
    assert_eq!(recorded.state().registered, Some(Hotkey::COMPANION));
    set_preview(&tabs, cx, false);
    assert_eq!(recorded.state().registered, None);
    assert_eq!(registered(cx), None);
}

/// Shown off before its save lands (#1220), the preview lets go of the
/// hotkey at once; a save that failed takes it again.
#[gpui::test]
fn the_preview_shown_off_lets_go_at_once(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let recorded = launch(&tabs, cx);
    set_preview(&tabs, cx, true);
    for (shown, held) in [(false, None), (true, Some(Hotkey::COMPANION))] {
        tabs.update(cx, |tabs, cx| {
            (tabs.settings).update(cx, |_, cx| {
                cx.emit(settings_ui::Event::AiConnectorShown(shown))
            })
        });
        cx.run_until_parked();
        assert_eq!(recorded.state().registered, held);
    }
}

/// Done when: a changed shortcut replaces the old one at once, off lets
/// go of it, and on again brings back the one kept.
#[gpui::test]
fn changing_it_re_registers_and_off_lets_go(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let recorded = launch(&tabs, cx);
    set_preview(&tabs, cx, true);
    let changed = Hotkey::new(true, false, true, false, "Space").unwrap();
    apply(&tabs, cx, |p| p.ai_connector.hotkey = changed);
    assert_eq!(recorded.state().registered, Some(changed));
    assert_eq!(
        recorded.state().calls,
        [
            (true, Hotkey::COMPANION),
            (false, Hotkey::COMPANION),
            (true, changed)
        ]
    );
    apply(&tabs, cx, |p| p.ai_connector.hotkey_off = true);
    assert_eq!(recorded.state().registered, None);
    // Off, a press does nothing, even with the companion's item on.
    press(cx);
    assert!(!companion_open(cx));
    apply(&tabs, cx, |p| p.ai_connector.hotkey_off = false);
    assert_eq!(recorded.state().registered, Some(changed));
}

/// Done when: a shortcut another app holds is refused, Settings says so,
/// the menu bar item still toggles the companion, and another shortcut
/// clears it.
#[gpui::test]
fn a_taken_shortcut_shows_why_and_the_item_still_works(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    let recorded = launch(&tabs, cx);
    recorded.take(Hotkey::COMPANION);
    set_preview(&tabs, cx, true);
    assert_eq!(registered(cx), None);
    let why = failed(&tabs, cx).expect("Settings shows why");
    assert!(
        why.starts_with("⌥M is in use by another app or by macOS."),
        "{why}"
    );
    cx.cx.update(menu_bar::item_clicked);
    assert!(companion_open(cx), "the menu bar icon still opens it");
    cx.cx.update(menu_bar::item_clicked);
    assert!(!companion_open(cx));
    let free = Hotkey::new(false, true, true, false, "KeyM").unwrap();
    apply(&tabs, cx, |p| p.ai_connector.hotkey = free);
    assert_eq!(registered(cx), Some(free));
    assert_eq!(failed(&tabs, cx), None);
    // The preview off clears it too.
    apply(&tabs, cx, |p| p.ai_connector.hotkey = Hotkey::COMPANION);
    assert!(failed(&tabs, cx).is_some());
    set_preview(&tabs, cx, false);
    assert_eq!(failed(&tabs, cx), None);
}

/// Done when: a press toggles the companion exactly as the menu bar
/// item's click does, and never activates a window.
#[gpui::test]
fn a_press_toggles_the_companion_as_the_item_does(cx: &mut TestAppContext) {
    let fixture = fixture();
    let (tabs, cx) = boot_fixture(cx, &fixture);
    launch(&tabs, cx);
    // Nothing registered: a stray press does nothing.
    press(cx);
    assert!(!companion_open(cx));
    set_preview(&tabs, cx, true);
    let active = cx.update(|_, cx| cx.active_window());
    press(cx);
    assert!(companion_open(cx));
    assert_eq!(
        cx.update(|_, cx| cx.active_window()),
        active,
        "no focus taken"
    );
    // The item and the hotkey toggle the one companion.
    cx.cx.update(menu_bar::item_clicked);
    assert!(!companion_open(cx));
    press(cx);
    assert!(companion_open(cx));
    press(cx);
    assert!(!companion_open(cx));
}
