//! Staying in the menu bar (macOS): Settings › General › "Show
//! mdoc in the menu bar" shows the item; its click opens the shelf in a
//! borderless popup under it, closed by a click outside. With the item on,
//! closing the workspace window hides it (tabs, undo, mappings and unsaved
//! text stay in memory) and mdoc leaves the Dock until the window shows
//! again. The first time that happens, a note under the icon says mdoc
//! is still there, once. Quit and Restart to update still go through the
//! quit flow. Off, mdoc quits with its last window as before.
//! The setting is on by default.
//!
//! With "AI connector (preview)" on, the item toggles the companion window
//! ([`crate::ai::companion`], ADR 0128) in place of the shelf popup; it also
//! opens by itself for a delivery or a review request, never taking focus.
//!
//! `MenuBarState` (`state.rs`) holds the rules, GPUI-free; this module
//! carries out its effects with GPUI and the vendored macOS calls. On
//! Windows the tray icon ([`crate::shelf::tray`]) is the item, only while
//! the preview is on; other platforms never turn it on.

mod state;

use crate::{
    ai::companion::{COMPANION_WIDTH, CompanionView},
    shelf::shelf_ui::{NOTE_HEIGHT, NOTE_WIDTH, NoteView, ShelfContent, ShelfView},
    workspace::Workspace,
};
use gpui::{
    App, AppContext as _, Bounds, DisplayId, Global, Pixels, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, point, px, size,
};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use state::{MenuBarEffect, MenuBarState};

/// The shelf's width.
pub(crate) const SHELF_WIDTH: f32 = 360.;
/// Space between the menu bar and the shelf, and around it on screen.
const MARGIN: f32 = 6.;

/// The app's menu bar state, the workspace window, the open shelf, the
/// companion and the first-close note.
struct MenuBar {
    state: MenuBarState,
    workspace: WindowHandle<Workspace>,
    shelf: Option<WindowHandle<ShelfView>>,
    companion: Option<WindowHandle<CompanionView>>,
    note: Option<WindowHandle<NoteView>>,
}

impl Global for MenuBar {}

/// Start following the setting for `workspace`, the app's one workspace;
/// `note_shown` says the first-close note has shown before.
pub(crate) fn install(
    workspace: WindowHandle<Workspace>,
    enabled: bool,
    note_shown: bool,
    cx: &mut App,
) {
    let mut state = MenuBarState::default();
    state.note_pending(!note_shown);
    cx.set_global(MenuBar {
        state,
        workspace,
        shelf: None,
        companion: None,
        note: None,
    });
    set_enabled(enabled, cx);
}

/// "AI connector (preview)" is `on` now: the item toggles the companion
/// in place of the shelf while it is (macOS, and Windows, where the tray
/// shows only then).
pub(crate) fn set_companion(on: bool, cx: &mut App) {
    let on = on && cfg!(any(target_os = "macos", windows));
    update(cx, |state| state.set_companion(on));
    crate::shelf::tray::preview_changed(on, cx);
}

/// An AI app delivered, or asked for a document that needs review: the
/// companion opens by itself when `open`, without taking focus, or reads
/// the window again while it shows.
pub(crate) fn ai_arrived(open: bool, cx: &mut App) {
    update(cx, |state| state.ai_arrived(open));
}

/// What the companion shows changed (a share, Stop sharing, a document
/// opened for review): it reads the window again while it shows.
pub(crate) fn companion_changed(cx: &mut App) {
    update(cx, |state| state.companion_changed());
}

/// The companion closed itself (its Hide).
pub(crate) fn companion_closed(cx: &mut App) {
    if let Some(bar) = cx.try_global::<MenuBar>()
        && bar.companion.is_some()
    {
        let bar = cx.global_mut::<MenuBar>();
        bar.companion = None;
        bar.state.companion_closed();
    }
}

/// The open companion, for tests.
#[cfg(all(test, any(target_os = "macos", windows)))]
pub(crate) fn companion(cx: &App) -> Option<WindowHandle<CompanionView>> {
    cx.try_global::<MenuBar>().and_then(|bar| bar.companion)
}

