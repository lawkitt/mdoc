//! Saved replies' words: each string a user reads of saving an AI reply
//! and of the Reply page is a constant or a function here, with its counts
//! worded only here.
use std::fmt::Display;

// Saving.

/// The notice when a reply box is opened before its document is in mdoc.
pub(crate) const NOT_IN_MDOC_OPEN: &str = "The AI reply could not be saved: its document is not \
     in mdoc yet. Type in it first, then open the reply again.";
/// The notice when Paste AI Reply runs before its document is in mdoc.
pub(crate) const NOT_IN_MDOC_PASTE: &str = "The AI reply could not be saved: its document is not \
     in mdoc yet. Type in it first, then paste the reply again.";
/// The notice when Paste AI Reply runs while its document moves.
pub(crate) const MOVING: &str = "The AI reply was not saved: its document is moving. Paste the \
     reply again once the move is done.";
/// A saved reply's title, from its document's `title`.
pub(crate) fn reply_to(title: impl Display) -> String {
    format!("Reply to {title}")
}
/// Why a reply waits: its Project's names.
pub(crate) const PROJECT_NAMES_UNSAVED: &str = "its Project's names are not saved yet";
/// Why a reply waits: its document's names.
pub(crate) const DOCUMENT_NAMES_UNSAVED: &str = "its document's names are not saved yet";
/// Why a reply was not saved: its document's write failed with `error`.
pub(crate) fn document_not_saved_first(error: impl Display) -> String {
    format!("its document could not be saved first: {error}")
}
/// Why a pasted reply was not saved, `why`, and what to do.
pub(crate) fn paste_it_again(why: impl Display) -> String {
    format!("{why}. Paste it again")
}
/// The notice when a reply could not be saved, and why (`why`).
pub(crate) fn could_not_save(why: impl Display) -> String {
    format!("Could not save the AI reply: {why}.")
}

// Promote to Document.

/// The notice when Promote stopped at the Project's names, with `error`.
pub(crate) fn names_not_saved_for_promote(error: impl Display) -> String {
    format!(
        "Could not promote the AI reply: its Project's names could not be saved. \
         {error}"
    )
}
/// Why a promoted document can't be opened: its id.
pub(crate) const INVALID_ID: &str = "Invalid id.";
/// The notice when Promote failed with `error`.
pub(crate) fn could_not_promote(error: impl Display) -> String {
    format!("Could not promote the AI reply: {error}.")
}

// Delete.

/// The reply the delete question names when its title is unknown.
pub(crate) const THIS_REPLY: &str = "this reply";
/// A reply's title in the delete question, quoted.
pub(crate) fn quoted(title: impl Display) -> String {
    format!("“{title}”")
}
/// The delete question, for `title` (from [`quoted`] or [`THIS_REPLY`]).
pub(crate) fn delete_title(title: impl Display) -> String {
    format!("Delete {title}?")
}
/// The delete question's detail.
pub(crate) const DELETE_DETAIL: &str =
    "Its text is deleted from this computer. This cannot be undone.";
/// The delete question's button.
pub(crate) const DELETE: &str = "Delete";
/// The notice when the delete failed with `error`.
pub(crate) fn could_not_delete(error: impl Display) -> String {
    format!("Could not delete the AI reply: {error}")
}

// The Reply page.

/// The page's title.
pub(crate) const AI_REPLY: &str = "AI reply";
/// The page when its reply can't be read, with `error`.
pub(crate) fn cant_be_opened(error: impl Display) -> String {
    format!("This reply can't be opened: {error}")
}
/// The place of a reply whose document is Unfiled.
pub(crate) const UNFILED: &str = "Unfiled";
/// The place of a reply in Project `place`.
pub(crate) fn in_project(place: impl Display) -> String {
    format!("Project “{place}”")
}
/// The summary: `values` restored, `unrecognized` not.
pub(crate) fn restored(values: usize, unrecognized: usize) -> String {
    format!(
        "{values} {} restored · {} unrecognized",
        if values == 1 { "value" } else { "values" },
        unrecognized
    )
}
/// Between the parts of the page's line about the reply, and of where
/// it came from.
pub(crate) const SEPARATOR: &str = " · ";
/// The page's Copy.
pub(crate) const COPY_REPLY: &str = "Copy reply";
/// The page's move to the next value not restored.
pub(crate) const NEXT_UNRECOGNIZED: &str = "Next unrecognized";
/// The page's Promote.
pub(crate) const PROMOTE_TO_DOCUMENT: &str = "Promote to Document";
/// The page's Delete.
pub(crate) const DELETE_REPLY: &str = "Delete reply…";
/// Where a reply came from: a document whose title is unknown.
pub(crate) const FROM_A_DOCUMENT: &str = "From a document";
/// Where a reply came from: document `title`.
pub(crate) fn from_document(title: impl Display) -> String {
    format!("From “{title}”")
}
/// Where a reply came from: its Project.
pub(crate) const FROM_THE_PROJECT: &str = "From the Project";
