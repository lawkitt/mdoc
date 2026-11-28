//! The menu bar item's rules, GPUI-free: whether the item shows, whether
//! closing the window hides it, when the shelf, the companion and the
//! first-close note open and close, and the click that closed the shelf;
//! each transition returns what the app carries out.
use std::time::{Duration, Instant};

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
    /// The first-close note under the item, once ever.
    ShowNote,
    /// The companion window, in place of the shelf while the preview is on.
    OpenCompanion,
    CloseCompanion,
    /// What it shows changed: it reads the window again.
    RefreshCompanion,
}

#[derive(Debug, Default)]
pub(crate) struct MenuBarState {
    enabled: bool,
    window_hidden: bool,
    shelf_open: bool,
    shelf_closed_at: Option<Instant>,
    /// The first-close note has not shown yet.
    note_pending: bool,
    /// "AI connector (preview)" is on: the item toggles the companion.
    companion: bool,
    pub(super) companion_open: bool,
}

impl MenuBarState {
    /// The first-close note shows at the next close (it has never shown).
    pub(crate) fn note_pending(&mut self, pending: bool) {
        self.note_pending = pending;
    }

    /// The setting changed (or was read at launch).
    pub(crate) fn set_enabled(&mut self, enabled: bool) -> Vec<MenuBarEffect> {
        if enabled == self.enabled {
            return Vec::new();
        }
        self.enabled = enabled;
        if enabled {
            return vec![MenuBarEffect::ShowItem];
        }
        let mut effects = self.close_popups();
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
        let mut effects = vec![MenuBarEffect::HideWindow, MenuBarEffect::Dock(false)];
        if std::mem::take(&mut self.note_pending) {
            effects.push(MenuBarEffect::ShowNote);
        }
        effects
    }

    /// The first-close note's "Quit when windows close": the item goes and
    /// the quit that follows decides about the hidden window (it shows to
    /// ask, or mdoc ends).
    pub(crate) fn quit_when_windows_close(&mut self) -> Vec<MenuBarEffect> {
        if !std::mem::take(&mut self.enabled) {
            return Vec::new();
        }
        let mut effects = self.close_popups();
        effects.push(MenuBarEffect::RemoveItem);
        effects
    }

    /// Close the shelf and the companion, whichever is open.
    fn close_popups(&self) -> Vec<MenuBarEffect> {
        let mut effects = Vec::new();
        if self.shelf_open {
            effects.push(MenuBarEffect::CloseShelf);
        }
        if self.companion_open {
            effects.push(MenuBarEffect::CloseCompanion);
        }
        effects
    }

    /// "AI connector (preview)" is `on` now: the item toggles the
    /// companion while it is, and the shelf popup again once it is off.
    pub(crate) fn set_companion(&mut self, on: bool) -> Vec<MenuBarEffect> {
        if on == self.companion {
            return Vec::new();
        }
        self.companion = on;
        // No click closed what was open: the next one opens.
        self.shelf_closed_at = None;
        match on {
            true if self.shelf_open => vec![MenuBarEffect::CloseShelf],
            false if self.companion_open => vec![MenuBarEffect::CloseCompanion],
            _ => Vec::new(),
        }
    }

    /// An AI app delivered, or asked for a document that needs review:
    /// the companion opens if `open` and it is closed, and reads the
    /// window again if it is open. Nothing while the preview or the item
    /// is off.
    pub(crate) fn ai_arrived(&mut self, open: bool) -> Vec<MenuBarEffect> {
        if !self.enabled || !self.companion {
            return Vec::new();
        }
        match (self.companion_open, open) {
            (true, _) => vec![MenuBarEffect::RefreshCompanion],
            (false, true) => vec![MenuBarEffect::OpenCompanion],
            (false, false) => Vec::new(),
        }
    }

    /// What the companion shows changed: it reads the window again if it
    /// is open.
    pub(crate) fn companion_changed(&self) -> Vec<MenuBarEffect> {
        if self.companion_open {
            vec![MenuBarEffect::RefreshCompanion]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn companion_opened(&mut self) {
        self.companion_open = true;
    }

    pub(crate) fn companion_closed(&mut self) {
        self.companion_open = false;
    }

    /// Bring the workspace window forward: a shelf row, Open mdoc, a reopen
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
        // The companion stays up while other apps are used: the item is
        // its toggle, with no closing click to guard against.
        if self.companion {
            return self.toggle_companion();
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

    /// The companion's hotkey: it toggles the companion as the item does,
    /// and nothing while the preview or the item is off.
    pub(crate) fn hotkey_pressed(&mut self) -> Vec<MenuBarEffect> {
        if !self.enabled || !self.companion {
            return Vec::new();
        }
        self.toggle_companion()
    }

    fn toggle_companion(&self) -> Vec<MenuBarEffect> {
        match self.companion_open {
            true => vec![MenuBarEffect::CloseCompanion],
            false => vec![MenuBarEffect::OpenCompanion],
        }
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
