//! Staying in the menu bar (ADR 0084, macOS): Settings › General › "Show
//! mdoc in the menu bar" shows the item; its click opens the shelf in a
//! borderless popup under it, closed by a click outside. With the item on,
//! closing the workspace window hides it (tabs, undo, mappings and unsaved
//! text stay in memory) and mdoc leaves the Dock until the window shows
//! again. Quit and Restart to update still go through the quit flow
//! (ADR 0071). Off, mdoc quits with its last window as before.
//!
//! `MenuBarState` holds the rules, GPUI-free; this module carries out its
//! effects with GPUI and the vendored macOS calls. Other platforms never
//! turn it on (no tray yet).

use crate::{shelf_ui::ShelfView, workspace::Workspace};
use gpui::{
    App, AppContext as _, Bounds, Global, Pixels, Window, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, point, px, size,
};
use std::time::{Duration, Instant};

/// The shelf's width (ADR 0084).
pub(crate) const SHELF_WIDTH: f32 = 360.;
/// Space between the menu bar and the shelf, and around it on screen.
const MARGIN: f32 = 6.;
/// A click on the item this soon after the shelf closed was the click that
/// closed it: the item toggles instead of reopening.
// Only the macOS status item clicks; elsewhere the setting stays off.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// What the app carries out for a menu bar transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuBarEffect {
    ShowItem,
    RemoveItem,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenShelf,
    CloseShelf,
    /// Hide the workspace window without closing it.
    HideWindow,
    /// Bring the workspace window forward (showing it if hidden).
    ShowWindow,
    /// Show mdoc in the Dock, or leave it.
    Dock(bool),
}

#[derive(Debug, Default)]
pub(crate) struct MenuBarState {
    enabled: bool,
    window_hidden: bool,
    shelf_open: bool,
    shelf_closed_at: Option<Instant>,
}

impl MenuBarState {
    /// The setting changed (or was read at launch).
    pub(crate) fn set_enabled(&mut self, enabled: bool) -> Vec<MenuBarEffect> {
        if enabled == self.enabled {
            return Vec::new();
        }
        self.enabled = enabled;
        if enabled {
            return vec![MenuBarEffect::ShowItem];
        }
        let mut effects = Vec::new();
        if self.shelf_open {
            effects.push(MenuBarEffect::CloseShelf);
        }
        effects.push(MenuBarEffect::RemoveItem);
        // Without the item a hidden window could never come back.
        if self.window_hidden {
            effects.extend(self.reveal());
        }
        effects
    }

    /// Whether closing the workspace window hides it instead of quitting.
    pub(crate) fn keeps_running(&self) -> bool {
        self.enabled
    }

    pub(crate) fn window_hidden(&self) -> bool {
        self.window_hidden
    }

    /// The workspace window's close button (or ⌘⇧W): hide it and leave the
    /// Dock. Only while the item is on.
    pub(crate) fn close_window(&mut self) -> Vec<MenuBarEffect> {
        if !self.enabled || self.window_hidden {
            return Vec::new();
        }
        self.window_hidden = true;
        vec![MenuBarEffect::HideWindow, MenuBarEffect::Dock(false)]
    }

    /// Bring the workspace window forward: a shelf row, Show all, a reopen
    /// or a quit that has a question to ask.
    pub(crate) fn reveal(&mut self) -> Vec<MenuBarEffect> {
        let mut effects = vec![MenuBarEffect::ShowWindow];
        if std::mem::take(&mut self.window_hidden) {
            effects.push(MenuBarEffect::Dock(true));
        }
        effects
    }

