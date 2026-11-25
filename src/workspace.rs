//! Window-level tabs. Each initialized tab owns a retained document view;
//! unopened session records allocate no editor, PDF, or conversion worker.
use crate::{
    comparison, comparison_ui,
    document_view::{
        Background, DocumentView, DocumentViewDependencies, OcrState, known_ocr_state,
    },
    pii::ui::PiiApplyAll,
    settings_ui,
    style::{self, Theme},
    ui,
};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, Context, Entity, EventEmitter, Focusable,
    PromptLevel, Subscription, Window, WindowBounds, WindowOptions, actions, div, prelude::*, px,
    size,
};
use mdoc_import::{import, model_work, ocr};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::{
    document::{self, Document},
    document_id::DocumentId,
    session_store::{self, Session, TabRecord},
    settings,
};
use std::{
    cell::Cell,
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

mod ai_deliveries;
mod ai_project;
mod ai_project_ui;
mod autosave;
mod document_writer;
mod docx_queue;
use docx_queue::{DocxEffect, DocxJob};
mod focus_regions;
mod folder_sheet;
mod folder_sheet_ui;
#[cfg(all(
    debug_assertions,
    feature = "headless-renders",
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) mod headless_renders;
mod home;
mod intake;
mod keychain_ui;
mod navigation;
use navigation::Arrival;
mod navigation_ui;
mod new_project;
mod notices;
pub(crate) use navigation_ui::{Back, Forward};
mod project_page;
mod projects;
pub(crate) use projects::ProjectLink;
mod export_project;
mod projects_ui;
mod quit_flow;
mod recovery_key;
mod saved_replies;
pub(crate) use quit_flow::CloseWatch;
use quit_flow::QuitEffect;
pub(crate) use recovery_key::OpenWithRecoveryKey;
mod session_writer;
mod shelf_source;
pub(crate) use shelf_source::{ClipboardNames, ClipboardSource};
mod shell;
pub(crate) mod shortcuts_ui;
use session_writer::SessionEffect;
mod live_views;
mod sidebar;
mod switcher;
mod switcher_ui;
mod tree;
mod tree_ui;
mod user_lists;
mod vault_state;
mod vault_ui;
use live_views::LiveViewsEffect;
pub(crate) use projects_ui::{
    AddProjectName, CancelProjectName, ClearProjectName, ConfirmProjectName,
    bind_keys as bind_project_keys,
};
pub(crate) use shell::{
    CloseNotices, OpenPalette, PALETTE, PaletteClose, ShowHome, bind_keys as bind_shell_keys,
};
use sidebar::{REVEAL_SLIDE, Reveal, SLIDE, SidebarEffect};
mod app_actions;
pub(crate) use app_actions::bind as bind_app_actions;
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
    ToggleTheme,
    /// "💬 N" or "Hidden text (N)" was switched: app-wide, so every tab's
    /// Source follows and Settings keeps it.
    SourceDisplay(crate::preview::SourceDisplay),
}

/// Tabs to add (`LiveViews`).
pub(super) enum TabsEvent {
    New,
    Open(Vec<PathBuf>),
}

/// AI replies to the tab's document (`saved_replies`).
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
type Tab = live_views::LiveView<Entity<DocumentView>>;

/// A drag of the expanded sidebar's edge; the sidebar itself follows it.
#[derive(Clone)]
struct SidebarResize;
impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

