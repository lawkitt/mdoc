//! The command table: each command the menu bar, the palette, the ⋯ menu and
//! the shortcuts sheet offer, written once with its names and what it runs.
//!
//! A command's keys are its row in the shortcuts table, which names the
//! command instead of typing its name and action again; [`Command::key`]
//! reads them from there. Which commands a list offers, in what order and
//! when, stays with the list.

use crate::{
    commands::{menus, shortcuts, text},
    document,
    document::search_session as find,
    review,
    workspace::{self, palette::shortcuts_ui},
};
use gpui::{Action, OsAction, SharedString};

/// What a command runs.
pub(crate) type Make = fn() -> Box<dyn Action>;

/// One command: its names in each list, and the action it runs.
#[derive(Clone, Copy)]
pub(crate) struct Command {
    /// Its name in the palette and the ⋯ menu, and in the sheet unless
    /// [`Self::sheet`] says otherwise.
    pub label: &'static str,
    /// Its name in the menu bar.
    pub menu: &'static str,
    /// Its name in the shortcuts sheet.
    pub sheet: &'static str,
    /// The action it runs.
    pub make: Make,
    /// The OS action its menu bar item stands for, so the item also reaches
    /// native panels.
    pub os: Option<OsAction>,
}

impl Command {
    /// A command named `label` in every list.
    const fn new(label: &'static str, make: Make) -> Self {
        Self {
            label,
            menu: label,
            sheet: label,
            make,
            os: None,
        }
    }
    const fn menu(self, menu: &'static str) -> Self {
        Self { menu, ..self }
    }
    const fn sheet(self, sheet: &'static str) -> Self {
        Self { sheet, ..self }
    }
    const fn os(self, os: OsAction) -> Self {
        Self {
            os: Some(os),
            ..self
        }
    }

    /// The action it runs, to dispatch.
    pub(crate) fn action(&self) -> Box<dyn Action> {
        (self.make)()
    }

    /// Its key on this platform, as the shortcuts table writes it.
    pub(crate) fn key(&self) -> Option<SharedString> {
        shortcuts::label(&*self.action()).map(Into::into)
    }
}

// ---- The application menu ---------------------------------------------------------

pub(crate) const ABOUT: Command = Command::new(text::ABOUT_MDOC, || Box::new(menus::About));
pub(crate) const CHECK_FOR_UPDATES: Command = Command::new(text::CHECK_FOR_UPDATES, || {
    Box::new(workspace::CheckForUpdates)
});
pub(crate) const SETTINGS: Command =
    Command::new(text::SETTINGS, || Box::new(workspace::Settings)).menu(text::SETTINGS_MENU);
pub(crate) const HIDE_MDOC: Command = Command::new(text::HIDE_MDOC, || Box::new(menus::HideApp));
pub(crate) const HIDE_OTHERS: Command =
    Command::new(text::HIDE_OTHERS, || Box::new(menus::HideOtherApps)).menu(text::HIDE_OTHERS_MENU);
pub(crate) const SHOW_ALL: Command = Command::new(text::SHOW_ALL, || Box::new(menus::ShowAllApps));
pub(crate) const QUIT: Command =
    Command::new(text::QUIT, || Box::new(workspace::Quit)).menu(text::QUIT_MENU);

// ---- File ------------------------------------------------------------------------

pub(crate) const NEW: Command = Command::new(text::NEW_DOCUMENT, || Box::new(workspace::New))
    .menu(text::NEW_MENU)
    .sheet(text::NEW_TAB);
pub(crate) const OPEN: Command =
    Command::new(text::OPEN_FILES, || Box::new(workspace::Open)).menu(text::OPEN_MENU);
pub(crate) const OPEN_PROJECT: Command =
    Command::new(text::OPEN_A_PROJECT, || Box::new(workspace::OpenProject))
        .menu(text::OPEN_PROJECT_MENU);
pub(crate) const CONVERT_TO_MARKDOWN: Command =
    Command::new(text::CONVERT_TO_MARKDOWN, || Box::new(workspace::Import));
pub(crate) const SAVE: Command = Command::new(text::SAVE, || Box::new(document::Save));
pub(crate) const EXPORT_MARKDOWN: Command =
    Command::new(text::EXPORT_MARKDOWN, || Box::new(document::ExportMarkdown));