/// The setting is `enabled` now; Windows needs the preview on too, and
/// other platforms never turn it on.
pub(crate) fn set_enabled(enabled: bool, cx: &mut App) {
    let enabled = crate::shelf::tray::item_enabled(enabled, cx);
    update(cx, |state| state.set_enabled(enabled));
}

/// Whether closing the workspace window hides it instead of quitting.
pub(crate) fn keeps_running(cx: &App) -> bool {
    crate::shelf::tray::CLOSE_TO_TRAY
        && crate::shelf::tray::icon_usable()
        && cx
            .try_global::<MenuBar>()
            .is_some_and(|bar| bar.state.keeps_running())
}

/// Whether the workspace window is hidden.
pub(crate) fn window_hidden(cx: &App) -> bool {
    cx.try_global::<MenuBar>()
        .is_some_and(|bar| bar.state.window_hidden())
}

/// Hide the workspace `window` instead of closing it (the item is on).
pub(crate) fn close_window(window: &mut Window, cx: &mut App) {
    if cx.try_global::<MenuBar>().is_none() {
        return;
    }
    let effects = cx.global_mut::<MenuBar>().state.close_window();
    for effect in effects {
        match effect {
            MenuBarEffect::HideWindow => hide(window),
            effect => carry_out(effect, cx),
        }
    }
}

/// Bring the workspace window forward, showing it if hidden. From inside
/// the workspace, pass its `window`; elsewhere `reveal` updates it.
pub(crate) fn reveal_window(window: &mut Window, cx: &mut App) {
    let effects = match cx.try_global::<MenuBar>() {
        Some(_) => cx.global_mut::<MenuBar>().state.reveal(),
        None => vec![MenuBarEffect::ShowWindow],
    };
    for effect in effects {
        match effect {
            MenuBarEffect::ShowWindow => {
                close_note(cx);
                #[cfg(all(windows, not(test)))]
                crate::shelf::tray::show_window(window);
                cx.activate(true);
                window.activate_window();
            }
            effect => carry_out(effect, cx),
        }
    }
}

/// Show the hidden workspace `window` before it asks something; a window
/// that shows already stays where it is.
pub(crate) fn reveal_if_hidden(window: &mut Window, cx: &mut App) {
    if window_hidden(cx) {
        reveal_window(window, cx);
    }
}

/// Bring the workspace window forward from outside it (the shelf, a
/// reopen), then run `then` in it.
pub(crate) fn reveal(
    cx: &mut App,
    then: impl FnOnce(&mut Workspace, &mut Window, &mut gpui::Context<Workspace>) + 'static,
) {
    let Some(workspace) = cx.try_global::<MenuBar>().map(|bar| bar.workspace) else {
        return;
    };
    let _ = workspace.update(cx, |workspace, window, cx| {
        reveal_window(window, cx);
        then(workspace, window, cx);
    });
}

/// The workspace window became active (by any path, Settings included);
/// mdoc is back in the Dock and the first-close note goes.
pub(crate) fn window_activated(cx: &mut App) {
    close_note(cx);
    update(cx, MenuBarState::window_activated);
}

/// The item was clicked: open or close the shelf.
// Only the macOS status item clicks; elsewhere the setting stays off.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn item_clicked(cx: &mut App) {
    update(cx, |state| state.item_clicked(Instant::now()));
}

/// The companion's hotkey was pressed in any app: toggle the companion,
/// as the item's click does.
pub(crate) fn hotkey_pressed(cx: &mut App) {
    update(cx, MenuBarState::hotkey_pressed);
}

/// The shelf window closed (a click outside, Esc, an action).
pub(crate) fn shelf_closed(cx: &mut App) {
    if let Some(bar) = cx.try_global::<MenuBar>()
        && bar.shelf.is_some()
    {
        let bar = cx.global_mut::<MenuBar>();
        bar.shelf = None;
        bar.state.shelf_closed(Instant::now());
    }
}

