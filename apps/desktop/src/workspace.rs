//! The window: the `Workspace` struct with its tabs and the events its
//! documents send it (`TabEvent`); its render is in `view.rs`, what it carries
//! out in `actions.rs`. Each initialized tab owns a retained document view;
//! unopened session records allocate no editor, PDF, or conversion worker.
use crate::{
    document::{DocumentView, OcrState, known_ocr_state},
    ui::style::{self, Theme},
};
#[cfg(test)]
use gpui::div;
use gpui::{
    App, Context, Entity, EventEmitter, Focusable, Subscription, Window, actions, prelude::*,
};
use mdoc_import::ocr;
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::{
    document_id::DocumentId,
    session_store::{self, Session, TabRecord},
    settings,
};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

mod actions;
use actions::check_ocr;
mod ai;
pub(crate) use ai::companion::{CompanionContent, CompanionProposal, ReviewMark, ReviewStep};
mod app_actions;
pub(crate) use app_actions::bind as bind_app_actions;
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) mod headless_renders;
mod home;
mod notices;
pub(crate) mod palette;
mod projects;
pub(crate) use projects::ProjectLink;
pub(crate) use projects::projects_ui::{
    AddProjectName, CancelProjectName, ClearProjectName, ConfirmProjectName,
    bind_keys as bind_project_keys,
};
mod replies;
mod saving;
// The tests read these through `use super::*`.
use gpui::KeyBinding;
use palette::Palette;
use palette::PaletteConfirm;
use palette::PaletteDown;
use palette::PaletteUp;
pub(crate) use saving::quit_flow::CloseWatch;
#[cfg(test)]
use sidebar::state::{REVEAL_SLIDE, Reveal};
use std::collections::HashMap;
#[cfg(test)]
use std::collections::VecDeque;
mod shelf_source;
pub(crate) use self::bind_keys as bind_shell_keys;
pub(crate) use home::ShowHome;
pub(crate) use notices::CloseNotices;
pub(crate) use palette::{OpenPalette, PALETTE, PaletteClose};
pub(crate) use shelf_source::{ClipboardNames, ClipboardSource};
mod sidebar;
mod text;
pub(crate) use sidebar::navigation_ui::{Back, Forward};
mod vault;
pub(crate) use vault::recovery_key::OpenWithRecoveryKey;
mod view;
actions!(
    mdoc,
    [
        New,
        Open,
        Import,
        Settings,
        Quit,
        Close,
        SwitchDocument,
        SwitchDocumentBack,
        ToggleSidebar,
        ToggleTheme,
        ToggleNotices,
        OpenProject,
        NewProject,
        RetryDocument,
        PasteAiResponse,
        PseudonymizeClipboard,
        RestoreClipboard,
        CheckForUpdates,
        AddDocumentToProject,
        RenameProject,
        ChangeProjectColour,
        ProjectCategories,
        ExportProject,
        DeleteProject,
        MoveToProject,
        MoveToNewProject,
    ]
);

/// How a document reaches the window: one channel, grouped by the owner
/// that handles it. The window commands a document through its
/// methods; a document holds no handle to its window.
pub(super) enum TabEvent {
    Tabs(TabsEvent),
    Import(ImportEvent),
    Docx(DocxEvent),
    Save(SaveEvent),
    Project(ProjectEvent),
    Reply(ReplyEvent),
    /// Copy for AI, from the Share tab, the menu or its shortcut: the
    /// window checks it as the shelf does before anything is copied.
    CopyForAi,
    /// The header's Ask (Milestone 2 item 10): `with`, and for an Unfiled
    /// document, whether it moves to a new Project first, or `into` the
    /// shared Project (#1312).
    Ask {
        with: mdoc_store::settings::AskWith,
        move_to_new: bool,
        into: Option<String>,
    },
    /// Discovery or the edited paragraphs' scan finished: a Copy for AI
    /// waiting for it is checked again.
    NamesFound,
    /// A refused Copy for AI's step, from its notice or the Share tab: the
    /// window runs it unless the document is kept from AI.
    CopyStep(crate::review::refusal::Step),
    ToggleTheme,
    /// "💬 N" or "Hidden text (N)" was switched: app-wide, so every tab's
    /// Source follows and Settings keeps it.
    SourceDisplay(crate::document::preview::SourceDisplay),
}

