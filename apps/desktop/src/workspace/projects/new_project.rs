//! New Project (Vault & Projects item 31): from the Project menu, the
//! palette or the "+" on the tree's Projects header. A sheet asks for the
//! name, as both phones do; Create queues one job on the window's writer,
//! which makes the empty Project at the head of the queue and then opens
//! its page.
//! The sheet's struct and the queued job are here; its state, view and
//! actions each have their file.
mod actions;
mod state;
mod view;

use crate::document::markdown_search::SearchInput;
use gpui::{Entity, FocusHandle};

/// The name question while it shows: view state only.
pub(in crate::workspace) struct NewProjectSheet {
    input: Entity<SearchInput>,
    /// Where focus goes back when it is cancelled.
    return_focus: Option<FocusHandle>,
    _changes: gpui::Subscription,
}

/// New Project, queued: the name as asked. What the name becomes, and the
/// Project's categories, are read when the job reaches the head.
pub(in crate::workspace) struct NewProjectJob {
    name: String,
}