/// Close the shelf if it is open.
pub(crate) fn close_shelf(cx: &mut App) {
    carry_out(MenuBarEffect::CloseShelf, cx);
}

/// Close the companion if it is open.
pub(crate) fn close_companion(cx: &mut App) {
    if cx.try_global::<MenuBar>().is_some() {
        carry_out(MenuBarEffect::CloseCompanion, cx);
    }
}

/// Close the first-close note if it shows.
pub(crate) fn close_note(cx: &mut App) {
    let Some(note) = cx
        .try_global::<MenuBar>()
        .is_some()
        .then(|| cx.global_mut::<MenuBar>().note.take())
        .flatten()
    else {
        return;
    };
    let _ = note.update(cx, |_, window, _| window.remove_window());
}

/// The first-close note closed itself.
pub(crate) fn note_closed(cx: &mut App) {
    if cx.try_global::<MenuBar>().is_some() {
        cx.global_mut::<MenuBar>().note = None;
    }
}

/// The first-close note's "Quit when windows close": the item goes; the
/// caller runs the quit.
pub(crate) fn quit_when_windows_close(cx: &mut App) {
    update(cx, MenuBarState::quit_when_windows_close);
}

fn update(cx: &mut App, transition: impl FnOnce(&mut MenuBarState) -> Vec<MenuBarEffect>) {
    if cx.try_global::<MenuBar>().is_none() {
        return;
    }
    let effects = transition(&mut cx.global_mut::<MenuBar>().state);
    for effect in effects {
        carry_out(effect, cx);
    }
}

fn carry_out(effect: MenuBarEffect, cx: &mut App) {
    match effect {
        MenuBarEffect::ShowItem => show_item(cx),
        MenuBarEffect::RemoveItem => {
            close_note(cx);
            // The companion can't open without the item: its hotkey goes
            // too, even if mdoc stays open after "Quit when windows close".
            crate::ai::companion::hotkey::set(None, cx);
            #[cfg(all(target_os = "macos", not(test)))]
            gpui_macos::set_status_item(None);
            #[cfg(all(windows, not(test)))]
            crate::shelf::tray::remove();
        }
        MenuBarEffect::OpenShelf => {
            close_note(cx);
            open_shelf(cx)
        }
        // The window is being hidden from inside its own update: open the
        // note once that is done.
        MenuBarEffect::ShowNote => cx.defer(open_note),
        MenuBarEffect::CloseShelf => {
            let shelf = cx.global_mut::<MenuBar>().shelf.take();
            if let Some(shelf) = shelf {
                let _ = shelf.update(cx, |_, window, _| window.remove_window());
                cx.global_mut::<MenuBar>()
                    .state
                    .shelf_closed(Instant::now());
            }
        }
        MenuBarEffect::HideWindow => {
            let workspace = cx.global::<MenuBar>().workspace;
            let _ = workspace.update(cx, |_, window, _| hide(window));
        }
        MenuBarEffect::ShowWindow => {
            close_note(cx);
            let workspace = cx.global::<MenuBar>().workspace;
            cx.activate(true);
            let _ = workspace.update(cx, |_, window, _| {
                #[cfg(all(windows, not(test)))]
                crate::shelf::tray::show_window(window);
                window.activate_window()
            });
        }
        MenuBarEffect::Dock(visible) => {
            #[cfg(all(target_os = "macos", not(test)))]
            gpui_macos::set_dock_visible(visible);
            let _ = visible;
        }
        // Asked from inside the workspace's own update (a delivery, a
        // share): it reads the workspace once that is done.
        MenuBarEffect::OpenCompanion => {
            close_note(cx);
            cx.global_mut::<MenuBar>().state.companion_opened();
            cx.defer(open_companion);
        }
        MenuBarEffect::CloseCompanion => {
            let companion = cx.global_mut::<MenuBar>().companion.take();
            cx.global_mut::<MenuBar>().state.companion_closed();
            if let Some(companion) = companion {
                let _ = companion.update(cx, |_, window, _| window.remove_window());
            }
        }
        MenuBarEffect::RefreshCompanion => cx.defer(|cx| {
            let companion = cx.try_global::<MenuBar>().and_then(|bar| bar.companion);
            if let Some(companion) = companion {
                let _ = companion.update(cx, |view, window, cx| view.refresh(window, cx));
            }
        }),
    }
}

