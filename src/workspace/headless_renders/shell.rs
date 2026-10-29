//! The window's overlays: the command palette and the keyboard shortcuts
//! sheet (ADR 0073).
use super::{Driver, Result};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    d.change(|w, window, cx| w.open_palette(window, cx))?;
    d.capture("command-palette")?;
    // Tab may leave a popover's key context. Reset state explicitly between
    // scenes rather than assuming Escape still reaches the previous overlay.
    d.change(|w, window, cx| w.close_palette(window, cx))?;
    // The keyboard shortcuts sheet, opened by its key as people do (ADR 0073).
    d.key(if cfg!(target_os = "macos") {
        "cmd-/"
    } else {
        "ctrl-/"
    })?;
    d.capture("keyboard-shortcuts")?;
    d.change(|w, window, cx| w.close_shortcuts(window, cx))?;
    Ok(())
}