/// Tabs to add (`LiveViews`).
pub(super) enum TabsEvent {
    New,
    Open(Vec<PathBuf>),
}

/// AI replies to the tab's document (`replies`).
pub(super) enum ReplyEvent {
    /// The reply box's text settled, or Open reply asked for it: save it,
    /// and show it when `open`.
    Settled { open: bool },
    /// Paste AI Reply: save this text and show it.
    Pasted(String),
    /// Show saved reply `id`.
    Open(String),
}

/// Conversions and OCR setup.
pub(super) enum ImportEvent {
    /// A conversion started or is waiting.
    State,
    /// A conversion ended; others waiting for the import slot may start.
    Finished,
    Ocr(OcrState),
}

/// The DOCX preview worker (`DocxQueue`).
pub(super) enum DocxEvent {
    Queue {
        path: PathBuf,
        generation: u64,
        cancel: Arc<AtomicBool>,
    },
    /// The job for `generation` ended; the worker is free.
    Finished { generation: u64 },
}

/// The document saves itself.
pub(super) enum SaveEvent {
    /// Typing paused, or Save was chosen: write what it shows now.
    Changed,
}

/// The tab's Project (`Projects`).
pub(super) enum ProjectEvent {
    /// Show this tab's Project page.
    OpenPage,
    /// The tab's mapping changed; fold it into the tab's Project.
    Changed,
    /// Add a name to the tab's Project before it appears.
    AddTerm(String),
    /// Keep a dismissed merge hint, as its (shorter, fuller) primary
    /// wordings, in the tab's Project.
    DismissHint(String, String),
    /// Move to New Project, from Share.
    MoveToNew,
    /// The breadcrumb's Project name field ends: saved, or not (Esc).
    FinishRename { save: bool },
    /// Open the move menu at this point: Share's Move to Project…, the ⋯
    /// menu's (#1312).
    MoveMenu(gpui::Point<gpui::Pixels>),
    /// Restoring or pseudonymizing found no mapping in the tab; a locked
    /// Vault may hold it.
    MappingMissing,
}
impl EventEmitter<TabEvent> for DocumentView {}

// Avoid repainting the sidebar for cursor movement, search results, or PDF raster
// completions. Only sidebar-visible metadata invalidates the parent view.
#[derive(PartialEq)]
struct SidebarStatus {
    source: Option<PathBuf>,
    loading: bool,
    queued: bool,
    importing: bool,
    error: bool,
    pseudonymized: bool,
    recognized: bool,
}
impl SidebarStatus {
    fn of(view: &DocumentView) -> Self {
        Self {
            source: view.preview.source().cloned(),
            loading: view.lifecycle.loading() || view.preview.loading(),
            queued: view.preview.queued() || view.import.auto_pending(),
            importing: view.import.busy(),
            error: view.error.is_some()
                || view.preview.failed()
                || view.import.ocr_required().is_some()
                || view.import.password_prompt().is_some(),
            pseudonymized: view.pseudonymized(),
            recognized: view.recognized(),
        }
    }
}

/// A tab is a live view the sidebar lists.
type Tab = saving::live_views::LiveView<Entity<DocumentView>>;

