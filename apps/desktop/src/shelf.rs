//! The menu bar shelf: the pseudonymized documents it lists and the
//! preview each shows, the popup that shows them, the menu bar item and the
//! tray icon that open it, the files a drag carries, and its words.

mod exports;
pub(crate) mod menu_bar;
mod preview;
pub(crate) mod shelf_ui;
mod state;
#[cfg(test)]
mod tests;
pub(crate) mod text;
pub(crate) mod tray;

pub(crate) use exports::ShelfExports;
pub(crate) use preview::Preview;
pub(crate) use state::{
    ClipboardRestore, MAX_DOCUMENTS, OpenTab, Outbound, Readiness, ShelfDocument, ShelfKey,
    StoredDocument, list, restore_clipboard,
};
#[cfg(test)]
use {
    crate::workspace::ProjectLink,
    mdoc_pii::restore::{ExportedIdentity, RestoreMap},
    mdoc_store::document_id::VaultId,
    preview::{LINE_MARK, PARAGRAPH_BYTES, PREVIEW_CHARS, preview_lines},
    std::time::SystemTime,
    text::neutral_name,
};
