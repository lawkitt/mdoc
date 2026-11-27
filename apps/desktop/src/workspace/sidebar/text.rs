//! The sidebar's words: the rail and panel controls, the tree's rows and
//! menus, the delete question, navigation's stops and the switcher, with their
//! counts worded here.
use std::fmt::Display;

pub(crate) const COLLAPSE_SIDEBAR: &str = "Collapse sidebar";
pub(crate) const EXPAND_SIDEBAR: &str = "Expand sidebar";
pub(crate) const FILES_CANNOT_OPEN: &str = "These files can't be opened";
pub(crate) const PROJECTS_AND_DOCUMENTS: &str = "Projects and documents";
pub(crate) const SEARCH_OR_RUN: &str = "Search or run an action";
pub(crate) const NEW_DOCUMENT: &str = "New document";
pub(crate) const SEARCH: &str = "Search…";
pub(crate) const SETTINGS_UPDATE_AVAILABLE: &str = "Settings — update available";
pub(crate) const RECENT_DOCUMENTS: &str = "Recent documents";
pub(crate) const MOVED_BEFORE_DELETED: &str = "A document moved into a Project before it was deleted, so nothing was \
     deleted. Choose again.";
pub(crate) const DOCUMENTS_ONCE_VAULT_OPENS: &str = "Documents show here once the Vault opens.";
pub(crate) const NO_PROJECTS_YET: &str = "No Projects yet";
pub(crate) const SHOW_FEWER: &str = "Show fewer";
pub(crate) const MOVE_TO_NEW_PROJECT: &str = "Move to New Project";
pub(crate) const MOVE_TO_UNFILED: &str = "Move to Unfiled";

/// A Project as a Back or Forward stop.
pub(crate) fn project_stop(name: impl Display) -> String {
    format!("Project “{name}”")
}

/// The delete question for one document, by its title.
pub(crate) fn delete_one(title: impl Display) -> String {
    format!("Delete “{title}”?")
}

/// The delete question for several documents.
pub(crate) fn delete_many(count: impl Display) -> String {
    format!("Delete {count} documents?")
}

/// What the delete question says goes, for `count` documents.
pub(crate) fn delete_detail(count: usize) -> &'static str {
    if count == 1 {
        "Its text, original, names and AI replies are deleted from this computer. This \
         cannot be undone."
    } else {
        "Their text, originals, names and AI replies are deleted from this computer. This \
         cannot be undone."
    }
}

pub(crate) fn could_not_delete(error: impl Display) -> String {
    format!("Could not delete every document: {error}")
}

/// A Project row, as a screen reader names it.
pub(crate) fn project_row(name: impl Display, documents: impl Display) -> String {
    format!("Project {name}, {documents} documents")
}

/// A Project row's tooltip.
pub(crate) fn project_row_tooltip(name: impl Display) -> String {
    format!("Project “{name}” · its names are kept in the Vault")
}

/// Unfiled's delete button for the chosen documents.
pub(crate) fn delete_chosen(chosen: impl Display) -> String {
    format!("Delete {chosen}…")
}

/// That button, as a screen reader names it.
pub(crate) fn delete_chosen_label(chosen: impl Display) -> String {
    format!("Delete {chosen} documents")
}

/// Unfiled's row that shows the documents past its newest seven.
pub(crate) fn show_all(hidden: impl Display) -> String {
    format!("Show all ({hidden} more)")
}

/// The move menu's row for a Project.
pub(crate) fn move_to(name: impl Display) -> String {
    format!("Move to “{name}”")
}

/// A sidebar row's Delete, for the `count` documents it deletes.
pub(crate) fn delete_documents(count: usize) -> String {
    if count > 1 {
        format!("Delete {count} Documents…")
    } else {
        "Delete Document…".to_string()
    }
}
