//! Window-level tabs. Each initialized tab owns a retained document view;
//! unopened session records allocate no editor, PDF, or conversion worker.
use super::*;
use crate::session_store::{Session, TabRecord};
use gpui::{Animation, AnimationExt, AnyElement, EventEmitter};
use std::{collections::VecDeque, time::Duration};

#[path = "workspace_cases.rs"]
mod cases_ui;
#[path = "workspace_keychain.rs"]
mod keychain_ui;
#[path = "workspace_shell.rs"]
mod shell;
pub(crate) use cases_ui::{CancelCaseName, ConfirmCaseName, bind_keys as bind_case_keys};
pub(crate) use shell::bind_keys as bind_shell_keys;

pub(super) enum TabEvent {
    New,
    Open(Vec<PathBuf>),
    ImportState,
    CloseRequested,
    CloseResolved,
    ToggleTheme,
    Saved,
    OcrState(OcrState),
    CloseCancelled,
    SaveConflict(u64),
    Docx {
        path: PathBuf,
        generation: u64,
        cancel: Arc<AtomicBool>,
    },
    /// Open a copied AI reply in a new tab and restore its aliases (ADR 0041).
    Restored(Arc<pii::ui::RestoredReply>),
    /// The tab's mapping changed; save it into the tab's case.
    CaseChanged,
    /// Add a name to the tab's case before it appears (ADR 0041 #11).
    AddCaseTerm(String),
    /// Save the tab's mapping as a case, from the Inspector (ADR 0048, C).
    CreateCase,
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
            source: view.preview.source.clone(),
            dirty: view.dirty_cached,
            loading: view.loading || view.preview.loading,
            queued: view.preview.queued || view.auto_convert_pending,
            importing: view.job.busy(),
            error: view.error.is_some() || view.preview.retryable || view.ocr_required.is_some(),
            pseudonymized: view.pseudonymized(),
            recognized: view.recognized(),
        }
    }
}

struct Tab {
    id: u64,
    record: TabRecord,
    identity: Option<PathBuf>,
    view: Option<Entity<DocumentView>>,
    /// Last opened/saved path observed, independent of the capped recent list.
    recent_path: Option<PathBuf>,
    subscriptions: Vec<Subscription>,
}
struct DocxJob {
    id: u64,
    path: PathBuf,
    generation: u64,
    cancel: Arc<AtomicBool>,
}

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
    tabs: Vec<Tab>,
    active: u64,
    next_id: u64,
    sidebar_visible: bool,
    sidebar_choice: Option<bool>,
    /// Bumped by explicit toggles so only they animate the sidebar width.
    sidebar_generation: u64,
    /// When the current toggle's slide ends (ADR 0048).
    sidebar_slide: Option<std::time::Instant>,
    reveal: Reveal,
    reveal_generation: u64,
    /// Opened by keyboard focus, so focus inside it also keeps it open.
    reveal_keyboard: bool,
    /// Pointer over (rail, reveal).
    reveal_hover: (bool, bool),
    /// The timer still allowed to open (`true`) or close (`false`) the reveal.
    reveal_pending: Option<(bool, u64)>,
    reveal_ticket: u64,
    reveal_focus: gpui::FocusHandle,
    /// The rail control that keyboard focus came from; Escape returns there.
    reveal_return: Option<gpui::FocusHandle>,
    /// A rail control focused by Escape that must not reveal again.
    reveal_escaped: Option<gpui::FocusHandle>,
    /// An OS file drag over the sidebar (ADR 0028): the row under the pointer
    /// with the insertion index its half selects, the list under the pointer,
    /// and the collapsed rail under the pointer.
    file_drop_row: Option<(u64, usize)>,
    file_drop_list: bool,
    file_drag_rail: bool,
    rail_toggle_focus: gpui::FocusHandle,
    rail_new_focus: gpui::FocusHandle,
    panel_toggle_focus: gpui::FocusHandle,
    panel_new_focus: gpui::FocusHandle,
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
    pending_open: Vec<PathBuf>,
    notice: Option<String>,
    /// Last run's masked crash report and whether its preview is open (ADR 0047).
    crash: Option<(String, bool)>,
    tab_menu: Option<(u64, gpui::Point<gpui::Pixels>)>,
    import_busy: Arc<AtomicBool>,
    docx_queue: VecDeque<DocxJob>,
    docx_running: Option<(u64, u64)>,
    quitting: Option<VecDeque<u64>>,
    quit_active: Option<u64>,
    closing_tab: Option<u64>,
    finishing: bool,
    /// Histories whose save/discard decisions have already been resolved, by tab.
    quit_checked: std::collections::HashMap<u64, u64>,
    session_path: Option<PathBuf>,
    persisted: Option<Session>,
    writing: bool,
    pending_write: Option<Session>,
    cases: cases_ui::CaseState,
    /// The sidebar's theme pick; `theme` holds what it resolves to (ADR 0048).
    theme_choice: style::ThemeChoice,
    /// Home fills the card instead of the active tab (ADR 0048).
    home: bool,
    /// Shared with document headers: title-bar inset and drag state.
    title: style::TitleBar,
    shell: shell::ShellState,
    /// The sidebar's Settings row; Settings returns focus here (ADR 0048).
    settings_focus: gpui::FocusHandle,
    _appearance: Option<Subscription>,
    _checkpoint: Option<gpui::Task<()>>,
}

