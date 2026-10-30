//! Window-level tabs. Each initialized tab owns a retained document view;
//! unopened session records allocate no editor, PDF, or conversion worker.
use crate::{
    comparison, comparison_ui,
    document_view::{
        Background, DocumentView, DocumentViewDependencies, Next, OcrState, known_ocr_state,
    },
    pii,
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

mod case_coordinator;
mod case_page;
mod cases_ui;
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
mod keychain_ui;
mod navigation;
use navigation::Arrival;
mod navigation_ui;
mod notices;
pub(crate) use navigation_ui::{Back, Forward};
mod quit_flow;
pub(crate) use quit_flow::CloseWatch;
use quit_flow::{QuitEffect, QuitEnd};
mod session_writer;
mod shell;
pub(crate) mod shortcuts_ui;
use session_writer::SessionEffect;
mod sidebar;
mod tab_set;
pub(crate) use cases_ui::{
    AddCaseName, CancelCaseName, ClearCaseName, ConfirmCaseName, bind_keys as bind_case_keys,
};
pub(crate) use shell::{
    CloseNotices, OpenPalette, PALETTE, PaletteClose, ShowHome, bind_keys as bind_shell_keys,
};
use sidebar::{Reveal, SLIDE, SidebarEffect};
use tab_set::TabsEffect;
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
        NextTab,
        PreviousTab,
        ToggleSidebar,
        ToggleTheme,
        ToggleNotices,
        OpenCase,
        RetryDocument,
        PasteAiResponse,
        PseudonymizeClipboard,
    ]
);

/// How a document reaches the window: one channel, grouped by the owner
/// that handles it (ADR 0069). The window commands a document through its
/// methods; a document holds no handle to its window.
pub(super) enum TabEvent {
    Tabs(TabsEvent),
    Close(CloseEvent),
    Import(ImportEvent),
    Docx(DocxEvent),
    Save(SaveEvent),
    Case(CaseEvent),
    ToggleTheme,
}

/// Tabs to add (`TabSet`).
pub(super) enum TabsEvent {
    New,
    Open(Vec<PathBuf>),
    /// Open a copied AI reply in a new tab and restore its aliases (ADR 0041).
    Restored(Arc<pii::ui::RestoredReply>),
}

/// Closing this tab, alone or in a quit (`QuitFlow`).
pub(super) enum CloseEvent {
    Requested,
    Resolved,
    Cancelled,
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

/// Saving the document.
pub(super) enum SaveEvent {
    /// Save to `path` unless another tab has that file open, then go on
    /// with `next`.
    Write {
        path: PathBuf,
        next: Option<Next>,
    },
    Saved,
}

/// The tab's case (`CaseCoordinator`).
pub(super) enum CaseEvent {
    /// Show this tab's case page (ADR 0077).
    OpenPage,
    /// The tab's mapping changed; save it into the tab's case.
    Changed,
    /// The tab's AI reply changed or was cleared; keep it in the tab's case
    /// (ADR 0078).
    Reply,
    /// Add a name to the tab's case before it appears (ADR 0041 #11).
    AddTerm(String),
    /// Keep a dismissed merge hint, as its (shorter, fuller) primary wordings, in the
    /// tab's case (ADR 0080).
    DismissHint(String, String),
    /// Save the tab's mapping as a case, from the Inspector (ADR 0048, C),
    /// or once the document is saved; `close` closes the tab after.
    Create { close: bool },
    /// Restoring or pseudonymizing found no mapping in the tab; a locked
    /// case may hold it (ADR 0043).
    MappingMissing,
}
impl EventEmitter<TabEvent> for DocumentView {}

// Avoid repainting the sidebar for cursor movement, search results, or PDF raster
// completions. Only sidebar-visible metadata invalidates the parent view.
#[derive(PartialEq)]
struct SidebarStatus {
    path: Option<PathBuf>,
    source: Option<PathBuf>,
    dirty: bool,
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
            path: view.session.document.path.clone(),
            source: view.preview.source().cloned(),
            dirty: view.lifecycle.dirty(),
            loading: view.lifecycle.loading() || view.preview.loading(),
            queued: view.preview.queued() || view.import.auto_pending(),
            importing: view.import.busy(),
            error: view.error.is_some()
                || view.preview.failed()
                || view.import.ocr_required().is_some(),
            pseudonymized: view.pseudonymized(),
            recognized: view.recognized(),
        }
    }
}

type Tab = tab_set::Tab<Entity<DocumentView>>;

#[derive(Clone)]
struct TabDrag {
    id: u64,
    label: String,
}
impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child(self.label.clone())
    }
}

pub(super) struct Workspace {
    focus: gpui::FocusHandle,
    /// F6 regions the workspace owns, and the control last focused in each
    /// region when F6 left it (ADR 0062).
    focus_regions: focus_regions::FocusRegions<gpui::FocusHandle>,
    /// The Open Case list holds focus while open, and gives it back on
    /// close; the notices popover takes Esc (ADR 0062).
    pub(super) case_list_focus: gpui::FocusHandle,
    pub(super) case_list_return: Option<gpui::FocusHandle>,
    pub(super) notices_focus: gpui::FocusHandle,
    tabs: tab_set::TabSet<Entity<DocumentView>>,
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
    /// enters the sidebar on Home (ADR 0062).
    rail_home_focus: gpui::FocusHandle,
    panel_home_focus: gpui::FocusHandle,
    menu_focus: gpui::FocusHandle,
    /// The Keychain explainer's Continue, focused when it opens (ADR 0043).
    explainer_focus: gpui::FocusHandle,
    explainer_focus_pending: bool,
    /// The explainer's focus group, and where focus returns when it closes.
    explainer_group: gpui::FocusHandle,
    explainer_return: Option<gpui::FocusHandle>,
    menu_return: Option<gpui::FocusHandle>,
    sidebar_scroll: gpui::ScrollHandle,
    compact_scroll: gpui::ScrollHandle,
    compact_row_focus: std::cell::RefCell<std::collections::HashMap<u64, gpui::FocusHandle>>,
    sidebar_row_focus: std::cell::RefCell<std::collections::HashMap<u64, gpui::FocusHandle>>,
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
    /// their paths, sidebar position and case header (ADR 0075).
    pending_open: Vec<(Vec<PathBuf>, Option<usize>, Option<String>)>,
    /// Opens wait in `pending_open` for a dialog to be answered.
    awaiting_prompt: bool,
    notices: notices::Notices,
    tab_menu: Option<(u64, gpui::Point<gpui::Pixels>)>,
    import_busy: Arc<AtomicBool>,
    docx_queue: docx_queue::DocxQueue,
    quit_flow: quit_flow::QuitFlow,
    /// The quit's summary question is up (ADR 0071).
    quit_summary: bool,
    session_writer: session_writer::SessionWriter<Session>,
    cases: cases_ui::CaseState,
    /// Dropped folders' sheets, one at a time (ADR 0075).
    folder_sheets: folder_sheet::FolderSheets,
    folder_sheet_focus: Option<folder_sheet_ui::SheetFocus>,
    case_coordinator: case_coordinator::CaseCoordinator<gpui::AnyWindowHandle>,
    /// The sidebar's theme pick; `theme` holds what it resolves to (ADR 0048).
    theme_choice: ThemeChoice,
    /// The theme button's tooltip reads the choice live.
    theme_tip: Rc<Cell<ThemeChoice>>,
    /// The title last given to the window: "mdoc" on Home, else the active
    /// document's.
    window_title: String,
    /// Shared with document headers: title-bar inset and drag state.
    title: style::TitleBar,
    shell: shell::ShellState,
    /// The sidebar's Settings row; Settings returns focus here (ADR 0048).
    settings_focus: gpui::FocusHandle,
    _appearance: Option<Subscription>,
    _checkpoint: Option<gpui::Task<()>>,
}

