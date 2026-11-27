//! The folder sheet: what a dropped folder shows before anything opens, a
//! modal dialog listing its files as checkboxes, with Open as New Project
//! (or Add to “X”), Open files and Cancel. The view's focus and the queued
//! Project are here; its state, view and actions each have their file.
mod actions;
mod state;
mod view;

use gpui::{FocusHandle, ScrollHandle};
pub(in crate::workspace) use state::{
    CAP, CAP_NOTE, DroppedFolder, FolderSheets, Pending, Sheet, Status, Target,
};

/// A dropped folder's new Project, waiting for its turn on the writer.
pub(in crate::workspace) struct FolderProject {
    sheet: Sheet,
    /// Its categories, from Settings › Detection.
    skip: Vec<String>,
}

/// The sheet's focus: its group, one handle per row, and where focus goes
/// back when it closes. View state only; the sheet itself is `FolderSheets`.
pub(in crate::workspace) struct SheetFocus {
    pub(in crate::workspace) group: FocusHandle,
    rows: Vec<FocusHandle>,
    scroll: ScrollHandle,
    return_focus: Option<FocusHandle>,
}