pub(super) struct Workspace {
    focus: gpui::FocusHandle,
    /// F6 regions the workspace owns, and the control last focused in each
    /// region when F6 left it.
    focus_regions: sidebar::focus_regions::FocusRegions<gpui::FocusHandle>,
    /// The Open Project list holds focus while open, and gives it back on
    /// close; the notices popover takes Esc.
    pub(super) project_list_focus: gpui::FocusHandle,
    pub(super) project_list_return: Option<gpui::FocusHandle>,
    pub(super) notices_focus: gpui::FocusHandle,
    tabs: saving::live_views::LiveViews<Entity<DocumentView>>,
    navigation: sidebar::navigation::History,
    /// Each shown tab's event and change subscriptions, dropped with it.
    tab_subscriptions: std::collections::HashMap<u64, Vec<Subscription>>,
    sidebar: sidebar::Sidebar<gpui::FocusHandle>,
    /// The sidebar's focus handles, scrolls and open menus.
    sidebar_controls: sidebar::SidebarControls,
    menu_focus: gpui::FocusHandle,
    /// The Keychain explainer's Continue, focused when it opens.
    explainer_focus: gpui::FocusHandle,
    explainer_focus_pending: bool,
    /// The explainer's focus group, and where focus returns when it closes.
    explainer_group: gpui::FocusHandle,
    explainer_return: Option<gpui::FocusHandle>,
    menu_return: Option<gpui::FocusHandle>,
    /// The tree's expansion, Show all and Unfiled selection.
    tree: sidebar::tree::VaultTree,
    /// The recency switcher while ⌃Tab holds it open.
    switcher: Option<sidebar::switcher::Switcher>,
    preferences: settings::Shared,
    settings: Entity<crate::settings::Panel>,
    settings_subscription: Option<Subscription>,
    theme: Rc<Cell<Theme>>,
    ready: bool,
    ocr_state: OcrState,
    /// Keyboard input source languages for recognition defaults; tests
    /// inject their own so they never read this computer's keyboards.
    pub(super) input_languages: fn() -> Vec<mdoc_import::recognition::Language>,
    /// Opens that wait for launch, or for a dialog to be answered, whole:
    /// their paths and the Project row they were dropped on.
    pending_open: Vec<(Vec<PathBuf>, Option<String>)>,
    /// Opens wait in `pending_open` for a dialog to be answered.
    awaiting_prompt: bool,
    notices: notices::Notices,
    import_busy: Arc<AtomicBool>,
    docx_queue: saving::docx_queue::DocxQueue,
    quit_flow: saving::quit_flow::QuitFlow,
    session_writer: saving::session_writer::SessionWriter<Session>,
    project_ui: projects::projects_ui::ProjectsState,
    /// The Reply page, when it shows: view state only.
    reply_page: Option<replies::ReplyPage>,
    /// Dropped folders' sheets, one at a time.
    folder_sheets: projects::folder_sheet::FolderSheets,
    folder_sheet_focus: Option<projects::folder_sheet_ui::SheetFocus>,
    /// New Project's name question, while it shows.
    new_project: Option<projects::new_project::NewProjectSheet>,
    new_project_group: gpui::FocusHandle,
    projects: projects::Projects,
    /// Documents a move or a delete is rewriting: their autosaves wait
    /// until they load again.
    moving: std::collections::HashSet<mdoc_store::document_id::VaultId>,
    /// Of those, the ones whose tab loads them again now, with that load's
    /// generation: an older load ending is not the reload.
    reloading: std::collections::HashMap<mdoc_store::document_id::VaultId, u64>,
    /// The running job started reloads and ends once they have (ADR 0130).
    reloads_hold_writer: bool,
    /// Projects a document's save is writing off the main thread: their
    /// names are not the Vault's until it ends.
    projects_saving: std::collections::HashSet<String>,
    /// The quit at the head of the queue waits for a tab list write.
    quit_waits_for_session: bool,
    /// Tests only: moves run in the order ADR 0130 forbids.
    #[cfg(test)]
    broken_order: bool,
    /// Tests only: the documents each delete question asked about, in the
    /// order the questions show.
    #[cfg(test)]
    asked_to_delete: VecDeque<Vec<mdoc_store::document_id::VaultId>>,
    /// Tests only: what each export read, as stored then.
    #[cfg(test)]
    exports_seen: Vec<projects::export_project::ExportSeen>,
    /// Moves and deletes in the writer's queue are numbered.
    next_op: u64,
    /// The window, for writes that end after the call that queued them.
    window_handle: gpui::AnyWindowHandle,
    /// The Vault, once the key opened it.
    vault: vault::vault_state::VaultState,
    /// The AI connector and its listener, while "AI connector (preview)"
    /// is on and the Vault is open.
    ai_connector: Option<crate::ai::connector::Running>,
    /// What the window knows of the AI project: the Project last viewed,
    /// the share prompt, the activity shown.
    ai: ai::project::AiProject,
    /// Hears from the connector while it runs.
    ai_poll: Option<gpui::Task<()>>,
    /// How many system file pickers had ended at the last poll: one ended
    /// since may have been up when a delivery in the next batch arrived.
    pickers_ended: u64,
    /// Where deliveries' and review requests' notifications go: the OS's,
    /// or a recorder in tests and renders.
    notifier: ai::deliveries::Notifier,
    /// An Ask waiting for its Unfiled document's move (item 10).
    ai_ask: crate::ai::ask::Waiting,
    /// Settings › Vault's key, the first-Project nudge and the wrong-key
    /// screen.
    recovery: vault::recovery_key::Recovery,
    /// Documents' autosaves, one write at a time.
    document_writer: saving::autosave::Writer,
    /// Where each imported tab's file was opened from, so opening it again
    /// finds the tab.
    admitted_from: std::collections::HashMap<u64, PathBuf>,
    /// Viewing copies of originals: an app-private cache folder, one
    /// subfolder per tab, removed when the tab closes and cleared at launch.
    viewing_root: Option<PathBuf>,
    /// Tabs whose close waits for their last text to be written: a write
    /// that fails keeps the tab and its edit.
    closing_after_write: std::collections::HashSet<u64>,
    /// Of those, the ones eviction closes: viewing one again keeps it.
    evicting: std::collections::HashSet<u64>,
    /// Views whose last write failed: their edit is in no file, so eviction
    /// keeps them.
    unsaved: std::collections::HashSet<u64>,
    /// What the window keeps for the shelf and its Copy for AI.
    shelf_source: shelf_source::ShelfSource,
    /// One admission at a time, so a repeat waits for the import before it.
    admitting: bool,
    admit_queue: std::collections::VecDeque<(Vec<PathBuf>, Option<String>)>,
    /// What a test's loud key read yields.
    #[cfg(test)]
    staged_vault: Option<Result<mdoc_vault::Vault, mdoc_store::os_key::KeyError>>,
    /// A test window's own Vault folder, removed with it.
    #[cfg(test)]
    test_vault_dir: Option<tempfile::TempDir>,
    /// Tests only: where the connector listens; `None` listens nowhere.
    #[cfg(test)]
    pub(super) ai_endpoint: Option<mdoc_mcp::ipc::Endpoint>,
    /// The sidebar's theme pick; `theme` holds what it resolves to.
    theme_choice: ThemeChoice,
    /// The theme button's tooltip reads the choice live.
    theme_tip: Rc<Cell<ThemeChoice>>,
    /// The title last given to the window: "mdoc" on Home, else the active
    /// document's.
    window_title: String,
    /// The live views the Window menu lists, as last handed to it (#1160).
    live_view_menu: Vec<crate::commands::menus::LiveViewItem>,
    /// Shared with document headers: title-bar inset and drag state.
    title: style::TitleBar,
    shell: ShellState,
    /// The sidebar's Settings row; Settings returns focus here.
    settings_focus: gpui::FocusHandle,
    _appearance: Option<Subscription>,
    _checkpoint: Option<gpui::Task<()>>,
}

