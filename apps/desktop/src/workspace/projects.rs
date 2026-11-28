//! The window's Projects: every Project record in the Vault, read when it
//! opens and kept here while the window edits them. A change marks the
//! Project for the window's one writer, which saves the record as it is
//! when its write starts; moves and deletes run through the same writer and
//! hand back the records they wrote. Here are the owner's struct on the
//! window and the move toast; its state, view, actions and words, and each
//! job's own folder or file, sit beside.

mod actions;
mod delete;
pub(super) mod export_project;
pub(super) mod folder_sheet;
mod moves;
pub(super) mod new_project;
pub(super) mod project_page;
mod state;
mod text;
mod view;

use crate::document::markdown_search;
use crate::workspace::Workspace;
use gpui::{App, Context, Entity, Window};
use mdoc_store::document_id::VaultId;
use mdoc_vault::MoveUndo;
pub(crate) use state::ProjectLink;
pub(super) use state::{Projects, fold, keep_fields};
use std::collections::HashSet;

gpui::actions!(
    projects,
    [
        ConfirmProjectName,
        CancelProjectName,
        AddProjectName,
        ClearProjectName
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Projects, cx);
    cx.bind_keys(bindings);
}

/// The toast a move shows: what happened, and how to undo it.
pub(in crate::workspace) struct MoveToast {
    pub(in crate::workspace) text: String,
    /// Undo moves the document back; a Project the move created goes
    /// with it once empty.
    undo: Option<(MoveUndo, Option<String>)>,
    _timeout: gpui::Task<()>,
}

pub(in crate::workspace) struct ProjectsState {
    pub(in crate::workspace) page: Option<crate::workspace::projects::project_page::ProjectPage>,
    pub(in crate::workspace) header_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// The Project page's Ask menu (Milestone 2 item 10).
    pub(in crate::workspace) ask_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    pub(in crate::workspace) renaming: Option<(String, Entity<markdown_search::SearchInput>)>,
    /// Saves the rename when focus leaves its field.
    rename_blur: Option<gpui::Subscription>,
    /// Each document Move to New Project moved, and the Project it made:
    /// its name is ready to edit once the document's tab has loaded again.
    naming: std::collections::HashMap<VaultId, String>,
    pub(in crate::workspace) list_open: bool,
    /// The Keychain explainer is shown.
    pub(in crate::workspace) explainer: bool,
    /// What runs once the explainer's Continue opened the key.
    then: Option<AfterUnlock>,
    /// Projects changed since their last write was queued.
    unwritten: HashSet<String>,
    pub(in crate::workspace) toast: Option<MoveToast>,
    /// Member loader invocations, including attempts that would fail to read.
    #[cfg(test)]
    pub(in crate::workspace) member_reads: usize,
    /// The Open Project list holds focus while open, and gives it back on
    /// close.
    pub(in crate::workspace) project_list_focus: gpui::FocusHandle,
    pub(in crate::workspace) project_list_return: Option<gpui::FocusHandle>,
    /// Dropped folders' sheets, one at a time.
    pub(in crate::workspace) folder_sheets: folder_sheet::FolderSheets,
    pub(in crate::workspace) folder_sheet_focus: Option<folder_sheet::SheetFocus>,
    /// New Project's name question, while it shows.
    pub(in crate::workspace) new_project: Option<new_project::NewProjectSheet>,
    pub(in crate::workspace) new_project_group: gpui::FocusHandle,
}

type AfterUnlock = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

impl ProjectsState {
    /// Nothing open yet: no page, menu, sheet or toast.
    pub(in crate::workspace) fn new(cx: &mut gpui::App) -> Self {
        Self {
            page: None,
            header_menu: None,
            ask_menu: None,
            renaming: None,
            rename_blur: None,
            naming: Default::default(),
            list_open: false,
            explainer: false,
            then: None,
            unwritten: Default::default(),
            toast: None,
            #[cfg(test)]
            member_reads: 0,
            project_list_focus: cx.focus_handle(),
            project_list_return: None,
            folder_sheets: Default::default(),
            folder_sheet_focus: None,
            new_project: None,
            new_project_group: cx.focus_handle(),
        }
    }

    /// Project `id`'s last write failed: queue it again with the next.
    pub(in crate::workspace) fn mark_unwritten(&mut self, id: &str) {
        self.unwritten.insert(id.to_string());
    }

    /// Project `id` changed since its last write was queued, or that write
    /// failed.
    pub(in crate::workspace) fn is_unwritten(&self, id: &str) -> bool {
        self.unwritten.contains(id)
    }

    /// A job writes Project `id` now: it no longer waits.
    pub(in crate::workspace) fn take_unwritten(&mut self, id: &str) -> bool {
        self.unwritten.remove(id)
    }
}
