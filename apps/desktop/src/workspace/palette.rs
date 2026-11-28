//! The command palette, with its search over the Vault and the shown page's
//! actions, and the keyboard shortcuts sheet it opens. The palette's rows
//! and its struct live here; its view, actions and words each have their file.
use crate::commands::shortcuts;
use crate::commands::table;
use crate::document::markdown_search::SearchInput;
use gpui::Entity;
use gpui::Subscription;
use gpui::actions;
use mdoc_store::document_id::VaultId;
mod actions;
pub(crate) mod shortcuts_ui;
mod text;
mod view;

actions!(
    mdoc_shell,
    [
        OpenPalette,
        PaletteUp,
        PalettePrevious,
        PaletteDown,
        PaletteConfirm,
        PaletteClose,
    ]
);

pub(crate) const PALETTE: &str = "CommandPalette";

pub(super) struct Palette {
    input: Entity<SearchInput>,
    selected: usize,
    /// The list's scroll, which keeps the selected row in view.
    scroll: gpui::ScrollHandle,
    return_focus: Option<gpui::FocusHandle>,
    _changes: Subscription,
}

/// What a palette row does when chosen.
#[derive(Clone)]
enum Command {
    NewTab,
    OpenFiles,
    PasteText,
    Pseudonymize,
    PseudonymizeClipboard,
    RestoreReply,
    CopyForAi,
    CopyMarkdown,
    Save,
    ExportMarkdown,
    Close,
    BeforeAfter,
    /// The active document's What you did; no key of its own.
    DecisionHistory,
    /// The active document's Notes tab.
    Notes,
    /// The AI reply the active document was promoted from.
    SourceReply,
    ExpandIdentities,
    CollapseIdentities,
    /// Stop the active tab's running conversion (#950); no key of its own.
    CancelConversion,
    ToggleSidebar,
    OpenProject,
    NewProject,
    Home,
    Settings,
    SettingsCategory(crate::settings::Category),
    Shortcuts,
    Theme,
    /// The active document's heading whose line starts at this offset.
    Heading(usize),
    Document(VaultId),
    Project(String),
    /// An action of the Project page or Reply page shown.
    Page(PageAction),
}

/// The header and ⋯ actions of the page shown, in the palette too.
#[derive(Clone, Copy)]
pub(super) enum PageAction {
    RenameProject,
    ProjectColour,
    ProjectCategories,
    AddDocument,
    ExportProject,
    DeleteProject,
    CopyReply,
    PromoteReply,
    DeleteReply,
}

impl Command {
    /// The action this command runs, when it is one: the palette shows its
    /// key from the shortcuts table.
    fn action(&self) -> Option<Box<dyn gpui::Action>> {
        Some(match self {
            Command::NewTab => table::NEW.action(),
            Command::OpenFiles => table::OPEN.action(),
            Command::Pseudonymize => table::PSEUDONYMIZE.action(),
            Command::PseudonymizeClipboard => table::PSEUDONYMIZE_CLIPBOARD.action(),
            Command::RestoreReply => table::PASTE_AI_REPLY.action(),
            Command::CopyForAi => table::COPY_FOR_AI.action(),
            Command::CopyMarkdown => table::COPY_MARKDOWN.action(),
            Command::Save => table::SAVE.action(),
            Command::ExportMarkdown => table::EXPORT_MARKDOWN.action(),
            Command::Close => table::CLOSE.action(),
            Command::BeforeAfter => table::SIDE_BY_SIDE.action(),
            Command::ToggleSidebar => table::TOGGLE_SIDEBAR.action(),
            Command::OpenProject => table::OPEN_PROJECT.action(),
            Command::NewProject => table::NEW_PROJECT.action(),
            Command::Home => table::HOME.action(),
            Command::Settings => table::SETTINGS.action(),
            Command::Shortcuts => table::KEYBOARD_SHORTCUTS.action(),
            Command::Notes => table::NOTES.action(),
            Command::SourceReply => table::OPEN_SOURCE_REPLY.action(),
            Command::PasteText
            | Command::DecisionHistory
            | Command::ExpandIdentities
            | Command::CollapseIdentities
            | Command::CancelConversion
            | Command::Theme
            | Command::SettingsCategory(_)
            | Command::Heading(_)
            | Command::Document(_)
            | Command::Project(_)
            | Command::Page(_) => return None,
        })
    }

    /// The key that runs this command on this platform, if any.
    fn shortcut(&self) -> Option<String> {
        self.action().and_then(|action| shortcuts::label(&*action))
    }
}

struct Row {
    section: &'static str,
    label: String,
    detail: Option<String>,
    command: Command,
}

/// The key `action` runs from, as the shortcuts table writes it.
pub(super) fn key(action: &dyn gpui::Action) -> String {
    shortcuts::label(action).unwrap_or_default()
}