impl Workspace {
    /// Every prompt in the app is mdoc's own confirm dialog (ADR 0064).
    pub(crate) fn install_prompts(&self, cx: &mut App) {
        crate::confirm_dialog::install(self.theme.clone(), cx);
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::empty(window, cx);
        this.ocr_state = OcrState::Checking;
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                this.quit(window, cx);
                false
            })
            .unwrap_or(true)
        });
        if !cfg!(test) {
            this.notices.load_crash(crate::crash::pending());
        }
        this.session_writer.set_path(if cfg!(test) {
            None
        } else {
            mdoc_store::app_paths::data_local_dir().map(|path| path.join("mdoc/session.json"))
        });
        let path = this.session_writer.path().cloned();
        let task = cx.background_executor().spawn(async move {
            path.map_or_else(|| Ok(Session::default()), |path| session_store::load(&path))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
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
        let mut this = Self {
            focus: cx.focus_handle(),
            focus_regions: focus_regions::FocusRegions::new(cx.focus_handle(), cx.focus_handle()),
            case_list_focus: cx.focus_handle(),
            case_list_return: None,
            notices_focus: cx.focus_handle(),
            tabs: tab_set::TabSet::default(),
            navigation: Default::default(),
            back_focus: cx.focus_handle().tab_stop(true),
            forward_focus: cx.focus_handle().tab_stop(true),
            tab_subscriptions: std::collections::HashMap::new(),
            sidebar: sidebar::Sidebar::default(),
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
            compact_scroll: gpui::ScrollHandle::new(),
            compact_row_focus: Default::default(),
            sidebar_row_focus: Default::default(),
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
            tab_menu: None,
            import_busy: Arc::new(AtomicBool::new(false)),
            docx_queue: docx_queue::DocxQueue::default(),
            quit_flow: quit_flow::QuitFlow::default(),
            quit_summary: false,
            session_writer: session_writer::SessionWriter::new(None),
            cases: Default::default(),
            folder_sheets: Default::default(),
            folder_sheet_focus: None,
            case_coordinator: Default::default(),
            theme_choice: ThemeChoice::default(),
            theme_tip: Rc::default(),
            window_title: String::new(),
            title: style::TitleBar::default(),
            shell: shell::ShellState::new(cx),
            settings_focus: cx.focus_handle().tab_stop(true),
            _appearance: None,
            _checkpoint: None,
        };
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
                            // selects ended; the tab may be gone (ADR 0066).
                            let state = known_ocr_state(&this.preferences, this.settings.read(cx));
                            this.set_ocr(state, cx);
                        }
                    }
                    settings_ui::Event::Finished(_, _) => {}
                    settings_ui::Event::Compare(model) => {
                        this.open_comparison(*model, window, cx);
                    }
                    settings_ui::Event::RestartToUpdate => this.restart_to_update(window, cx),
                    settings_ui::Event::ClearRecents => {
                        this.shell.recents.clear();
                        crate::document_view::RecentFolders::share(Vec::new(), cx);
                        cx.notify();
                    }
                    settings_ui::Event::Shortcuts => this.toggle_shortcuts(window, cx),
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
    /// The launch-time update check, unless the user turned it off (ADR 0038).
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
    /// anything leaves the Mac (ADR 0047). Send and Copy also discard it.
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

    /// Restart now (ADR 0040): the ordinary quit — unsaved-change prompts,
    /// session save — swaps the staged update in, then mdoc relaunches.
    pub(super) fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::updater::request_restart(cx);
        self.settings
            .update(cx, |panel, cx| panel.close(window, cx));
        self.quit(window, cx);
        // Quitting can't start now (a prompt is open): don't relaunch later.
        if self.quit_flow.may_quit() && !self.quit_summary {
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
            view.read(cx).session.document.path.clone(),
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
        self.apply_theme(window, cx);
        self.cases.collapsed = session.collapsed_cases.iter().cloned().collect();
        self.case_coordinator
            .restore_summaries(session.cases.clone());
        let selected = session.active;
        // Session paths were normalized when opened/saved. Do not stat every file
        // during startup; missing files are diagnosed when activated.
        for record in session.tabs {
            if self.tabs.iter().any(|tab| {
                if record.source_only {
                    tab.record().attachment == record.attachment
                } else {
                    !record.markdown.as_os_str().is_empty()
                        && !tab.record().source_only
                        && tab.record().markdown == record.markdown
                }
            }) {
                continue;
            }
            self.push(record);
        }
        self.ready = true;
        // Nothing to restore: Home shows over a hidden placeholder tab until
        // it gets content (ADR 0048).
        if self.tabs.is_empty() {
            self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            });
        }
        let active = self.tabs[selected.min(self.tabs.len() - 1)].id();
        self.activate(active, window, cx);
        for (paths, at, case) in std::mem::take(&mut self.pending_open) {
            self.open_paths_into(paths, at, case, window, cx);
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
            // Keep a file's title while loading, including a failed load,
            // without accepting its path as a successfully opened document.
            view.session.name = record
                .markdown
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .or_else(|| record.name.clone());
            view.lifecycle.restore(
                !record.markdown.as_os_str().is_empty(),
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
        let changes = cx.observe(&view, move |this, view, cx| {
            this.remember_recent(id, cx);
            let next = SidebarStatus::of(view.read(cx));
            if next != status {
                status = next;
                cx.notify();
            }
        });
        self.tabs.open(id, view.clone());
        self.tab_subscriptions.insert(id, vec![events, changes]);
        if !record.markdown.as_os_str().is_empty() {
            self.load_document(id, record, window, cx);
        } else if record.source_only
            && let Some(path) = record.attachment
        {
            view.update(cx, |view, cx| view.open_source(path, window, cx));
        }
    }

    fn load_document(
        &mut self,
        id: u64,
        record: TabRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        {
            self.cases.member_reads += 1;
        }
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let generation = view.update(cx, |view, cx| {
            let generation = view.lifecycle.begin_load();
            view.error = None;
            cx.notify();
            generation
        });
        let weak = view.downgrade();
        let path = record.markdown.clone();
        let task = cx
            .background_executor()
            .spawn(async move { Document::open(session_store::identity(&path)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.tabs.contains(id) {
                    return;
                }
                if let Ok(document) = &result {
                    this.tabs.set_identity(id, document.path.clone());
                }
                // A dialog keeps the keyboard while the document loads behind it.
                let dialog = this.dialog_up(window);
                let _ = weak.update(cx, |view, cx| {
                    if !view.lifecycle.finish_load(generation, result.is_ok()) {
                        return;
                    }
                    match result {
                        Ok(document) => {
                            view.session.replace(document);
                            view.reset_pii(cx);
                            view.editor.update(cx, |editor, cx| {
                                editor.set_text(view.session.document.saved.clone(), cx);
                                let mut caret = record.caret.min(editor.text().len());
                                while !editor.text().is_char_boundary(caret) {
                                    caret -= 1;
                                }
                                editor.set_cursor(caret, cx);
                            });
                            view.replace_images(view.save_directory(), window, cx);
                            view.scroll
                                .set_offset(gpui::point(px(0.), px(record.scroll_y.min(0.))));
                            if record.preview_visible
                                && view.preview.generation() == 0
                                && let Some(path) = record.attachment
                            {
                                view.open_path(path, window, cx);
                            }
                            if view.active && !dialog {
                                window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                            }
                        }
                        Err(error) => {
                            view.error = Some(format!(
                                "Could not open {}: {error}",
                                record.markdown.display()
                            ));
                        }
                    }
                    cx.notify();
                });
                this.attach_case(id, window, cx);
                cx.notify();
            });
        })
        .detach();
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
        self.cases.page = None;
        self.end_rename(true, cx);
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
        // they would reach nothing (ADR 0048).
        if self.showing_overview(cx) {
            window.focus(&self.focus, cx);
        }
        self.note_destination(arrival, cx);
        cx.notify();
    }

    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> u64 {
        // From Home, the hidden placeholder becomes the new tab (ADR 0048).
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

    pub fn find_path(
        &self,
        identity: &std::path::Path,
        except: Option<u64>,
        cx: &App,
    ) -> Option<u64> {
        self.tabs
            .iter()
            .find(|tab| {
                Some(tab.id()) != except
                    && (tab.identity().map(PathBuf::as_path) == Some(identity)
                        || tab.view().is_some_and(|view| {
                            view.read(cx).session.document.path.as_deref() == Some(identity)
                        }))
            })
            .map(|tab| tab.id())
    }

    #[cfg(test)]
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths(vec![path], window, cx);
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths_at(paths, None, window, cx);
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
                    for (paths, at, case) in std::mem::take(&mut this.pending_open) {
                        this.open_paths_into(paths, at, case, window, cx);
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

    /// A confirm dialog, the Keychain explainer or a folder sheet keeps
    /// focus until answered.
    fn dialog_up(&self, window: &Window) -> bool {
        window.has_active_prompt() || self.cases.explainer || self.folder_sheets.open()
    }

    /// Open paths as new tabs before the tab now at `at` (ADR 0028), or after
    /// the last tab. Already-open paths keep their position.
    pub fn open_paths_at(
        &mut self,
        paths: Vec<PathBuf>,
        at: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_paths_into(paths, at, None, window, cx);
    }

    /// Open paths. Plain files open at once; each folder among them shows
    /// its sheet in turn, offering to add to `case` when it was dropped on
    /// that case's header (ADR 0075).
    fn open_paths_into(
        &mut self,
        paths: Vec<PathBuf>,
        at: Option<usize>,
        case: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ready {
            self.pending_open.push((paths, at, case));
            return;
        }
        // Opening activates a tab and takes focus; a confirm dialog keeps it
        // until answered, and the paths open after (ADR 0064). Checked before
        // quitting, so a cancelled quit still opens them.
        if self.dialog_up(window) {
            self.pending_open.push((paths, at, case));
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
                    this.pending_open.push((requested, at, case));
                    this.open_after_prompt(window, cx);
                    return;
                }
                this.finish_open(paths, at, window, cx);
                for folder in folders {
                    let pending = folder_sheet::Pending {
                        folder,
                        case: case.clone(),
                        at,
                    };
                    this.queue_folder(pending, window, cx);
                }
            });
        })
        .detach();
    }

    fn finish_open(
        &mut self,
        paths: Vec<(PathBuf, bool)>,
        at: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<u64>, Vec<u64>) {
        if !self.quit_flow.may_quit() {
            return Default::default();
        }
        // Anchor on a tab, not an index: tabs may close while paths resolve.
        let anchor = at.and_then(|at| self.tabs.at(at).map(|tab| tab.id()));
        let mut opened = Vec::new();
        let mut created = Vec::new();
        let mut first = None;
        let mut unsupported = Vec::new();
        for (path, supported) in paths {
            if !supported {
                // Names only: the notice is no place for the full path.
                unsupported.push(
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                        .into_owned(),
                );
                continue;
            }
            let existing = self.open_tab_for(&path, cx);
            let id = existing.unwrap_or_else(|| {
                let id = if document::is_markdown(&path) {
                    self.push(TabRecord {
                        markdown: path,
                        ..TabRecord::default()
                    })
                } else {
                    self.push(TabRecord {
                        source_only: true,
                        attachment: Some(path),
                        preview_visible: true,
                        ..TabRecord::default()
                    })
                };
                created.push(id);
                id
            });
            first.get_or_insert(id);
            opened.push(id);
        }
        if at.is_some() && !created.is_empty() {
            self.tabs.place(&created, anchor);
        }
        if !unsupported.is_empty() {
            self.notices.post(format!(
                "Unsupported files skipped: {}",
                unsupported.join(", ")
            ));
        }
        if let Some(first) = first {
            let disposable: Vec<_> = self
                .tabs
                .iter()
                .filter(|tab| {
                    tab.id() != first
                        && tab.view().map_or(tab.record().blank_disposable, |view| {
                            let view = view.read(cx);
                            view.lifecycle.blank_disposable()
                                && view.editor.read(cx).text().is_empty()
                                && view.session.document.path.is_none()
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
        cx.notify();
        (opened, created)
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
                self.new_tab(window, cx);
            }
            TabEvent::Tabs(TabsEvent::Open(paths)) => self.open_paths(paths.clone(), window, cx),
            TabEvent::Tabs(TabsEvent::Restored(reply)) if !self.quit_flow.quitting() => {
                let reply = reply.clone();
                let tab = self.new_tab(window, cx);
                // The reply sits right after the tab it answers.
                let after = self.tabs.position(id).map(|i| i + 1);
                if let Some(next) = after.and_then(|i| self.tabs.at(i)).map(|t| t.id())
                    && next != tab
                {
                    self.reorder(tab, next, cx);
                }
                if let Some(view) = self.tabs.get(tab).and_then(|t| t.view().cloned()) {
                    view.update(cx, |view, cx| view.start_restored(&reply, window, cx));
                    if let Some(case) = reply
                        .case
                        .as_ref()
                        .and_then(|link| self.case_coordinator.case(&link.id))
                    {
                        let reserved = case.reserved_aliases();
                        let dismissed = case.dismissed_hints.clone();
                        view.update(cx, |view, cx| {
                            view.pii.review_mut().reserve_aliases(&reserved);
                            view.dismiss_case_hints(&dismissed, cx);
                        });
                    }
                }
            }
            TabEvent::Import(ImportEvent::State) => {
                self.refresh_import(window, cx);
                // A converted document of a folder case joins it now.
                self.attach_case(id, window, cx);
            }
            TabEvent::Close(CloseEvent::Requested) => self.close_tab(id, window, cx),
            TabEvent::Import(ImportEvent::Ocr(state)) => self.set_ocr(state.clone(), cx),
            TabEvent::Save(SaveEvent::Saved) => {
                let saved = self
                    .tabs
                    .get(id)
                    .and_then(|tab| tab.view())
                    .and_then(|view| view.read(cx).session.document.path.clone());
                let previous = self.tabs.set_identity(id, saved);
                // Save As moves the member: the case forgets the old path.
                self.case_member_moved(id, previous, cx);
                self.case_changed(id, cx);
            }
            TabEvent::Case(CaseEvent::Changed) => self.case_changed(id, cx),
            TabEvent::Case(CaseEvent::Reply) => self.case_reply_changed(id, cx),
            TabEvent::Case(CaseEvent::OpenPage) => {
                if let Some(case) = self.tabs.get(id).and_then(|tab| tab.record().case.clone()) {
                    self.open_case(&case, window, cx);
                }
            }
            TabEvent::Case(CaseEvent::MappingMissing) => {
                // A quiet look: launch onto Home may not have checked yet.
                self.load_cases(window, cx);
                if self.case_key_banner().is_some() {
                    self.carry_out_notices(self.notices.show_locked_cases());
                    cx.notify();
                }
            }
            TabEvent::Case(CaseEvent::AddTerm(term)) => self.add_case_term(id, term, cx),
            TabEvent::Case(CaseEvent::DismissHint(from, into)) => {
                self.dismiss_case_hint(id, from, into, cx)
            }
            TabEvent::Case(CaseEvent::Create { close: true }) => {
                self.save_case_and_close(id, window, cx)
            }
            TabEvent::Case(CaseEvent::Create { close: false }) => {
                if self.quit_case_saved(id, window, cx) {
                } else if self.can_create_case(id, cx) {
                    self.create_case(id, window, cx);
                }
            }
            TabEvent::ToggleTheme => self.cycle_theme(window, cx),
            TabEvent::Close(CloseEvent::Resolved) => {
                if let Some(effect) = self.quit_flow.resolved(id) {
                    self.carry_out_quit(effect, window, cx);
                }
            }
            TabEvent::Close(CloseEvent::Cancelled) => {
                self.cases.closing_members.cancel();
                crate::updater::cancel_restart(cx);
                self.quit_flow.cancel();
                self.mapping_close_cancelled(id);
                self.cancel_case_quit();
                let views: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.view().cloned())
                    .collect();
                let owner = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    for view in views {
                        view.update(cx, |view, cx| view.resume_import(window, cx));
                    }
                    let _ = owner.update(cx, |this, cx| this.refresh_import(window, cx));
                });
            }
            TabEvent::Save(SaveEvent::Write { path, next }) => {
                let Some(other) = self.find_path(path, Some(id), cx) else {
                    if let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) {
                        let (path, next) = (path.clone(), next.clone());
                        view.update(cx, |view, cx| view.save_to(path, next, window, cx));
                    }
                    return;
                };
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "That file is already open in another tab",
                    Some("Switch to the existing tab, or choose a different Save As path."),
                    &["Switch to tab", "Cancel"],
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if answer.await.ok() == Some(0) {
                        let _ =
                            this.update_in(cx, |this, window, cx| this.activate(other, window, cx));
                    }
                })
                .detach();
                self.event(id, &TabEvent::Close(CloseEvent::Cancelled), window, cx);
            }
            TabEvent::Import(ImportEvent::Finished) => self.refresh_import(window, cx),
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
        if !self.tabs.contains(id) || !self.confirm_mapping_close(id, window, cx) {
            return;
        }
        // A member's latest mapping reaches its case before the tab goes.
        self.case_changed(id, cx);
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if let Some(view) = tab.view().cloned() {
            if !view.read(cx).dirty(cx) {
                self.remove(id, window, cx);
                return;
            }
            if view.read(cx).lifecycle.prompting() {
                return;
            }
            let effect = self.quit_flow.close(id);
            self.carry_out_quit(effect, window, cx);
        } else {
            self.remove(id, window, cx);
        }
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
        self.sidebar_row_focus.borrow_mut().remove(&id);
        let removed_opener = self.compact_row_focus.borrow_mut().remove(&id);
        let closed_reveal = self.sidebar.forget_opener(removed_opener.as_ref());
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
        // no new stop when the closed tab wasn't the current one (ADR 0081).
        let gone = navigation::Stop::Tab(id);
        let arrival = if self.navigation.current() == Some(&gone) {
            Arrival::Replace
        } else {
            Arrival::Unrecorded
        };
        let activated = if let Some(TabsEffect::Activate(next)) = effect {
            self.activate_as(next, arrival, window, cx);
            true
        } else {
            false
        };
        self.navigation.forget(&gone);
        if !activated && closed_reveal {
            self.dismiss_reveal(window, cx);
            self.focus_active(window, cx);
        }
        self.checkpoint(window, cx);
        cx.notify();
        if let Some(next) = self.cases.closing_members.removed(id) {
            self.close_tab(next, window, cx);
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
        // A question already up (the summary, or a tab's mapping) is
        // answered first.
        if self.quit_summary || self.mapping_prompt_open() {
            return;
        }
        let at_risk = self.at_risk_tabs(cx);
        if at_risk.len() < 2 {
            self.start_quit(at_risk, window, cx);
            return;
        }
        // Two or more to lose: one question first (ADR 0071).
        self.quit_summary = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "{} documents have unsaved changes or mappings",
                at_risk.len()
            ),
            Some(
                "Review each one, or discard them all and quit. A mapping not saved as a \
                 case can't be recovered, so AI replies to its document can't be restored.",
            ),
            &[
                gpui::PromptButton::new("Review…"),
                gpui::PromptButton::new("Discard All"),
                gpui::PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.quit_summary = false;
                match answer {
                    Some(0) => {
                        let at_risk = this.at_risk_tabs(cx);
                        this.start_quit(at_risk, window, cx);
                    }
                    Some(1) => this.start_quit(VecDeque::new(), window, cx),
                    _ => crate::updater::cancel_restart(cx),
                }
            });
        })
        .detach();
    }

    /// Tabs a quit would lose something from, in sidebar order: unsaved
    /// changes, an unsaved mapping, or both (ADR 0071).
    fn at_risk_tabs(&mut self, cx: &mut Context<Self>) -> VecDeque<u64> {
        self.mapping_quit_started();
        self.tabs
            .iter()
            .map(|tab| tab.id())
            .filter(|&id| {
                self.tabs
                    .get(id)
                    .and_then(|tab| tab.view())
                    .is_some_and(|view| view.read(cx).dirty(cx))
                    || self.mapping_at_risk(id, true, cx)
            })
            .collect()
    }

    /// Ask each tab in `at_risk` in turn, then quit.
    fn start_quit(&mut self, at_risk: VecDeque<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.quit_flow.quit(self.tabs.active(), at_risk);
        self.carry_out_quit(effect, window, cx);
    }

    fn carry_out_quit(&mut self, effect: QuitEffect, window: &mut Window, cx: &mut Context<Self>) {
        match effect {
            QuitEffect::Resolve(id) => {
                if self.quit_flow.quitting() && self.ask_quit_mapping(id, window, cx) {
                    return;
                }
                // The prompt's tab comes to the front; it isn't a stop.
                self.activate_as(id, Arrival::Unrecorded, window, cx);
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.request(Next::Close, window, cx));
                }
            }
            QuitEffect::Settle { restore } => {
                if let Some(active) = restore {
                    self.activate_as(active, Arrival::Unrecorded, window, cx);
                }
                let histories: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| {
                        Some((tab.id(), tab.view()?.read(cx).editor.read(cx).history_id()))
                    })
                    .collect();
                self.quit_flow.settle(histories);
                self.checkpoint(window, cx);
                if !self.session_writer.writing() {
                    self.finish_quit(window, cx);
                }
            }
            QuitEffect::Remove(id) => self.remove(id, window, cx),
        }
    }

    fn finish_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed: VecDeque<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let view = tab.view()?.read(cx);
                (view.dirty(cx)
                    && self
                        .quit_flow
                        .changed_since(tab.id(), view.editor.read(cx).history_id()))
                .then_some(tab.id())
            })
            .collect();
        match self.quit_flow.end(changed) {
            QuitEnd::Ask(effect) => {
                self.cancel_case_quit();
                self.carry_out_quit(effect, window, cx);
            }
            QuitEnd::Close => {
                if self.flush_cases(window, cx) {
                    self.close_window(window, cx);
                }
            }
        }
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.update(cx, |panel, cx| panel.cancel_setup(cx));
        for tab in &self.tabs {
            if let Some(view) = tab.view() {
                Self::release(view, window, cx);
            }
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
            collapsed_cases: {
                let mut ids: Vec<_> = self.cases.collapsed.iter().cloned().collect();
                ids.sort();
                ids
            },
            cases: self.case_coordinator.summaries(),
            ..Session::default()
        };
        for tab in &self.tabs {
            let mut record = tab.record().clone();
            if let Some(view) = tab.view() {
                let view = view.read(cx);
                record.source_only =
                    view.lifecycle.source_only() || view.lifecycle.generated_unedited();
                record.blank = !record.source_only
                    && record.markdown.as_os_str().is_empty()
                    && view.session.document.path.is_none()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty();
                record.blank_disposable = record.blank && view.lifecycle.blank_disposable();
                record.name = if record.blank {
                    view.session.name.clone()
                } else {
                    None
                };
                // A document still waiting to join its folder's case keeps it.
                if let Some(link) = view.case.link() {
                    record.case = Some(link.id.clone());
                }
                if let Some(path) = &view.session.document.path {
                    record.markdown = path.clone();
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
            if record.markdown.as_os_str().is_empty() && !record.source_only && !record.blank {
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
                        this.notices.post(format!("Could not save session: {error}"));
                        let was_quitting = this.quit_flow.abandon();
                        this.cancel_case_quit();
                        if was_quitting {
                            let answer = window.prompt(PromptLevel::Warning,
                                "Could not save the tab list",
                                Some("Your saved Markdown files are safe. Cancel to retry, or quit using the previous session list."),
                                &["Cancel", "Quit without updating session"], cx);
                            cx.spawn_in(window, async move |this, cx| {
                                if answer.await.ok() == Some(1) {
                                    let _ = this.update_in(cx, |this, window, cx| this.finish_quit(window, cx));
                                } else {
                                    let _ = this.update(cx, |_, cx| crate::updater::cancel_restart(cx));
                                }
                            }).detach();
                        }
                        cx.notify();
                        return;
                    }
                }
                if this.quit_flow.finishing() && !this.session_writer.writing() {
                    this.finish_quit(window, cx);
                }
            });
        })
        .detach();
    }

    fn cycle(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.quit_flow.quitting() || self.tabs.is_empty() {
            return;
        }
        if let Some(TabsEffect::Activate(next)) = self.tabs.cycle(delta) {
            self.activate(next, window, cx);
        }
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

    /// Track the pointer over the collapsed rail or its reveal (ADR 0027).
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
                    cx.background_executor().timer(SLIDE).await;
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

    /// A context menu, drag, the pointer, or keyboard focus keeps the reveal open.
    fn reveal_held(&self, window: &Window, cx: &App) -> bool {
        self.tab_menu.is_some()
            || cx.has_active_drag()
            || self
                .sidebar
                .holds(|| self.reveal_focus.contains_focused(window, cx))
    }

    fn show_reveal(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.sidebar.show(keyboard);
        cx.notify();
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
        let mut focused = [
            (&self.rail_toggle_focus, &self.panel_toggle_focus),
            (&self.rail_new_focus, &self.panel_new_focus),
            (&self.rail_home_focus, &self.panel_home_focus),
        ]
        .into_iter()
        .find(|(rail, _)| rail.is_focused(window))
        .map(|(rail, panel)| (rail.clone(), panel.clone()));
        if focused.is_none() {
            let compact = self
                .compact_row_focus
                .borrow()
                .iter()
                .find(|(_, focus)| focus.is_focused(window))
                .map(|(id, focus)| (*id, focus.clone()));
            focused = compact.map(|(id, rail)| {
                let row = self
                    .sidebar_row_focus
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone();
                (rail, row)
            });
        }
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

    /// Where files dropped on the document list would open (ADR 0028).
    fn file_drop_index(&self) -> Option<usize> {
        self.sidebar.drop_index(self.tabs.len())
    }

    fn clear_file_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let effect = self.sidebar.clear_drag();
        self.carry_out_sidebar(effect, window, cx);
        cx.notify();
    }

    /// Open OS files dropped on the sidebar: at the insertion line on the
    /// list, or after the last tab on the bare rail.
    fn drop_files(
        &mut self,
        paths: &gpui::ExternalPaths,
        list: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = list.then(|| self.file_drop_index()).flatten();
        self.clear_file_drag(window, cx);
        // A drop from Finder leaves mdoc in the background; what it opens
        // (a dialog among them) needs the keyboard.
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            self.open_paths_at(paths.paths().to_vec(), at, window, cx);
        }
    }

    /// OS files dropped on a case header: a folder among them offers Add to
    /// that case (ADR 0075); plain files open as on the list.
    fn drop_on_case(
        &mut self,
        paths: &gpui::ExternalPaths,
        case: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_file_drag(window, cx);
        activate_for_drop(window, cx);
        if !self.quit_flow.quitting() {
            let paths = paths.paths().to_vec();
            self.open_paths_into(paths, None, Some(case.to_string()), window, cx);
        }
    }

    fn reorder(&mut self, from: u64, to: u64, cx: &mut Context<Self>) {
        if self.quit_flow.quitting() {
            return;
        }
        if let Some(TabsEffect::Repaint) = self.tabs.reorder(from, to) {
            cx.notify();
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let expanded = self.sidebar.visible();
        let revealed = !expanded && self.sidebar.reveal() != Reveal::Hidden;
        let home = self.showing_overview(cx);
        let mut compact_rows = Vec::with_capacity(self.tabs.len());
        // One Tab stop for the list: the active tab, else the first listed.
        let listed = self.listed_tabs(cx);
        let roving = self
            .sidebar
            .cursor()
            .filter(|id| listed.contains(id))
            .or_else(|| {
                listed
                    .iter()
                    .copied()
                    .find(|id| *id == self.tabs.active() && !home)
            })
            .or(listed.first().copied());
        let accent = theme.search_accent();
        let drop_index = self.file_drop_index();
        let labels: Vec<TabLabel> = self.tabs.iter().map(|tab| tab_label(tab, cx)).collect();
        let rows = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let id = tab.id();
                // Home's blank placeholder is never listed (ADR 0048).
                if self.is_placeholder(tab, cx) {
                    return None;
                }
                let view = tab.view().map(|v| v.read(cx));
                let TabLabel {
                    name,
                    identity,
                    format,
                    source,
                } = labels[index].clone();
                let dirty = view.is_some_and(|v| v.lifecycle.dirty());
                let running = view.is_some_and(|v| {
                    v.lifecycle.loading() || v.import.busy() || v.preview.loading()
                });
                let busy = running || view.is_some_and(|v| v.preview.queued());
                let error = view.is_some_and(|v| {
                    v.error.is_some() || v.preview.failed() || v.import.ocr_required().is_some()
                });
                let pseudonymized = view.is_some_and(|v| v.pseudonymized());
                let recognized = view.is_some_and(|v| v.recognized());
                let identity = identity.as_path();
                let parent = identity
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let tooltip = format!(
                    "{name}\n{parent}{}{}{}{}{}{}",
                    // A saved conversion's badge is MD; the tooltip keeps
                    // where it came from.
                    source
                        .as_ref()
                        .filter(|source| source.as_path() != identity)
                        .and_then(|source| source.file_name())
                        .map(|source| format!("\nFrom {}", source.to_string_lossy()))
                        .unwrap_or_default(),
                    if recognized {
                        "\nRecognized with OCR"
                    } else {
                        ""
                    },
                    if pseudonymized { "\nPseudonymized" } else { "" },
                    if dirty { "\nUnsaved changes" } else { "" },
                    if busy { "\nProcessing…" } else { "" },
                    if error { "\nNeeds attention" } else { "" }
                );
                // Same-name tabs show the shortest distinguishing folder, or
                // their type when they share one (a DOCX's conversion beside
                // an .md of that name).
                let peers: Vec<&TabLabel> =
                    labels.iter().filter(|other| other.name == name).collect();
                let duplicate = peers.len() > 1;
                let peers: Vec<PathBuf> = peers
                    .iter()
                    .map(|other| other.identity.clone())
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect();
                let parent_label = if identity.as_os_str().is_empty() {
                    String::new()
                } else {
                    disambiguating_parent(identity, &peers).unwrap_or_else(|| format.clone())
                };
                // Unsaved duplicates have no folder; an empty second line would
                // lift the name off the row's centre.
                let duplicate = duplicate && !parent_label.is_empty();
                let row_label = if duplicate {
                    format!("{name} · {parent_label}")
                } else {
                    name.clone()
                };
                let focus = self
                    .sidebar_row_focus
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone()
                    .tab_stop(roving == Some(id));
                // Rail controls leave the tab order while the reveal covers them.
                let compact_focus = self
                    .compact_row_focus
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone()
                    .tab_stop(!revealed && roving == Some(id));
                compact_rows.push(
                    ui::control(format!("compact-tab-{id}-at-{index}"), "", theme, true)
                        .map(|v| self.sidebar_tab_menu(v, id, cx))
                        .key_context("UiControl SidebarTab")
                        .tab_stop(!revealed && roving == Some(id))
                        .map(|v| {
                            ui::reveal_focus(
                                v.track_focus(&compact_focus),
                                compact_focus.clone(),
                                self.compact_scroll.clone(),
                            )
                        })
                        .role(gpui::Role::Tab)
                        .aria_selected(self.tabs.active() == id && !home)
                        .aria_label(name.clone())
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("compact-tab-{id}"))
                        })
                        // A document glyph, as Anoni's rail draws its items: the
                        // active tab gets a soft fill; status is a small dot.
                        .p_0()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(self.tabs.active() == id && !home, |v| {
                            v.bg(theme.sidebar_selected())
                        })
                        .child(
                            gpui::svg()
                                .data(ui::Icon::Source.data())
                                .size(px(16.))
                                .text_color(if self.tabs.active() == id && !home {
                                    palette.header_fg
                                } else {
                                    palette.header_muted
                                }),
                        )
                        .when(!revealed, |v| {
                            v.tooltip(style::tooltip(format!("{tooltip}\n{format}"), theme))
                        })
                        .when(dirty || busy || error, |v| {
                            v.child(if running && !error {
                                div()
                                    .absolute()
                                    .top(px(1.))
                                    .right(px(1.))
                                    .size(px(10.))
                                    .child(ui::spinner(("compact-tab-activity", id), theme))
                            } else {
                                div()
                                    .absolute()
                                    .top(px(5.))
                                    .right(px(5.))
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(if error {
                                        style::markdown_style(theme).alert_warning
                                    } else {
                                        palette.header_muted
                                    })
                            })
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if !this.quit_flow.quitting() {
                                this.activate(id, window, cx);
                            }
                        }))
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.menu_return = window.focused(cx);
                                window.focus(&this.menu_focus, cx);
                                this.tab_menu = Some((id, event.position));
                                cx.notify();
                            }),
                        ),
                );
                Some(
                    // Tooltip state includes its old hover bounds. A row
                    // moved by insertion or reordering needs fresh state.
                    crate::ui::focus_ring(format!("tab-{id}-at-{index}"), theme)
                        .map(|v| self.sidebar_tab_menu(v, id, cx))
                        .map(|v| {
                            ui::reveal_focus(
                                v.track_focus(&focus),
                                focus,
                                self.sidebar_scroll.clone(),
                            )
                        })
                        .key_context("UiControl SidebarTab")
                        .tab_index(0)
                        .tab_stop(roving == Some(id))
                        .role(gpui::Role::Tab)
                        .aria_selected(self.tabs.active() == id && !home)
                        .aria_label(row_label.clone())
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("tab-{id}"))
                        })
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap_1()
                        .px_2()
                        .min_h(px(32.))
                        .mb(px(2.))
                        .rounded_md()
                        .when(self.tabs.active() == id && !home, |v| {
                            v.bg(theme.sidebar_selected())
                        })
                        .hover(move |v| v.bg(palette.placeholder_bg))
                        .tooltip(style::tooltip(tooltip, theme))
                        .cursor_pointer()
                        .child(
                            // One badge column, so names line up whether the
                            // tab is MD, PDF or DOCX.
                            div()
                                .min_w(px(32.))
                                .text_size(px(10.))
                                .text_color(palette.header_muted)
                                .flex_shrink_0()
                                .child(format),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().truncate().child(name.clone()))
                                .when(duplicate, |v| {
                                    v.child(
                                        div()
                                            .truncate()
                                            .text_size(px(10.))
                                            .text_color(palette.header_muted)
                                            .child(parent_label),
                                    )
                                }),
                        )
                        .when(recognized, |v| {
                            v.child(tab_badge(ui::Icon::Ocr, palette.header_muted))
                        })
                        .when(pseudonymized, |v| {
                            v.child(tab_badge(ui::Icon::Anonymous, palette.header_muted))
                        })
                        .when(dirty || busy || error, |v| {
                            v.child(
                                div()
                                    .text_color(if error {
                                        style::markdown_style(theme).alert_warning
                                    } else {
                                        palette.header_muted
                                    })
                                    .child(if running && !error {
                                        ui::spinner(("tab-activity", id), theme)
                                    } else {
                                        div()
                                            .child(if error {
                                                "!"
                                            } else if busy {
                                                "…"
                                            } else {
                                                "•"
                                            })
                                            .into_any_element()
                                    }),
                            )
                        })
                        .child(
                            ui::control(("close-tab", id), "×", theme, true)
                                // ⌫ on the row closes it; the button is for the pointer.
                                .tab_stop(false)
                                // Named for its tab, as the tab is (ADR 0064).
                                .aria_label(format!("Close {row_label}"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_tab(id, window, cx);
                                })),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if !this.quit_flow.quitting() {
                                this.activate(id, window, cx);
                            }
                        }))
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.menu_return = window.focused(cx);
                                window.focus(&this.menu_focus, cx);
                                this.tab_menu = Some((id, event.position));
                                cx.notify();
                            }),
                        )
                        .on_drag(TabDrag { id, label: name }, |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        })
                        .drag_over::<TabDrag>(move |v, _, _, _| {
                            v.border_t_2().border_color(theme.search_accent())
                        })
                        // The same line marks where dropped OS files open.
                        .when(drop_index == Some(index), |v| {
                            v.border_t_2().border_color(accent)
                        })
                        .on_drag_move(cx.listener(
                            move |this,
                                  event: &gpui::DragMoveEvent<gpui::ExternalPaths>,
                                  window,
                                  cx| {
                                let position = event.event.position;
                                let row = event.bounds.contains(&position).then(|| {
                                    let lower = position.y > event.bounds.center().y;
                                    (id, index + usize::from(lower))
                                });
                                let effect = this.sidebar.drag_over_row(id, row);
                                this.carry_out_sidebar(effect, window, cx);
                            },
                        ))
                        .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                            this.reorder(drag.id, id, cx)
                        })),
                )
            })
            .collect::<Vec<_>>();
        // A case's header sits above its first member in each run of tabs.
        let mut rows_with_cases = Vec::with_capacity(rows.len());
        if let Some(page) = &self.cases.page
            && !self.tabs.iter().any(|tab| {
                self.sidebar_case_link(tab, cx)
                    .is_some_and(|link| link.id == page.id())
            })
            && let Some(summary) = self
                .case_coordinator
                .summaries()
                .iter()
                .find(|summary| summary.id == page.id())
        {
            rows_with_cases.push(self.case_header(&summary.link(), 0, summary.documents, cx));
        }
        let mut previous: Option<Option<String>> = None;
        for (index, row) in rows.into_iter().enumerate() {
            let Some(row) = row else {
                continue;
            };
            let link = self.sidebar_case_link(&self.tabs[index], cx);
            let id = link.as_ref().map(|l| l.id.clone());
            // Tabs without a case gather under "Open".
            if link.is_none() && previous.as_ref() != Some(&None) {
                rows_with_cases.push(self.section_label("Open"));
            }
            if let Some(link) = link.filter(|_| previous.as_ref() != Some(&id)) {
                let count = self
                    .tabs
                    .iter()
                    .skip(index)
                    .take_while(|t| self.sidebar_case_link(t, cx).map(|link| link.id) == id)
                    .count();
                rows_with_cases.push(self.case_header(&link, self.tabs[index].id(), count, cx));
            }
            let folded = id
                .as_ref()
                .is_some_and(|id| self.cases.collapsed.contains(id))
                && self.tabs[index].id() != self.tabs.active();
            previous = Some(id);
            if !folded {
                rows_with_cases.push(row.into_any_element());
            }
        }
        let rows = rows_with_cases;
        let new_row = ui::control("sidebar-new", "", theme, true)
            .track_focus(&self.panel_new_focus)
            .aria_label("New Markdown tab")
            .tooltip(style::tooltip("New Markdown tab".into(), theme))
            .when(cfg!(test), |v| v.debug_selector(|| "sidebar-new".into()))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .mt_1()
            .min_h(px(30.))
            .text_color(palette.header_muted)
            .child(
                gpui::svg()
                    .data(ui::Icon::Plus.data())
                    .size(px(14.))
                    .text_color(palette.header_muted),
            )
            .child("New tab")
            .when(drop_index == Some(self.tabs.len()), |v| {
                v.border_t_2().border_color(accent)
            })
            .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx));
        // The full list, either pinned open or revealed over the editor. Its
        // width is fixed so animations clip it rather than reflow it.
        // A collapse slides the full list away over the content (ADR 0048).
        let sliding = self.sidebar.sliding(std::time::Instant::now()) && !cx.reduce_motion();
        let collapsing = sliding && !expanded && !revealed;
        let mut panel = (expanded || revealed || collapsing).then(|| {
            div()
                .id("sidebar-panel")
                .w(px(PANEL_WIDTH - 1.))
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
                        this.drop_files(paths, true, window, cx)
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
                        .id("sidebar-tabs")
                        .role(gpui::Role::TabList)
                        .aria_label("Open documents")
                        .track_scroll(&self.sidebar_scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px_2()
                        .pb_1()
                        .children(rows)
                        .child(new_row),
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
                            .child(div().w(px(PANEL_WIDTH)).h_full().child(list))
                            .with_animation(
                                ("sidebar-open", self.sidebar.generation()),
                                Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                                |v, delta| v.w(px(RAIL_WIDTH + (PANEL_WIDTH - RAIL_WIDTH) * delta)),
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
                // lights, the expand toggle and the document header (ADR 0048).
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
                // A hairline between the app's places and the open tabs.
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .py_2()
                        .flex_shrink_0()
                        .child(div().w(px(18.)).h(px(1.)).bg(palette.border)),
                )
                .child(
                    div()
                        .id("compact-tabs")
                        .track_scroll(&self.compact_scroll)
                        .role(gpui::Role::TabList)
                        .aria_label("Open documents")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        // Tabs share the icons' centre line (ADR 0048).
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .pb_1()
                        .children(compact_rows)
                        .child(
                            ui::header_icon(
                                "sidebar-new-collapsed",
                                "New Markdown tab",
                                ui::Icon::Plus,
                                theme,
                                false,
                            )
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
        let width = |open: bool| if open { PANEL_WIDTH } else { RAIL_WIDTH };
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
                        this.drop_files(paths, false, window, cx)
                    },
                ))
            });
        if revealed && let Some(panel) = panel.take() {
            let (from, to) = if self.sidebar.reveal() == Reveal::Shown {
                (RAIL_WIDTH, PANEL_WIDTH)
            } else {
                (PANEL_WIDTH, RAIL_WIDTH)
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
                        Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
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
                    .child(div().w(px(PANEL_WIDTH)).h_full().child(list))
                    .with_animation(
                        ("sidebar-close", self.sidebar.generation()),
                        Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                        |v, delta| v.w(px(PANEL_WIDTH - (PANEL_WIDTH - RAIL_WIDTH) * delta)),
                    ),
            ));
        }
        sidebar.into_any_element()
    }
}