impl Workspace {
    /// The theme, shared with the shelf's window.
    pub(crate) fn theme_cell(&self) -> Rc<std::cell::Cell<style::Theme>> {
        self.theme.clone()
    }
    /// Settings › General › "Show mdoc in the menu bar".
    pub(crate) fn menu_bar_setting(&self) -> bool {
        self.preferences
            .borrow()
            .current
            .as_ref()
            .is_some_and(|preferences| preferences.general.menu_bar)
    }
    /// Every prompt in the app is mdoc's own confirm dialog.
    pub(crate) fn install_prompts(&self, cx: &mut App) {
        crate::ui::confirm_dialog::install(self.theme.clone(), cx);
    }
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::empty(window, cx);
        this.ocr_state = OcrState::Checking;
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                this.close_requested(window, cx);
                false
            })
            .unwrap_or(true)
        });
        cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                crate::shelf::menu_bar::window_activated(cx);
            }
        })
        .detach();
        if !cfg!(test) {
            this.notices.load_crash(crate::app::crash::pending());
        }
        this.session_writer.set_path(if cfg!(test) {
            None
        } else {
            mdoc_store::app_paths::data_local_dir().map(|path| path.join("mdoc/session.json"))
        });
        let path = this.session_writer.path().cloned();
        // Tabs name Vault documents, so the Vault opens before they restore.
        let viewing = this.viewing_root.clone();
        let task = cx.background_executor().spawn(async move {
            // Viewing copies a crash left behind go before anything opens.
            if let Some(viewing) = viewing {
                let _ = std::fs::remove_dir_all(viewing);
            }
            let session =
                path.map_or_else(|| Ok(Session::default()), |path| session_store::load(&path));
            (session, (!cfg!(test)).then(Self::open_vault_at_launch))
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, opening) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(opening) = opening {
                    this.vault_opened(opening, window, cx);
                }
                let session = match result {
                    Ok(session) => session,
                    Err(error) => {
                        this.notices.post(text::could_not_restore_tabs(error));
                        Session::default()
                    }
                };
                this.restore(session, window, cx);
            });
        })
        .detach();
        // Reading positions can change without document notifications. Sampling
        // metadata is cheap; unchanged snapshots never touch disk.
        let config = this
            .preferences
            .borrow()
            .snapshot()
            .map(|p| p.ocr)
            .map_err(|e| e.to_string());
        let captured = config.clone();
        let check = cx
            .background_executor()
            .spawn(async move { config.and_then(|c| check_ocr(&c)) });
        cx.spawn(async move |this, cx| {
            let result = check.await;
            let _ = this.update(cx, |this, cx| {
                let current = this.preferences.borrow().snapshot();
                if current.map(|p| p.ocr).map_err(|e| e.to_string()) != captured {
                    return;
                }
                let state = match result {
                    Ok(Some(installed)) => OcrState::Ready(installed),
                    Ok(None) if ocr::SUPPORTED => OcrState::Missing,
                    Ok(None) => OcrState::Unsupported,
                    Err(error) => OcrState::Failed(error),
                };
                this.set_ocr(state, cx);
            });
        })
        .detach();
        this._checkpoint = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update_in(cx, |this, window, cx| this.checkpoint(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        this
    }
    pub(super) fn empty(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preferences = if cfg!(test) {
            settings::Store::in_memory()
        } else {
            settings::Store::open(settings::Store::default_path())
        };
        let theme = Rc::new(Cell::new(Theme::default()));
        let panel =
            cx.new(|cx| crate::settings::Panel::new(preferences.clone(), theme.clone(), cx));
        let mut sidebar = sidebar::Sidebar::default();
        sidebar.restore_width(
            preferences
                .borrow()
                .current
                .as_ref()
                .and_then(|preferences| preferences.general.sidebar_width),
        );
        let mut this = Self {
            focus: cx.focus_handle(),
            focus_regions: sidebar::focus_regions::FocusRegions::new(
                cx.focus_handle(),
                cx.focus_handle(),
            ),
            project_list_focus: cx.focus_handle(),
            project_list_return: None,
            notices_focus: cx.focus_handle(),
            tabs: saving::live_views::LiveViews::default(),
            navigation: Default::default(),
            tab_subscriptions: std::collections::HashMap::new(),
            sidebar,
            sidebar_controls: sidebar::SidebarControls::new(cx),
            menu_focus: cx.focus_handle(),
            // Continue's own handle, so it must say it is a Tab stop.
            explainer_focus: cx.focus_handle().tab_stop(true),
            explainer_focus_pending: false,
            explainer_group: cx.focus_handle(),
            explainer_return: None,
            menu_return: None,
            tree: Default::default(),
            switcher: None,
            preferences,
            settings: panel.clone(),
            settings_subscription: None,
            theme,
            ready: false,
            ocr_state: if ocr::SUPPORTED {
                OcrState::Missing
            } else {
                OcrState::Unsupported
            },
            input_languages: if cfg!(test) {
                Vec::new
            } else {
                mdoc_import::recognition::input_source_languages
            },
            pending_open: Vec::new(),
            awaiting_prompt: false,
            notices: Default::default(),
            import_busy: Arc::new(AtomicBool::new(false)),
            docx_queue: saving::docx_queue::DocxQueue::default(),
            quit_flow: saving::quit_flow::QuitFlow::default(),
            session_writer: saving::session_writer::SessionWriter::new(None),
            project_ui: Default::default(),
            reply_page: None,
            folder_sheets: Default::default(),
            folder_sheet_focus: None,
            new_project: None,
            new_project_group: cx.focus_handle(),
            projects: Default::default(),
            moving: Default::default(),
            reloading: Default::default(),
            reloads_hold_writer: false,
            projects_saving: Default::default(),
            quit_waits_for_session: false,
            #[cfg(test)]
            broken_order: false,
            #[cfg(test)]
            asked_to_delete: VecDeque::new(),
            #[cfg(test)]
            exports_seen: Vec::new(),
            next_op: 0,
            window_handle: window.window_handle(),
            vault: Default::default(),
            ai_connector: None,
            ai: Default::default(),
            ai_poll: None,
            pickers_ended: 0,
            notifier: Default::default(),
            ai_ask: Default::default(),
            recovery: Default::default(),
            document_writer: Default::default(),
            admitted_from: Default::default(),
            closing_after_write: Default::default(),
            evicting: Default::default(),
            unsaved: Default::default(),
            shelf_source: Default::default(),
            admitting: false,
            admit_queue: Default::default(),
            // Tests name their own; they never touch the real cache.
            viewing_root: if cfg!(test) {
                None
            } else {
                mdoc_store::app_paths::cache_dir().map(|dir| dir.join("mdoc").join("viewing"))
            },
            #[cfg(test)]
            staged_vault: None,
            #[cfg(test)]
            test_vault_dir: None,
            #[cfg(test)]
            ai_endpoint: None,
            theme_choice: ThemeChoice::default(),
            theme_tip: Rc::default(),
            window_title: String::new(),
            live_view_menu: Vec::new(),
            title: style::TitleBar::default(),
            shell: ShellState::new(),
            settings_focus: cx.focus_handle().tab_stop(true),
            _appearance: None,
            _checkpoint: None,
        };
        // A test window opens as launch would: with a Vault of its own.
        #[cfg(test)]
        this.use_temp_vault();
        this._appearance =
            Some(cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_theme(window, cx)
            }));
        this.theme
            .set(style::resolve(this.theme_choice, window.appearance()));
        this.settings_subscription =
            Some(
                cx.subscribe_in(&panel, window, |this, _, event, window, cx| match event {
                    crate::settings::Event::ThemeSelected(choice) => {
                        this.select_theme(*choice, window, cx);
                    }
                    crate::settings::Event::Applied | crate::settings::Event::Checked => {
                        this.refresh_model_settings(cx);
                        crate::shelf::menu_bar::set_enabled(this.menu_bar_setting(), cx);
                        // "AI connector (preview)" starts or stops at once; a
                        // model now set up lets entering documents scan.
                        this.refresh_ai_connector(window, cx);
                        this.scan_on_entry(cx);
                        // Spelling choices apply at once; views re-read them on render.
                        for view in this.tabs.iter().filter_map(|tab| tab.view()) {
                            view.update(cx, |_, cx| cx.notify());
                        }
                    }
                    // Off stops before its save lands; a failed save
                    // starts it again (#1220).
                    crate::settings::Event::AiConnectorShown(false) => this.stop_ai_connector(cx),
                    crate::settings::Event::AiConnectorShown(true) => {
                        this.refresh_ai_connector(window, cx)
                    }
                    crate::settings::Event::Finished(settings::Model::Ocr(model), result) => {
                        let selected = this.preferences.borrow().snapshot().map(|p| p.ocr);
                        if let Ok(config) = selected
                            && config.model == *model
                        {
                            let state = match result {
                                Ok(Some(installed)) => {
                                    let mut installed = installed.clone();
                                    installed.config = config;
                                    OcrState::Ready(installed)
                                }
                                Ok(None) => OcrState::Missing,
                                Err(e) => OcrState::Failed(e.clone()),
                            };
                            this.set_ocr(state, cx);
                        } else if matches!(this.ocr_state, OcrState::Installing) {
                            // A tab's setup of a model Settings no longer
                            // selects ended; the tab may be gone.
                            let state = known_ocr_state(&this.preferences, this.settings.read(cx));
                            this.set_ocr(state, cx);
                        }
                    }
                    // The detection model set up: AI project members that
                    // waited for it scan now.
                    crate::settings::Event::Finished(settings::Model::Pii(_), Ok(_)) => {
                        this.scan_on_entry(cx);
                    }
                    crate::settings::Event::Finished(_, _) => {}
                    crate::settings::Event::Compare(model) => {
                        this.open_comparison(*model, window, cx);
                    }
                    crate::settings::Event::RestartToUpdate => this.restart_to_update(window, cx),
                    crate::settings::Event::Shortcuts => this.toggle_shortcuts(window, cx),
                    crate::settings::Event::UserListsChanged(lists) => {
                        this.save_user_lists(lists.clone(), window, cx)
                    }
                    crate::settings::Event::UnlockLists => this.unlock_projects(window, cx),
                    crate::settings::Event::ShowRecoveryKey => this.show_recovery_key(cx),
                    crate::settings::Event::CategoriesUnticked(categories) => {
                        for view in this.tabs.iter().filter_map(|tab| tab.view().cloned()) {
                            view.update(cx, |view, cx| {
                                view.default_categories_unticked(categories, cx)
                            });
                        }
                    }
                }),
            );
        this
    }
    /// The live view of `document`, other than `except`: the one named for
    /// it, or one whose view holds it now.
    pub(crate) fn find_document(
        &self,
        document: &DocumentId,
        except: Option<u64>,
        cx: &App,
    ) -> Option<u64> {
        self.tabs
            .iter()
            .find(|tab| {
                Some(tab.id()) != except
                    && (tab.document() == Some(document)
                        || tab
                            .view()
                            .is_some_and(|view| view.read(cx).session.document.is(document)))
            })
            .map(|tab| tab.id())
    }
    /// A confirm dialog, the Keychain explainer, a folder sheet or New
    /// Project's sheet keeps focus until answered.
    fn dialog_up(&self, window: &Window) -> bool {
        window.has_active_prompt()
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
    }
    /// Focus the active document without changing which document is active.
    fn focus_active(&self, window: &mut Window, cx: &mut App) {
        let Some(view) = self.active_view() else {
            return;
        };
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
            return;
        }
        let view = view.read(cx);
        let focus = if view.lifecycle.source_only() {
            view.focus.clone()
        } else if view.lifecycle.loading() || view.lifecycle.unavailable() {
            self.focus.clone()
        } else {
            view.editor.read(cx).focus_handle(cx)
        };
        window.focus(&focus, cx);
    }
}