fn hide(window: &mut Window) {
    #[cfg(all(target_os = "macos", not(test)))]
    gpui_macos::hide_window(window);
    #[cfg(all(windows, not(test)))]
    crate::shelf::tray::hide_window(window);
    let _ = window;
}

fn show_item(cx: &mut App) {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        let async_cx = cx.to_async();
        gpui_macos::set_status_item(Some(gpui_macos::StatusItem {
            image: gpui_macos::StatusImage {
                points: 18.,
                pngs: &[
                    include_bytes!("../../resources/icons/menubar.png"),
                    include_bytes!("../../resources/icons/menubar@2x.png"),
                ],
            },
            label: "mdoc",
            on_click: Box::new(move || on_item_click(&async_cx)),
        }));
    }
    #[cfg(all(windows, not(test)))]
    crate::shelf::tray::show(cx);
    let _ = cx;
}

/// A click arrives inside an AppKit event, possibly while GPUI is busy:
/// queue it on the foreground executor.
#[cfg(all(target_os = "macos", not(test)))]
fn on_item_click(cx: &gpui::AsyncApp) {
    let cx = cx.clone();
    cx.foreground_executor()
        .clone()
        .spawn(async move {
            cx.update(item_clicked);
        })
        .detach();
}

/// A popup's top under the item.
fn popup_top(anchor: Bounds<Pixels>) -> f32 {
    (anchor.origin.y + anchor.size.height).as_f32() + MARGIN / 2.
}

/// The most a popup under the item may grow: down to the display's edge;
/// a taller shelf scrolls.
fn popup_max_height(anchor: Bounds<Pixels>, display: Bounds<Pixels>) -> f32 {
    if cfg!(all(windows, not(test))) {
        return crate::shelf::tray::popup_max_height(display);
    }
    (display.size.height.as_f32() - popup_top(anchor) - MARGIN).max(120.)
}

/// Where popups may sit on `display`: under the macOS menu bar, all of
/// it; by the Windows tray, its work area (without the taskbar).
fn popup_area(display: &dyn gpui::PlatformDisplay) -> Bounds<Pixels> {
    if cfg!(windows) {
        display.visible_bounds()
    } else {
        display.bounds()
    }
}

/// A popup's bounds under the item, kept on its display.
fn popup_bounds(
    anchor: Bounds<Pixels>,
    display: Bounds<Pixels>,
    width: f32,
    height: f32,
) -> Bounds<Pixels> {
    if cfg!(all(windows, not(test))) {
        return crate::shelf::tray::popup_bounds(anchor, display, width, height);
    }
    let max_x = (display.size.width.as_f32() - width - MARGIN).max(MARGIN);
    let x = anchor.origin.x.as_f32().clamp(MARGIN, max_x);
    Bounds {
        origin: point(px(x), px(popup_top(anchor))),
        size: size(px(width), px(height.min(popup_max_height(anchor, display)))),
    }
}

/// The status item's display and bounds.
/// Set in tests: the menu bar item has no place on screen.
#[cfg(test)]
pub(crate) struct NoItemForTests;

#[cfg(test)]
impl Global for NoItemForTests {}

fn item_anchor(cx: &App) -> Option<(DisplayId, Bounds<Pixels>)> {
    #[cfg(all(target_os = "macos", not(test)))]
    let anchor = {
        let _ = cx;
        gpui_macos::status_item_bounds()
    };
    // Tests open popups under a pretend item on the primary display, unless
    // they took the item away.
    #[cfg(test)]
    let anchor = (!cx.has_global::<NoItemForTests>())
        .then(|| cx.primary_display())
        .flatten()
        .map(|display| {
            let item = Bounds {
                origin: point(px(1000.), px(0.)),
                size: size(px(30.), px(24.)),
            };
            (display.id(), item)
        });
    #[cfg(all(windows, not(test)))]
    let anchor = crate::shelf::tray::anchor(cx);
    #[cfg(all(not(target_os = "macos"), not(windows), not(test)))]
    let anchor = {
        let _ = cx;
        None
    };
    anchor
}

