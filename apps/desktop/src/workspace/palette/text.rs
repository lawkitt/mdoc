//! The palette's and the shortcuts sheet's words: each string a user reads in
//! them is a constant or a function here, with the palette's document count
//! worded only here. Labels the menus share wait for one command table.

/// The search field's placeholder.
pub(crate) const SEARCH_OR_RUN: &str = "Search or run an action…";
/// The list with no row for the query.
pub(crate) const NOTHING_MATCHES: &str = "Nothing matches.";

/// The sections, in the palette's order.
pub(crate) const PAGE: &str = "Page";
pub(crate) const ACTIONS: &str = "Actions";
pub(crate) const GO_TO: &str = "Go to";
pub(crate) const GO_TO_HEADING: &str = "Go to heading";
pub(crate) const DOCUMENTS: &str = "Documents";
pub(crate) const PROJECTS: &str = "Projects";

/// The Actions rows.
pub(crate) const NEW_DOCUMENT: &str = "New document";
pub(crate) const OPEN_FILES: &str = "Open files…";
pub(crate) const PASTE_TEXT: &str = "Paste text as new document";
pub(crate) const PSEUDONYMIZE: &str = "Pseudonymize";
pub(crate) const COPY_FOR_AI: &str = "Copy for AI";
pub(crate) const COPY_MARKDOWN: &str = "Copy Markdown";
pub(crate) const SAVE: &str = "Save";
pub(crate) const EXPORT_MARKDOWN: &str = "Export Markdown…";
pub(crate) const SIDE_BY_SIDE: &str = "Side by side";
pub(crate) const WHAT_YOU_DID: &str = "What you did";
pub(crate) const NOTES: &str = "Notes";
pub(crate) const OPEN_SOURCE_REPLY: &str = "Open source AI reply";
pub(crate) const EXPAND_IDENTITIES: &str = "Expand all identities";
pub(crate) const COLLAPSE_IDENTITIES: &str = "Collapse all identities";
pub(crate) const CANCEL_CONVERSION: &str = "Cancel conversion";
pub(crate) const PASTE_AI_REPLY: &str = "Paste AI reply";
pub(crate) const PSEUDONYMIZE_CLIPBOARD: &str = "Pseudonymize clipboard";
pub(crate) const OPEN_A_PROJECT: &str = "Open a Project…";
pub(crate) const NEW_PROJECT: &str = "New Project…";
pub(crate) const TOGGLE_SIDEBAR: &str = "Toggle sidebar";
pub(crate) const KEYBOARD_SHORTCUTS: &str = "Keyboard shortcuts";

/// The Go to rows.
pub(crate) const HOME: &str = "Home";
pub(crate) const SETTINGS: &str = "Settings";
/// A Settings category's row.
pub(crate) fn settings_category(category: impl std::fmt::Display) -> String {
    format!("Settings › {category}")
}
/// A heading row's detail: its level.
pub(crate) fn heading_level(level: impl std::fmt::Display) -> String {
    format!("H{level}")
}

/// A document row's place for a document in no Project.
pub(crate) const UNFILED: &str = "Unfiled";
/// A document row's detail: its place and its kind.
pub(crate) fn document_detail(place: &str, kind: impl std::fmt::Display) -> String {
    format!("{place} · {kind}")
}
/// A Project row's detail: how many documents it holds.
pub(crate) fn documents(n: usize) -> String {
    match n {
        1 => "1 document".to_string(),
        n => format!("{n} documents"),
    }
}

/// The Page rows of the Reply page.
pub(crate) const COPY_REPLY: &str = "Copy reply";
pub(crate) const PROMOTE_TO_DOCUMENT: &str = "Promote to Document";
pub(crate) const DELETE_REPLY: &str = "Delete reply…";
/// The Page rows of the Project page.
pub(crate) const ADD_DOCUMENT_TO_PROJECT: &str = "Add document to Project…";
pub(crate) const RENAME_PROJECT: &str = "Rename Project";
pub(crate) const PROJECT_COLOUR: &str = "Project colour";
pub(crate) const PROJECT_CATEGORIES: &str = "Project categories to detect";
pub(crate) const EXPORT_PROJECT: &str = "Export Project…";
pub(crate) const DELETE_PROJECT: &str = "Delete Project…";

/// The shortcuts sheet's title, also its name for a screen reader.
pub(crate) const KEYBOARD_SHORTCUTS_TITLE: &str = "Keyboard Shortcuts";
/// The sheet's filter field's placeholder.
pub(crate) const FILTER_SHORTCUTS: &str = "Filter shortcuts";
/// The sheet's list, as a screen reader names it.
pub(crate) const SHORTCUTS: &str = "Shortcuts";
/// The sheet with no row for the filter.
pub(crate) const NO_SHORTCUT_MATCHES: &str = "No shortcut matches.";
/// A sheet row, as a screen reader reads it: its label, then its keys.
pub(crate) fn shortcut_row(label: &str, keys: &[String]) -> String {
    format!("{label}, {}", keys.join(", "))
}