pub(crate) const COPY_MARKDOWN: Command =
    Command::new(text::COPY_MARKDOWN, || Box::new(document::CopyMarkdown));
pub(crate) const COPY_FOR_AI: Command =
    Command::new(text::COPY_FOR_AI, || Box::new(review::CopyForAi));
pub(crate) const PASTE_AI_REPLY: Command = Command::new(text::PASTE_AI_REPLY, || {
    Box::new(workspace::PasteAiResponse)
})
.menu(text::PASTE_AI_REPLY_MENU);
pub(crate) const CLOSE: Command =
    Command::new(crate::ui::text::CLOSE, || Box::new(workspace::Close));

// ---- Project ---------------------------------------------------------------------

pub(crate) const NEW_PROJECT: Command =
    Command::new(text::NEW_PROJECT, || Box::new(workspace::NewProject));
pub(crate) const MOVE_TO_PROJECT: Command =
    Command::new(text::MOVE_TO_PROJECT, || Box::new(workspace::MoveToProject));
pub(crate) const MOVE_TO_NEW_PROJECT: Command = Command::new(text::MOVE_TO_NEW_PROJECT, || {
    Box::new(workspace::MoveToNewProject)
});
pub(crate) const ADD_DOCUMENT_TO_PROJECT: Command =
    Command::new(text::ADD_DOCUMENT_TO_PROJECT, || {
        Box::new(workspace::AddDocumentToProject)
    })
    .menu(text::ADD_DOCUMENT_MENU);
pub(crate) const RENAME_PROJECT: Command =
    Command::new(text::RENAME_PROJECT, || Box::new(workspace::RenameProject));
pub(crate) const PROJECT_COLOUR: Command = Command::new(text::PROJECT_COLOUR, || {
    Box::new(workspace::ChangeProjectColour)
})
.menu(text::PROJECT_COLOUR_MENU);
pub(crate) const PROJECT_CATEGORIES: Command = Command::new(text::PROJECT_CATEGORIES, || {
    Box::new(workspace::ProjectCategories)
})
.menu(text::PROJECT_CATEGORIES_MENU);
pub(crate) const EXPORT_PROJECT: Command =
    Command::new(text::EXPORT_PROJECT, || Box::new(workspace::ExportProject));
pub(crate) const DELETE_PROJECT: Command =
    Command::new(text::DELETE_PROJECT, || Box::new(workspace::DeleteProject));

// ---- Edit ------------------------------------------------------------------------

pub(crate) const UNDO: Command =
    Command::new(text::UNDO, || Box::new(mdoc_editor::Undo)).os(OsAction::Undo);
pub(crate) const REDO: Command =
    Command::new(text::REDO, || Box::new(mdoc_editor::Redo)).os(OsAction::Redo);
pub(crate) const CUT: Command =
    Command::new(text::CUT, || Box::new(mdoc_editor::Cut)).os(OsAction::Cut);
pub(crate) const COPY: Command =
    Command::new(text::COPY, || Box::new(mdoc_editor::Copy)).os(OsAction::Copy);
pub(crate) const PASTE: Command =
    Command::new(text::PASTE, || Box::new(mdoc_editor::Paste)).os(OsAction::Paste);
pub(crate) const SELECT_ALL: Command =
    Command::new(text::SELECT_ALL, || Box::new(mdoc_editor::SelectAll))
        .menu(text::SELECT_ALL_MENU)
        .os(OsAction::SelectAll);
pub(crate) const FIND: Command =
    Command::new(text::FIND, || Box::new(find::FindMarkdown)).menu(text::FIND_MENU);
pub(crate) const FIND_AND_REPLACE: Command = Command::new(text::FIND_AND_REPLACE, || {
    Box::new(find::FindReplaceMarkdown)
})
.menu(text::FIND_AND_REPLACE_MENU);
pub(crate) const FIND_NEXT: Command =
    Command::new(text::FIND_NEXT, || Box::new(find::FindNextMarkdown)).menu(text::FIND_NEXT_MENU);
pub(crate) const FIND_PREVIOUS: Command =
    Command::new(text::FIND_PREVIOUS, || Box::new(find::FindPreviousMarkdown))
        .menu(text::FIND_PREVIOUS_MENU);