impl Workspace {
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
            this.crash = crate::crash::pending().map(|report| (report, false));
        }
        this.session_path = if cfg!(test) {
            None
        } else {
            dirs::data_local_dir().map(|path| path.join("mdoc/session.json"))
        };
        let path = this.session_path.clone();
        let task = cx.background_executor().spawn(async move {
            path.map_or_else(|| Ok(Session::default()), |path| session_store::load(&path))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let session = match result {
                    Ok(session) => session,
                    Err(error) => {
                        this.notice = Some(format!("Could not restore tabs: {error}"));
                        Session::default()
                    }
                };
                this.restore(session, window, cx);
            });
        })
        .detach();
        // Reading positions can change without document notifications. Sampling
        // metadata is cheap; unchanged snapshots never touch disk.
        let config = this.preferences.borrow().snapshot().map(|p| p.ocr);
        let captured = config.clone();
        let check = cx
            .background_executor()
            .spawn(async move { config.and_then(|c| ocr::check_config(&c)) });
        cx.spawn(async move |this, cx| {
            let result = check.await;
            let _ = this.update(cx, |this, cx| {
                if this.preferences.borrow().snapshot().map(|p| p.ocr) != captured {
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
        let preferences = settings::Store::new();
        let theme = Rc::new(Cell::new(Theme::default()));
        let panel = cx.new(|cx| settings_ui::Panel::new(preferences.clone(), theme.clone(), cx));
        let mut this = Self {
            focus: cx.focus_handle(),
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            sidebar_visible: true,
            sidebar_choice: None,
            sidebar_generation: 0,
            sidebar_slide: None,
            reveal: Reveal::Hidden,
            reveal_generation: 0,
            reveal_keyboard: false,
            reveal_hover: (false, false),
            reveal_pending: None,
            reveal_ticket: 0,
            reveal_focus: cx.focus_handle(),
            reveal_return: None,
            reveal_escaped: None,
            file_drop_row: None,
            file_drop_list: false,
            file_drag_rail: false,
            rail_toggle_focus: cx.focus_handle().tab_stop(true),
            rail_new_focus: cx.focus_handle().tab_stop(true),
            panel_toggle_focus: cx.focus_handle().tab_stop(true),
            panel_new_focus: cx.focus_handle().tab_stop(true),
            menu_focus: cx.focus_handle(),
            explainer_focus: cx.focus_handle(),
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
            pending_open: Vec::new(),
            notice: None,
            crash: None,
            tab_menu: None,
            import_busy: Arc::new(AtomicBool::new(false)),
            docx_queue: VecDeque::new(),
            docx_running: None,
            quitting: None,
            quit_active: None,
            closing_tab: None,
            finishing: false,
            quit_checked: std::collections::HashMap::new(),
            session_path: None,
            persisted: None,
            writing: false,
            pending_write: None,
            cases: Default::default(),
            theme_choice: style::ThemeChoice::default(),
            home: false,
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
            .set(this.theme_choice.resolve(window.appearance()));
        this.settings_subscription =
            Some(
                cx.subscribe_in(&panel, window, |this, _, event, window, cx| match event {
                    settings_ui::Event::Applied | settings_ui::Event::Checked => {
                        this.refresh_model_settings(cx);
                        // Spelling choices apply at once; views re-read them on render.
                        for view in this.tabs.iter().filter_map(|tab| tab.view.as_ref()) {
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
                        }
                    }
                    settings_ui::Event::Finished(_, _) => {}
                    settings_ui::Event::Compare(model) => {
                        this.open_comparison(*model, window, cx);
                    }
                    settings_ui::Event::RestartToUpdate => this.restart_to_update(window, cx),
                    settings_ui::Event::ClearRecents => {
                        this.shell.recents.clear();
                        cx.notify();
                    }
                }),
            );
        this
    }

    fn refresh_model_settings(&mut self, cx: &mut Context<Self>) {
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.ocr,
            Err(e) => {
                self.notice = Some(e);
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
            .spawn(async move { ocr::check_config(&config) });
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
        let (report, open) = self.crash.clone()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = style::markdown_style(theme).alert_warning;
        let done = |this: &mut Self, cx: &mut Context<Self>| {
            crate::crash::discard();
            this.crash = None;
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
                                    if let Some((_, open)) = &mut this.crash {
                                        *open = true;
                                    }
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
        if self.quitting.is_none() && !self.finishing {
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
            self.notice = Some("Open a document before comparing models.".into());
            cx.notify();
            return;
        };
        let defaults = match self.preferences.borrow().snapshot() {
            Ok(p) => p,
            Err(e) => {
                self.notice = Some(e);
                cx.notify();
                return;
            }
        };
        let mut protected: Vec<_> = [
            view.read(cx).session.document.path.clone(),
            view.read(cx).session.source.clone(),
            view.read(cx).preview.attachment.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let (source, input) = match model {
            settings::Model::Ocr(_) => {
                let source = view
                    .read(cx)
                    .preview
                    .attachment
                    .clone()
                    .or_else(|| view.read(cx).session.source.clone());
                let Some(source) = source else {
                    self.notice = Some("OCR comparison requires an attached original PDF.".into());
                    cx.notify();
                    return;
                };
                (Some(source), None)
            }
            settings::Model::Pii(_) => {
                if !view.read(cx).can_copy_markdown() {
                    self.notice =
                        Some("Convert the document to Markdown before comparing detection.".into());
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
                    this.notice = Some(e);
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
        self.sidebar_visible = session.sidebar_visible;
        self.sidebar_choice = session.sidebar_choice;
        self.theme_choice = session.theme_choice;
        self.apply_theme(window, cx);
        self.cases.collapsed = session.collapsed_cases.iter().cloned().collect();
        let selected = session.active;
        // Session paths were normalized when opened/saved. Do not stat every file
        // during startup; missing files are diagnosed when activated.
        for record in session.tabs {
            if self.tabs.iter().any(|tab| {
                if record.source_only {
                    tab.record.attachment == record.attachment
                } else {
                    !record.markdown.as_os_str().is_empty()
                        && !tab.record.source_only
                        && tab.record.markdown == record.markdown
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
        let active = self.tabs[selected.min(self.tabs.len() - 1)].id;
        self.activate(active, window, cx);
        let pending = std::mem::take(&mut self.pending_open);
        if !pending.is_empty() {
            self.open_paths(pending, window, cx);
        }
        cx.notify();
    }

    fn push(&mut self, record: TabRecord) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let identity = (!record.markdown.as_os_str().is_empty()).then(|| record.markdown.clone());
        self.tabs.push(Tab {
            id,
            record,
            identity,
            view: None,
            recent_path: None,
            subscriptions: Vec::new(),
        });
        id
    }

    pub(super) fn active_view(&self) -> Option<Entity<DocumentView>> {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active)?
            .view
            .clone()
    }

    fn initialize(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if self.tabs[index].view.is_some() {
            return;
        }
        let record = self.tabs[index].record.clone();
        let owner = cx.entity().downgrade();
        let theme = self.theme.clone();
        let import_busy = self.import_busy.clone();
        let view = cx.new(|cx| {
            let mut view = DocumentView::new(
                DocumentViewDependencies {
                    owner: (id, owner),
                    preferences: self.preferences.clone(),
                    model_panel: self.settings.clone(),
                    theme,
                    import_busy,
                    ocr: self.ocr_state.clone(),
                    title: self.title.clone(),
                },
                window,
                cx,
            );
            view.active = id == self.active;
            view.source_only = record.source_only;
            view.blank_disposable = record.blank_disposable;
            view.auto_convert_pending = record.source_only
                && record.attachment.as_ref().is_some_and(|path| {
                    path.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("pdf") || ext.eq_ignore_ascii_case("docx")
                    })
                });
            view.loading = !record.markdown.as_os_str().is_empty();
            view.preview.attachment = record.attachment.clone();
            view.preview.source = record.attachment.clone();
            view.preview.visible = record.preview_visible;
            view.preview.split_ratio = record.preview_split;
            view.preview.restore_position = record
                .preview_zoom
                .map(|zoom| (record.preview_page, zoom, record.preview_fit));
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
        self.tabs[index].view = Some(view.clone());
        self.tabs[index].subscriptions = vec![events, changes];
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
        let Some(view) = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.view.clone())
        else {
            return;
        };
        let generation = view.update(cx, |view, cx| {
            view.load_generation = view.load_generation.wrapping_add(1);
            view.loading = true;
            view.unavailable = false;
            view.error = None;
            cx.notify();
            view.load_generation
        });
        let weak = view.downgrade();
        let path = record.markdown.clone();
        let task = cx
            .background_executor()
            .spawn(async move { Document::open(session_store::identity(&path)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.tabs.iter().any(|tab| tab.id == id) {
                    return;
                }
                if let Ok(document) = &result
                    && let Some(tab) = this.tabs.iter_mut().find(|tab| tab.id == id)
                {
                    tab.identity = document.path.clone();
                }
                let _ = weak.update(cx, |view, cx| {
                    if view.load_generation != generation {
                        return;
                    }
                    view.loading = false;
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
                                && view.preview.generation == 0
                                && let Some(path) = record.attachment
                            {
                                view.open_path(path, window, cx);
                            }
                            view.update_title(window, cx);
                            if view.active {
                                window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                            }
                        }
                        Err(error) => {
                            view.unavailable = true;
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
        if self.finishing || !self.tabs.iter().any(|tab| tab.id == id) {
            return;
        }
        if let Some(old) = self.active_view() {
            old.update(cx, |view, _| {
                view.active = false;
                view.pii.dismiss_popup();
            });
        }
        self.active = id;
        self.home = false;
        self.initialize(id, window, cx);
        if let Some(view) = self.active_view() {
            view.update(cx, |view, cx| {
                view.active = true;
                view.try_auto_convert(window, cx);
                view.update_title(window, cx);
                if view.source_only {
                    window.focus(&view.focus, cx);
                } else if view.loading || view.unavailable {
                    window.focus(&self.focus, cx);
                } else {
                    window.focus(&view.editor.read(cx).focus_handle(cx), cx);
                }
            });
        }
        // Home covers a placeholder's editor; keys go to the workspace, or
        // they would reach nothing (ADR 0048).
        if self.showing_home(cx) {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> u64 {
        // From Home, the hidden placeholder becomes the new tab (ADR 0048).
        if let Some(id) = self
            .tabs
            .iter()
            .find(|tab| self.is_placeholder(tab, cx))
            .map(|tab| tab.id)
        {
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.record.blank_disposable = false;
                if let Some(view) = &tab.view {
                    view.update(cx, |view, _| view.blank_disposable = false);
                }
            }
            self.activate(id, window, cx);
            return id;
        }
        for tab in &mut self.tabs {
            tab.record.blank_disposable = false;
            if let Some(view) = &tab.view {
                view.update(cx, |view, _| view.blank_disposable = false);
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
                Some(tab.id) != except
                    && (tab.identity.as_deref() == Some(identity)
                        || tab.view.as_ref().is_some_and(|view| {
                            view.read(cx).session.document.path.as_deref() == Some(identity)
                        }))
            })
            .map(|tab| tab.id)
    }

    #[cfg(test)]
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths(vec![path], window, cx);
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_paths_at(paths, None, window, cx);
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
        if !self.ready {
            self.pending_open.extend(paths);
            return;
        }
        if self.quitting.is_some() || self.finishing {
            return;
        }
        let task = cx.background_executor().spawn(async move {
            let supported = |path: &std::path::Path| {
                !path.is_dir() && (document::is_markdown(path) || import::supported_source(path))
            };
            let mut files = Vec::new();
            let mut folders = Vec::new();
            for path in paths {
                if let Some(folder) = cases_ui::DroppedFolder::read(&path, supported) {
                    files.extend(folder.files.iter().map(|f| (f.clone(), true)));
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
                this.finish_open(paths, at, window, cx);
                for folder in folders {
                    this.open_folder_case(folder, window, cx);
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
    ) {
        if self.quitting.is_some() || self.finishing {
            return;
        }
        // Anchor on a tab, not an index: tabs may close while paths resolve.
        let anchor = at.and_then(|at| self.tabs.get(at).map(|tab| tab.id));
        let mut created = Vec::new();
        let mut first = None;
        let mut unsupported = Vec::new();
        for (path, supported) in paths {
            if !supported {
                unsupported.push(path.display().to_string());
                continue;
            }
            let existing = self.find_path(&path, None, cx).or_else(|| {
                self.tabs
                    .iter()
                    .find(|tab| {
                        let source = tab
                            .view
                            .as_ref()
                            .and_then(|view| {
                                let view = view.read(cx);
                                view.session
                                    .source
                                    .as_ref()
                                    .or(view.preview.attachment.as_ref())
                                    .or(view.preview.source.as_ref())
                            })
                            .or(tab.record.attachment.as_ref());
                        source == Some(&path)
                    })
                    .map(|tab| tab.id)
            });
            let id = existing.unwrap_or_else(|| {
                created.push(self.next_id);
                if document::is_markdown(&path) {
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
                }
            });
            first.get_or_insert(id);
        }
        if at.is_some() && !created.is_empty() {
            let (moved, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.tabs)
                .into_iter()
                .partition(|tab| created.contains(&tab.id));
            self.tabs = kept;
            let index = anchor
                .and_then(|anchor| self.tabs.iter().position(|tab| tab.id == anchor))
                .unwrap_or(self.tabs.len());
            self.tabs.splice(index..index, moved);
        }
        if !unsupported.is_empty() {
            self.notice = Some(format!(
                "Unsupported files skipped: {}",
                unsupported.join(", ")
            ));
        }
        if let Some(first) = first {
            let disposable: Vec<_> = self
                .tabs
                .iter()
                .filter(|tab| {
                    tab.id != first
                        && tab
                            .view
                            .as_ref()
                            .map_or(tab.record.blank_disposable, |view| {
                                let view = view.read(cx);
                                view.blank_disposable
                                    && view.editor.read(cx).text().is_empty()
                                    && view.session.document.path.is_none()
                                    && view.preview.source.is_none()
                            })
                })
                .map(|tab| tab.id)
                .collect();
            self.activate(first, window, cx);
            for id in disposable {
                self.remove(id, window, cx);
            }
        }
        cx.notify();
    }

    pub fn refresh_import(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resolving_close() || self.finishing {
            return;
        }
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                view.update(cx, |view, cx| {
                    view.try_auto_convert(window, cx);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    fn event(&mut self, id: u64, event: &TabEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.iter().any(|tab| tab.id == id) {
            return;
        }
        match event {
            TabEvent::New if self.quitting.is_none() => {
                self.new_tab(window, cx);
            }
            TabEvent::Open(paths) => self.open_paths(paths.clone(), window, cx),
            TabEvent::Restored(reply) if self.quitting.is_none() => {
                let reply = reply.clone();
                let tab = self.new_tab(window, cx);
                // The reply sits right after the tab it answers.
                let after = self.tabs.iter().position(|t| t.id == id).map(|i| i + 1);
                if let Some(next) = after.and_then(|i| self.tabs.get(i)).map(|t| t.id)
                    && next != tab
                {
                    self.reorder(tab, next, cx);
                }
                if let Some(view) = self
                    .tabs
                    .iter()
                    .find(|t| t.id == tab)
                    .and_then(|t| t.view.clone())
                {
                    view.update(cx, |view, cx| view.start_restored(&reply, window, cx));
                }
            }
            TabEvent::ImportState => {
                self.refresh_import(window, cx);
                // A converted document of a folder case joins it now.
                self.attach_case(id, window, cx);
            }
            TabEvent::CloseRequested => self.close_tab(id, window, cx),
            TabEvent::OcrState(state) => self.set_ocr(state.clone(), cx),
            TabEvent::Saved => {
                let mut previous = None;
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                    previous = tab.identity.take();
                    tab.identity = tab
                        .view
                        .as_ref()
                        .and_then(|view| view.read(cx).session.document.path.clone());
                }
                // Save As moves the member: the case forgets the old path.
                self.case_member_moved(id, previous, cx);
                self.case_changed(id, cx);
            }
            TabEvent::CaseChanged => self.case_changed(id, cx),
            TabEvent::AddCaseTerm(term) => self.add_case_term(id, term, cx),
            TabEvent::CreateCase => {
                if self.can_create_case(id, cx) {
                    self.create_case(id, window, cx);
                }
            }
            TabEvent::ToggleTheme => self.cycle_theme(window, cx),
            TabEvent::CloseResolved => {
                if self.quitting.is_some() {
                    self.continue_quit(window, cx);
                } else if self.closing_tab == Some(id) {
                    self.closing_tab = None;
                    self.remove(id, window, cx);
                }
            }
            TabEvent::CloseCancelled => {
                crate::updater::cancel_restart(cx);
                self.quitting = None;
                self.cancel_case_quit();
                self.quit_active = None;
                self.closing_tab = None;
                let views: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.view.clone())
                    .collect();
                let owner = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    for view in views {
                        view.update(cx, |view, cx| view.resume_import(window, cx));
                    }
                    let _ = owner.update(cx, |this, cx| this.refresh_import(window, cx));
                });
            }
            TabEvent::SaveConflict(other) => {
                let other = *other;
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
            }
            TabEvent::Docx {
                path,
                generation,
                cancel,
            } => {
                self.docx_queue.push_back(DocxJob {
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
            if let Some(view) = &tab.view {
                view.update(cx, |view, cx| {
                    view.ocr_state = state.clone();
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    pub fn resolving_close(&self) -> bool {
        self.quitting.is_some() || self.closing_tab.is_some()
    }

    fn pump_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.docx_running.is_some() {
            return;
        }
        while let Some(job) = self.docx_queue.pop_front() {
            if job.cancel.load(Ordering::Relaxed) {
                continue;
            }
            let Some(view) = self
                .tabs
                .iter()
                .find(|tab| tab.id == job.id)
                .and_then(|tab| tab.view.clone())
            else {
                continue;
            };
            self.docx_running = Some((job.id, job.generation));
            view.update(cx, |view, cx| {
                view.run_docx(job.path, job.generation, job.cancel, window, cx)
            });
            break;
        }
    }

    pub fn docx_finished(
        &mut self,
        id: u64,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.docx_running == Some((id, generation)) {
            self.docx_running = None;
        }
        self.pump_docx(window, cx);
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.quitting.is_some() || self.closing_tab.is_some() || self.finishing {
            return;
        }
        if !self.tabs.iter().any(|tab| tab.id == id) || !self.confirm_mapping_close(id, window, cx)
        {
            return;
        }
        // A member's latest mapping reaches its case before the tab goes.
        self.case_changed(id, cx);
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        if let Some(view) = tab.view.clone() {
            if !view.read(cx).dirty(cx) {
                self.remove(id, window, cx);
                return;
            }
            if view.read(cx).prompting {
                return;
            }
            self.closing_tab = Some(id);
            self.activate(id, window, cx);
            view.update(cx, |view, cx| view.request(Next::Close, window, cx));
        } else {
            self.remove(id, window, cx);
        }
    }

    fn release(view: &Entity<DocumentView>, window: &mut Window, cx: &mut App) {
        view.update(cx, |view, cx| {
            view.import_cancel.store(true, Ordering::Relaxed);
            view.session.generation = view.session.generation.wrapping_add(1);
            view.import_permit = None;
            view.close_preview(window, cx);
            view.images.release(window, cx);
        });
    }

    fn remove(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_row_focus.borrow_mut().remove(&id);
        let removed_opener = self.compact_row_focus.borrow_mut().remove(&id);
        let removed_reveal_opener =
            removed_opener.is_some() && self.reveal_return == removed_opener;
        let closed_reveal = self.reveal == Reveal::Shown && removed_reveal_opener;
        if removed_reveal_opener {
            self.reveal_return = None;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        if let Some(view) = &self.tabs[index].view {
            Self::release(view, window, cx);
        }
        self.tabs.remove(index);
        self.docx_queue.retain(|job| job.id != id);
        if self.tabs.is_empty() {
            let id = self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            });
            self.activate(id, window, cx);
        } else if self.active == id {
            self.activate(self.tabs[index.min(self.tabs.len() - 1)].id, window, cx);
        } else if closed_reveal {
            self.dismiss_reveal(window, cx);
            self.focus_active(window, cx);
        }
        self.checkpoint(window, cx);
        cx.notify();
    }

    pub(super) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || self.quitting.is_some() || self.finishing {
            return;
        }
        if self.tabs.iter().any(|tab| {
            tab.view
                .as_ref()
                .is_some_and(|view| view.read(cx).prompting)
        }) {
            return;
        }
        self.closing_tab = None;
        self.quit_active = Some(self.active);
        self.quitting = Some(
            self.tabs
                .iter()
                .filter(|tab| {
                    tab.view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).dirty(cx))
                })
                .map(|tab| tab.id)
                .collect(),
        );
        self.continue_quit(window, cx);
    }

    fn continue_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(queue) = &mut self.quitting else {
            return;
        };
        if let Some(id) = queue.pop_front() {
            self.activate(id, window, cx);
            if let Some(view) = self.active_view() {
                view.update(cx, |view, cx| view.request(Next::Close, window, cx));
            }
        } else {
            if let Some(active) = self.quit_active.take() {
                self.activate(active, window, cx);
            }
            self.quit_checked = self
                .tabs
                .iter()
                .filter_map(|tab| {
                    Some((
                        tab.id,
                        tab.view.as_ref()?.read(cx).editor.read(cx).history_id(),
                    ))
                })
                .collect();
            self.finishing = true;
            self.checkpoint(window, cx);
            if !self.writing {
                self.finish_quit(window, cx);
            }
        }
    }

    fn finish_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed: VecDeque<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let view = tab.view.as_ref()?.read(cx);
                (view.dirty(cx)
                    && self
                        .quit_checked
                        .get(&tab.id)
                        .is_none_or(|history| *history != view.editor.read(cx).history_id()))
                .then_some(tab.id)
            })
            .collect();
        if !changed.is_empty() {
            self.finishing = false;
            self.quitting = Some(changed);
            self.cancel_case_quit();
            self.continue_quit(window, cx);
            return;
        }
        self.finishing = true;
        if self.flush_cases(window, cx) {
            self.close_window(window, cx);
        }
    }

    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
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
            sidebar_visible: self.sidebar_visible,
            sidebar_choice: self.sidebar_choice,
            theme_choice: self.theme_choice,
            collapsed_cases: {
                let mut ids: Vec<_> = self.cases.collapsed.iter().cloned().collect();
                ids.sort();
                ids
            },
            ..Session::default()
        };
        for tab in &self.tabs {
            let mut record = tab.record.clone();
            if let Some(view) = &tab.view {
                let view = view.read(cx);
                record.source_only = view.source_only || view.generated_unedited;
                record.blank = !record.source_only
                    && view.session.document.path.is_none()
                    && view.preview.source.is_none()
                    && view.editor.read(cx).text().is_empty();
                record.blank_disposable = record.blank && view.blank_disposable;
                // A document still waiting to join its folder's case keeps it.
                if let Some(link) = &view.case {
                    record.case = Some(link.id.clone());
                }
                if let Some(path) = &view.session.document.path {
                    record.markdown = path.clone();
                }
                if !view.loading && !view.unavailable {
                    record.caret = view.editor.read(cx).cursor();
                    record.scroll_y = f32::from(view.scroll.offset().y);
                }
                record.attachment = view
                    .preview
                    .attachment
                    .clone()
                    .or_else(|| view.preview.source.clone());
                record.preview_visible = view.preview.visible;
                record.preview_split = view.preview.split_ratio;
                if let Some(pdf) = &view.preview.pdf {
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
            if tab.id == self.active {
                session.active = session.tabs.len();
            }
            session.tabs.push(record);
        }
        session
    }

    fn checkpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || self.session_path.is_none() {
            return;
        }
        let snapshot = self.snapshot(cx);
        if self.persisted.as_ref() == Some(&snapshot) && !self.writing {
            return;
        }
        self.pending_write = Some(snapshot);
        self.write_pending(window, cx);
    }

    fn write_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        let Some(snapshot) = self.pending_write.take() else {
            return;
        };
        let Some(path) = self.session_path.clone() else {
            return;
        };
        self.writing = true;
        let task = cx.background_executor().spawn(async move {
            let result = session_store::save(&path, &snapshot);
            (snapshot, result)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (snapshot, result) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.writing = false;
                match result {
                    Ok(()) => {
                        this.persisted = Some(snapshot);
                    }
                    Err(error) => {
                        this.notice = Some(format!("Could not save session: {error}"));
                        let was_quitting = this.finishing;
                        this.finishing = false;
                        this.quitting = None;
                        this.cancel_case_quit();
                        this.pending_write = None;
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
                if this.pending_write.as_ref() == this.persisted.as_ref() {
                    this.pending_write = None;
                }
                this.write_pending(window, cx);
                if this.finishing && !this.writing {
                    this.finish_quit(window, cx);
                }
            });
        })
        .detach();
    }

    fn cycle(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.quitting.is_some() || self.tabs.is_empty() {
            return;
        }
        let index = self
            .tabs
            .iter()
            .position(|tab| tab.id == self.active)
            .unwrap_or(0);
        let next = (index as isize + delta).rem_euclid(self.tabs.len() as isize) as usize;
        self.activate(self.tabs[next].id, window, cx);
    }

    /// Focus the active document without changing which document is active.
    fn focus_active(&self, window: &mut Window, cx: &mut App) {
        let Some(view) = self.active_view() else {
            return;
        };
        if self.showing_home(cx) {
            window.focus(&self.focus, cx);
            return;
        }
        let view = view.read(cx);
        let focus = if view.source_only {
            view.focus.clone()
        } else if view.loading || view.unavailable {
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
        if panel {
            self.reveal_hover.1 = hovered;
        } else {
            self.reveal_hover.0 = hovered;
        }
        self.update_reveal(window, cx);
    }

    /// Open or close the reveal for the pointer, or for OS files dragged over
    /// the rail, which GPUI does not report as hover (ADR 0028).
    fn update_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_visible {
            return;
        }
        let pending = self.reveal_pending.map(|(open, _)| open);
        let hovered = self.reveal_hover.0 || self.reveal_hover.1 || self.file_drag_rail;
        match (hovered, self.reveal) {
            (true, Reveal::Shown) => self.reveal_pending = None,
            (true, Reveal::Hiding) => self.show_reveal(false, cx),
            (true, Reveal::Hidden) if pending != Some(true) => {
                self.schedule_reveal(true, window, cx)
            }
            (false, Reveal::Shown) if pending != Some(false) => {
                self.schedule_reveal(false, window, cx)
            }
            (false, Reveal::Hidden) => self.reveal_pending = None,
            _ => {}
        }
    }

    /// Open after hover intent, or close after the grace period. Only the most
    /// recently scheduled timer acts.
    fn schedule_reveal(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_ticket += 1;
        let ticket = self.reveal_ticket;
        self.reveal_pending = Some((open, ticket));
        let delay = if open { REVEAL_DELAY } else { REVEAL_GRACE };
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.reveal_pending != Some((open, ticket)) {
                    return;
                }
                this.reveal_pending = None;
                if open {
                    this.show_reveal(false, cx);
                } else if this.reveal_held(window, cx) {
                    this.schedule_reveal(false, window, cx);
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

    /// A context menu, drag, the pointer, or keyboard focus keeps the reveal open.
    fn reveal_held(&self, window: &Window, cx: &App) -> bool {
        self.tab_menu.is_some()
            || cx.has_active_drag()
            || self.reveal_hover.0
            || self.reveal_hover.1
            || (self.reveal_keyboard && self.reveal_focus.contains_focused(window, cx))
    }

    fn show_reveal(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.reveal_pending = None;
        if self.reveal != Reveal::Shown {
            self.reveal_generation += 1;
            self.reveal = Reveal::Shown;
            self.reveal_keyboard = keyboard;
        }
        cx.notify();
    }

    /// Slide the reveal back; it unmounts once the animation has finished.
    fn dismiss_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_pending = None;
        if self.reveal != Reveal::Shown {
            return;
        }
        self.reveal_keyboard = false;
        self.reveal_return = None;
        self.reveal_generation += 1;
        if cx.reduce_motion() {
            self.reveal = Reveal::Hidden;
            self.reveal_hover.1 = false;
        } else {
            self.reveal = Reveal::Hiding;
            let generation = self.reveal_generation;
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(SLIDE).await;
                let _ = this.update(cx, |this, cx| {
                    if this.reveal == Reveal::Hiding && this.reveal_generation == generation {
                        this.reveal = Reveal::Hidden;
                        this.reveal_hover.1 = false;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Keyboard focus on a rail control reveals the full list at once and moves
    /// focus to the matching control there; focus leaving the reveal closes it.
    fn reveal_for_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_visible {
            return;
        }
        if self
            .reveal_escaped
            .as_ref()
            .is_some_and(|focus| !focus.is_focused(window))
        {
            self.reveal_escaped = None;
        }
        let mut focused = [
            (&self.rail_toggle_focus, &self.panel_toggle_focus),
            (&self.rail_new_focus, &self.panel_new_focus),
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
            Some((rail, panel))
                if self.reveal != Reveal::Shown
                    && self.reveal_escaped.is_none()
                    && window.last_input_was_keyboard() =>
            {
                self.show_reveal(true, cx);
                self.reveal_keyboard = true;
                self.reveal_return = Some(rail);
                window.defer(cx, move |window, cx| window.focus(&panel, cx));
            }
            None if self.reveal == Reveal::Shown
                && self.reveal_keyboard
                && !self.reveal_held(window, cx) =>
            {
                self.dismiss_reveal(window, cx);
            }
            _ => {}
        }
    }

    /// Where files dropped on the document list would open (ADR 0028).
    fn file_drop_index(&self) -> Option<usize> {
        self.file_drop_row
            .map(|(_, index)| index)
            .or(self.file_drop_list.then_some(self.tabs.len()))
    }

    fn clear_file_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rail = std::mem::take(&mut self.file_drag_rail);
        self.file_drop_row = None;
        self.file_drop_list = false;
        if rail {
            self.update_reveal(window, cx);
        }
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
        if self.quitting.is_none() {
            self.open_paths_at(paths.paths().to_vec(), at, window, cx);
        }
    }

    fn reorder(&mut self, from: u64, to: u64, cx: &mut Context<Self>) {
        if self.quitting.is_some() {
            return;
        }
        let Some(from) = self.tabs.iter().position(|tab| tab.id == from) else {
            return;
        };
        let Some(to) = self.tabs.iter().position(|tab| tab.id == to) else {
            return;
        };
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        cx.notify();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let expanded = self.sidebar_visible;
        let revealed = !expanded && self.reveal != Reveal::Hidden;
        let home = self.showing_home(cx);
        let mut compact_rows = Vec::with_capacity(self.tabs.len());
        let accent = theme.search_accent();
        let drop_index = self.file_drop_index();
        let rows = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let id = tab.id;
                // Home's blank placeholder is never listed (ADR 0048).
                if self.is_placeholder(tab, cx) {
                    return None;
                }
                let view = tab.view.as_ref().map(|v| v.read(cx));
                let name = view.map(|v| v.display_name()).unwrap_or_else(|| {
                    let path = if tab.record.source_only {
                        tab.record
                            .attachment
                            .as_ref()
                            .unwrap_or(&tab.record.markdown)
                    } else {
                        &tab.record.markdown
                    };
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Untitled.md".into())
                });
                let path = view
                    .and_then(|v| v.session.document.path.as_ref())
                    .unwrap_or(&tab.record.markdown);
                let source = view
                    .and_then(|v| v.preview.attachment.as_ref().or(v.preview.source.as_ref()))
                    .or(tab.record.attachment.as_ref());
                let format = source
                    .or(Some(path))
                    .and_then(|p| p.extension())
                    .and_then(|e| e.to_str())
                    .unwrap_or("md")
                    .to_uppercase();
                let dirty = view.is_some_and(|v| v.dirty_cached);
                let running = view.is_some_and(|v| v.loading || v.job.busy() || v.preview.loading);
                let busy = running || view.is_some_and(|v| v.preview.queued);
                let error = view.is_some_and(|v| {
                    v.error.is_some() || v.preview.retryable || v.ocr_required.is_some()
                });
                let pseudonymized = view.is_some_and(|v| v.pseudonymized());
                let recognized = view.is_some_and(|v| v.recognized());
                let identity = source.unwrap_or(path);
                let parent = identity
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let tooltip = format!(
                    "{name}\n{parent}{}{}{}{}{}",
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
                let duplicate = self
                    .tabs
                    .iter()
                    .filter(|other| {
                        let other_view = other.view.as_ref().map(|v| v.read(cx));
                        let other_name =
                            other_view.map(|v| v.display_name()).unwrap_or_else(|| {
                                other
                                    .record
                                    .markdown
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned()
                            });
                        other_name == name
                    })
                    .count()
                    > 1;
                let peers: Vec<PathBuf> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| {
                        let view = tab.view.as_ref().map(|v| v.read(cx));
                        view.and_then(|v| {
                            v.preview
                                .attachment
                                .as_ref()
                                .or(v.preview.source.as_ref())
                                .or(v.session.document.path.as_ref())
                        })
                        .or(tab.record.attachment.as_ref())
                        .or(Some(&tab.record.markdown))
                        .filter(|p| !p.as_os_str().is_empty())
                        .cloned()
                    })
                    .collect();
                let parent_label = disambiguating_parent(identity, &peers);
                // Unsaved duplicates have no folder; an empty second line would
                // lift the name off the row's centre.
                let duplicate = duplicate && !parent_label.is_empty();
                let focus = self
                    .sidebar_row_focus
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone();
                // Rail controls leave the tab order while the reveal covers them.
                let compact_focus = self
                    .compact_row_focus
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone()
                    .tab_stop(!revealed);
                compact_rows.push(
                    ui::control(("compact-tab", id), "", theme, true)
                        .map(|v| {
                            ui::reveal_focus(
                                v.track_focus(&compact_focus),
                                compact_focus.clone(),
                                self.compact_scroll.clone(),
                            )
                        })
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
                        .when(self.active == id && !home, |v| {
                            v.bg(theme.sidebar_selected())
                        })
                        .child(
                            gpui::svg()
                                .data(ui::Icon::Source.data())
                                .size(px(16.))
                                .text_color(if self.active == id && !home {
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
                            if this.quitting.is_none() {
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
                    div()
                        .id(("tab", id))
                        .map(|v| {
                            ui::reveal_focus(
                                v.track_focus(&focus),
                                focus,
                                self.sidebar_scroll.clone(),
                            )
                        })
                        .key_context("UiControl")
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label(if duplicate {
                            format!("{name} · {parent_label}")
                        } else {
                            name.clone()
                        })
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
                        .when(self.active == id && !home, |v| {
                            v.bg(theme.sidebar_selected())
                        })
                        .hover(move |v| v.bg(palette.placeholder_bg))
                        .focus_visible(move |v| v.bg(palette.placeholder_bg))
                        .tooltip(style::tooltip(tooltip, theme))
                        .cursor_pointer()
                        .child(
                            div()
                                .text_size(px(9.))
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
                                .aria_label("Close tab")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_tab(id, window, cx);
                                })),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.quitting.is_none() {
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
                            move |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                                let position = event.event.position;
                                let row = event.bounds.contains(&position).then(|| {
                                    let lower = position.y > event.bounds.center().y;
                                    (id, index + usize::from(lower))
                                });
                                if row.is_some() && this.file_drop_row != row {
                                    this.file_drop_row = row;
                                    cx.notify();
                                } else if row.is_none()
                                    && this.file_drop_row.is_some_and(|(row, _)| row == id)
                                {
                                    this.file_drop_row = None;
                                    cx.notify();
                                }
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
        let mut previous: Option<Option<String>> = None;
        for (index, row) in rows.into_iter().enumerate() {
            let Some(row) = row else {
                continue;
            };
            let link = self.tabs[index]
                .view
                .as_ref()
                .and_then(|v| v.read(cx).case.clone());
            let id = link.as_ref().map(|l| l.id.clone());
            // Tabs without a case gather under "Open".
            if link.is_none() && previous.as_ref() != Some(&None) {
                rows_with_cases.push(self.section_label("Open"));
            }
            if let Some(link) = link.filter(|_| previous.as_ref() != Some(&id)) {
                let count = self.tabs[index..]
                    .iter()
                    .take_while(|t| {
                        t.view
                            .as_ref()
                            .and_then(|v| v.read(cx).case.as_ref().map(|l| l.id.clone()))
                            == id
                    })
                    .count();
                rows_with_cases.push(self.case_header(&link, self.tabs[index].id, count, cx));
            }
            let folded = id
                .as_ref()
                .is_some_and(|id| self.cases.collapsed.contains(id))
                && self.tabs[index].id != self.active;
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
        let sliding = self
            .sidebar_slide
            .is_some_and(|until| std::time::Instant::now() < until)
            && !cx.reduce_motion();
        let collapsing = sliding && !expanded && !revealed;
        let mut panel = (expanded || revealed || collapsing).then(|| {
            div()
                .id("sidebar-panel")
                .w(px(PANEL_WIDTH - 1.))
                .h_full()
                .flex()
                .flex_col()
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        if this.file_drop_list != over {
                            this.file_drop_list = over;
                            cx.notify();
                        }
                    },
                ))
                .on_drop(
                    cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, true, window, cx)
                    }),
                )
                .child(self.sidebar_top(
                    sidebar_toggle(&self.panel_toggle_focus, expanded, theme).into_any_element(),
                ))
                .child(self.sidebar_nav(cx))
                .child(
                    div()
                        .id("sidebar-tabs")
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
                                ("sidebar-open", self.sidebar_generation),
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
                            ui::header_icon("rail-home", "Home", ui::Icon::Home, theme, home)
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
            .relative()
            .w(px(width(expanded)))
            .flex_shrink_0()
            .h_full()
            .text_size(px(13.))
            .child(div().size_full().overflow_hidden().child(content))
            .when(!expanded, |v| {
                v.on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                    this.hover_sidebar(false, *hovered, window, cx)
                }))
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        if this.file_drag_rail != over {
                            this.file_drag_rail = over;
                            this.update_reveal(window, cx);
                        }
                    },
                ))
                .on_drop(cx.listener(
                    |this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, false, window, cx)
                    },
                ))
            });
        if revealed && let Some(panel) = panel.take() {
            let (from, to) = if self.reveal == Reveal::Shown {
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
                        match this.reveal_return.clone() {
                            Some(focus) => {
                                window.focus(&focus, cx);
                                this.reveal_escaped = Some(focus);
                            }
                            None => this.focus_active(window, cx),
                        }
                        this.dismiss_reveal(window, cx);
                    }))
                    .child(panel)
                    .with_animation(
                        ("sidebar-reveal", self.reveal_generation),
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
                        ("sidebar-close", self.sidebar_generation),
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
const SLIDE: Duration = Duration::from_millis(150);

/// The collapsed rail's hover reveal of the full document list (ADR 0027).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    Hidden,
    Shown,
    /// Sliding back; unmounted when the animation ends.
    Hiding,
}

/// A small mark on a sidebar tab for work done in it (OCR, pseudonymization).
fn tab_badge(icon: ui::Icon, color: gpui::Hsla) -> gpui::Svg {
    gpui::svg()
        .data(icon.data())
        .size(px(12.))
        .flex_shrink_0()
        .text_color(color)
}

fn sidebar_toggle(
    focus: &gpui::FocusHandle,
    expanded: bool,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
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

/// Show the shortest parent suffix that distinguishes this document's location.
fn disambiguating_parent(path: &std::path::Path, peers: &[PathBuf]) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let components: Vec<_> = parent.components().collect();
    for depth in 1..=components.len() {
        let suffix: PathBuf = components[components.len() - depth..].iter().collect();
        if !peers
            .iter()
            .any(|peer| peer != path && peer.parent().is_some_and(|p| p.ends_with(&suffix)))
        {
            return suffix.to_string_lossy().into_owned();
        }
    }
    parent.to_string_lossy().into_owned()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar_visible = self.sidebar_choice.unwrap_or(true);
        self.reveal_for_keyboard(window, cx);
        // A finished or abandoned OS drag leaves no insertion line or reveal.
        if !cx.has_active_drag()
            && (self.file_drag_rail || self.file_drop_list || self.file_drop_row.is_some())
        {
            self.clear_file_drag(window, cx);
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let home = self.showing_home(cx);
        if !cx.has_active_drag() && self.shell.drop.is_some() {
            self.shell.drop = None;
        }
        // Notices wait behind the header's bell; a new one opens it (ADR 0048).
        let notices = usize::from(self.case_key_banner().is_some())
            + usize::from(self.crash.is_some())
            + usize::from(self.notice.is_some());
        if notices > self.shell.notice_count {
            self.title.notices_open.set(true);
        } else if notices == 0 {
            self.title.notices_open.set(false);
        }
        self.shell.notice_count = notices;
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
            if self.sidebar_choice.unwrap_or(true) {
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
        self.title.detached.set(!self.sidebar_visible);
        self.title.inset.set(if self.sidebar_visible {
            0.
        } else {
            (style::TRAFFIC_LIGHT_INSET + 32. + 12. - RAIL_WIDTH - 16.).max(0.)
        });
        div()
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.sidebar_bg())
            .text_color(palette.header_fg)
            .on_action(
                cx.listener(|this, _: &shell::OpenPalette, window, cx| {
                    this.open_palette(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &shell::ShowHome, window, cx| this.show_home(window, cx)),
            )
            .on_action(cx.listener(|this, _: &shell::PasteAsNewTab, window, cx| {
                this.paste_as_new_tab(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleTheme, window, cx| this.cycle_theme(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleNotices, _, cx| {
                this.title.notices_open.set(!this.title.notices_open.get());
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &PiiApplyAll, window, cx| {
                if let Some(view) = this.active_view().filter(|_| !this.showing_home(cx)) {
                    view.update(cx, |view, cx| view.apply_all_pii(action, window, cx));
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
            .on_action(cx.listener(|this, _: &Settings, window, cx| {
                this.show_settings(window, cx);
            }))
            // About shows the version, in Settings › Updates.
            .on_action(cx.listener(|this, _: &crate::menus::About, window, cx| {
                window.activate_window();
                this.settings
                    .update(cx, |panel, cx| panel.show_updates(window, cx));
            }))
            .on_action(cx.listener(|this, _: &New, window, cx| {
                if this.ready && this.quitting.is_none() {
                    this.new_tab(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Quit, window, cx| this.quit(window, cx)))
            .on_action(
                cx.listener(|this, _: &OpenCase, window, cx| this.show_case_list(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Close, window, cx| this.close_tab(this.active, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.open(&Open, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &Import, window, cx| {
                if let Some(view) = this.active_view() {
                    view.update(cx, |view, cx| view.import(&Import, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.cycle(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.cycle(-1, window, cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.sidebar_visible = !this.sidebar_visible;
                this.sidebar_choice = Some(this.sidebar_visible);
                this.sidebar_generation += 1;
                // Only the sidebar layer animates; one repaint ends it.
                this.sidebar_slide = Some(std::time::Instant::now() + SLIDE);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(SLIDE + Duration::from_millis(16))
                        .await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
                this.reveal = Reveal::Hidden;
                this.reveal_pending = None;
                this.reveal_hover = (false, false);
                this.reveal_keyboard = false;
                this.reveal_return = None;
                // Keep keyboard focus on the toggle that replaces the clicked one.
                if this.sidebar_visible && this.rail_toggle_focus.is_focused(window) {
                    window.focus(&this.panel_toggle_focus, cx);
                } else if !this.sidebar_visible && this.panel_toggle_focus.is_focused(window) {
                    window.focus(&this.rail_toggle_focus, cx);
                    this.reveal_escaped = Some(this.rail_toggle_focus.clone());
                }
                cx.notify();
            }))
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            .on_action(cx.listener(|this, _: &RetryDocument, window, cx| {
                if let Some(tab) = this.tabs.iter().find(|tab| tab.id == this.active) {
                    this.load_document(tab.id, tab.record.clone(), window, cx);
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
                                    div()
                                        .id("context-close-tab")
                                        .role(gpui::Role::Button)
                                        .aria_label("Close tab")
                                        .key_context("UiControl")
                                        .tab_index(0)
                                        .focus_visible(|s| {
                                            s.bg(self.theme.get().pdf_style().placeholder_bg)
                                        })
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
            .children(self.case_categories(cx))
            .children(self.keychain_explainer(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(self.card(home, window, cx)),
            )
            // Collapsed: the expand toggle sits right of the traffic lights in
            // the title row, before the document header (ADR 0048).
            .when(!self.sidebar_visible, |v| {
                v.child(
                    div()
                        .absolute()
                        .left(px(style::TRAFFIC_LIGHT_INSET))
                        .top(px(style::CARD_MARGIN + (style::HEADER_HEIGHT - 32.) / 2.))
                        .child(sidebar_toggle(
                            &self
                                .rail_toggle_focus
                                .clone()
                                .tab_stop(self.reveal == Reveal::Hidden),
                            false,
                            theme,
                        )),
                )
            })
            .children(self.notices_view(window, cx))
            .children(self.palette_view(cx))
            .when(self.settings.read(cx).open, |v| {
                v.child(self.settings.clone())
            })
    }
}

impl Workspace {
    /// The inset content card on the window ground (ADR 0048): workspace bars,
    /// then Home or the active document.
    fn card(&mut self, home: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let content = if home {
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
            .when(self.sidebar_visible, |v| v.p(px(style::CARD_MARGIN)))
            .when(!self.sidebar_visible, |v| {
                v.pr(px(style::CARD_MARGIN)).pb(px(style::CARD_MARGIN))
            })
            .child(
                div()
                    .id("content-card")
                    .size_full()
                    .flex()
                    .flex_col()
                    .when(self.sidebar_visible, |v| ui::card_surface(v, theme))
                    .when(home, |v| {
                        v.on_drop(
                            cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                                this.shell.drop = None;
                                this.open_paths(paths.paths().to_vec(), window, cx);
                            }),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .into_any_element()
    }

    fn notice_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.notice.clone().map(|notice| {
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
                            div()
                                .id("dismiss-session-notice")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "dismiss-session-notice".into())
                                })
                                .flex_shrink_0()
                                .role(gpui::Role::Button)
                                .aria_label("Dismiss notice")
                                .key_context("UiControl")
                                .tab_index(0)
                                .focus_visible(|s| {
                                    s.bg(self.theme.get().pdf_style().placeholder_bg)
                                })
                                .aria_label("Dismiss notice")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|v| v.bg(palette.placeholder_bg))
                                .child("×")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        })
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