/// The collapsed rail. Narrow: the traffic lights and the expand toggle sit
/// in the title row beside the document header, above it (ADR 0048).
const RAIL_WIDTH: f32 = 44.;
const PANEL_WIDTH: f32 = 240.;
const REVEAL_DELAY: Duration = Duration::from_millis(200);
const REVEAL_GRACE: Duration = Duration::from_millis(300);

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
    // Quiet, like every other rail and title-bar icon (ADR 0048).
    ui::header_icon("sidebar-toggle", label, ui::Icon::Sidebar, theme, false)
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

/// What a sidebar row names a tab by.
#[derive(Clone)]
struct TabLabel {
    name: String,
    /// The source while it is only a conversion, else the saved Markdown.
    identity: PathBuf,
    /// The badge: the source's type until the tab is saved as Markdown.
    format: String,
    source: Option<PathBuf>,
}

fn tab_label(tab: &Tab, cx: &App) -> TabLabel {
    let view = tab.view().map(|v| v.read(cx));
    let name = view.map(|v| v.display_name()).unwrap_or_else(|| {
        // A conversion not loaded yet is named as it will be once loaded
        // (`DocumentSession::display_name`: the source's name as `.md`), so
        // a restored tab's name doesn't change when it is first opened.
        let path = if tab.record().source_only {
            tab.record()
                .attachment
                .as_ref()
                .map_or_else(|| tab.record().markdown.clone(), |p| p.with_extension("md"))
        } else {
            tab.record().markdown.clone()
        };
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| tab.record().name.clone())
            .unwrap_or_else(|| "Untitled.md".into())
    });
    let saved = view.map(|v| v.session.document.path.as_ref());
    let path = saved.flatten().unwrap_or(&tab.record().markdown);
    let saved = saved.map_or(
        !tab.record().source_only && !tab.record().markdown.as_os_str().is_empty(),
        |path| path.is_some(),
    );
    let source = view
        .and_then(|v| v.preview.attachment().or(v.preview.source()))
        .or(tab.record().attachment.as_ref())
        .cloned();
    let identity = if saved {
        path.clone()
    } else {
        source.clone().unwrap_or_else(|| path.clone())
    };
    let format = identity
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md")
        .to_uppercase();
    TabLabel {
        name,
        identity,
        format,
        source,
    }
}

