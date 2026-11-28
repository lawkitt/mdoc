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