pub(super) struct Workspace {
    focus: gpui::FocusHandle,
    /// F6 regions the workspace owns, and the control last focused in each
    /// region when F6 left it.
    focus_regions: focus_regions::FocusRegions<gpui::FocusHandle>,
    /// The Open Project list holds focus while open, and gives it back on
    /// close; the notices popover takes Esc.
    pub(super) project_list_focus: gpui::FocusHandle,
    pub(super) project_list_return: Option<gpui::FocusHandle>,
    pub(super) notices_focus: gpui::FocusHandle,
    tabs: live_views::LiveViews<Entity<DocumentView>>,
    navigation: navigation::History,
    back_focus: gpui::FocusHandle,
    forward_focus: gpui::FocusHandle,
    /// Each shown tab's event and change subscriptions, dropped with it.
    tab_subscriptions: std::collections::HashMap<u64, Vec<Subscription>>,
    sidebar: sidebar::Sidebar<gpui::FocusHandle>,
    reveal_focus: gpui::FocusHandle,
    rail_toggle_focus: gpui::FocusHandle,
    rail_new_focus: gpui::FocusHandle,
    panel_toggle_focus: gpui::FocusHandle,
    panel_new_focus: gpui::FocusHandle,
    /// Home's row in the collapsed rail and the expanded list: where F6
    /// enters the sidebar on Home.
    rail_home_focus: gpui::FocusHandle,
    panel_home_focus: gpui::FocusHandle,
    menu_focus: gpui::FocusHandle,
    /// The Keychain explainer's Continue, focused when it opens.
    explainer_focus: gpui::FocusHandle,
    explainer_focus_pending: bool,
    /// The explainer's focus group, and where focus returns when it closes.
    explainer_group: gpui::FocusHandle,
    explainer_return: Option<gpui::FocusHandle>,
    menu_return: Option<gpui::FocusHandle>,
    sidebar_scroll: gpui::ScrollHandle,
    /// Each tree row's focus handle, by row.
    tree_row_focus: std::cell::RefCell<std::collections::HashMap<tree::RowKey, gpui::FocusHandle>>,
    /// The tree's expansion, Show all and Unfiled selection.
    tree: tree::VaultTree,
    /// The recency switcher while ⌃Tab holds it open, its focus, and where
    /// focus returns when it is cancelled.
    switcher: Option<switcher::Switcher>,
    switcher_focus: gpui::FocusHandle,
    switcher_return: Option<gpui::FocusHandle>,
    preferences: settings::Shared,
    settings: Entity<settings_ui::Panel>,
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
    /// A document row's menu: its document, and where it opened.
    document_menu: Option<(mdoc_store::document_id::VaultId, gpui::Point<gpui::Pixels>)>,
    import_busy: Arc<AtomicBool>,
    docx_queue: docx_queue::DocxQueue,
    quit_flow: quit_flow::QuitFlow,
    session_writer: session_writer::SessionWriter<Session>,
    project_ui: projects_ui::ProjectsState,
    /// The Reply page, when it shows: view state only.
    reply_page: Option<saved_replies::ReplyPage>,
    /// Dropped folders' sheets, one at a time.
    folder_sheets: folder_sheet::FolderSheets,
    folder_sheet_focus: Option<folder_sheet_ui::SheetFocus>,
    /// New Project's name question, while it shows.
    new_project: Option<new_project::NewProjectSheet>,
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
    exports_seen: Vec<export_project::ExportSeen>,
    /// Moves and deletes in the writer's queue are numbered.
    next_op: u64,
    /// The window, for writes that end after the call that queued them.
    window_handle: gpui::AnyWindowHandle,
    /// The Vault, once the key opened it.
    vault: vault_state::VaultState,
    /// The AI connector and its listener, while "AI connector (preview)"
    /// is on and the Vault is open.
    ai_connector: Option<crate::ai_connector::Running>,
    /// What the window knows of the AI project: the Project last viewed,
    /// the share prompt, the activity shown.
    ai: ai_project::AiProject,
    /// Hears from the connector while it runs.
    ai_poll: Option<gpui::Task<()>>,
    /// Where deliveries' and review requests' notifications go: the OS's,
    /// or a recorder in tests and renders.
    notifier: ai_deliveries::Notifier,
    /// Settings › Vault's key, the first-Project nudge and the wrong-key
    /// screen.
    recovery: recovery_key::Recovery,
    /// Documents' autosaves, one write at a time.
    document_writer: autosave::Writer,
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
    /// The tab whose Copy for AI waits for its writes to land before the
    /// Vault's outbound check runs, with the clipboard's text when it was
    /// asked.
    copy_for_ai_waiting: Option<(u64, Option<String>)>,
    /// One admission at a time, so a repeat waits for the import before it.
    admitting: bool,
    admit_queue: std::collections::VecDeque<(Vec<PathBuf>, Option<String>)>,
    /// What a test's loud key read yields.
    #[cfg(test)]
    staged_vault: Option<Result<mdoc_vault::Vault, mdoc_store::os_key::KeyError>>,
    /// A test window's own Vault folder, removed with it.
    #[cfg(test)]
    test_vault_dir: Option<tempfile::TempDir>,
    /// The sidebar's theme pick; `theme` holds what it resolves to.
    theme_choice: ThemeChoice,
    /// The theme button's tooltip reads the choice live.
    theme_tip: Rc<Cell<ThemeChoice>>,
    /// The title last given to the window: "mdoc" on Home, else the active
    /// document's.
    window_title: String,
    /// Shared with document headers: title-bar inset and drag state.
    title: style::TitleBar,
    shell: shell::ShellState,
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
        crate::confirm_dialog::install(self.theme.clone(), cx);
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
                crate::menu_bar::window_activated(cx);
            }
        })
        .detach();
        if !cfg!(test) {
            this.notices.load_crash(crate::crash::pending());
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
                        this.notices
                            .post(format!("Could not restore tabs: {error}"));
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
        let panel = cx.new(|cx| settings_ui::Panel::new(preferences.clone(), theme.clone(), cx));
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
            focus_regions: focus_regions::FocusRegions::new(cx.focus_handle(), cx.focus_handle()),
            project_list_focus: cx.focus_handle(),
            project_list_return: None,
            notices_focus: cx.focus_handle(),
            tabs: live_views::LiveViews::default(),
            navigation: Default::default(),
            back_focus: cx.focus_handle().tab_stop(true),
            forward_focus: cx.focus_handle().tab_stop(true),
            tab_subscriptions: std::collections::HashMap::new(),
            sidebar,
            reveal_focus: cx.focus_handle(),
            rail_toggle_focus: cx.focus_handle().tab_stop(true),
            rail_new_focus: cx.focus_handle().tab_stop(true),
            panel_toggle_focus: cx.focus_handle().tab_stop(true),
            panel_new_focus: cx.focus_handle().tab_stop(true),
            rail_home_focus: cx.focus_handle().tab_stop(true),
            panel_home_focus: cx.focus_handle().tab_stop(true),
            menu_focus: cx.focus_handle(),
            // Continue's own handle, so it must say it is a Tab stop.
            explainer_focus: cx.focus_handle().tab_stop(true),
            explainer_focus_pending: false,
            explainer_group: cx.focus_handle(),
            explainer_return: None,
            menu_return: None,
            sidebar_scroll: gpui::ScrollHandle::new(),
            tree_row_focus: Default::default(),
            tree: Default::default(),
            switcher: None,
            switcher_focus: cx.focus_handle(),
            switcher_return: None,
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
            document_menu: None,
            import_busy: Arc::new(AtomicBool::new(false)),
            docx_queue: docx_queue::DocxQueue::default(),
            quit_flow: quit_flow::QuitFlow::default(),
            session_writer: session_writer::SessionWriter::new(None),
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
            notifier: Default::default(),
            recovery: Default::default(),
            document_writer: Default::default(),
            admitted_from: Default::default(),
            closing_after_write: Default::default(),
            evicting: Default::default(),
            unsaved: Default::default(),
            copy_for_ai_waiting: None,
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
            theme_choice: ThemeChoice::default(),
            theme_tip: Rc::default(),
            window_title: String::new(),
            title: style::TitleBar::default(),
            shell: shell::ShellState::new(),
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
                    settings_ui::Event::ThemeSelected(choice) => {
                        this.select_theme(*choice, window, cx);
                    }
                    settings_ui::Event::Applied | settings_ui::Event::Checked => {
                        this.refresh_model_settings(cx);
                        crate::menu_bar::set_enabled(this.menu_bar_setting(), cx);
                        // "AI connector (preview)" starts or stops at once; a
                        // model now set up lets entering documents scan.
                        this.refresh_ai_connector(window, cx);
                        this.scan_on_entry(cx);
                        // Spelling choices apply at once; views re-read them on render.
                        for view in this.tabs.iter().filter_map(|tab| tab.view()) {
                            view.update(cx, |_, cx| cx.notify());
                        }
                    }
                    settings_ui::Event::Finished(settings::Model::Ocr(model), result) => {
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
                    settings_ui::Event::Finished(settings::Model::Pii(_), Ok(_)) => {
                        this.scan_on_entry(cx);
                    }
                    settings_ui::Event::Finished(_, _) => {}
                    settings_ui::Event::Compare(model) => {
                        this.open_comparison(*model, window, cx);
                    }
                    settings_ui::Event::RestartToUpdate => this.restart_to_update(window, cx),
                    settings_ui::Event::Shortcuts => this.toggle_shortcuts(window, cx),
                    settings_ui::Event::UserListsChanged(lists) => {
                        this.save_user_lists(lists.clone(), window, cx)
                    }
                    settings_ui::Event::UnlockLists => this.unlock_projects(window, cx),
                    settings_ui::Event::ShowRecoveryKey => this.show_recovery_key(cx),
                    settings_ui::Event::CategoriesUnticked(categories) => {
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

    fn refresh_model_settings(&mut self, cx: &mut Context<Self>) {
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.ocr,
            Err(e) => {
                self.notices.post(e.to_string());
                cx.notify();
                return;
            }
        };
        let model = settings::Model::Ocr(config.model);
        let status = self.settings.read(cx).statuses[settings::Model::ALL
            .iter()
            .position(|m| *m == model)
            .unwrap()]
        .clone();
        match status {
            settings_ui::Status::Missing => {
                self.set_ocr(OcrState::Missing, cx);
                return;
            }
            settings_ui::Status::Damaged(e) | settings_ui::Status::Unavailable(e) => {
                self.set_ocr(
                    if ocr::SUPPORTED {
                        OcrState::Failed(e)
                    } else {
                        OcrState::Unsupported
                    },
                    cx,
                );
                return;
            }
            _ => {}
        }
        // Parameter-only changes reuse the already checked model; setup publishes
        // its checked paths directly, so it cannot race a consent continuation.
        if let OcrState::Ready(installed) = &self.ocr_state
            && installed.config.model == config.model
        {
            let mut installed = installed.clone();
            installed.config = config;
            self.set_ocr(OcrState::Ready(installed), cx);
            return;
        }
        let cached = self
            .settings
            .read(cx)
            .ocr_installations
            .iter()
            .find(|i| i.config.model == config.model)
            .cloned();
        if let Some(mut installed) = cached {
            installed.config = config;
            self.set_ocr(OcrState::Ready(installed), cx);
            return;
        }
        if model_work::busy() {
            return;
        }
        let captured = config.clone();
        let task = cx
            .background_executor()
            .spawn(async move { check_ocr(&config) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .preferences
                    .borrow()
                    .snapshot()
                    .is_ok_and(|p| p.ocr != captured)
                {
                    return;
                }
                let state = match result {
                    Ok(Some(i)) => OcrState::Ready(i),
                    Ok(None) if ocr::SUPPORTED => OcrState::Missing,
                    Ok(None) => OcrState::Unsupported,
                    Err(e) => OcrState::Failed(e),
                };
                this.set_ocr(state, cx);
            });
        })
        .detach();
    }
    /// The launch-time update check, unless the user turned it off.
    pub(super) fn check_for_updates_at_launch(&self, cx: &mut Context<Self>) {
        let updates = self
            .preferences
            .borrow()
            .snapshot()
            .unwrap_or_default()
            .updates;
        if updates.check_automatically {
            crate::updater::spawn_check(updates.include_prereleases, true, cx);
        }
    }
    /// "mdoc had a problem last time it ran", with the exact report before
    /// anything leaves the Mac. Send and Copy also discard it.
    fn crash_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (report, open) = self
            .notices
            .crash()
            .map(|(report, open)| (report.to_owned(), open))?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = style::markdown_style(theme).alert_warning;
        let done = |this: &mut Self, cx: &mut Context<Self>| {
            if let Some(effect) = this.notices.finish_crash() {
                this.carry_out_notices(effect);
            }
            cx.notify();
        };
        let action = |id: &'static str, label: &'static str| {
            ui::control(id, label, theme, true)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .text_size(px(12.))
        };
        let send_report = report.clone();
        let copy_report = report.clone();
        Some(
            div()
                .id("crash-bar")
                .when(cfg!(test), |v| v.debug_selector(|| "crash-bar".into()))
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .border_b_0()
                .border_color(palette.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(palette.header_fg)
                                .child("mdoc had a problem last time it ran."),
                        )
                        .when(!open, |v| {
                            v.child(action("crash-preview", "See what would be sent").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.notices.preview_crash();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(open, |v| {
                            v.child(
                                action("crash-send", "Send…")
                                    .tooltip(style::tooltip(
                                        "Opens a new GitHub issue with this report for you to check and submit."
                                            .into(),
                                        theme,
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.open_url(&crate::crash::issue_url(&send_report));
                                        done(this, cx);
                                    })),
                            )
                            .child(action("crash-copy", "Copy").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copy_report.clone(),
                                    ));
                                    done(this, cx);
                                },
                            )))
                        })
                        .child(
                            action("crash-dismiss", "Dismiss")
                                .on_click(cx.listener(move |this, _, _, cx| done(this, cx))),
                        ),
                )
                .when(open, |v| {
                    v.child(
                        div()
                            .id("crash-report")
                            .when(cfg!(test), |v| v.debug_selector(|| "crash-report".into()))
                            .max_h(px(220.))
                            .overflow_y_scroll()
                            .p_2()
                            .rounded_md()
                            .bg(palette.bg)
                            .border_1()
                            .border_color(palette.border)
                            .font_family("Menlo")
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child(report),
                    )
                })
                .into_any_element(),
        )
    }

    /// Restart now: the ordinary quit — unsaved-change prompts,
    /// session save — swaps the staged update in, then mdoc relaunches.
    pub(super) fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::updater::request_restart(cx);
        self.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        self.quit(window, cx);
        // Quitting can't start now (a prompt is open): don't relaunch later.
        if self.quit_flow.may_quit() {
            crate::updater::cancel_restart(cx);
        }
    }
    pub(super) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        self.settings.update(cx, |panel, cx| panel.show(window, cx));
    }
    pub(super) fn open_comparison(
        &mut self,
        model: settings::Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.active_view() else {
            self.notices
                .post("Open a document before comparing models.");
            cx.notify();
            return;
        };
        let defaults = match self.preferences.borrow().snapshot() {
            Ok(p) => p,
            Err(e) => {
                self.notices.post(e.to_string());
                cx.notify();
                return;
            }
        };
        let mut protected: Vec<_> = [
            view.read(cx).session.source.clone(),
            view.read(cx).preview.attachment().cloned(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let (source, input) = match model {
            settings::Model::Ocr(_) => {
                let source = view
                    .read(cx)
                    .preview
                    .attachment()
                    .cloned()
                    .or_else(|| view.read(cx).session.source.clone());
                let Some(source) = source else {
                    self.notices
                        .post("OCR comparison requires an attached original PDF.");
                    cx.notify();
                    return;
                };
                (Some(source), None)
            }
            settings::Model::Pii(_) => {
                if !view.read(cx).can_copy_markdown() {
                    self.notices
                        .post("Convert the document to Markdown before comparing detection.");
                    cx.notify();
                    return;
                }
                (
                    None,
                    Some(comparison::Input::Markdown(Arc::new(
                        view.read(cx).editor.read(cx).text().to_string(),
                    ))),
                )
            }
        };
        self.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        let owner = cx.entity().downgrade();
        let theme = self.theme.clone();
        let panel = self.settings.clone();
        let source_for_worker = source.clone();
        if let Some(source) = source {
            protected.insert(0, source);
        }
        let task = cx.background_executor().spawn(async move {
            let input = match input {
                Some(i) => i,
                None => comparison::Input::pdf(source_for_worker.as_ref().unwrap())?,
            };
            let hash = input.hash();
            Ok::<_, String>((input, hash))
        });
        let bounds = Bounds::centered(None, size(px(1000.), px(750.)), cx);
        cx.spawn(async move |this, cx| match task.await {
            Ok((input, hash)) => {
                if this.upgrade().is_none() {
                    return;
                }
                let _ = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    move |window, cx| {
                        window.set_window_title(if matches!(input, comparison::Input::Pdf(_)) {
                            "Compare OCR models — mdoc"
                        } else {
                            "Compare pseudonymization models — mdoc"
                        });
                        let view = cx.new(|cx| {
                            comparison_ui::View::new(
                                (input, hash),
                                defaults,
                                theme,
                                owner,
                                panel,
                                protected,
                                cx,
                            )
                        });
                        window.focus(&view.read(cx).focus_handle(cx), cx);
                        view
                    },
                );
            }
            Err(e) => {
                let _ = this.update(cx, |this, cx| {
                    this.notices.post(e);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn restore(
        &mut self,
        session: Session,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar
            .restore(session.sidebar_visible, session.sidebar_choice);
        self.theme_choice = session.theme_choice;
        self.sync_app_appearance(cx);
        self.apply_theme(window, cx);
        self.tree.restore(session.expanded_projects);
        self.load_projects(cx);
        let selected = session.active;
        // Session paths were normalized when opened/saved. Do not stat every file
        // during startup; missing files are diagnosed when activated.
        for record in session.tabs {
            if self.tabs.iter().any(|tab| {
                if record.source_only {
                    tab.record().attachment == record.attachment
                } else {
                    !tab.record().source_only && record.same_document(tab.record())
                }
            }) {
                continue;
            }
            let vault = record.document.is_some();
            let id = self.push(record);
            if vault {
                self.name_from_vault(id);
            }
        }
        self.ready = true;
        // Nothing to restore: Home shows over a hidden placeholder tab until
        // it gets content.
        if self.tabs.is_empty() {
            self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            });
        }
        let active = self.tabs[selected.min(self.tabs.len() - 1)].id();
        self.activate(active, window, cx);
        // Sealed with another key: Home asks for the recovery key first.
        self.recovery_screen_opened(window, cx);
        for (paths, project) in std::mem::take(&mut self.pending_open) {
            self.open_paths_into(paths, project, window, cx);
        }
        cx.notify();
    }

    fn push(&mut self, record: TabRecord) -> u64 {
        self.tabs.push(record)
    }

    pub(super) fn active_view(&self) -> Option<Entity<DocumentView>> {
        self.tabs.active_tab()?.view().cloned()
    }

    fn initialize(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.position(id) else {
            return;
        };
        if self.tabs[index].view().is_some() {
            return;
        }
        let names: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| !tab.record().blank || tab.record().name.is_some())
            .map(|tab| tab_label(tab, cx).name)
            .collect();
        self.tabs.name_blank_tabs(names);
        let record = self.tabs[index].record().clone();
        let document = record.document();
        let theme = self.theme.clone();
        let import_busy = self.import_busy.clone();
        let view = cx.new(|cx| {
            let mut view = DocumentView::new(
                DocumentViewDependencies {
                    closing: self.quit_flow.watch(),
                    preferences: self.preferences.clone(),
                    model_panel: self.settings.clone(),
                    theme,
                    import_busy,
                    ocr: self.ocr_state.clone(),
                    title: self.title.clone(),
                    input_languages: self.input_languages,
                },
                window,
                cx,
            );
            view.active = id == self.tabs.active();
            view.session.name = record.blank.then(|| record.name.clone()).flatten();
            view.lifecycle.restore(
                document.is_some(),
                record.source_only,
                record.blank_disposable,
            );
            view.import.queue_auto(
                record.source_only
                    && record.attachment.as_ref().is_some_and(|path| {
                        path.extension().is_some_and(|ext| {
                            ext.eq_ignore_ascii_case("pdf") || ext.eq_ignore_ascii_case("docx")
                        })
                    }),
            );
            view.preview.restore(
                record.attachment.clone(),
                record.preview_visible,
                record.preview_split,
                record
                    .preview_zoom
                    .map(|zoom| (record.preview_page, zoom, record.preview_fit)),
            );
            view
        });
        let events = cx.subscribe_in(&view, window, move |this, _, event, window, cx| {
            this.event(id, event, window, cx);
        });
        let mut status = SidebarStatus::of(view.read(cx));
        let changes = cx.observe(&view, move |_, view, cx| {
            let next = SidebarStatus::of(view.read(cx));
            if next != status {
                status = next;
                cx.notify();
            }
        });
        self.tabs.open(id, view.clone());
        self.tab_subscriptions.insert(id, vec![events, changes]);
        if let Some(document) = document {
            self.load_document(id, document, record, window, cx);
        } else if record.source_only
            && let Some(path) = record.attachment
        {
            view.update(cx, |view, cx| view.open_source(path, window, cx));
        }
    }

    /// Load tab `id`'s document from the Vault; the load's generation when
    /// one started.
    fn load_document(
        &mut self,
        id: u64,
        document: DocumentId,
        record: TabRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        #[cfg(test)]
        {
            self.project_ui.member_reads += 1;
        }
        let view = self.tabs.get(id).and_then(|tab| tab.view().cloned())?;
        let (generation, notes_writes) = view.update(cx, |view, cx| {
            let generation = view.lifecycle.begin_load();
            view.error = None;
            cx.notify();
            // Notes written after this read are newer than what it reads.
            (generation, view.notes.writes())
        });
        let weak = view.downgrade();
        let loading = document.vault().clone();
        let task: gpui::Task<Result<Loaded, String>> = match document {
            DocumentId::Vault(document) => match self.vault.vault().cloned() {
                Some(vault) => {
                    let viewing = self.viewing_dir(id);
                    cx.background_executor()
                        .spawn(async move { load_from_vault(&vault, document, viewing) })
                }
                None => {
                    match self.vault.unavailable() {
                        // No Vault this run: say why where the text would be.
                        Some(why) => {
                            let error = format!("This document can't open. {why}");
                            view.update(cx, |view, cx| {
                                view.lifecycle.finish_load(generation, false);
                                view.error = Some(error);
                                cx.notify();
                            });
                        }
                        // It loads once the key opens the Vault.
                        None => self.vault.wait(id),
                    }
                    return None;
                }
            },
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.move_reload_ended(&loading, generation, window, cx);
                if !this.tabs.contains(id) {
                    // Its viewing copy may have been made after the tab went.
                    if let Some(dir) = this.viewing_dir(id) {
                        let _ = std::fs::remove_dir_all(dir);
                    }
                    return;
                }
                if let Ok(loaded) = &result {
                    this.tabs.set_document(id, loaded.document.id());
                }
                // A dialog, or the recency switcher, keeps the keyboard while
                // the document loads behind it.
                let dialog = this.dialog_up(window) || this.switcher.is_some();
                let _ = weak.update(cx, |view, cx| {
                    if !view.lifecycle.finish_load(generation, result.is_ok()) {
                        return;
                    }
                    match result {
                        Ok(Loaded { document, vault }) => {
                            let review = document.review.clone();
                            view.session.replace(document);
                            let mut copy = None;
                            let mut names = None;
                            let mut source_error = None;
                            if let Some(vault) = vault {
                                source_error = vault.source_error;
                                names = vault.names;
                                view.load_notes(loading.as_str(), vault.notes, notes_writes, cx);
                                view.from_reply = (vault.from_reply).map(|id| {
                                    crate::document_view::notes::FromReply { id, gone: false }
                                });
                                view.session.name = Some(vault.title);
                                view.session.follows_first_line = vault.follows;
                                view.session.warning = vault.warning;
                                view.session.left_out = vault.left_out;
                                copy = vault.copy;
                                // Admitted but not converted: its source shows
                                // and converts as an opened file does.
                                if vault.unconverted
                                    && let Some(source) = copy.clone()
                                {
                                    view.reset_pii(cx);
                                    view.import
                                        .queue_auto(source.extension().is_some_and(|ext| {
                                            ext.eq_ignore_ascii_case("pdf")
                                                || ext.eq_ignore_ascii_case("docx")
                                        }));
                                    view.open_source(source, window, cx);
                                    if view.active {
                                        view.try_auto_convert(window, cx);
                                    }
                                    cx.notify();
                                    return;
                                }
                                view.session.source = copy.clone();
                            }
                            view.reset_pii(cx);
                            view.editor.update(cx, |editor, cx| {
                                editor.set_text(view.session.document.saved.clone(), cx);
                                let mut caret = record.caret.min(editor.text().len());
                                while !editor.text().is_char_boundary(caret) {
                                    caret -= 1;
                                }
                                editor.set_cursor(caret, cx);
                            });
                            // Its review comes back as it was: no rescan.
                            // Without one (a move cleared it), an Unfiled
                            // document's own names come back.
                            if let Some(review) = review {
                                view.restore_review(review, cx);
                            } else if let Some(names) = names {
                                view.open_names(&names.identities, &names.reserved, cx);
                                view.pii.review_mut().leave_off(&names.not_names);
                            }
                            view.replace_images(view.save_directory(), window, cx);
                            view.scroll
                                .set_offset(gpui::point(px(0.), px(record.scroll_y.min(0.))));
                            // A Vault document's Source is its viewing copy.
                            let from_vault = copy.is_some();
                            let attachment = copy.or(record.attachment);
                            if from_vault {
                                view.preview.restore(
                                    attachment.clone(),
                                    record.preview_visible,
                                    record.preview_split,
                                    record.preview_zoom.map(|zoom| {
                                        (record.preview_page, zoom, record.preview_fit)
                                    }),
                                );
                            }
                            if record.preview_visible
                                && view.preview.generation() == 0
                                && let Some(path) = attachment
                            {
                                view.open_path(path, window, cx);
                            }
                            if let Some(error) = source_error {
                                view.preview.unavailable(error);
                            }
                            if view.active && !dialog {
                                window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                            }
                        }
                        Err(error) => view.error = Some(error),
                    }
                    cx.notify();
                });
                this.attach_project(id, cx);
                this.refresh_saved_replies(cx);
                // A view kept while it loaded may be evicted now.
                this.evict(window, cx);
                cx.notify();
            });
        })
        .detach();
        Some(generation)
    }

    /// Where tab `id` keeps its viewing copy.
    fn viewing_dir(&self, id: u64) -> Option<PathBuf> {
        Some(self.viewing_root.as_ref()?.join(id.to_string()))
    }

    fn activate(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_as(id, Arrival::Visit, window, cx);
    }

    /// Activate tab `id`, recording it in Back/Forward as `arrival` says.
    fn activate_as(
        &mut self,
        id: u64,
        arrival: Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.finishing() || !self.tabs.contains(id) {
            return;
        }
        self.project_ui.page = None;
        self.reply_page = None;
        self.end_rename(true, cx);
        // Viewed again before its eviction's write ended: it stays. An
        // explicit close (a Project's Close all) goes on.
        if self.evicting.remove(&id) {
            self.closing_after_write.remove(&id);
        }
        let left = self.tabs.active();
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.tabs.activate(id);
        self.initialize(id, window, cx);
        if let Some(view) = self.active_view() {
            view.update(cx, |view, cx| {
                view.active = true;
                view.try_auto_convert(window, cx);
                if view.lifecycle.source_only() {
                    window.focus(&view.focus, cx);
                } else if view.lifecycle.loading() || view.lifecycle.unavailable() {
                    window.focus(&self.focus, cx);
                } else {
                    window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                }
            });
        }
        // Home covers a placeholder's editor; keys go to the workspace, or
        // they would reach nothing.
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        }
        self.note_destination(arrival, cx);
        if left != id {
            self.drop_if_empty_new(left, window, cx);
        }
        self.evict(window, cx);
        cx.notify();
    }

    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> u64 {
        // From Home, the hidden placeholder becomes the new tab.
        if let Some(id) = self
            .tabs
            .iter()
            .find(|tab| self.is_placeholder(tab, cx))
            .map(|tab| tab.id())
        {
            self.tabs.keep_blank(Some(id));
            if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view()) {
                view.update(cx, |view, _| view.lifecycle.keep_blank());
            }
            self.activate(id, window, cx);
            return id;
        }
        self.tabs.keep_blank(None);
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, _| view.lifecycle.keep_blank());
            }
        }
        let id = self.push(TabRecord {
            blank: true,
            ..TabRecord::default()
        });
        self.activate(id, window, cx);
        id
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

    #[cfg(test)]
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths(vec![path], window, cx);
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths_into(paths, None, window, cx);
    }

    /// Open what arrived while a confirm dialog was up, once it is answered.
    fn open_after_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.awaiting_prompt {
            return;
        }
        self.awaiting_prompt = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let open = this.update_in(cx, |this, window, cx| {
                    if this.dialog_up(window) {
                        return false;
                    }
                    this.awaiting_prompt = false;
                    for (paths, project) in std::mem::take(&mut this.pending_open) {
                        this.open_paths_into(paths, project, window, cx);
                    }
                    true
                });
                if open.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// A confirm dialog, the Keychain explainer, a folder sheet or New
    /// Project's sheet keeps focus until answered.
    fn dialog_up(&self, window: &Window) -> bool {
        window.has_active_prompt()
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
    }

    /// Open paths. Plain files open at once; each folder among them shows
    /// its sheet in turn, offering to add to `project` when it was dropped on
    /// that Project's row.
    fn open_paths_into(
        &mut self,
        paths: Vec<PathBuf>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ready {
            self.pending_open.push((paths, project));
            return;
        }
        if self.intake_refused(window, cx) {
            return;
        }
        // Opening activates a tab and takes focus; a confirm dialog keeps it
        // until answered, and the paths open after. Checked before
        // quitting, so a cancelled quit still opens them.
        if self.dialog_up(window) {
            self.pending_open.push((paths, project));
            self.open_after_prompt(window, cx);
            return;
        }
        if !self.quit_flow.may_quit() {
            return;
        }
        let requested = paths.clone();
        let task = cx.background_executor().spawn(async move {
            let supported = |path: &std::path::Path| {
                !path.is_dir() && (document::is_markdown(path) || import::supported_source(path))
            };
            let mut files = Vec::new();
            let mut folders = Vec::new();
            for path in paths {
                if let Some(folder) = folder_sheet::DroppedFolder::read(&path, supported) {
                    folders.push(folder);
                } else {
                    let ok = supported(&path);
                    files.push((session_store::identity(&path), ok));
                }
            }
            (files, folders)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (paths, folders) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // A dialog that came up while the paths resolved keeps focus.
                if this.dialog_up(window) {
                    this.pending_open.push((requested, project));
                    this.open_after_prompt(window, cx);
                    return;
                }
                this.finish_open(paths, project.clone(), window, cx);
                for folder in folders {
                    let pending = folder_sheet::Pending {
                        folder,
                        project: project.clone(),
                    };
                    this.queue_folder(pending, window, cx);
                }
            });
        })
        .detach();
    }

    /// Open `paths`, supported or not, as Vault documents: Unfiled, or in
    /// Project `project`. A file open already, unchanged, comes to the
    /// front.
    fn finish_open(
        &mut self,
        paths: Vec<(PathBuf, bool)>,
        project: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quit_flow.may_quit() || self.vault.vault().is_none() {
            return;
        }
        let (supported, unsupported): (Vec<_>, Vec<_>) =
            paths.into_iter().partition(|(_, supported)| *supported);
        self.post_unsupported(unsupported.into_iter().map(|(path, _)| path));
        // Each is hashed first: a file open in a tab comes to the front only
        // while its bytes are the ones that tab imported (`intake`).
        let paths = supported.into_iter().map(|(path, _)| path).collect();
        self.admit(paths, project, window, cx);
        cx.notify();
    }

    /// Unsupported files are skipped, by name: the notice is no place for
    /// the full path.
    fn post_unsupported(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let names: Vec<_> = paths
            .into_iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        if !names.is_empty() {
            self.notices
                .post(format!("Unsupported files skipped: {}", names.join(", ")));
        }
    }

    /// Activate `first`, newly opened, and drop the placeholder tabs it
    /// replaces.
    fn show_opened(&mut self, first: u64, window: &mut Window, cx: &mut Context<Self>) {
        {
            let disposable: Vec<_> = self
                .tabs
                .iter()
                .filter(|tab| {
                    tab.id() != first
                        && tab.view().map_or(tab.record().blank_disposable, |view| {
                            let view = view.read(cx);
                            view.lifecycle.blank_disposable()
                                && view.editor.read(cx).text().is_empty()
                                && view.preview.source().is_none()
                        })
                })
                .map(|tab| tab.id())
                .collect();
            self.activate(first, window, cx);
            for id in disposable {
                self.remove(id, window, cx);
            }
        }
    }

    pub fn refresh_import(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resolving_close() || self.quit_flow.finishing() {
            return;
        }
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| {
                    view.try_auto_convert(window, cx);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    fn event(&mut self, id: u64, event: &TabEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.contains(id) {
            return;
        }
        match event {
            TabEvent::Tabs(TabsEvent::New) if !self.quit_flow.quitting() => {
                if !self.intake_refused(window, cx) {
                    self.new_tab(window, cx);
                }
            }
            TabEvent::Tabs(TabsEvent::Open(paths)) => self.open_paths(paths.clone(), window, cx),
            TabEvent::Reply(event) if !self.quit_flow.quitting() => match event {
                ReplyEvent::Settled { open } => self.queue_box_save(id, *open, window, cx),
                ReplyEvent::Pasted(text) => self.queue_pasted_reply(id, text.clone(), window, cx),
                ReplyEvent::Open(reply) => self.open_reply(reply, window, cx),
            },
            TabEvent::Import(ImportEvent::State) => {
                self.refresh_import(window, cx);
                // A converted document of a Project gets its names now.
                self.attach_project(id, cx);
            }
            TabEvent::Import(ImportEvent::Ocr(state)) => self.set_ocr(state.clone(), cx),
            TabEvent::Project(ProjectEvent::Changed) => self.project_changed(id, cx),
            TabEvent::Project(ProjectEvent::OpenPage) => {
                let project = (self.tab_view(id))
                    .and_then(|view| view.read(cx).project.link().map(|link| link.id.clone()));
                if let Some(project) = project {
                    self.open_project(&project, window, cx);
                }
            }
            TabEvent::Project(ProjectEvent::MappingMissing) => {
                if self.vault_banner().is_some() {
                    self.carry_out_notices(self.notices.show_locked_projects());
                    cx.notify();
                }
            }
            TabEvent::Project(ProjectEvent::AddTerm(term)) => self.add_project_term(id, term, cx),
            TabEvent::Project(ProjectEvent::DismissHint(from, into)) => {
                self.dismiss_project_hint(id, from, into, cx)
            }
            TabEvent::Project(ProjectEvent::MoveToNew) => self.move_to_new_project(id, window, cx),
            TabEvent::CopyForAi => self.copy_tab_for_ai(id, window, cx),
            TabEvent::ToggleTheme => self.cycle_theme(window, cx),
            TabEvent::SourceDisplay(display) => {
                let display = *display;
                // Kept for this window when the save fails; the error is logged.
                let _ = self.update_general(
                    move |general| {
                        general.show_comments = display.comments;
                        general.show_hidden_text = display.hidden_text;
                    },
                    cx,
                );
                let views: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.view().cloned())
                    .collect();
                for view in views {
                    view.update(cx, |view, cx| view.source_display_changed(window, cx));
                }
            }
            TabEvent::Save(SaveEvent::Changed) => self.autosave(id, window, cx),
            TabEvent::Import(ImportEvent::Finished) => {
                self.refresh_import(window, cx);
                // A converted member of the AI project has text to scan now.
                self.scan_on_entry(cx);
            }
            TabEvent::Docx(DocxEvent::Finished { generation }) => {
                self.docx_finished(id, *generation, window, cx);
            }
            TabEvent::Docx(DocxEvent::Queue {
                path,
                generation,
                cancel,
            }) => {
                self.docx_queue.push(DocxJob {
                    id,
                    path: path.clone(),
                    generation: *generation,
                    cancel: cancel.clone(),
                });
                self.pump_docx(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn set_ocr(&mut self, state: OcrState, cx: &mut Context<Self>) {
        self.ocr_state = state.clone();
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                view.update(cx, |view, cx| {
                    view.ocr_state = state.clone();
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    pub fn resolving_close(&self) -> bool {
        self.quit_flow.resolving()
    }

    fn pump_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = &self.tabs;
        let effect = self
            .docx_queue
            .next(|id| tabs.get(id).is_some_and(|tab| tab.view().is_some()));
        if let Some(DocxEffect::Run(job)) = effect
            && let Some(view) = self.tabs.get(job.id).and_then(|tab| tab.view().cloned())
        {
            view.update(cx, |view, cx| {
                view.run_docx(job.path, job.generation, job.cancel, window, cx)
            });
        }
    }

    fn docx_finished(
        &mut self,
        id: u64,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.docx_queue.finished(id, generation);
        self.pump_docx(window, cx);
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.quit_flow.may_close() {
            return;
        }
        if !self.tabs.contains(id) {
            return;
        }
        // A member's latest mapping reaches its Project before the tab goes.
        self.project_changed(id, cx);
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if tab
            .view()
            .is_some_and(|view| view.read(cx).lifecycle.prompting())
        {
            return;
        }
        // Its last text is written before the tab goes; a failed write keeps
        // the tab, and its edit, open.
        self.autosave(id, window, cx);
        if self.write_pending_for(id, cx) {
            self.closing_after_write.insert(id);
            return;
        }
        // Notes whose write just failed keep the tab, as its text would.
        if (self.tabs.get(id).and_then(|tab| tab.view()))
            .is_some_and(|view| view.read(cx).notes.unsaved())
        {
            return;
        }
        self.remove(id, window, cx);
    }

    fn release(view: &Entity<DocumentView>, window: &mut Window, cx: &mut App) {
        view.update(cx, |view, cx| {
            view.session.generation = view.session.generation.wrapping_add(1);
            view.import.cancel();
            view.close_preview(window, cx);
            view.images.release(window, cx);
        });
    }

    fn remove(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(switcher) = &mut self.switcher
            && !switcher.forget(id)
        {
            self.switcher = None;
        }
        self.closing_after_write.remove(&id);
        self.evicting.remove(&id);
        self.unsaved.remove(&id);
        self.vault.forget(id);
        self.admitted_from.remove(&id);
        if let Some(dir) = self.viewing_dir(id) {
            let _ = std::fs::remove_dir_all(dir);
        }
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if let Some(view) = tab.view().cloned() {
            // A closed document can't report its running jobs; their end
            // still frees the import slot and the DOCX worker.
            let background = view.update(cx, |view, _| view.take_background());
            for task in background {
                cx.spawn_in(window, async move |this, cx| {
                    let done = task.await;
                    let _ = this.update_in(cx, |this, window, cx| match done {
                        Background::Import => this.refresh_import(window, cx),
                        Background::Docx { generation } => {
                            this.docx_finished(id, generation, window, cx)
                        }
                    });
                })
                .detach();
            }
            Self::release(&view, window, cx);
        }
        let Some((_, effect)) = self.tabs.remove(id) else {
            return;
        };
        self.tab_subscriptions.remove(&id);
        self.docx_queue.forget(id);
        // The tab that takes a closed tab's page takes its stop too; it is
        // no new stop when the closed tab wasn't the current one.
        let gone = navigation::Stop::Tab(id);
        let arrival = if self.navigation.current() == Some(&gone) {
            Arrival::Replace
        } else {
            Arrival::Unrecorded
        };
        if let Some(LiveViewsEffect::Activate(next)) = effect {
            if self.tabs.home() || self.project_ui.page.is_some() {
                // Home, or a Project page, stays over the next most recent
                // view.
                let home = self.tabs.home();
                self.tabs.activate(next);
                if home {
                    self.tabs.show_home();
                }
            } else {
                self.activate_as(next, arrival, window, cx);
            }
        }
        self.navigation.forget(&gone);
        self.checkpoint(window, cx);
        cx.notify();
    }

    /// Files opened from Finder or another app: a window hidden by the
    /// menu bar item shows them.
    pub(crate) fn open_paths_from_os(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::menu_bar::reveal_if_hidden(window, cx);
        self.open_paths(paths, window, cx);
    }

    /// The window's close button: quit, or with the menu bar item on, hide
    /// the window with every tab kept in memory. A quit already
    /// asking goes on.
    pub(super) fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::menu_bar::keeps_running(cx) && self.quit_flow.may_quit() {
            crate::menu_bar::close_window(window, cx);
        } else {
            self.quit(window, cx);
        }
    }

    pub(super) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || !self.quit_flow.may_quit() {
            return;
        }
        if self.tabs.iter().any(|tab| {
            tab.view()
                .is_some_and(|view| view.read(cx).lifecycle.prompting())
        }) {
            return;
        }
        // Documents and their names save themselves: nothing is asked.
        self.start_quit(VecDeque::new(), window, cx);
    }

    /// A quit from outside the app (macOS: the Dock, an Apple Event,
    /// logout): Quit's questions. Returns whether a quit is underway; when
    /// another question blocks it, mdoc comes to the front to show it.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn quit_from_system(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.quit(window, cx);
        let underway = !self.quit_flow.may_quit();
        if !underway {
            cx.activate(true);
            window.activate_window();
        }
        underway
    }

    /// The user cancelled a quit: no relaunch, and a system quit waiting on
    /// it is refused.
    pub(super) fn quit_cancelled(cx: &mut App) {
        crate::updater::cancel_restart(cx);
        #[cfg(any(target_os = "macos", test))]
        crate::system_quit::cancelled(cx);
    }

    /// Ask each tab in `at_risk` in turn, then quit.
    fn start_quit(&mut self, at_risk: VecDeque<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.quit_flow.quit(self.tabs.active(), at_risk);
        self.carry_out_quit(effect, window, cx);
    }

    fn carry_out_quit(&mut self, effect: QuitEffect, window: &mut Window, cx: &mut Context<Self>) {
        match effect {
            QuitEffect::Resolve(_) => {
                if let Some(effect) = self.quit_flow.resolved() {
                    self.carry_out_quit(effect, window, cx);
                }
            }
            QuitEffect::Settle { restore } => {
                if let Some(active) = restore {
                    self.activate_as(active, Arrival::Unrecorded, window, cx);
                }
                self.queue_quit(window, cx);
            }
        }
    }

    /// The questions are over: the quit is the writer's last job (ADR
    /// 0130). Editing goes on while the jobs before it run; it reads
    /// everything when it reaches the head.
    fn queue_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quit_flow.settle();
        self.queue_write(
            document_writer::Target::Quit,
            autosave::Write::Quit,
            window,
            cx,
        );
    }

    /// A load of `document` ends, done or with its tab gone. When it is a
    /// move's reload (its `generation`, not an older load's), and the last
    /// of its job's reloads, the next job may read the view once it holds
    /// the moved text.
    fn move_reload_ended(
        &mut self,
        document: &mdoc_store::document_id::VaultId,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reloading.get(document) != Some(&generation) {
            return;
        }
        self.reloading.remove(document);
        self.moving.remove(document);
        if self.reloading.is_empty() && std::mem::take(&mut self.reloads_hold_writer) {
            cx.defer_in(window, |this, window, cx| this.advance_writer(window, cx));
        }
    }

    /// The quit at the head of the writer's queue, all in this one turn:
    /// what every tab shows (its names, its text, its notes, its reply
    /// box), every Project whose names wait, the tab list and the Vault's
    /// summaries are written, and the window closes, so no keystroke can
    /// land between what is written and the close. Anything that can't be
    /// written cancels the quit, its edit still in its tab.
    fn run_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> autosave::Ran {
        // Cancelled while it waited: a job before it failed.
        if !self.quit_flow.finishing() {
            return autosave::Ran::Ended;
        }
        // A tab list write still running would land after this one.
        if self.session_writer.writing() {
            self.quit_waits_for_session = true;
            return autosave::Ran::Holds;
        }
        let ids: Vec<u64> = self.tabs.iter().map(|tab| tab.id()).collect();
        for view in self.tabs.iter().filter_map(|tab| tab.view().cloned()) {
            view.update(cx, |view, _| view.cancel_autosave());
        }
        if let Some(vault) = self.vault.vault().cloned() {
            for id in ids {
                if let Err(error) = self.quit_writes(id, &vault, window, cx) {
                    self.unsaved.insert(id);
                    let name = (self.tab_view(id)).map(|view| view.read(cx).display_name());
                    self.notices.post(format!(
                        "Could not save \u{201c}{}\u{201d}: {error}",
                        name.unwrap_or_default()
                    ));
                    self.quit_failed(window, cx);
                    return autosave::Ran::Ended;
                }
                // Its notes or its reply failed, and said so.
                if !self.quit_flow.finishing() {
                    return autosave::Ran::Ended;
                }
            }
            let projects: Vec<String> = (self.projects.summaries().into_iter())
                .map(|summary| summary.id)
                .filter(|id| {
                    self.project_ui.is_unwritten(id)
                        || self
                            .document_writer
                            .has(&document_writer::Target::Project(id.clone()))
                })
                .collect();
            for id in projects {
                if let Err(error) = self.write_now(&vault, &[Some(id.clone())]) {
                    self.project_ui.mark_unwritten(&id);
                    let name =
                        (self.projects.project(&id)).map_or_else(String::new, |p| p.name.clone());
                    self.notices.post(format!(
                        "Could not save the Project \u{201c}{name}\u{201d}: {error}"
                    ));
                    self.quit_failed(window, cx);
                    return autosave::Ran::Ended;
                }
                self.project_ui.take_unwritten(&id);
            }
        }
        // The tab list as it stands now that every document has its id.
        if self.session_writer.enabled()
            && let Some(SessionEffect::Write { path, snapshot }) =
                self.session_writer.checkpoint(self.snapshot(cx))
        {
            match session_store::save(&path, &snapshot) {
                Ok(()) => {
                    let _ = self.session_writer.saved(snapshot);
                }
                Err(error) => {
                    self.session_writer.failed();
                    self.session_failed(error.to_string(), window, cx);
                    return autosave::Ran::Ended;
                }
            }
        }
        if let Some(vault) = self.vault.vault()
            && let Err(error) = vault.flush()
        {
            self.notices
                .post(format!("Could not save the document list: {error}"));
        }
        self.close_window(window, cx);
        // The window is gone: nothing after the last job runs.
        autosave::Ran::Holds
    }

    /// The quit's writes of tab `id`: its document (made now if new), its
    /// notes and the reply in its box.
    fn quit_writes(
        &mut self,
        id: u64,
        vault: &mdoc_vault::Vault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.tab_document(id, cx).is_none() {
            self.create_tab_now(id, vault, cx)?;
        } else {
            self.write_tab_now(id, vault, cx)
                .map_err(|error| error.to_string())?;
        }
        let Some(document) = self.tab_document(id, cx) else {
            return Ok(());
        };
        let Some(view) = self.tab_view(id) else {
            return Ok(());
        };
        if view.read(cx).notes.needs_write() {
            self.run_notes_save(&document, window, cx);
        }
        if self.quit_flow.finishing() && view.read(cx).reply.unsaved().is_some() {
            let save = saved_replies::ReplySave::Box {
                tab: id,
                open: false,
            };
            self.run_reply_save(save, window, cx);
        }
        Ok(())
    }

    /// A write the quit needed failed: the window stays, with the edit.
    fn quit_failed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quit_flow.abandon() {
            Self::quit_cancelled(cx);
            crate::menu_bar::reveal_if_hidden(window, cx);
        }
        cx.notify();
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.update(cx, |panel, cx| panel.cancel_setup(cx));
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                Self::release(view, window, cx);
            }
        }
        // Viewing copies leave with the window; launch clears any it misses.
        if let Some(root) = &self.viewing_root {
            let _ = std::fs::remove_dir_all(root);
        }
        for handle in cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<comparison_ui::View>())
        {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        window.remove_window();
    }

    fn snapshot(&self, cx: &App) -> Session {
        let mut session = Session {
            sidebar_visible: self.sidebar.visible(),
            sidebar_choice: self.sidebar.choice(),
            theme_choice: self.theme_choice,
            expanded_projects: self.tree.expanded_ids(),
            ..Session::default()
        };
        for tab in &self.tabs {
            let mut record = tab.record().clone();
            if let Some(view) = tab.view() {
                let view = view.read(cx);
                record.source_only =
                    view.lifecycle.source_only() || view.lifecycle.generated_unedited();
                record.blank = !record.source_only
                    && !record.names_document()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty();
                record.blank_disposable = record.blank && view.lifecycle.blank_disposable();
                // A source tab keeps the name it shows, so after a restart it
                // reads the same before and after it is first opened.
                record.name = if record.blank {
                    view.session.name.clone()
                } else if record.source_only {
                    Some(view.display_name())
                } else {
                    None
                };
                if let Some(document) = view.session.document.id() {
                    record.set_document(document);
                }
                if !view.lifecycle.loading() && !view.lifecycle.unavailable() {
                    record.caret = view.editor.read(cx).cursor();
                    record.scroll_y = f32::from(view.scroll.offset().y);
                }
                record.attachment = view
                    .preview
                    .attachment()
                    .cloned()
                    .or_else(|| view.preview.source().cloned());
                // A Vault tab's Source is a viewing copy, made again on load.
                if record.document.is_some() {
                    record.attachment = None;
                    record.source_only = false;
                    record.blank = false;
                    record.blank_disposable = false;
                }
                record.preview_visible = view.preview.visible();
                record.preview_split = view.preview.split_ratio();
                if let Some(pdf) = view.preview.pdf() {
                    let (page, zoom) = pdf.read(cx).reading_position();
                    record.preview_page = page;
                    record.preview_zoom = Some(zoom);
                    record.preview_fit = match pdf.read(cx).fit_mode() {
                        Some(gpui_pdf::FitMode::Width) => session_store::PreviewFit::Width,
                        Some(gpui_pdf::FitMode::Page) => session_store::PreviewFit::Page,
                        None => session_store::PreviewFit::Manual,
                    };
                }
            }
            // A Vault tab is its id: no title, ever.
            if record.document.is_some() {
                record.name = None;
            }
            if !record.names_document() && !record.source_only && !record.blank {
                continue;
            }
            if tab.id() == self.tabs.active() {
                session.active = session.tabs.len();
            }
            session.tabs.push(record);
        }
        session
    }

    fn checkpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || !self.session_writer.enabled() {
            return;
        }
        let snapshot = self.snapshot(cx);
        let effect = self.session_writer.checkpoint(snapshot);
        self.carry_out_session(effect, window, cx);
    }

    fn carry_out_session(
        &mut self,
        effect: Option<SessionEffect<Session>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(SessionEffect::Write { path, snapshot }) = effect else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let result = session_store::save(&path, &snapshot);
            (snapshot, result)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (snapshot, result) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => {
                        let effect = this.session_writer.saved(snapshot);
                        this.carry_out_session(effect, window, cx);
                    }
                    Err(error) => {
                        this.session_writer.failed();
                        this.session_failed(error.to_string(), window, cx);
                    }
                }
                // A quit waiting at the head for this write goes on.
                if !this.session_writer.writing()
                    && std::mem::take(&mut this.quit_waits_for_session)
                    && this.run_quit(window, cx) == autosave::Ran::Ended
                {
                    this.advance_writer(window, cx);
                }
            });
        })
        .detach();
    }

    /// The tab list could not be saved; during a quit, ask whether to quit
    /// with the previous one.
    fn session_failed(&mut self, error: String, window: &mut Window, cx: &mut Context<Self>) {
        self.notices
            .post(format!("Could not save session: {error}"));
        if self.quit_flow.abandon() {
            // A window hidden in the menu bar shows to ask.
            crate::menu_bar::reveal_if_hidden(window, cx);
            let answer = window.prompt(
                PromptLevel::Warning,
                "Could not save the tab list",
                Some(
                    "Your documents are saved. Cancel to retry, or quit using the previous \
                     session list.",
                ),
                &["Cancel", "Quit without updating session"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if answer.await.ok() == Some(1) {
                    let _ = this.update_in(cx, |this, window, cx| {
                        // The previous tab list stays.
                        this.session_writer.set_path(None);
                        this.queue_quit(window, cx);
                    });
                } else {
                    let _ = this.update(cx, |_, cx| Self::quit_cancelled(cx));
                }
            })
            .detach();
        }
        cx.notify();
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

    /// Track the pointer over the collapsed rail or its reveal.
    fn hover_sidebar(
        &mut self,
        panel: bool,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.sidebar.hover(panel, hovered);
        self.carry_out_sidebar(effect, window, cx);
    }

    fn carry_out_sidebar(
        &mut self,
        effect: Option<SidebarEffect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            None => {}
            Some(SidebarEffect::Repaint) => cx.notify(),
            Some(SidebarEffect::EndSlide) => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(SLIDE + Duration::from_millis(16))
                        .await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            Some(SidebarEffect::Schedule { open, ticket }) => {
                let delay = if open { REVEAL_DELAY } else { REVEAL_GRACE };
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if !this.sidebar.fire(open, ticket) {
                            return;
                        }
                        if open {
                            this.show_reveal(false, cx);
                        } else if this.reveal_held(window, cx) {
                            let effect = this.sidebar.schedule(false);
                            this.carry_out_sidebar(Some(effect), window, cx);
                        } else {
                            let focused = this.reveal_focus.contains_focused(window, cx);
                            this.dismiss_reveal(window, cx);
                            if focused {
                                this.focus_active(window, cx);
                            }
                        }
                    });
                })
                .detach();
            }
            Some(SidebarEffect::FinishHiding(generation)) => {
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(REVEAL_SLIDE).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        let effect = this.sidebar.finish_hiding(generation);
                        this.carry_out_sidebar(effect, window, cx);
                    });
                })
                .detach();
                cx.notify();
            }
        }
    }

    /// A tab's or a Project's context menu is open. The sidebar then shows no
    /// tooltips (#310): GPUI keeps a shown tooltip while the pointer rests
    /// on it, draws it over the menu, and shows a new one when the pointer
    /// slips off the menu onto a row. Rendering a control without its
    /// tooltip is GPUI's one way to dismiss it.
    pub(super) fn context_menu_open(&self) -> bool {
        self.document_menu.is_some() || self.project_ui.header_menu.is_some()
    }

    /// A context menu, drag, the pointer, or keyboard focus keeps the reveal open.
    fn reveal_held(&self, window: &Window, cx: &App) -> bool {
        self.document_menu.is_some()
            || cx.has_active_drag()
            || self
                .sidebar
                .holds(|| self.reveal_focus.contains_focused(window, cx))
    }

    fn show_reveal(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.sidebar.show(keyboard);
        cx.notify();
    }

    /// A reveal slide that started ends after `REVEAL_SLIDE` with a repaint,
    /// so the document beside it is drawn afresh, uncached.
    fn end_reveal_slide_later(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.sidebar.reveal_slide_to_schedule() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVEAL_SLIDE).await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.end_reveal_slide(generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Slide the reveal back; it unmounts once the animation has finished.
    fn dismiss_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.sidebar.dismiss(cx.reduce_motion());
        self.carry_out_sidebar(effect, window, cx);
    }

    /// Keyboard focus on a rail control reveals the full list at once and moves
    /// focus to the matching control there; focus leaving the reveal closes it.
    fn reveal_for_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.visible() {
            return;
        }
        let may_open = self
            .sidebar
            .keyboard_may_open(|focus| focus.is_focused(window));
        let focused = [
            (&self.rail_toggle_focus, &self.panel_toggle_focus),
            (&self.rail_new_focus, &self.panel_new_focus),
            (&self.rail_home_focus, &self.panel_home_focus),
        ]
        .into_iter()
        .find(|(rail, _)| rail.is_focused(window))
        .map(|(rail, panel)| (rail.clone(), panel.clone()));
        match focused {
            Some((rail, panel)) if may_open && window.last_input_was_keyboard() => {
                self.sidebar.open_for_keyboard(rail);
                cx.notify();
                window.defer(cx, move |window, cx| window.focus(&panel, cx));
            }
            None if self.sidebar.keyboard_reveal() && !self.reveal_held(window, cx) => {
                self.dismiss_reveal(window, cx);
            }
            _ => {}
        }
    }

    fn clear_file_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.sidebar.clear_drag();
        self.carry_out_sidebar(effect, window, cx);
        cx.notify();
    }

    /// Import OS files dropped on the tree or the rail into Unfiled.
    fn drop_files(
        &mut self,
        paths: &gpui::ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        // A drop from Finder leaves mdoc in the background; what it opens
        // (a dialog among them) needs the keyboard.
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            self.open_paths(paths.paths().to_vec(), window, cx);
        }
    }

    /// OS files dropped on a Project header: they are imported into that
    /// Project, and a folder among them offers Add to it.
    fn drop_on_project(
        &mut self,
        paths: &gpui::ExternalPaths,
        project: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            let paths = paths.paths().to_vec();
            self.open_paths_into(paths, Some(project.to_string()), window, cx);
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let expanded = self.sidebar.visible();
        let revealed = !expanded && self.sidebar.reveal() != Reveal::Hidden;
        let panel_width = self.sidebar.width();
        let menu_open = self.context_menu_open();
        let rows = self.tree_view(cx);
        let sliding = self.sidebar.sliding(std::time::Instant::now()) && !cx.reduce_motion();
        let collapsing = sliding && !expanded && !revealed;
        let mut panel = (expanded || revealed || collapsing).then(|| {
            div()
                .id("sidebar-panel")
                .w(px(panel_width - 1.))
                .h_full()
                .flex()
                .flex_col()
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let refused = over && !openable_drag(event.drag(cx));
                        let effect = this.sidebar.drag_over_list(over, refused);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(
                    cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    }),
                )
                .child(
                    self.sidebar_top(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(sidebar_toggle(&self.panel_toggle_focus, expanded, theme))
                            .when(expanded || revealed, |v| {
                                v.child(self.navigation_buttons(cx))
                            })
                            .into_any_element(),
                    ),
                )
                .child(self.sidebar_nav(cx))
                // Above the scrolling list, so it shows however long it is.
                .when(self.sidebar.drop_refused(), |v| {
                    v.child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "drop-refused".into()))
                            .px_4()
                            .py_1()
                            .text_color(style::markdown_style(theme).alert_caution)
                            .child("These files can't be opened"),
                    )
                })
                .child(
                    div()
                        .id("sidebar-tree")
                        .role(gpui::Role::Tree)
                        .aria_label("Projects and documents")
                        .track_scroll(&self.sidebar_scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px_2()
                        .pb_1()
                        .children(rows),
                )
                .child(self.sidebar_foot(cx))
        });
        let content = if expanded {
            // Expanding: the list is revealed inside its final width, so the
            // content beside it is laid out once, not on every frame.
            let list = div().size_full().children(panel.take());
            if sliding {
                div()
                    .size_full()
                    .child(
                        div()
                            .h_full()
                            .overflow_hidden()
                            .child(div().w(px(panel_width)).h_full().child(list))
                            .with_animation(
                                ("sidebar-open", self.sidebar.generation()),
                                Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                                move |v, delta| {
                                    v.w(px(RAIL_WIDTH + (panel_width - RAIL_WIDTH) * delta))
                                },
                            ),
                    )
                    .into_any_element()
            } else {
                list.into_any_element()
            }
        } else {
            div()
                .w(px(RAIL_WIDTH - 1.))
                .h_full()
                .flex()
                .flex_col()
                // The rail starts below the title row, which holds the traffic
                // lights, the expand toggle and the document header.
                .child(ui::title_drag(
                    div()
                        .flex_shrink_0()
                        .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT)),
                    self.title.armed.clone(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .pb_2()
                        .flex_shrink_0()
                        .child(
                            ui::header_icon(
                                "rail-search",
                                "Search or run an action",
                                ui::Icon::Search,
                                theme,
                                false,
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(shell::OpenPalette), cx)
                            }),
                        )
                        .child(
                            ui::header_icon(
                                "rail-home",
                                "Home",
                                ui::Icon::Home,
                                theme,
                                self.showing_home(cx),
                            )
                            .track_focus(&self.rail_home_focus.clone().tab_stop(!revealed))
                            .when(cfg!(test), |v| v.debug_selector(|| "rail-home".into()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_home(window, cx)),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .flex_1()
                        .min_h_0()
                        .child(
                            ui::header_icon_untipped(
                                "sidebar-new-collapsed",
                                "New document",
                                ui::Icon::Plus,
                                theme,
                                false,
                            )
                            .when(!menu_open, |v| {
                                v.tooltip(style::tooltip(
                                    crate::shortcuts::tip("New document", &New),
                                    theme,
                                ))
                            })
                            .track_focus(&self.rail_new_focus.clone().tab_stop(!revealed))
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "sidebar-new-collapsed".into())
                            })
                            .size(px(32.))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx)),
                        ),
                )
                .child(self.rail_foot(cx))
                .into_any_element()
        };
        let width = |open: bool| if open { panel_width } else { RAIL_WIDTH };
        let mut sidebar = div()
            .id("sidebar")
            .track_focus(self.focus_regions.sidebar())
            .key_context("SidebarRegion")
            .relative()
            .w(px(width(expanded)))
            .flex_shrink_0()
            .h_full()
            .text_size(px(13.))
            // Keep the collapsed title controls in the sidebar's focus region
            // and before its list in Tab order. Deferred painting leaves them
            // above the adjacent content card without clipping to the rail.
            .when(!expanded && !revealed, |v| {
                v.child(gpui::deferred(
                    div()
                        .id("collapsed-title-controls")
                        .tab_index(-1)
                        .tab_group()
                        .tab_stop(false)
                        .absolute()
                        .left(px(style::TRAFFIC_LIGHT_INSET))
                        .top(px(style::CARD_MARGIN + (style::HEADER_HEIGHT - 32.) / 2.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(sidebar_toggle(&self.rail_toggle_focus, false, theme))
                        .child(self.navigation_buttons(cx)),
                ))
            })
            .child(div().size_full().overflow_hidden().child(content))
            // The expanded sidebar's edge drags its width; the
            // grip straddles the gap beside the content card.
            .when(expanded && !sliding, |v| {
                v.child(
                    div()
                        .id("sidebar-resize")
                        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-resize".into()))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-RESIZE_GRIP))
                        .w(px(2. * RESIZE_GRIP))
                        .cursor_col_resize()
                        .on_drag(SidebarResize, |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        }),
                )
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<SidebarResize>, _, cx| {
                        if !this.sidebar.resizing() {
                            this.sidebar.begin_resize();
                        }
                        let width = (event.event.position.x - event.bounds.left()) / px(1.);
                        if this.sidebar.resize_to(width) {
                            cx.notify();
                        }
                    },
                ))
            })
            .when(!expanded, |v| {
                v.on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                    this.hover_sidebar(false, *hovered, window, cx)
                }))
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let effect = this.sidebar.drag_over_rail(over);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(cx.listener(
                    |this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    },
                ))
            });
        if revealed && let Some(panel) = panel.take() {
            let (from, to) = if self.sidebar.reveal() == Reveal::Shown {
                (RAIL_WIDTH, panel_width)
            } else {
                (panel_width, RAIL_WIDTH)
            };
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .id("sidebar-reveal")
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(px(to))
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-reveal".into()))
                    .key_context("UiPanel UiMenu")
                    .track_focus(&self.reveal_focus)
                    // Ahead of the editor in tab order, as the rail it covers.
                    .tab_index(-1)
                    .tab_group()
                    .tab_stop(false)
                    .occlude()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .rounded_r_lg()
                    .shadow_lg()
                    .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                        this.hover_sidebar(true, *hovered, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                        match this.sidebar.opener().cloned() {
                            Some(focus) => {
                                window.focus(&focus, cx);
                                this.sidebar.escape_to(focus);
                            }
                            None => this.focus_active(window, cx),
                        }
                        this.dismiss_reveal(window, cx);
                    }))
                    .child(panel)
                    .with_animation(
                        ("sidebar-reveal", self.sidebar.reveal_generation()),
                        Animation::new(REVEAL_SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(from + (to - from) * delta)),
                    ),
            ));
        }
        // Collapsing: the content already has its final width; the full list
        // narrows over it to the rail, drawn above the card.
        if collapsing && let Some(list) = panel.take() {
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .child(div().w(px(panel_width)).h_full().child(list))
                    .with_animation(
                        ("sidebar-close", self.sidebar.generation()),
                        Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(panel_width - (panel_width - RAIL_WIDTH) * delta)),
                    ),
            ));
        }
        sidebar.into_any_element()
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

