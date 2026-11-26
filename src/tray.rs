//! The system tray (Windows; Milestone 2 item 17): while "AI connector
//! (preview)" is on, a notification-area icon stands in for the macOS
//! menu bar item ([`crate::menu_bar`], whose rules it follows). A click
//! on it, or Enter or Space, toggles the companion; its menu has Open
//! mdoc, Settings and Quit; and with "Keep mdoc in the system tray" on,
//! closing the window hides it there. With the preview off Windows has
//! no tray, so nothing new is reachable.
//!
//! This module holds what is platform-free: the setting's words, whether
//! the icon shows, where a popup sits by the icon, and the guard against
//! a repeated select. `native.rs` makes the Win32 calls.

#[cfg(all(windows, not(test)))]
mod native;
#[cfg(all(windows, debug_assertions, not(test)))]
pub(crate) use native::icon_status;
#[cfg(all(windows, not(test)))]
pub(crate) use native::{anchor, hide_window, remove, show, show_window};

use gpui::{App, Bounds, Global, Pixels, point, px, size};
use std::time::{Duration, Instant};

/// The setting's name, in Settings › General and in what mdoc says about
/// it. The same stored switch (`general.menu_bar`), on by default; the
/// default lives with the store's.
pub(crate) const SETTING: &str = if cfg!(windows) {
    "Keep mdoc in the system tray"
} else {
    "Show mdoc in the menu bar"
};

/// The first-close note's title.
pub(crate) const NOTE_TITLE: &str = if cfg!(windows) {
    "mdoc is still in the system tray."
} else {
    "mdoc is still in the menu bar."
};

/// Closing the window hides it while the item shows. On Windows this is
/// the maintainer's choice for item 17 (#1164); `!cfg!(windows)` keeps the
/// icon there while closing quits.
pub(crate) const CLOSE_TO_TRAY: bool = true;

/// Space between a popup and the taskbar or the screen's edge.
const MARGIN: f32 = 6.;

/// The switch and the preview as last told, so either changing decides
/// whether the item shows.
#[derive(Default)]
struct Inputs {
    setting: bool,
    preview: bool,
}

impl Global for Inputs {}

/// Whether the item shows for the switch `setting` and the preview: on
/// macOS the switch alone decides; on Windows the preview must be on too;
/// elsewhere there is none.
fn shows(setting: bool, preview: bool) -> bool {
    if cfg!(target_os = "macos") {
        setting
    } else {
        cfg!(windows) && setting && preview
    }
}

/// The switch is `setting` now: whether the item shows.
pub(crate) fn item_enabled(setting: bool, cx: &mut App) -> bool {
    let inputs = cx.default_global::<Inputs>();
    inputs.setting = setting;
    shows(setting, inputs.preview)
}

/// The preview is `on` now. On Windows the item comes and goes with it.
pub(crate) fn preview_changed(on: bool, cx: &mut App) {
    let inputs = cx.default_global::<Inputs>();
    inputs.preview = on;
    if cfg!(windows) {
        let setting = inputs.setting;
        crate::menu_bar::set_enabled(setting, cx);
    }
}

/// Whether the item can bring a hidden window back: on Windows, whether
/// the icon is in the notification area (Explorer may not be up yet).
pub(crate) fn icon_usable() -> bool {
    #[cfg(all(windows, not(test)))]
    let usable = native::usable();
    #[cfg(not(all(windows, not(test))))]
    let usable = true;
    usable
}

/// Whether Settings › General shows the switch: always on macOS; on
/// Windows only while the preview is on, as only then does it act.
pub(crate) fn setting_shown(preview: bool) -> bool {
    cfg!(target_os = "macos") || (cfg!(windows) && preview)
}

/// The most a popup by the icon may grow inside `work`, the display's
/// area without the taskbar; a taller one scrolls.
pub(crate) fn popup_max_height(work: Bounds<Pixels>) -> f32 {
    (work.size.height.as_f32() - 2. * MARGIN).max(120.)
}

/// A popup's bounds by the icon at `anchor`, inside `work`, on the
/// taskbar's side of the screen: above a bottom taskbar, below a top one,
/// beside a side one, and near the icon along it. With the taskbar hidden
/// the icon is inside `work`: the popup opens towards the screen's middle.
pub(crate) fn popup_bounds(
    anchor: Bounds<Pixels>,
    work: Bounds<Pixels>,
    width: f32,
    height: f32,
) -> Bounds<Pixels> {
    let height = height.min(popup_max_height(work));
    let (left, top) = (work.origin.x.as_f32(), work.origin.y.as_f32());
    let (right, bottom) = (
        left + work.size.width.as_f32(),
        top + work.size.height.as_f32(),
    );
    let anchor_top = anchor.origin.y.as_f32();
    let anchor_bottom = anchor_top + anchor.size.height.as_f32();
    let center = anchor.center();
    let (center_x, center_y) = (center.x.as_f32(), center.y.as_f32());
    let max_x = (right - width - MARGIN).max(left + MARGIN);
    let max_y = (bottom - height - MARGIN).max(top + MARGIN);
    let side = center_x < left || center_x > right;
    let x = if center_x < left {
        left + MARGIN
    } else if center_x > right {
        max_x
    } else {
        (center_x - width / 2.).clamp(left + MARGIN, max_x)
    };
    let y = if side {
        // Its bottom level with the icon's.
        anchor_bottom - height
    } else if center_y > top + work.size.height.as_f32() / 2. {
        anchor_top.min(bottom) - height - MARGIN
    } else {
        anchor_bottom.max(top) + MARGIN
    };
    Bounds {
        origin: point(px(x), px(y.clamp(top + MARGIN, max_y))),
        size: size(px(width), px(height)),
    }
}

