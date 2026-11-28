//! The keyboard shortcuts sheet: a read-only modal over the
//! window listing the one shortcut table by context, with this platform's
//! keys only. A filter field narrows it; Esc closes it and focus goes back
//! where it was.
//!
//! Its files split by kind: `state.rs` has the rows a filter shows,
//! `view.rs` draws the sheet, and `actions.rs` opens, closes and scrolls it.

mod actions;
mod state;
mod view;

pub(crate) use state::visible;

use crate::document::markdown_search::SearchInput;
use gpui::{Entity, FocusHandle, ScrollHandle, Subscription, actions};

actions!(mdoc_shortcuts, [ShowShortcuts, CloseShortcuts]);

/// The sheet's key context.
pub(crate) const SHEET: &str = "ShortcutsSheet";
/// The filter field's key context, one of the text fields'.
const FILTER: &str = "ShortcutsFilter";
/// How far one arrow press scrolls the list.
const LINE: f32 = 28.;

pub(in crate::workspace) struct ShortcutsSheet {
    filter: Entity<SearchInput>,
    /// The sheet itself: keeps Tab inside.
    focus: FocusHandle,
    /// The list, a Tab stop that scrolls with the arrow keys.
    list: FocusHandle,
    scroll: ScrollHandle,
    return_focus: Option<FocusHandle>,
    _changes: Subscription,
    _focus_out: Subscription,
}