/// The collapsed rail. Narrow: the traffic lights and the expand toggle sit
/// in the title row beside the document header, above it.
const RAIL_WIDTH: f32 = 44.;
/// The expanded sidebar's width until it is dragged.
#[cfg(test)]
const PANEL_WIDTH: f32 = sidebar::DEFAULT_WIDTH;
/// How far either side of the expanded sidebar's edge a drag can start.
const RESIZE_GRIP: f32 = 3.;
const REVEAL_DELAY: Duration = Duration::from_millis(100);
const REVEAL_GRACE: Duration = Duration::from_millis(300);

/// Bring mdoc to the front after a drop from another app. Making the window
/// key is not enough on macOS: the application must activate too.
pub(crate) fn activate_for_drop(window: &mut Window, cx: &mut App) {
    cx.activate(true);
    window.activate_window();
}

/// What the switcher and Back's tooltip name a live view by.
#[derive(Clone)]
struct TabLabel {
    name: String,
    /// The badge: the source's type until it is saved as Markdown.
    format: String,
}

fn tab_label(tab: &Tab, cx: &App) -> TabLabel {
    let view = tab.view().map(|v| v.read(cx));
    let name = view.map(|v| v.display_name()).unwrap_or_else(|| {
        if tab.record().document.is_some()
            && let Some(name) = &tab.record().name
        {
            return name.clone();
        }
        // A source tab not loaded yet shows the name it showed when the
        // session was saved; a record from before that is named as a
        // finished conversion is (`DocumentSession::display_name`).
        if tab.record().source_only
            && let Some(name) = &tab.record().name
        {
            return name.clone();
        }
        // A source tab is named as its conversion will be.
        let path = tab
            .record()
            .source_only
            .then(|| (tab.record().attachment.as_ref()).map(|path| path.with_extension("md")));
        path.flatten()
            .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
            .or_else(|| tab.record().name.clone())
            .unwrap_or_else(|| "Untitled.md".into())
    });
    let source = view
        .and_then(|v| v.preview.attachment().or(v.preview.source()))
        .or(tab.record().attachment.as_ref());
    let format = source
        .and_then(|source| source.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("md")
        .to_uppercase();
    TabLabel { name, format }
}

/// The window's title: a dev profile says so, so its window is never taken
/// for the user's own mdoc.
fn window_title(title: String, dev_profile: bool) -> String {
    if dev_profile {
        text::dev_profile_title(title)
    } else {
        title
    }
}

/// The palette, Home, Notices and the shortcuts sheet from the shortcuts
/// table; moving inside the palette's list stays here. Esc in
/// `Workspace` is the window's own, below every deeper one: Notices opens
/// without focus, so Esc closes it from a source-only tab or Home.
pub(crate) fn bind_keys(cx: &mut App) {
    let bindings =
        crate::commands::shortcuts::bindings(crate::commands::shortcuts::Owner::Shell, cx);
    cx.bind_keys(bindings);
    cx.bind_keys([
        KeyBinding::new("up", PaletteUp, Some(PALETTE)),
        KeyBinding::new("down", PaletteDown, Some(PALETTE)),
        KeyBinding::new("enter", PaletteConfirm, Some(PALETTE)),
    ]);
    sidebar::switcher_ui::bind_keys(cx);
}

struct ShellState {
    pub palette: Option<Palette>,
    /// The keyboard shortcuts sheet, while open.
    pub shortcuts: Option<palette::shortcuts_ui::ShortcutsSheet>,
    /// Files dragged over the window while Home shows: how many are droppable.
    pub drop: Option<usize>,
    pub drop_generation: u64,
    home_scroll: gpui::ScrollHandle,
    /// Focus for each card of Home's Continue strip, by its document's or
    /// reply's id, so a reordered strip keeps focus on the same card.
    pub card_focus: std::cell::RefCell<HashMap<String, gpui::FocusHandle>>,
    /// The pointer is over Home's "Documents stay on this Mac".
    pub local_note_hover: bool,
}

impl ShellState {
    pub fn new() -> Self {
        Self {
            palette: None,
            shortcuts: None,
            drop: None,
            drop_generation: 0,
            home_scroll: gpui::ScrollHandle::new(),
            card_focus: Default::default(),
            local_note_hover: false,
        }
    }
}

#[cfg(test)]
mod tests;

/// Keyboard reach: F6 regions, Tab from the window, and the
/// sidebar's tab list.
impl Workspace {
    /// The visible document owns chrome undo, unless a field or modal surface
    /// owns focus. Deeper editor/popup handlers run before this fallback.
    fn history_document(&self, window: &Window, cx: &App) -> Option<Entity<DocumentView>> {
        if self.showing_overview(cx)
            || self.settings.read(cx).open
            || self.project_ui.list_open
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
            || self.shell.palette.is_some()
            || self.shell.shortcuts.is_some()
            || window
                .context_stack()
                .iter()
                .any(|context| context.contains("TextInput"))
        {
            return None;
        }
        self.active_view()
    }
}