    /// The workspace window became active by any other path (a menu
    /// command, Settings): it shows, so mdoc is back in the Dock.
    pub(crate) fn window_activated(&mut self) -> Vec<MenuBarEffect> {
        if std::mem::take(&mut self.window_hidden) {
            vec![MenuBarEffect::Dock(true)]
        } else {
            Vec::new()
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn item_clicked(&mut self, now: Instant) -> Vec<MenuBarEffect> {
        if !self.enabled {
            return Vec::new();
        }
        if self.shelf_open {
            return vec![MenuBarEffect::CloseShelf];
        }
        if self
            .shelf_closed_at
            .is_some_and(|closed| now.saturating_duration_since(closed) < REOPEN_GUARD)
        {
            return Vec::new();
        }
        vec![MenuBarEffect::OpenShelf]
    }

    pub(crate) fn shelf_opened(&mut self) {
        self.shelf_open = true;
    }

    pub(crate) fn shelf_closed(&mut self, now: Instant) {
        if std::mem::take(&mut self.shelf_open) {
            self.shelf_closed_at = Some(now);
        }
    }
}

/// The app's menu bar state, the workspace window and the open shelf.
struct MenuBar {
    state: MenuBarState,
    workspace: WindowHandle<Workspace>,
    shelf: Option<WindowHandle<ShelfView>>,
}

impl Global for MenuBar {}

/// Start following the setting for `workspace`, the app's one workspace.
pub(crate) fn install(workspace: WindowHandle<Workspace>, enabled: bool, cx: &mut App) {
    cx.set_global(MenuBar {
        state: MenuBarState::default(),
        workspace,
        shelf: None,
    });
    set_enabled(enabled, cx);
}

/// The setting is `enabled` now; other platforms never turn it on.
pub(crate) fn set_enabled(enabled: bool, cx: &mut App) {
    let enabled = enabled && cfg!(target_os = "macos");
    update(cx, |state| state.set_enabled(enabled));
}

/// Whether closing the workspace window hides it instead of quitting.
pub(crate) fn keeps_running(cx: &App) -> bool {
    cx.try_global::<MenuBar>()
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

/// The workspace window became active; mdoc is back in the Dock.
pub(crate) fn window_activated(cx: &mut App) {
    update(cx, MenuBarState::window_activated);
}

/// The item was clicked: open or close the shelf.
// Only the macOS status item clicks; elsewhere the setting stays off.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn item_clicked(cx: &mut App) {
    update(cx, |state| state.item_clicked(Instant::now()));
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
            #[cfg(all(target_os = "macos", not(test)))]
            gpui_macos::set_status_item(None);
        }
        MenuBarEffect::OpenShelf => open_shelf(cx),
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
            let workspace = cx.global::<MenuBar>().workspace;
            cx.activate(true);
            let _ = workspace.update(cx, |_, window, _| window.activate_window());
        }
        MenuBarEffect::Dock(visible) => {
            #[cfg(all(target_os = "macos", not(test)))]
            gpui_macos::set_dock_visible(visible);
            let _ = visible;
        }
    }
}

fn hide(window: &mut Window) {
    #[cfg(all(target_os = "macos", not(test)))]
    gpui_macos::hide_window(window);
    let _ = window;
}

fn show_item(cx: &mut App) {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        let async_cx = cx.to_async();
        gpui_macos::set_status_item(Some(gpui_macos::StatusItem {
            symbol: "doc.text",
            label: "mdoc",
            on_click: Box::new(move || on_item_click(&async_cx)),
        }));
    }
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

/// The shelf's bounds under the item, kept on its display.
fn shelf_bounds(anchor: Bounds<Pixels>, display: Bounds<Pixels>, height: f32) -> Bounds<Pixels> {
    let max_x = (display.size.width.as_f32() - SHELF_WIDTH - MARGIN).max(MARGIN);
    let x = anchor.origin.x.as_f32().clamp(MARGIN, max_x);
    let y = (anchor.origin.y + anchor.size.height).as_f32() + MARGIN / 2.;
    let max_height = (display.size.height.as_f32() - y - MARGIN).max(120.);
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(SHELF_WIDTH), px(height.min(max_height))),
    }
}

fn open_shelf(cx: &mut App) {
    #[cfg(all(target_os = "macos", not(test)))]
    let anchor = gpui_macos::status_item_bounds();
    // Tests open the shelf under a pretend item on the primary display.
    #[cfg(test)]
    let anchor = cx.primary_display().map(|display| {
        let item = Bounds {
            origin: point(px(1000.), px(0.)),
            size: size(px(30.), px(24.)),
        };
        (display.id(), item)
    });
    #[cfg(all(not(target_os = "macos"), not(test)))]
    let anchor: Option<(gpui::DisplayId, Bounds<Pixels>)> = None;
    let Some((display_id, anchor)) = anchor else {
        return;
    };
    let Some(display) = cx.find_display(display_id) else {
        return;
    };
    let workspace = cx.global::<MenuBar>().workspace;
    let height = ShelfView::height(workspace, cx);
    let bounds = shelf_bounds(anchor, display.bounds(), height);
    let shelf = cx.open_window(
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
            ..Default::default()
        },
        |window, cx| cx.new(|cx| ShelfView::new(workspace, window, cx)),
    );
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
    fn off_by_default_closing_quits_as_before() {
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
    fn the_shelf_sits_under_the_item_and_stays_on_screen() {
        let display = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1440.), px(900.)),
        };
        let item = |x: f32| Bounds {
            origin: point(px(x), px(0.)),
            size: size(px(30.), px(24.)),
        };
        let shelf = shelf_bounds(item(1000.), display, 400.);
        assert_eq!(shelf.origin, point(px(1000.), px(27.)));
        assert_eq!(shelf.size, size(px(SHELF_WIDTH), px(400.)));
        let right = shelf_bounds(item(1420.), display, 400.);
        assert_eq!(right.origin.x, px(1440. - SHELF_WIDTH - MARGIN));
        let tall = shelf_bounds(item(10.), display, 5000.);
        assert_eq!(
            tall.size.height,
            px(900. - 27. - MARGIN),
            "never past the screen"
        );
    }
}
