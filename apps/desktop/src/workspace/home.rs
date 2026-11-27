//! Home: its view with the drop zone, the quick actions and the Continue
//! strip, whose rows, like the palette's search over the Vault, are read
//! from the manifest's summaries in memory, so no document record is opened.
//! The owner's state, view, actions and words each have their file.

mod actions;
mod state;
mod text;
mod view;

pub(super) use state::{
    Card, STRIP_CARDS, Strip, age, needs_review, now, proposals, search, strip,
};

use gpui::actions;

actions!(mdoc_shell, [ShowHome, PasteAsNewTab]);

/// Home's own fields on the window: what is dragged over it, its scroll,
/// its cards' focus and the local note's hover.
pub(in crate::workspace) struct Home {
    /// Files dragged over the window while Home shows: how many are droppable.
    pub(in crate::workspace) drop: Option<usize>,
    pub(in crate::workspace) drop_generation: u64,
    pub(in crate::workspace) home_scroll: gpui::ScrollHandle,
    /// Focus for each card of Home's Continue strip, by its document's or
    /// reply's id, so a reordered strip keeps focus on the same card.
    pub(in crate::workspace) card_focus:
        std::cell::RefCell<std::collections::HashMap<String, gpui::FocusHandle>>,
    /// The pointer is over Home's "Documents stay on this Mac".
    pub(in crate::workspace) local_note_hover: bool,
}

impl Home {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            drop: None,
            drop_generation: 0,
            home_scroll: gpui::ScrollHandle::new(),
            card_focus: Default::default(),
            local_note_hover: false,
        }
    }
}