/// A popup's new top and bottom when it grows by `delta`: the edge next
/// to the taskbar stays put, so a popup in the lower half of `work` grows
/// upwards. Device pixels.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn resized(
    top: i32,
    bottom: i32,
    work_top: i32,
    work_bottom: i32,
    delta: i32,
) -> (i32, i32) {
    if top + bottom > work_top + work_bottom {
        (top - delta, bottom)
    } else {
        (top, bottom + delta)
    }
}

/// The shell may report one select twice (Enter, or a double click):
/// one within `within` of the last is the same.
#[derive(Debug, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct SelectGuard {
    last: Option<Instant>,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl SelectGuard {
    pub(crate) fn accept(&mut self, now: Instant, within: Duration) -> bool {
        let repeat = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < within);
        if !repeat {
            self.last = Some(now);
        }
        !repeat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        }
    }

    /// A 1920×1080 display with a 48-pt taskbar at the bottom.
    const WORK: (f32, f32, f32, f32) = (0., 0., 1920., 1032.);

    fn work() -> Bounds<Pixels> {
        bounds(WORK.0, WORK.1, WORK.2, WORK.3)
    }

    #[test]
    fn above_a_bottom_taskbar_by_the_icon_and_on_screen() {
        let icon = bounds(1700., 1040., 24., 32.);
        let popup = popup_bounds(icon, work(), 360., 400.);
        assert_eq!(
            popup.origin,
            point(px(1712. - 180.), px(1032. - 400. - MARGIN))
        );
        assert_eq!(popup.size, size(px(360.), px(400.)));
        // Near the corner it stays inside the work area.
        let corner = popup_bounds(bounds(1900., 1040., 20., 32.), work(), 360., 400.);
        assert_eq!(corner.origin.x, px(1920. - 360. - MARGIN));
        // Taller than the screen: cut to it, and it scrolls.
        let tall = popup_bounds(icon, work(), 360., 5000.);
        assert_eq!(tall.size.height, px(1032. - 2. * MARGIN));
        assert_eq!(tall.origin.y, px(MARGIN));
    }

    #[test]
    fn below_a_top_taskbar_and_beside_a_side_one() {
        let top_work = bounds(0., 48., 1920., 1032.);
        let popup = popup_bounds(bounds(1700., 8., 24., 32.), top_work, 360., 400.);
        assert_eq!(popup.origin.y, px(48. + MARGIN));
        // A taskbar on the left, its icons at the bottom.
        let left_work = bounds(62., 0., 1858., 1080.);
        let side = popup_bounds(bounds(18., 1000., 24., 24.), left_work, 360., 400.);
        assert_eq!(side.origin, point(px(62. + MARGIN), px(1024. - 400.)));
        let right_work = bounds(0., 0., 1858., 1080.);
        let side = popup_bounds(bounds(1876., 1000., 24., 24.), right_work, 360., 400.);
        assert_eq!(side.origin.x, px(1858. - 360. - MARGIN));
    }

    #[test]
    fn with_the_taskbar_hidden_it_opens_towards_the_middle() {
        let full = bounds(0., 0., 1920., 1080.);
        let popup = popup_bounds(bounds(1700., 1050., 24., 30.), full, 360., 400.);
        assert_eq!(popup.origin.y, px(1050. - 400. - MARGIN));
    }

    #[test]
    fn on_a_second_display_it_stays_on_that_display() {
        let second = bounds(1920., 0., 1280., 984.);
        let popup = popup_bounds(bounds(3150., 990., 24., 30.), second, 360., 300.);
        assert_eq!(popup.origin.x, px(3200. - 360. - MARGIN));
        assert_eq!(popup.origin.y, px(984. - 300. - MARGIN));
    }

    #[test]
    fn it_grows_away_from_the_taskbar() {
        assert_eq!(resized(600, 1000, 0, 1032, 50), (550, 1000));
        assert_eq!(resized(60, 400, 48, 1080, 50), (60, 450));
        assert_eq!(resized(600, 1000, 0, 1032, -50), (650, 1000));
    }

    #[test]
    fn a_repeated_select_counts_once() {
        let mut guard = SelectGuard::default();
        let now = Instant::now();
        let within = Duration::from_millis(500);
        assert!(guard.accept(now, within));
        assert!(!guard.accept(now + Duration::from_millis(100), within));
        assert!(guard.accept(now + Duration::from_millis(700), within));
    }

    #[test]
    fn the_item_shows_with_the_switch_and_on_windows_the_preview() {
        assert_eq!(shows(true, false), cfg!(target_os = "macos"));
        assert_eq!(shows(true, true), cfg!(any(target_os = "macos", windows)));
        assert!(!shows(false, true));
        assert_eq!(setting_shown(false), cfg!(target_os = "macos"));
        assert_eq!(setting_shown(true), cfg!(any(target_os = "macos", windows)));
    }
}