/// A borderless popup under the item with the native blurred background
/// (vibrancy); the system draws its corners and shadow.
fn popup_options(bounds: Bounds<Pixels>, display_id: DisplayId) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: Some(display_id),
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}

/// The first-close note under the item. Once it shows it is recorded, so
/// it never shows again; if it cannot open, the next close tries again.
fn open_note(cx: &mut App) {
    let Some(workspace) = cx.try_global::<MenuBar>().map(|bar| bar.workspace) else {
        return;
    };
    let opened = try_open_note(workspace, cx);
    if opened {
        let _ = workspace.update(cx, |workspace, _, cx| {
            workspace.update_general(|general| general.menu_bar_note_shown = true, cx)
        });
    } else {
        cx.global_mut::<MenuBar>().state.note_pending(true);
    }
}

/// Open the note window; whether it opened.
fn try_open_note(workspace: WindowHandle<Workspace>, cx: &mut App) -> bool {
    let Some((display_id, anchor)) = item_anchor(cx) else {
        return false;
    };
    let Some(display) = cx.find_display(display_id) else {
        return false;
    };
    let Ok(theme) = workspace.read(cx).map(|workspace| workspace.theme_cell()) else {
        return false;
    };
    close_note(cx);
    let bounds = popup_bounds(anchor, popup_area(&*display), NOTE_WIDTH, NOTE_HEIGHT);
    let note = cx.open_window(popup_options(bounds, display_id), |window, cx| {
        cx.new(|cx| NoteView::new(workspace, theme, window, cx))
    });
    match note {
        Ok(note) => {
            cx.global_mut::<MenuBar>().note = Some(note);
            true
        }
        Err(_) => false,
    }
}

/// The companion under the item, where the shelf would sit, never taking
/// focus ([`crate::ai::companion::open`]). If it cannot open, the next click
/// tries again.
fn open_companion(cx: &mut App) {
    if cx.try_global::<MenuBar>().is_none() {
        return;
    }
    let opened = try_open_companion(cx);
    let wanted = cx.global::<MenuBar>().state.companion_open;
    match opened {
        // Closed again before it opened (a second click, the preview off).
        Some(companion) if !wanted => {
            cx.global_mut::<MenuBar>().companion = None;
            let _ = companion.update(cx, |_, window, _| window.remove_window());
        }
        Some(companion) => cx.global_mut::<MenuBar>().companion = Some(companion),
        None => cx.global_mut::<MenuBar>().state.companion_closed(),
    }
}

fn try_open_companion(cx: &mut App) -> Option<WindowHandle<CompanionView>> {
    // Already open: a click and a delivery in one turn.
    if let Some(open) = cx.global::<MenuBar>().companion {
        return Some(open);
    }
    let (display_id, anchor) = item_anchor(cx)?;
    let display = cx.find_display(display_id)?;
    let workspace = cx.global::<MenuBar>().workspace;
    let content = workspace
        .read(cx)
        .ok()
        .map(|workspace| workspace.companion_content(cx))?;
    let height = crate::ai::companion::height(&content);
    let bounds = popup_bounds(anchor, popup_area(&*display), COMPANION_WIDTH, height);
    let max_height = popup_max_height(anchor, popup_area(&*display));
    crate::ai::companion::open(bounds, display_id, cx, move |window, cx| {
        cx.new(|cx| {
            let mut companion = CompanionView::new(workspace, content, window, cx);
            companion.fit_to_screen(bounds.size.height.as_f32(), max_height);
            companion
        })
    })
    .ok()
}