pub(crate) const PSEUDONYMIZE: Command =
    Command::new(text::PSEUDONYMIZE, || Box::new(review::Pseudonymize));
pub(crate) const PSEUDONYMIZE_CLIPBOARD: Command =
    Command::new(text::PSEUDONYMIZE_CLIPBOARD, || {
        Box::new(workspace::PseudonymizeClipboard)
    })
    .menu(text::PSEUDONYMIZE_CLIPBOARD_MENU);
pub(crate) const RESTORE_CLIPBOARD: Command = Command::new(text::RESTORE_CLIPBOARD_MENU, || {
    Box::new(workspace::RestoreClipboard)
});

// ---- View ------------------------------------------------------------------------

pub(crate) const HOME: Command = Command::new(text::HOME, || Box::new(workspace::ShowHome));
pub(crate) const COMMAND_PALETTE: Command =
    Command::new(text::COMMAND_PALETTE, || Box::new(workspace::OpenPalette))
        .menu(text::SEARCH_MENU);
pub(crate) const TOGGLE_SIDEBAR: Command =
    Command::new(text::TOGGLE_SIDEBAR, || Box::new(workspace::ToggleSidebar))
        .menu(text::TOGGLE_SIDEBAR_MENU);
pub(crate) const SHOW_EDIT_VIEW: Command =
    Command::new(text::EDIT, || Box::new(document::ShowEditView));
pub(crate) const SHOW_ORIGINAL_VIEW: Command =
    Command::new(text::ORIGINAL, || Box::new(document::ShowOriginalView));
pub(crate) const SHOW_PSEUDONYMIZED_VIEW: Command = Command::new(text::PSEUDONYMIZED, || {
    Box::new(document::ShowPseudonymizedView)
});
pub(crate) const SIDE_BY_SIDE: Command =
    Command::new(text::SIDE_BY_SIDE, || Box::new(document::ToggleCompare))
        .menu(text::SIDE_BY_SIDE_MENU);
pub(crate) const SHOW_REPLACEMENTS: Command = Command::new(text::SHOW_REPLACEMENTS, || {
    Box::new(document::ShowReplacements)
});
pub(crate) const NOTES: Command = Command::new(text::NOTES, || Box::new(document::ShowNotes));
pub(crate) const OPEN_SOURCE_REPLY: Command = Command::new(text::OPEN_SOURCE_REPLY, || {
    Box::new(document::OpenSourceReply)
})
.menu(text::OPEN_SOURCE_REPLY_MENU);
pub(crate) const ZOOM_IN: Command =
    Command::new(text::ZOOM_IN, || Box::new(document::ZoomIn)).menu(text::ZOOM_IN_MENU);
pub(crate) const ZOOM_OUT: Command =
    Command::new(text::ZOOM_OUT, || Box::new(document::ZoomOut)).menu(text::ZOOM_OUT_MENU);
pub(crate) const ACTUAL_SIZE: Command =
    Command::new(text::ACTUAL_SIZE, || Box::new(document::ZoomReset)).menu(text::ACTUAL_SIZE_MENU);

// ---- Window and Help -------------------------------------------------------------

pub(crate) const MINIMIZE: Command =
    Command::new(text::MINIMIZE, || Box::new(menus::MinimizeWindow));
pub(crate) const ZOOM_WINDOW: Command = Command::new(text::ZOOM, || Box::new(menus::ZoomWindow));
pub(crate) const SWITCH_DOCUMENT: Command = Command::new(text::SWITCH_TO_A_RECENT_DOCUMENT, || {
    Box::new(workspace::SwitchDocument)
})
.menu(text::SWITCH_DOCUMENT_MENU);
pub(crate) const SWITCH_DOCUMENT_BACK: Command = Command::new(text::SWITCH_BACKWARDS, || {
    Box::new(workspace::SwitchDocumentBack)
})
.menu(text::SWITCH_DOCUMENT_BACK_MENU);
pub(crate) const HELP: Command = Command::new(text::MDOC_HELP, || Box::new(menus::OpenHelp));
pub(crate) const KEYBOARD_SHORTCUTS: Command = Command::new(text::KEYBOARD_SHORTCUTS, || {
    Box::new(shortcuts_ui::ShowShortcuts)
})
.menu(text::KEYBOARD_SHORTCUTS_MENU);