/// Show the shortest parent suffix that distinguishes this document's
/// location; `None` when a peer shares its folder.
fn disambiguating_parent(path: &std::path::Path, peers: &[PathBuf]) -> Option<String> {
    let parent = path.parent()?;
    let components: Vec<_> = parent.components().collect();
    for depth in 1..=components.len() {
        let suffix: PathBuf = components[components.len() - depth..].iter().collect();
        if !peers
            .iter()
            .any(|peer| peer != path && peer.parent().is_some_and(|p| p.ends_with(&suffix)))
        {
            return Some(suffix.to_string_lossy().into_owned());
        }
    }
    None
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar.settle();
        self.note_region_focus(window, cx);
        self.reveal_for_keyboard(window, cx);
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
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        if !cx.has_active_drag() && self.shell.drop.is_some() {
            self.shell.drop = None;
        }
        // Notices wait behind the header's bell; a new one opens it (ADR 0048),
        // except locked cases, which open it only when a locked case is
        // needed (ADR 0043).
        let (notices, effect) = self
            .notices
            .count(self.case_key_banner().is_some(), self.case_key_needed());
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
        // traffic lights and the expand toggle that sit in it (ADR 0048).
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
                this.sidebar_step(-1, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ui::SidebarNext, window, cx| {
                    this.sidebar_step(1, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ui::SidebarCloseTab, window, cx| {
                this.sidebar_close_focused(window, cx)
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
                if let Some(view) = this.clipboard_document("Restore AI reply", window, cx) {
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
            .on_drag_move(cx.listener(
                |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    this.home_drag(event, cx)
                },
            ))
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                this.close_tab(this.tabs.active(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &Import, window, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.import(&Import, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.cycle(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.cycle(-1, window, cx)))
            // Esc closes Notices first, wherever focus is: a drop's notice
            // opens while the document keeps focus (ADR 0062).
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
                if let Some(tab) = this.tabs.get(this.tabs.active()) {
                    this.load_document(tab.id(), tab.record().clone(), window, cx);
                }
            }))
            .when_some(self.tab_menu, |view, (id, position)| {
                view.child(
                    gpui::deferred(
                        gpui::anchored().position(position).snap_to_window().child(
                            ui::panel("tab-context-menu", self.theme.get())
                                .key_context("UiPanel UiMenu")
                                .track_focus(&self.menu_focus)
                                .tab_group()
                                .tab_stop(false)
                                .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                    this.tab_menu = None;
                                    if let Some(focus) = this.menu_return.take() {
                                        window.focus(&focus, cx);
                                    }
                                    cx.notify();
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
                                        this.tab_menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(self.case_menu_items(id, cx))
                                .child(
                                    crate::ui::focus_ring("context-close-tab", self.theme.get())
                                        .role(gpui::Role::Button)
                                        .aria_label("Close tab")
                                        .key_context("UiControl")
                                        .tab_index(0)
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "context-close-tab".into())
                                        })
                                        .px_3()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .hover(|v| {
                                            v.bg(self.theme.get().pdf_style().placeholder_bg)
                                        })
                                        .child("Close tab")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.tab_menu = None;
                                            this.close_tab(id, window, cx);
                                        })),
                                ),
                        ),
                    )
                    // Above the sidebar reveal it was opened from.
                    .with_priority(1),
                )
            })
            .children(self.case_list(cx))
            .children(self.case_header_menu(cx))
            .children(self.folder_sheet_view(window, cx))
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
    /// The inset content card on the window ground (ADR 0048): workspace bars,
    /// then Home or the active document.
    fn card(&mut self, home: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let content = if self.cases.page.is_some() {
            self.case_page(window, cx)
        } else if home {
            self.home_view(window, cx)
        } else {
            match self.active_view() {
                Some(active) => active.into_any_element(),
                None => div().into_any_element(),
            }
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            // Expanded: one margin all round. Collapsed: the view's header
            // fills the title row and it draws its own card below (ADR 0048).
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
                    .when(home, |v| {
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

/// Keyboard reach (ADR 0062): F6 regions, Tab from the window, and the
/// sidebar's tab list.
impl Workspace {
    /// Ctrl+Enter or Shift+F10 (and the Menu key on Windows) opens the
    /// focused tab's menu at its rendered row, including the collapsed rail.
    /// It exposes case actions without a pointer.
    fn sidebar_tab_menu(&self, row: ui::Control, id: u64, cx: &mut Context<Self>) -> ui::Control {
        let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let drawn = bounds.clone();
        row.relative()
            .child(
                gpui::canvas(move |b, _, _| drawn.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_action(cx.listener(move |this, _: &ui::SidebarMenu, window, cx| {
                this.menu_return = window.focused(cx);
                this.tab_menu = Some((id, bounds.get().bottom_left()));
                window.focus(&this.menu_focus, cx);
                cx.notify();
            }))
    }

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
            || self.cases.list_open
            || self.cases.explainer
            || self.folder_sheets.open()
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
            || self.cases.list_open
            || self.cases.explainer
            || self.folder_sheets.open()
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
                    // The list's one Tab stop follows the active tab it
                    // landed on, so Tab leaves the list from there.
                    if region == *self.focus_regions.sidebar() && !self.showing_overview(cx) {
                        self.sidebar.set_cursor(Some(self.tabs.active()));
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

    /// Where F6 enters the sidebar (ADR 0062): the active tab's row, or
    /// Home's row on Home, in the layout drawn now. That is the expanded
    /// list (or the rail's open reveal), or the collapsed rail, whose row
    /// opens the reveal. None when no such row is shown (a case page): the
    /// region's first control.
    fn sidebar_landing(&self, cx: &App) -> Option<gpui::FocusHandle> {
        let list = self.sidebar.visible() || self.sidebar.reveal() == Reveal::Shown;
        if self.showing_home(cx) {
            let home = if list {
                &self.panel_home_focus
            } else {
                &self.rail_home_focus
            };
            return Some(home.clone());
        }
        if self.cases.page.is_some() {
            return None;
        }
        // The rail's row opens the reveal and hands focus to the list's row,
        // so the tab must be in the list too (a folded case member is not).
        let tab = self.tabs.active();
        if !self.listed_tabs(cx).contains(&tab) {
            return None;
        }
        let rows = if list {
            &self.sidebar_row_focus
        } else {
            &self.compact_row_focus
        };
        rows.borrow().get(&tab).cloned()
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

    /// Tab / Shift+Tab outside the editor, menus and dialogs (ADR 0062).
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

    /// The tabs the expanded sidebar lists, in order: not Home's
    /// placeholder, nor inactive members of a folded case.
    fn listed_tabs(&self, cx: &App) -> Vec<u64> {
        self.tabs
            .iter()
            .filter(|tab| !self.is_placeholder(tab, cx))
            .filter(|tab| {
                let case = tab.view().and_then(|v| v.read(cx).case.link());
                tab.id() == self.tabs.active()
                    || !case.is_some_and(|link| self.cases.collapsed.contains(&link.id))
            })
            .map(|tab| tab.id())
            .collect()
    }

    /// The tabs the collapsed rail shows: every one but the placeholder.
    fn rail_tabs(&self, cx: &App) -> Vec<u64> {
        self.tabs
            .iter()
            .filter(|tab| !self.is_placeholder(tab, cx))
            .map(|tab| tab.id())
            .collect()
    }

    /// The listed tab whose sidebar row (expanded or rail) has focus, with
    /// the row handles of that layout.
    fn focused_sidebar_row(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(
        usize,
        Vec<u64>,
        std::collections::HashMap<u64, gpui::FocusHandle>,
    )> {
        for (rows, listed) in [
            (&self.sidebar_row_focus, self.listed_tabs(cx)),
            (&self.compact_row_focus, self.rail_tabs(cx)),
        ] {
            let rows = rows.borrow();
            if let Some(index) = listed
                .iter()
                .position(|id| rows.get(id).is_some_and(|f| f.is_focused(window)))
            {
                return Some((index, listed, rows.clone()));
            }
        }
        None
    }

    /// ↑ / ↓ in the sidebar's tab list move focus to the neighbouring tab.
    fn sidebar_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((index, listed, rows)) = self.focused_sidebar_row(window, cx) else {
            return;
        };
        let next = index.saturating_add_signed(delta).min(listed.len() - 1);
        if let Some(focus) = rows.get(&listed[next]) {
            self.sidebar.set_cursor(Some(listed[next]));
            window.focus(focus, cx);
            cx.notify();
        }
    }

    /// ⌫ on a sidebar tab closes it; focus stays in the list.
    fn sidebar_close_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((index, listed, rows)) = self.focused_sidebar_row(window, cx) else {
            return;
        };
        let id = listed[index];
        let neighbour = listed
            .get(index + 1)
            .or(index.checked_sub(1).and_then(|i| listed.get(i)))
            .copied();
        self.close_tab(id, window, cx);
        if !self.tabs.contains(id)
            && let Some(focus) = neighbour.and_then(|n| rows.get(&n))
        {
            self.sidebar.set_cursor(neighbour);
            window.focus(focus, cx);
        }
        cx.notify();
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