fn open_shelf(cx: &mut App) {
    let Some((display_id, anchor)) = item_anchor(cx) else {
        return;
    };
    let Some(display) = cx.find_display(display_id) else {
        return;
    };
    let workspace = cx.global::<MenuBar>().workspace;
    // Member files are read now, while the shelf opens, and only kept in it.
    let Ok(content) = workspace
        .read(cx)
        .map(|workspace| ShelfContent::of(workspace, cx))
    else {
        return;
    };
    let bounds = popup_bounds(anchor, popup_area(&*display), SHELF_WIDTH, content.height());
    let max_height = popup_max_height(anchor, popup_area(&*display));
    let shelf = cx.open_window(popup_options(bounds, display_id), |window, cx| {
        cx.new(|cx| {
            let mut shelf = ShelfView::new(workspace, content, window, cx);
            // It follows its content from here, the picker and hint included.
            shelf.fit_to_screen(bounds.size.height.as_f32(), max_height);
            shelf
        })
    });
    if let Ok(shelf) = shelf {
        let bar = cx.global_mut::<MenuBar>();
        bar.shelf = Some(shelf);
        bar.state.shelf_opened();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MenuBarEffect::*;

    #[test]
    fn the_first_close_shows_the_note_once_and_never_again() {
        let mut state = MenuBarState::default();
        state.note_pending(true);
        state.set_enabled(true);
        assert_eq!(state.close_window(), [HideWindow, Dock(false), ShowNote]);
        state.reveal();
        assert_eq!(state.close_window(), [HideWindow, Dock(false)]);
        // Shown in an earlier run: never.
        let mut seen = MenuBarState::default();
        seen.set_enabled(true);
        assert_eq!(seen.close_window(), [HideWindow, Dock(false)]);
    }

    #[test]
    fn quit_when_windows_close_removes_the_item_and_keeps_the_window_hidden() {
        let mut state = MenuBarState::default();
        state.set_enabled(true);
        state.close_window();
        assert_eq!(state.quit_when_windows_close(), [RemoveItem]);
        assert!(!state.keeps_running());
        assert!(state.window_hidden(), "the quit decides");
        assert_eq!(state.quit_when_windows_close(), []);
        // A quit that asks shows the window and mdoc returns to the Dock.
        assert_eq!(state.reveal(), [ShowWindow, Dock(true)]);
    }

    #[test]
    fn off_closing_quits_as_before() {
        let mut state = MenuBarState::default();
        assert!(!state.keeps_running());
        assert_eq!(state.close_window(), []);
        assert_eq!(state.item_clicked(Instant::now()), []);
    }

    #[test]
    fn with_the_item_on_closing_hides_and_leaves_the_dock_until_shown() {
        let mut state = MenuBarState::default();
        assert_eq!(state.set_enabled(true), [ShowItem]);
        assert_eq!(state.set_enabled(true), [], "no change, nothing to do");
        assert!(state.keeps_running());
        assert_eq!(state.close_window(), [HideWindow, Dock(false)]);
        assert!(state.window_hidden());
        assert_eq!(state.close_window(), [], "already hidden");
        assert_eq!(state.reveal(), [ShowWindow, Dock(true)]);
        assert!(!state.window_hidden());
        assert_eq!(state.reveal(), [ShowWindow], "already in the Dock");
        // Settings or a menu command can show it too.
        state.close_window();
        assert_eq!(state.window_activated(), [Dock(true)]);
        assert_eq!(state.window_activated(), []);
    }

    #[test]
    fn turning_it_off_closes_the_shelf_and_brings_a_hidden_window_back() {
        let mut state = MenuBarState::default();
        state.set_enabled(true);
        state.close_window();
        state.shelf_opened();
        assert_eq!(
            state.set_enabled(false),
            [CloseShelf, RemoveItem, ShowWindow, Dock(true)]
        );
        assert!(!state.keeps_running());
    }

    #[test]
    fn the_item_toggles_the_shelf_and_the_closing_click_does_not_reopen_it() {
        let mut state = MenuBarState::default();
        state.set_enabled(true);
        let now = Instant::now();
        assert_eq!(state.item_clicked(now), [OpenShelf]);
        state.shelf_opened();
        assert_eq!(state.item_clicked(now), [CloseShelf]);
        // A click on the item first closes the shelf by deactivating it.
        state.shelf_closed(now);
        assert_eq!(state.item_clicked(now + Duration::from_millis(50)), []);
        assert_eq!(
            state.item_clicked(now + Duration::from_secs(1)),
            [OpenShelf]
        );
    }

    #[test]
    fn with_the_preview_on_the_item_toggles_the_companion_in_place_of_the_shelf() {
        let mut state = MenuBarState::default();
        state.set_enabled(true);
        let now = Instant::now();
        assert_eq!(state.item_clicked(now), [OpenShelf]);
        state.shelf_opened();
        // Turned on with the shelf open: the shelf goes.
        assert_eq!(state.set_companion(true), [CloseShelf]);
        state.shelf_closed(now);
        assert_eq!(state.set_companion(true), [], "no change");
        // No guard after a close: the companion never closes by itself.
        assert_eq!(state.item_clicked(now), [OpenCompanion]);
        state.companion_opened();
        assert_eq!(state.item_clicked(now), [CloseCompanion]);
        state.companion_closed();
        assert_eq!(state.item_clicked(now), [OpenCompanion]);
        state.companion_opened();
        // Turned off with it open: it goes, and the item opens the shelf.
        assert_eq!(state.set_companion(false), [CloseCompanion]);
        state.companion_closed();
        assert_eq!(
            state.item_clicked(now + Duration::from_secs(1)),
            [OpenShelf]
        );
    }

    #[test]
    fn the_hotkey_toggles_the_companion_as_the_item_does_and_never_the_shelf() {
        let mut state = MenuBarState::default();
        assert_eq!(state.hotkey_pressed(), [], "the item off");
        state.set_enabled(true);
        assert_eq!(state.hotkey_pressed(), [], "the preview off: no shelf");
        state.set_companion(true);
        assert_eq!(state.hotkey_pressed(), [OpenCompanion]);
        state.companion_opened();
        assert_eq!(state.hotkey_pressed(), [CloseCompanion]);
        state.companion_closed();
        assert_eq!(state.hotkey_pressed(), [OpenCompanion]);
    }

    #[test]
    fn the_companion_opens_by_itself_only_for_what_arrived() {
        let mut state = MenuBarState::default();
        // The preview off, or the item off: nothing.
        state.set_enabled(true);
        assert_eq!(state.ai_arrived(true), []);
        state.set_companion(true);
        state.set_enabled(false);
        assert_eq!(state.ai_arrived(true), []);
        state.set_enabled(true);
        // A review request repeated within its quiet time does not.
        assert_eq!(state.ai_arrived(false), []);
        assert_eq!(state.companion_changed(), []);
        assert_eq!(state.ai_arrived(true), [OpenCompanion]);
        state.companion_opened();
        assert_eq!(state.ai_arrived(true), [RefreshCompanion]);
        assert_eq!(state.ai_arrived(false), [RefreshCompanion]);
        assert_eq!(state.companion_changed(), [RefreshCompanion]);
        // The item going closes it with the rest.
        assert_eq!(state.set_enabled(false), [CloseCompanion, RemoveItem]);
    }

    #[test]
    fn the_shelf_sits_under_the_item_and_stays_on_screen() {
        let display = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1440.), px(900.)),
        };
        let item = |x: f32| Bounds {
            origin: point(px(x), px(0.)),
            size: size(px(30.), px(24.)),
        };
        let shelf = popup_bounds(item(1000.), display, SHELF_WIDTH, 400.);
        assert_eq!(shelf.origin, point(px(1000.), px(27.)));
        assert_eq!(shelf.size, size(px(SHELF_WIDTH), px(400.)));
        let right = popup_bounds(item(1420.), display, SHELF_WIDTH, 400.);
        assert_eq!(right.origin.x, px(1440. - SHELF_WIDTH - MARGIN));
        let tall = popup_bounds(item(10.), display, SHELF_WIDTH, 5000.);
        assert_eq!(
            tall.size.height,
            px(900. - 27. - MARGIN),
            "never past the screen"
        );
    }
}