/// A tab name that fits by losing its middle, never its extension, so
/// names with a shared start still differ. Drawn whole, so the
/// slack a cut leaves falls after the extension rather than before it.
fn middle_truncated(name: &str) -> gpui::Div {
    let name = gpui::SharedString::from(name.to_string());
    div()
        .w_full()
        .relative()
        .overflow_hidden()
        // A line's height for the row; the canvas draws the name over it.
        .child("\u{200b}")
        .child(
            gpui::canvas(
                move |bounds, window, _| {
                    let style = window.text_style();
                    let size = style.font_size.to_pixels(window.rem_size());
                    let shape = |text: gpui::SharedString, window: &mut Window| {
                        let run = style.to_run(text.len());
                        window.text_system().shape_line(text, size, &[run], None)
                    };
                    let (stem, extension) = split_extension(&name);
                    let whole = shape(name.clone(), window);
                    if whole.width() <= bounds.size.width {
                        return whole;
                    }
                    let stem_line = shape(stem.to_string().into(), window);
                    let tail = shape(format!("\u{2026}{extension}").into(), window);
                    let room = (bounds.size.width - tail.width()) / px(1.);
                    let (front, back) = middle_cut(
                        stem,
                        |index| stem_line.x_for_index(index) / px(1.),
                        stem_line.width() / px(1.),
                        room,
                    );
                    let cut = format!("{}\u{2026}{}{extension}", &stem[..front], &stem[back..]);
                    shape(cut.into(), window)
                },
                |bounds, line, window, cx| {
                    line.paint(
                        bounds.origin,
                        bounds.size.height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// Where to cut `stem`, whose characters start at `x` and which is `width`
/// wide, to fit `room`: its start gets up to two thirds, its end whatever
/// the start left, at character boundaries. Returns the start's end and the
/// end's start.
fn middle_cut(stem: &str, x: impl Fn(usize) -> f32, width: f32, room: f32) -> (usize, usize) {
    let room = room.max(0.);
    let front = stem
        .char_indices()
        .map(|(index, _)| index)
        .chain([stem.len()])
        .take_while(|&index| x(index) <= room * 2. / 3.)
        .last()
        .unwrap_or(0);
    let left = room - x(front);
    let back = stem[front..]
        .char_indices()
        .map(|(index, _)| front + index)
        .chain([stem.len()])
        .find(|&index| width - x(index) <= left)
        .unwrap_or(stem.len());
    (front, back)
}

/// A name's stem and its extension with the dot: a short final part
/// without spaces after a dot that does not start the name.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot)
            if dot > 0
                && dot + 1 < name.len()
                && name.len() - dot <= 6
                && !name[dot..].contains(char::is_whitespace) =>
        {
            name.split_at(dot)
        }
        _ => (name, ""),
    }
}

/// A small mark on a sidebar tab for work done in it (OCR, pseudonymization).
fn tab_badge(icon: ui::Icon, color: gpui::Hsla) -> gpui::Svg {
    gpui::svg()
        .data(icon.data())
        .size(px(12.))
        .flex_shrink_0()
        .text_color(color)
}

fn sidebar_toggle(focus: &gpui::FocusHandle, expanded: bool, theme: Theme) -> ui::Control {
    let label = if expanded {
        "Collapse sidebar"
    } else {
        "Expand sidebar"
    };
    // Quiet, like every other rail and title-bar icon.
    ui::header_icon_untipped("sidebar-toggle", label, ui::Icon::Sidebar, theme, false)
        .tooltip(style::tooltip(
            crate::shortcuts::tip(label, &ToggleSidebar),
            theme,
        ))
        .size(px(32.))
        .track_focus(focus)
        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-toggle".into()))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx))
}

/// Bring mdoc to the front after a drop from another app. Making the window
/// key is not enough on macOS: the application must activate too.
pub(crate) fn activate_for_drop(window: &mut Window, cx: &mut App) {
    cx.activate(true);
    window.activate_window();
}

/// Whether a file drag holds anything mdoc opens (by extension, or a folder).
fn openable_drag(paths: &gpui::ExternalPaths) -> bool {
    paths.paths().iter().any(|path| import::droppable(path))
}

/// The sidebar takes only drags it can open; others fall through untouched.
fn can_drop_files(drag: &dyn std::any::Any, _: &mut Window, _: &mut App) -> bool {
    drag.downcast_ref::<gpui::ExternalPaths>()
        .is_none_or(openable_drag)
}

/// What the switcher and Back's tooltip name a live view by.
#[derive(Clone)]
struct TabLabel {
    name: String,
    /// The badge: the source's type until it is saved as Markdown.
    format: String,
}

/// A document as it loaded.
struct Loaded {
    document: Document,
    vault: Option<VaultLoaded>,
}

/// What a Vault document brings beside its text.
struct VaultLoaded {
    title: String,
    /// A new document's title follows its first line.
    follows: bool,
    /// Its original's viewing copy, when it has a Source to show.
    copy: Option<PathBuf>,
    /// Why its original didn't open, shown in the Source pane.
    source_error: Option<String>,
    /// Admitted and not converted yet.
    unconverted: bool,
    warning: Option<String>,
    left_out: Option<String>,
    /// An Unfiled document's own names, for a load without its review.
    names: Option<LocalNames>,
    /// Its private notes.
    notes: String,
    /// The AI Reply it was promoted from.
    from_reply: Option<String>,
}

/// An Unfiled document's names, as its mapping keeps them.
struct LocalNames {
    identities: Vec<mdoc_pii::restore::ExportedIdentity>,
    reserved: Vec<String>,
    not_names: Vec<(String, String)>,
}

/// Read Vault document `id`; a document with a Source gets a viewing copy
/// in `viewing`. Off the main thread.
fn load_from_vault(
    vault: &mdoc_vault::Vault,
    id: mdoc_store::document_id::VaultId,
    viewing: Option<PathBuf>,
) -> Result<Loaded, String> {
    let stored = vault
        .document(id.as_str())
        .map_err(|error| format!("Could not open the document. {error}"))?;
    let original = (stored.original.as_ref()).filter(|original| {
        intake::needs_conversion(std::path::Path::new(&original.info.file_name))
    });
    // A source that doesn't open leaves its saved text and review to show,
    // with the error where the Source would be; only a document with
    // nothing else to show fails.
    let (copy, source_error) = match (original, viewing) {
        (Some(original), Some(dir)) => {
            match vault.copy_original_to(id.as_str(), &dir, &original.info.file_name) {
                Ok(copy) => (Some(copy), None),
                Err(error) if stored.pending_conversion() => {
                    return Err(format!("Could not open the document's source. {error}"));
                }
                Err(mdoc_vault::VaultError::NotFound { .. }) => (
                    None,
                    Some(
                        "The original file is missing from the Vault, so it can't be shown. \
                         The Markdown is not affected."
                            .to_string(),
                    ),
                ),
                Err(error) => (
                    None,
                    Some(format!(
                        "The original file could not be opened, so it can't be shown. The \
                         Markdown is not affected. {error}"
                    )),
                ),
            }
        }
        _ => (None, None),
    };
    // Admitted and its text never written, though a move may have changed
    // its place: a text emptied by its user stays so.
    let unconverted = copy.is_some() && stored.pending_conversion();
    let (warning, left_out) = original.map_or((None, None), |original| {
        (
            original.info.import_warning.clone(),
            original.info.left_out.clone(),
        )
    });
    let names = (stored.project.is_none() && !stored.mapping.is_empty()).then(|| LocalNames {
        identities: stored.mapping.exported(),
        reserved: stored.mapping.reserved_aliases_for(&stored.id),
        not_names: stored.mapping.not_names_of(&stored.id),
    });
    let mut document = Document::in_vault(id, stored.markdown);
    document.review = stored.review;
    Ok(Loaded {
        document,
        vault: Some(VaultLoaded {
            title: stored.title,
            follows: stored.origin == mdoc_vault::DocumentOrigin::New,
            copy,
            source_error,
            unconverted,
            warning,
            left_out,
            names,
            notes: stored.notes,
            from_reply: stored.from_reply,
        }),
    })
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

impl Workspace {
    /// The sidebar's edge drag ended: keep the width it left in settings.
    fn end_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(width) = self.sidebar.end_resize() {
            // Kept for this window when the save fails; the error is logged.
            let _ = self.update_general(move |general| general.sidebar_width = Some(width), cx);
        }
        cx.notify();
    }
}

/// The window's title: a dev profile says so, so its window is never taken
/// for the user's own mdoc.
fn window_title(title: String, dev_profile: bool) -> String {
    if dev_profile {
        format!("{title} — dev profile")
    } else {
        title
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Collapsed by a narrowing window, the list's focus moves to the
        // rail's toggle, as a collapse by the toggle does.
        if self.sidebar.settle(window.viewport_size().width / px(1.))
            && self.focus_regions.sidebar().contains_focused(window, cx)
        {
            window.focus(&self.rail_toggle_focus, cx);
            self.sidebar.escape_to(self.rail_toggle_focus.clone());
        }
        // A finished edge drag saves the width it left.
        if !cx.has_active_drag() && self.sidebar.resizing() {
            self.end_sidebar_resize(cx);
        }
        self.note_region_focus(window, cx);
        self.reveal_for_keyboard(window, cx);
        self.end_reveal_slide_later(cx);
        // A finished or abandoned OS drag leaves no insertion line or reveal.
        if !cx.has_active_drag() && self.sidebar.dragging() {
            self.clear_file_drag(window, cx);
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let home = self.showing_overview(cx);
        let title = self
            .active_view()
            .filter(|_| !home)
            .map(|view| view.read(cx).window_title())
            .unwrap_or_else(|| "mdoc".into());
        let title = window_title(title, mdoc_store::app_paths::dev_profile().is_some());
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        if !cx.has_active_drag() && self.shell.drop.is_some() {
            self.shell.drop = None;
        }
        // Notices wait behind the header's bell; a new one opens it, and so
        // does a locked or unavailable Vault. A Vault sealed with another key
        // waits behind it: Home asks for its recovery key.
        let locked = self.vault_banner().is_some();
        let needed = locked && self.vault.wrong_key_root().is_none();
        self.notices.set_recovery_nudge(self.recovery_nudge());
        let (notices, effect) = self.notices.count(locked, needed);
        if let Some(effect) = effect {
            self.carry_out_notices(effect);
        }
        self.title.notices.set(notices);
        // A crash report says what the window showed (no text, no names).
        crate::crash::note_state(format!(
            "{}, {} tabs, sidebar {}, {:?} theme{}",
            if home {
                "Home".to_string()
            } else {
                self.active_view()
                    .map(|v| v.read(cx).crash_state(cx))
                    .unwrap_or_default()
            },
            self.tabs.len(),
            if self.sidebar.choice().unwrap_or(true) {
                "expanded"
            } else {
                "collapsed"
            },
            theme,
            if self.shell.palette.is_some() {
                ", palette open"
            } else {
                ""
            },
        ));
        // Collapsed, headers join the title row: they leave room for the
        // traffic lights and the expand toggle that sit in it.
        self.title.detached.set(!self.sidebar.visible());
        self.title.inset.set(if self.sidebar.visible() {
            0.
        } else {
            (style::TRAFFIC_LIGHT_INSET + style::WORKSPACE_NAV_WIDTH + 12. - RAIL_WIDTH - 16.)
                .max(0.)
        });
        div()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.sidebar_bg())
            .child(self.navigation_mouse(cx))
            .on_action(cx.listener(|this, _: &Back, window, cx| {
                this.navigate(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Forward, window, cx| {
                this.navigate(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ui::NextRegion, window, cx| {
                this.cycle_region(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousRegion, window, cx| {
                this.cycle_region(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::FocusEditor, window, cx| {
                this.escape_to_editor(window, cx)
            }))
            .on_action(cx.listener(|this, _: &shell::CloseNotices, window, cx| {
                if this.title.notices_open.get() {
                    this.close_notices(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarPrevious, window, cx| {
                this.tree_step(-1, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ui::SidebarNext, window, cx| this.tree_step(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ui::SidebarDelete, window, cx| {
                this.tree_delete_focused(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarCollapse, window, cx| {
                this.tree_fold(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExpand, window, cx| {
                this.tree_fold(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExtendUp, window, cx| {
                this.tree_extend(-1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::SidebarExtendDown, window, cx| {
                this.tree_extend(1, window, cx)
            }))
            .text_color(palette.header_fg)
            .on_action(
                cx.listener(|this, _: &shell::OpenPalette, window, cx| {
                    this.open_palette(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &shell::ShowHome, window, cx| this.show_home(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &shortcuts_ui::ShowShortcuts, window, cx| {
                    this.toggle_shortcuts(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &shell::PasteAsNewTab, window, cx| {
                this.paste_as_new_tab(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleNotices, _, cx| {
                this.title.notices_open.set(!this.title.notices_open.get());
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &PiiApplyAll, window, cx| {
                if let Some(view) = this.active_view().filter(|_| !this.showing_overview(cx)) {
                    view.update(cx, |view, cx| view.apply_all_pii(action, window, cx));
                }
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Undo, window, cx| {
                if let Some(view) = this.history_document(window, cx) {
                    view.update(cx, |view, cx| {
                        view.editor
                            .update(cx, |editor, cx| editor.undo(action, window, cx));
                    });
                }
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Redo, window, cx| {
                if let Some(view) = this.history_document(window, cx) {
                    view.update(cx, |view, cx| {
                        view.editor
                            .update(cx, |editor, cx| editor.redo(action, window, cx));
                    });
                }
            }))
            // Home's clipboard actions need a document; documents handle them first.
            .on_action(cx.listener(|this, action: &PasteAiResponse, window, cx| {
                if let Some(view) = this.clipboard_document("Paste AI reply", window, cx) {
                    view.update(cx, |view, cx| view.paste_ai_response(action, window, cx));
                }
            }))
            .on_action(
                cx.listener(|this, action: &PseudonymizeClipboard, window, cx| {
                    if let Some(view) =
                        this.clipboard_document("Pseudonymize clipboard", window, cx)
                    {
                        view.update(cx, |view, cx| {
                            view.pseudonymize_clipboard(action, window, cx)
                        });
                    }
                }),
            )
            .on_action(cx.listener(|this, action: &RestoreClipboard, window, cx| {
                if let Some(view) = this.clipboard_document("Restore clipboard", window, cx) {
                    view.update(cx, |view, cx| view.restore_clipboard(action, window, cx));
                }
            }))
            .on_drag_move(cx.listener(
                |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    this.home_drag(event, cx)
                },
            ))
            // Nothing is open or closed: Close returns to Home, and the view
            // stays live for its undo and scroll.
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if !this.quit_flow.quitting() && !this.showing_home(cx) {
                    this.show_home(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Import, window, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.import(&Import, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &SwitchDocument, window, cx| {
                this.switch_document(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwitchDocumentBack, window, cx| {
                this.switch_document(true, window, cx)
            }))
            // Esc closes Notices first, wherever focus is: a drop's notice
            // opens while the document keeps focus.
            .capture_action(cx.listener(|this, _: &mdoc_editor::Dismiss, window, cx| {
                if this.title.notices_open.get() {
                    this.close_notices(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                this.tab_from_window(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                this.tab_from_window(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &RetryDocument, window, cx| {
                if let Some(tab) = this.tabs.get(this.tabs.active())
                    && let Some(document) = tab.record().document()
                {
                    this.load_document(tab.id(), document, tab.record().clone(), window, cx);
                }
            }))
            .when_some(self.document_menu.clone(), |view, (id, position)| {
                view.child(
                    gpui::deferred(
                        gpui::anchored().position(position).snap_to_window().child(
                            ui::panel("document-context-menu", self.theme.get())
                                .key_context("UiPanel UiMenu")
                                .track_focus(&self.menu_focus)
                                .tab_group()
                                .tab_stop(false)
                                .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                    this.close_document_menu(window, cx)
                                }))
                                .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                    ui::cycle(window, cx, Some(&this.menu_focus), false);
                                    cx.stop_propagation();
                                }))
                                .on_action(cx.listener(
                                    |this, _: &ui::PreviousControl, window, cx| {
                                        ui::cycle(window, cx, Some(&this.menu_focus), true);
                                        cx.stop_propagation();
                                    },
                                ))
                                .p_1()
                                .on_mouse_down_out(cx.listener(
                                    |this, _: &gpui::MouseDownEvent, _, cx| {
                                        this.document_menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(self.document_menu_items(&id, cx)),
                        ),
                    )
                    // Above the sidebar reveal it was opened from.
                    .with_priority(1),
                )
            })
            .children(self.project_list(cx))
            .children(self.switcher_view(cx))
            .children(self.move_toast(cx))
            .children(self.ai_prompt_view(window, cx))
            .children(self.project_header_menu(cx))
            .children(self.folder_sheet_view(window, cx))
            .children(self.new_project_view(window, cx))
            .children(self.keychain_explainer(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(self.card(home, window, cx)),
            )
            .children(self.notices_view(window, cx))
            .children(self.palette_view(cx))
            .when(self.settings.read(cx).open, |v| {
                v.child(self.settings.clone())
            })
            .children(self.shortcuts_view(window, cx))
    }
}

impl Workspace {
    /// The inset content card on the window ground: workspace bars,
    /// then Home or the active document.
    fn card(&mut self, home: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let content = if self.reply_page.is_some() {
            self.reply_page_view(window, cx)
        } else if self.project_ui.page.is_some() {
            self.project_page(window, cx)
        } else if home {
            self.home_view(window, cx)
        } else {
            match self.active_view() {
                // While the sidebar slides, the document beside it reuses its
                // last frame unless it changed itself: only a view
                // notified on its own redraws, so this holds only for the
                // slide, never as the document's steady state.
                Some(active)
                    if self.sidebar.animating(std::time::Instant::now()) && !cx.reduce_motion() =>
                {
                    active
                        .cached(gpui::StyleRefinement::default().size_full())
                        .into_any_element()
                }
                Some(active) => active.into_any_element(),
                None => div().into_any_element(),
            }
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            // Expanded: one margin all round. Collapsed: the view's header
            // fills the title row and it draws its own card below.
            .when(self.sidebar.visible(), |v| v.p(px(style::CARD_MARGIN)))
            .when(!self.sidebar.visible(), |v| {
                v.pr(px(style::CARD_MARGIN)).pb(px(style::CARD_MARGIN))
            })
            .child(
                div()
                    .id("content-card")
                    .size_full()
                    .flex()
                    .flex_col()
                    .when(self.sidebar.visible(), |v| ui::card_surface(v, theme))
                    .when(home && self.project_ui.page.is_none(), |v| {
                        v.on_drop(
                            cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                                this.shell.drop = None;
                                activate_for_drop(window, cx);
                                this.open_paths(paths.paths().to_vec(), window, cx);
                            }),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .into_any_element()
    }

    /// Carry out what a notices transition asks for.
    fn carry_out_notices(&mut self, effect: notices::NoticesEffect) {
        match effect {
            notices::NoticesEffect::DiscardCrashReport => crate::crash::discard(),
            notices::NoticesEffect::OpenBell => self.title.notices_open.set(true),
            notices::NoticesEffect::CloseBell => self.title.notices_open.set(false),
        }
    }

    fn notice_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.notices.message().map(str::to_owned).map(|notice| {
            let palette = self.theme.get().pdf_style();
            let accent = style::markdown_style(self.theme.get()).alert_warning;
            div()
                .flex()
                .flex_col()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .border_b_0()
                .border_color(palette.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .child(div().text_color(accent).child("ⓘ"))
                        .child(div().flex_1().min_w_0().child(notice))
                        .child(
                            crate::ui::focus_ring("dismiss-session-notice", self.theme.get())
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "dismiss-session-notice".into())
                                })
                                .flex_shrink_0()
                                .role(gpui::Role::Button)
                                .aria_label("Dismiss notice")
                                .key_context("UiControl")
                                .tab_index(0)
                                .aria_label("Dismiss notice")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|v| v.bg(palette.placeholder_bg))
                                .child("×")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notices.dismiss();
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
impl Workspace {
    /// Show the next most recent live view, as a quick `⌃Tab` does.
    fn show_next_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.tabs.iter().nth(1).map(|tab| tab.id()) {
            self.activate(next, window, cx);
        }
    }

    /// Release the shown live view, as eviction does.
    pub(crate) fn release_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.tabs.active(), window, cx);
    }
}

/// Keyboard reach: F6 regions, Tab from the window, and the
/// sidebar's tab list.
impl Workspace {
    /// The window's F6 regions in order, each with whether it is a single
    /// focus target (the editor) rather than a group of controls.
    fn regions(&self, cx: &App) -> Vec<(gpui::FocusHandle, bool)> {
        let home = self.showing_overview(cx);
        let document = self
            .active_view()
            .filter(|_| !home)
            .map(|view| view.read(cx).focus_regions(cx))
            .unwrap_or_default();
        self.focus_regions.order(home, document)
    }

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

    /// F6 / Shift+F6: leave the focused region for the next one, landing on
    /// the control last focused there, else its first control.
    pub(crate) fn cycle_region(&mut self, backwards: bool, window: &mut Window, cx: &mut App) {
        // Modal surfaces keep focus until closed.
        if self.settings.read(cx).open
            || self.project_ui.list_open
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
            || self.shell.palette.is_some()
            || self.shell.shortcuts.is_some()
        {
            return;
        }
        let regions = self.regions(cx);
        let current = self.note_region_focus(window, cx);
        let effect = self
            .focus_regions
            .cycle(&regions, current, backwards, || self.sidebar_landing(cx));
        match effect {
            None => {}
            Some(focus_regions::FocusEffect::Focus(target)) => window.focus(&target, cx),
            Some(focus_regions::FocusEffect::Enter { region, last }) => {
                if let Some(last) =
                    last.filter(|focus| self.region_holds(&region, focus, window, cx))
                {
                    // The tree's one Tab stop follows the row it lands on,
                    // so Tab leaves the tree from there.
                    if region == *self.focus_regions.sidebar() {
                        self.tree.set_cursor(None);
                    }
                    window.focus(&last, cx);
                } else if region == *self.focus_regions.sidebar() && !self.sidebar.visible() {
                    // The deferred title/reveal group precedes the sidebar
                    // container in window Tab order. Enter its first control
                    // explicitly, so Shift+Tab leaves from the region's edge.
                    let toggle = if self.sidebar.reveal() == Reveal::Hidden {
                        &self.rail_toggle_focus
                    } else {
                        &self.panel_toggle_focus
                    };
                    window.focus(toggle, cx);
                } else {
                    window.focus(&region, cx);
                    ui::cycle(window, cx, Some(&region), false);
                    if !region.contains_focused(window, cx) {
                        window.focus(&region, cx);
                    }
                }
            }
        }
    }

    /// Where F6 enters the sidebar, in the layout drawn now: on the collapsed
    /// rail its Home row, which opens the reveal; in the expanded tree (or
    /// the rail's open reveal) Home's row on Home, else the tree's selected
    /// row.
    fn sidebar_landing(&self, cx: &mut App) -> Option<gpui::FocusHandle> {
        let list = self.sidebar.visible() || self.sidebar.reveal() == Reveal::Shown;
        if !list {
            return Some(self.rail_home_focus.clone());
        }
        if self.showing_home(cx) {
            return Some(self.panel_home_focus.clone());
        }
        self.tree_landing(cx)
    }

    /// The region holding focus. Comments sit inside the preview: the
    /// innermost wins. The document's notices strip and view switcher count
    /// as its toolbar.
    fn region_index(
        &self,
        regions: &[(gpui::FocusHandle, bool)],
        window: &Window,
        cx: &App,
    ) -> Option<usize> {
        regions
            .iter()
            .rposition(|(region, _)| region.contains_focused(window, cx))
            .or_else(|| {
                let view = self.active_view().filter(|_| !self.showing_overview(cx))?;
                let view = view.read(cx);
                view.toolbar_members()
                    .iter()
                    .any(|member| member.contains_focused(window, cx))
                    .then(|| {
                        regions
                            .iter()
                            .position(|(region, _)| *region == view.toolbar_region)
                    })
                    .flatten()
            })
    }

    /// Whether `focus` sits in `region`, counting the document's toolbar
    /// members as its toolbar.
    fn region_holds(
        &self,
        region: &gpui::FocusHandle,
        focus: &gpui::FocusHandle,
        window: &Window,
        cx: &App,
    ) -> bool {
        region.contains(focus, window)
            || self.active_view().is_some_and(|view| {
                let view = view.read(cx);
                view.toolbar_region == *region
                    && view
                        .toolbar_members()
                        .iter()
                        .any(|member| member.contains(focus, window))
            })
    }

    /// Remember the focused control as its region's last; returns that
    /// region's index. Comments sit inside the preview: the innermost wins.
    fn note_region_focus(&mut self, window: &Window, cx: &App) -> Option<usize> {
        let regions = self.regions(cx);
        let current = self.region_index(&regions, window, cx);
        if let (Some(i), Some(focused)) = (current, window.focused(cx)) {
            self.focus_regions.note(&regions[i].0, &focused);
        }
        current
    }

    /// Esc in the sidebar or toolbar: close the More menu or the notices
    /// first, otherwise return to the editor.
    fn escape_to_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.title.notices_open.get() {
            self.title.notices_open.set(false);
            cx.notify();
            return;
        }
        // In the tree, Esc first clears Unfiled's selection.
        if self.focus_regions.sidebar().contains_focused(window, cx)
            && self.clear_tree_selection(cx)
        {
            return;
        }
        if let Some(view) = self.active_view().filter(|_| !self.showing_overview(cx))
            && view.read(cx).more_open
        {
            view.update(cx, |view, cx| view.close_more(window, cx));
            return;
        }
        self.note_region_focus(window, cx);
        if !self.showing_overview(cx) {
            self.focus_active(window, cx);
        }
    }

    /// Tab / Shift+Tab outside the editor, menus and dialogs.
    /// With no control focused on Home it enters Home's controls. Otherwise
    /// focus follows window order, except that the editor, which is no Tab
    /// stop, is entered rather than skipped: Tab past the toolbar's last
    /// control, and Shift+Tab back from the region after it.
    fn tab_from_window(&mut self, backwards: bool, window: &mut Window, cx: &mut App) {
        let from = self.note_region_focus(window, cx);
        if self.focus.is_focused(window) && self.showing_overview(cx) {
            let home = self.focus_regions.home().clone();
            window.focus(&home, cx);
            ui::cycle(window, cx, Some(&home), backwards);
            if home.contains_focused(window, cx) {
                return;
            }
            window.focus(&self.focus, cx);
        }
        let regions = self.regions(cx);
        ui::cycle(window, cx, None, backwards);
        let to = self.region_index(&regions, window, cx);
        if let Some(focus_regions::FocusEffect::Focus(editor)) =
            focus_regions::FocusRegions::tab_into_editor(&regions, from, to, backwards)
        {
            window.focus(&editor, cx);
        }
    }
}

/// The background OCR install check. Tests never read the real profile's
/// installed models.
fn check_ocr(
    config: &mdoc_store::settings::OcrConfig,
) -> Result<Option<mdoc_import::ocr::Installed>, String> {
    if cfg!(test) {
        Ok(None)
    } else {
        ocr::check_config(config).map_err(|e| e.to_string())
    }
}
