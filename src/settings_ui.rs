use crate::{
    document_view::{Save, SaveAs},
    markdown_search::SearchInput,
    models,
    style::Theme,
    ui, updater,
    workspace::{Close, Import, New, NextTab, Open, PreviousTab},
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, KeyBinding, PromptLevel, Window, actions, div,
    prelude::*, px,
};
use mdoc_import::{
    model_download::{self, Pending, Progress},
    model_work, ocr,
};
pub use mdoc_store::model_state::Status;
use mdoc_store::model_state::{Files, ModelState};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings::{Model, OcrConfig, PiiConfig, PiiModel, Preferences, Shared, save};
use std::{cell::Cell, rc::Rc, sync::atomic::Ordering, time::Duration};

mod categories;
pub(crate) mod dictionary;
pub(crate) use categories::Category;

fn status_label(status: &Status) -> String {
    match status {
        Status::Unknown => "Not checked",
        Status::Missing => "Not downloaded",
        Status::Ready => "Installed",
        Status::Damaged(_) => "Repair needed",
        Status::Unavailable(_) => "Unavailable",
    }
    .into()
}

actions!(
    model_settings,
    [
        CloseSettings,
        NextSettingsField,
        PreviousSettingsField,
        NextCategory,
        PreviousCategory,
        CommitNumber
    ]
);
pub enum Event {
    /// The window owns and persists the theme choice (ADR 0048).
    ThemeSelected(ThemeChoice),
    Applied,
    Checked,
    Compare(Model),
    Finished(Model, Result<Option<ocr::Installed>, String>),
    /// Restart now: quit through the workspace, then relaunch (ADR 0040).
    RestartToUpdate,
    /// Forget the recent files Home lists (ADR 0048).
    ClearRecents,
    Shortcuts,
    /// Saved: Settings › Detection stopped detecting these (ADR 0076).
    CategoriesUnticked(std::collections::HashSet<mdoc_pii::Category>),
}

struct SetupCompletion {
    result: Result<Option<ocr::Installed>, String>,
    status: Status,
    model_state: ModelState,
}

impl SetupCompletion {
    /// Runs on the setup worker, while it still owns the model permit.
    fn remember(
        model: Model,
        remove: bool,
        progress: &Progress,
        mut result: Result<Option<ocr::Installed>, String>,
        mut model_state: ModelState,
        checked_files: Option<Files>,
        paths: &[std::path::PathBuf],
    ) -> Self {
        model_state.reload();
        let status = match &result {
            Ok(_) if remove => Status::Missing,
            Ok(_) => Status::Ready,
            Err(_) if progress.cancel.load(Ordering::Relaxed) => Status::Unknown,
            Err(e) if progress.state.lock().unwrap().phase == "Checking runtime" => {
                Status::Unavailable(e.clone())
            }
            Err(e) => Status::Damaged(e.clone()),
        };
        let after = Files::read(paths);
        let status = if result.is_ok() && !remove && checked_files.is_none() {
            Status::Unknown
        } else {
            status
        };
        let before = checked_files.unwrap_or_else(|| after.clone());
        let status = model_state.record(model, &before, after, status);
        if result.is_ok() && !remove && status != Status::Ready {
            result = Err("Model files changed. Retry setup.".into());
        }
        if let Err(error) = model_state.save() {
            eprintln!("Could not remember model setup: {error}");
        }
        Self {
            result,
            status,
            model_state,
        }
    }
}

pub struct Panel {
    pub open: bool,
    pub shared: Shared,
    pub statuses: Vec<Status>,
    /// Persisted labels are never inputs to runtime readiness or admission.
    row_statuses: Option<Vec<Status>>,
    model_state: ModelState,
    #[cfg(test)]
    model_files: Option<[Vec<std::path::PathBuf>; 4]>,
    #[cfg(test)]
    check_blocked: bool,
    pub ocr_installations: Vec<ocr::Installed>,
    /// Bytes each model in `Model::ALL` still needs, measured cheaply by size.
    pub pending: Vec<Pending>,
    pub draft: Preferences,
    pub threshold: Entity<SearchInput>,
    pub confidence: Entity<SearchInput>,
    pub error: Option<String>,
    save_error: Option<String>,
    pub applying: bool,
    pub working: Option<Model>,
    progress: Option<Progress>,
    category: Category,
    category_focus: FocusHandle,
    number_errors: [Option<String>; 2],
    number_subscriptions: Vec<gpui::Subscription>,
    save_pending: bool,
    reset_theme: Option<u64>,
    theme_revision: u64,
    #[cfg(test)]
    save_gate: Option<gpui::Task<()>>,
    pub details: Option<Model>,
    theme: Rc<Cell<Theme>>,
    /// The window's current choice, projected into this control.
    theme_choice: ThemeChoice,
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    appearance_bounds: Rc<Cell<Option<[gpui::Bounds<gpui::Pixels>; 3]>>>,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    checks: u64,
    refresh_watch: Option<gpui::Task<()>>,
    /// Another app job holding the model slot, as `refresh_watch` last saw
    /// it. Read from the process-wide slot only there, so the layout
    /// changes on that notify, never in the middle of a click.
    other_work: Option<&'static str>,
    check_pending: bool,
    checking: bool,
    scroll: gpui::ScrollHandle,
    controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    /// The user dictionary view, replacing the Settings body while open.
    pub dictionary: Option<dictionary::DictionaryView>,
}
impl gpui::EventEmitter<Event> for Panel {}
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(crate::shortcuts::bindings(
        crate::shortcuts::Owner::Settings,
    ));
    cx.bind_keys([
        KeyBinding::new("tab", NextSettingsField, Some("ModelSettings")),
        KeyBinding::new("shift-tab", PreviousSettingsField, Some("ModelSettings")),
        KeyBinding::new("down", NextCategory, Some("SettingsCategories")),
        KeyBinding::new("up", PreviousCategory, Some("SettingsCategories")),
        KeyBinding::new("enter", CommitNumber, Some("SettingsNumber")),
    ]);
    dictionary::bind_keys(cx);
}
fn field(
    label: &'static str,
    value: &str,
    theme: Rc<Cell<Theme>>,
    cx: &mut Context<Panel>,
) -> Entity<SearchInput> {
    cx.new(|cx| {
        let mut input = SearchInput::new(cx)
            .with_theme(theme)
            .with_key_context("SettingsInput")
            .with_placeholder("0–1")
            .with_label(label);
        input.set_value(value.into(), cx);
        input
    })
}
/// Keep document commands blocked when another sheet covers Settings too.
pub(crate) fn block_document_commands<E: gpui::InteractiveElement>(element: E) -> E {
    element
        .on_action(|_: &New, _, cx| cx.stop_propagation())
        .on_action(|_: &Open, _, cx| cx.stop_propagation())
        .on_action(|_: &Close, _, cx| cx.stop_propagation())
        .on_action(|_: &NextTab, _, cx| cx.stop_propagation())
        .on_action(|_: &PreviousTab, _, cx| cx.stop_propagation())
        .on_action(|_: &Save, _, cx| cx.stop_propagation())
        .on_action(|_: &SaveAs, _, cx| cx.stop_propagation())
        .on_action(|_: &Import, _, cx| cx.stop_propagation())
}

pub fn control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> ui::Control {
    ui::action_control(id, label, theme, enabled, false)
}
fn quiet_control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> ui::Control {
    let label = label.into();
    crate::ui::focus_ring(id, theme)
        .key_context("UiControl")
        .tab_index(0)
        .tab_stop(enabled)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(12.))
        .text_color(theme.pdf_style().header_muted)
        .child(label)
        .when(enabled, |v| {
            v.cursor_pointer().hover(|v| {
                v.bg(theme.pdf_style().placeholder_bg)
                    .text_color(theme.pdf_style().header_fg)
            })
        })
        .when(!enabled, |v| v.opacity(0.45))
}
impl Panel {
    fn reveal(
        &self,
        id: impl Into<gpui::ElementId>,
        control: ui::Control,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let focus = self
            .controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle())
            .clone()
            .tab_stop(enabled);
        ui::reveal_focus(control.track_focus(&focus), focus, self.scroll.clone())
    }
    fn scrolled_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        theme: Theme,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let id = id.into();
        self.reveal(id.clone(), control(id, label, theme, enabled), enabled, cx)
    }
    fn scrolled_quiet(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        theme: Theme,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let id = id.into();
        self.reveal(
            id.clone(),
            quiet_control(id, label, theme, enabled),
            enabled,
            cx,
        )
    }

    pub fn new(shared: Shared, theme: Rc<Cell<Theme>>, cx: &mut Context<Self>) -> Self {
        let draft = shared.borrow().current.clone().unwrap_or_default();
        let model_state = ModelState::open(shared.borrow().path.as_deref());
        Self {
            open: false,
            shared,
            statuses: vec![Status::Unknown; Model::ALL.len()],
            row_statuses: None,
            model_state,
            #[cfg(test)]
            model_files: None,
            #[cfg(test)]
            check_blocked: false,
            ocr_installations: Vec::new(),
            pending: Model::ALL.map(models::pending).to_vec(),
            threshold: field(
                "Detection threshold",
                &draft.pseudonymization.threshold.to_string(),
                theme.clone(),
                cx,
            ),
            confidence: field(
                "OCR minimum confidence",
                &draft.ocr.minimum_confidence.to_string(),
                theme.clone(),
                cx,
            ),
            draft,
            error: None,
            save_error: None,
            applying: false,
            working: None,
            progress: None,
            category: Category::General,
            category_focus: cx.focus_handle().tab_stop(true),
            number_errors: [None, None],
            number_subscriptions: Vec::new(),
            save_pending: false,
            reset_theme: None,
            theme_revision: 0,
            #[cfg(test)]
            save_gate: None,
            details: None,
            theme,
            theme_choice: ThemeChoice::default(),
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            appearance_bounds: Rc::default(),
            focus: cx.focus_handle(),
            previous: None,
            checks: 0,
            refresh_watch: None,
            other_work: None,
            check_pending: false,
            checking: false,
            scroll: gpui::ScrollHandle::new(),
            controls: Default::default(),
            dictionary: None,
        }
    }
    /// A Settings › General choice changed outside the panel (the menu bar
    /// shelf's notes, ADR 0084): the draft follows, so the next save keeps it.
    pub fn general_changed(
        &mut self,
        general: mdoc_store::settings::GeneralConfig,
        cx: &mut Context<Self>,
    ) {
        self.draft.general = general;
        cx.notify();
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        if !self.applying {
            self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        }
        self.category = Category::General;
        self.dictionary = None;
        self.number_errors = [None, None];
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.error = self
            .save_error
            .clone()
            .or_else(|| self.shared.borrow().error.as_ref().map(ToString::to_string));
        self.update_fields(cx);
        self.previous = window.focused(cx);
        self.open = true;
        window.focus(&self.category_focus, cx);
        self.refresh(cx);
        self.other_work = model_work::description();
        self.refresh_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let Ok(open) = this.update(cx, |this, cx| {
                    if this.check_pending && !model_work::busy() {
                        this.refresh(cx);
                    }
                    let current = model_work::description();
                    if current != this.other_work {
                        this.other_work = current;
                        cx.notify();
                    }
                    this.open
                }) else {
                    break;
                };
                if !open {
                    break;
                }
            }
        }));
        cx.notify();
    }
    pub(crate) fn sync_theme(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        // A newer explicit choice wins over a General reset still being saved.
        if choice != self.theme_choice {
            self.theme_revision += 1;
            self.reset_theme = None;
        }
        self.theme_choice = choice;
        // The resolved theme may change even while System stays selected.
        cx.notify();
    }
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn selected_theme(&self) -> ThemeChoice {
        self.theme_choice
    }
    /// Show some Categories to detect off, unsaved, with keyboard focus on
    /// the chip `token`, for the Detection render (ADR 0076).
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn render_categories(
        &mut self,
        off: &[mdoc_pii::Category],
        token: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.categories = self
            .shared
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.clone())
            .unwrap_or_default();
        for &category in off {
            self.draft.categories.set(category, false);
        }
        let id =
            gpui::ElementId::from(gpui::SharedString::from(format!("settings-detect-{token}")));
        if let Some(focus) = self.controls.borrow().get(&id) {
            window.focus(focus, cx);
        }
        cx.notify();
    }
    /// Geometry from the three actual segments, cleared before each render:
    /// an omitted section or a choice below the fold cannot pass the contract.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn appearance_in_view(&self) -> bool {
        self.open
            && self.dictionary.is_none()
            && self.appearance_bounds.get().is_some_and(|segments| {
                segments.iter().all(|bounds| {
                    bounds.size.width > px(0.)
                        && bounds.size.height > px(0.)
                        && bounds.is_contained_within(&self.scroll.bounds())
                })
            })
    }
    /// Open Settings at one feature's models, e.g. from a setup card.
    pub fn show_section(&mut self, model: Model, window: &mut Window, cx: &mut Context<Self>) {
        self.show_category(
            match model {
                Model::Ocr(_) => Category::Recognition,
                Model::Pii(_) => Category::Detection,
            },
            window,
            cx,
        );
    }
    /// The update card opens General, which owns Automatic updates.
    pub fn show_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_category(Category::General, window, cx);
    }
    pub fn index(model: Model) -> usize {
        Model::ALL.iter().position(|m| *m == model).unwrap()
    }
    /// Whether `model` must be downloaded before use.
    pub fn needs_setup(&self, model: Model) -> bool {
        let index = Self::index(model);
        self.pending[index].total() > 0
            || matches!(self.statuses[index], Status::Missing | Status::Damaged(_))
    }
    /// Live setup state while `model` is downloading or being checked.
    pub fn progress_of(&self, model: Model) -> Option<model_download::State> {
        self.progress
            .as_ref()
            .filter(|_| self.working == Some(model))
            .map(|p| p.state.lock().unwrap().clone())
    }
    pub fn cancelling(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|p| p.cancel.load(Ordering::Relaxed))
    }
    /// Render a running setup without network or model access in GPUI tests.
    #[cfg(test)]
    pub(crate) fn simulate_setup(&mut self, model: Model, cx: &mut Context<Self>) -> Progress {
        let progress = Progress::default();
        progress.phase("Downloading test model");
        self.working = Some(model);
        self.progress = Some(progress.clone());
        cx.notify();
        progress
    }
    pub fn cancel_setup(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &self.progress {
            p.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }
    /// Select a model and save the choice at once (ADR 0026).
    pub fn select(&mut self, model: Model, cx: &mut Context<Self>) {
        if matches!(self.statuses[Self::index(model)], Status::Unavailable(_)) {
            return;
        }
        match model {
            Model::Ocr(m) => self.draft.ocr.model = m,
            Model::Pii(m) => self.draft.pseudonymization.model = m,
        }
        self.persist(cx);
    }
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_watch = None;
        self.open = false;
        self.dictionary = None;
        self.commit_visible_number(cx);
        if let Some(focus) = self.previous.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
    fn update_fields(&mut self, cx: &mut Context<Self>) {
        self.threshold.update(cx, |i, cx| {
            i.set_value(self.draft.pseudonymization.threshold.to_string(), cx)
        });
        self.confidence.update(cx, |i, cx| {
            i.set_value(self.draft.ocr.minimum_confidence.to_string(), cx)
        });
    }
    fn model_files(&self) -> [Vec<std::path::PathBuf>; 4] {
        #[cfg(test)]
        if let Some(files) = &self.model_files {
            return files.clone();
        }
        Model::ALL.map(models::files)
    }

    fn row_checking(&self, model: Model) -> bool {
        self.checking && matches!(self.row_status(model), Status::Unknown)
    }

    fn row_status(&self, model: Model) -> &Status {
        &self.row_statuses.as_ref().unwrap_or(&self.statuses)[Self::index(model)]
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.model_state.reload();
        let paths = self.model_files();
        if self.working.is_none() {
            self.row_statuses = Some(
                Model::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, model)| {
                        let files = Files::read(&paths[index]);
                        self.model_state
                            .status(model, &files)
                            .unwrap_or_else(|| self.statuses[index].clone())
                    })
                    .collect(),
            );
        }
        // Unit checks substitute the engines below and isolate admission too:
        // concurrent test windows must not take each other's process-wide slot.
        #[cfg(test)]
        let busy = self.check_blocked;
        #[cfg(not(test))]
        let busy = model_work::busy();
        if self.working.is_some() || busy {
            self.check_pending = true;
            return;
        }
        #[cfg(test)]
        let permit: Option<model_work::Permit> = None;
        #[cfg(not(test))]
        let permit = match model_work::Permit::acquire_for("checking installed models") {
            Ok(permit) => Some(permit),
            Err(_) => {
                self.check_pending = true;
                return;
            }
        };
        self.checks += 1;
        self.check_pending = false;
        self.checking = true;
        let generation = self.checks;
        let mut model_state = self.model_state.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let mut installed = Vec::new();
            let pending = Model::ALL.map(models::pending).to_vec();
            let statuses = Model::ALL
                .into_iter()
                .enumerate()
                .map(|(index, model)| {
                    let paths = &paths[index];
                    let before = Files::read(paths);
                    let result = match model {
                        // Tests never read the real profile's installed models.
                        Model::Ocr(_) if cfg!(test) => Ok(false),
                        Model::Ocr(m) => ocr::check_reserved(&OcrConfig {
                            model: m,
                            ..Default::default()
                        })
                        .map(|v| {
                            let ready = v.is_some();
                            if let Some(v) = v {
                                installed.push(v);
                            }
                            ready
                        })
                        .map_err(|e| e.to_string()),
                        Model::Pii(_) if cfg!(test) => Ok(false),
                        Model::Pii(m) => mdoc_detect::check_reserved(m).map_err(|e| e.to_string()),
                    };
                    let status = models::check_status(result);
                    model_state.record(model, &before, Files::read(paths), status)
                })
                .collect::<Vec<_>>();
            if let Err(error) = model_state.save() {
                eprintln!("Could not remember model checks: {error}");
            }
            (statuses, installed, pending, model_state)
        });
        cx.spawn(async move |this, cx| {
            let (mut statuses, mut installed, pending, model_state) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.checks == generation && this.working.is_none() {
                    // A different model's runtime check may take time. Recheck
                    // metadata at publication too, so an earlier result cannot
                    // label a file removed while the rest of the catalog ran.
                    let paths = this.model_files();
                    for (index, model) in Model::ALL.into_iter().enumerate() {
                        statuses[index] = model_state
                            .status(model, &Files::read(&paths[index]))
                            .unwrap_or(Status::Unknown);
                    }
                    installed.retain(|i| {
                        statuses[Self::index(Model::Ocr(i.config.model))] == Status::Ready
                    });
                    this.checking = false;
                    if this.statuses != statuses {
                        this.statuses = statuses;
                    }
                    this.row_statuses = None;
                    this.ocr_installations = installed;
                    this.pending = pending;
                    this.model_state = model_state;
                    cx.emit(Event::Checked);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Commit just this category's numeric field, preserving the last good value.
    fn commit_visible_number(&mut self, cx: &mut Context<Self>) {
        match self.category {
            Category::Detection => self.commit_number(0, cx),
            Category::Recognition => self.commit_number(1, cx),
            _ => {}
        }
    }
    fn commit_number(&mut self, index: usize, cx: &mut Context<Self>) {
        let input = if index == 0 {
            &self.threshold
        } else {
            &self.confidence
        };
        let value = input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
        let Some(value) = value else {
            self.number_errors[index] =
                Some("Enter a number from 0 to 1. The previous value is still active.".into());
            cx.notify();
            return;
        };
        self.number_errors[index] = None;
        let previous = if index == 0 {
            self.draft.pseudonymization.threshold
        } else {
            self.draft.ocr.minimum_confidence
        };
        if previous != value {
            if index == 0 {
                self.draft.pseudonymization.threshold = value;
            } else {
                self.draft.ocr.minimum_confidence = value;
            }
            self.persist(cx);
        }
        cx.notify();
    }
    /// Reset only preferences owned by the visible category. About has no preferences.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        let defaults = Preferences::default();
        match self.category {
            Category::General => {
                self.draft.updates = defaults.updates;
                // Notes already seen stay seen (ADR 0084).
                self.draft.general.menu_bar = defaults.general.menu_bar;
            }
            Category::Detection => {
                self.draft.pseudonymization = defaults.pseudonymization;
                self.draft.categories = defaults.categories;
                self.threshold.update(cx, |i, cx| {
                    i.set_value(self.draft.pseudonymization.threshold.to_string(), cx)
                });
                self.number_errors[0] = None;
            }
            Category::Recognition => {
                self.draft.ocr = defaults.ocr;
                self.confidence.update(cx, |i, cx| {
                    i.set_value(self.draft.ocr.minimum_confidence.to_string(), cx)
                });
                self.number_errors[1] = None;
            }
            Category::Spelling => self.draft.spelling = defaults.spelling,
            Category::Cases => self.draft.cases = defaults.cases,
            Category::About => return,
        }
        if self.category == Category::General {
            self.reset_theme = Some(self.theme_revision);
        }
        self.persist(cx);
    }
    /// Atomically save `draft`. A failure restores the previous choices.
    fn persist(&mut self, cx: &mut Context<Self>) {
        if self.applying {
            self.save_pending = true;
            return;
        }
        let prefs = self.draft.clone();
        let reset_theme = self.reset_theme.take();
        let saved_updates = prefs.updates;
        let check_updates = self
            .shared
            .borrow()
            .current
            .as_ref()
            .is_some_and(|old| old.updates != prefs.updates)
            && prefs.updates.check_automatically;
        let unticked: std::collections::HashSet<_> = self
            .shared
            .borrow()
            .current
            .as_ref()
            .map(|old| {
                mdoc_pii::Category::ALL
                    .into_iter()
                    .filter(|&c| old.categories.detects(c) && !prefs.categories.detects(c))
                    .collect()
            })
            .unwrap_or_default();
        let path = self.shared.borrow().path.clone();
        self.applying = true;
        #[cfg(test)]
        let save_gate = self.save_gate.take();
        let task = cx.background_executor().spawn({
            let prefs = prefs.clone();
            async move {
                #[cfg(test)]
                if let Some(gate) = save_gate {
                    gate.await;
                }
                path.map_or(Ok(()), |path| save(&path, &prefs))
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.applying = false;
                match result {
                    Ok(()) => {
                        this.shared.borrow_mut().current = Some(prefs);
                        this.shared.borrow_mut().error = None;
                        this.error = None;
                        this.save_error = None;
                        if reset_theme == Some(this.theme_revision) {
                            cx.emit(Event::ThemeSelected(ThemeChoice::System));
                        }
                        if check_updates && this.draft.updates == saved_updates {
                            updater::spawn_check(saved_updates.include_prereleases, true, cx);
                        }
                        cx.emit(Event::Applied);
                        if !unticked.is_empty() {
                            cx.emit(Event::CategoriesUnticked(unticked));
                        }
                        if this.save_pending {
                            this.save_pending = false;
                            this.persist(cx);
                        }
                    }
                    Err(e) => {
                        this.reset_theme = None;
                        this.save_pending = false;
                        this.draft = this.shared.borrow().current.clone().unwrap_or_default();
                        this.update_fields(cx);
                        this.error = Some(format!(
                            "Could not save settings: {e}. Previous defaults remain active."
                        ));
                        this.save_error = this.error.clone();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub fn setup(&mut self, model: Model, remove: bool, cx: &mut Context<Self>) {
        if self.working.is_some() || model_work::busy() {
            self.error = Some("Another model job is running. Retry when it finishes.".into());
            cx.notify();
            return;
        }
        let permit = match model_work::Permit::acquire_for(models::setup_label(model, remove)) {
            Ok(permit) => permit,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.checks += 1;
        self.checking = false;
        self.working = Some(model);
        self.error = None;
        let progress = Progress::default();
        progress.state.lock().unwrap().planned = if remove {
            0
        } else {
            self.pending[Self::index(model)].total()
        };
        self.progress = Some(progress.clone());
        let model_state = self.model_state.clone();
        let paths = self.model_files()[Self::index(model)].clone();
        let task = cx.background_executor().spawn({
            let progress = progress.clone();
            async move {
                let _permit = permit;
                let result = if remove {
                    match model {
                        Model::Ocr(m) => ocr::remove_reserved(m).map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::remove_reserved(m).map_err(|e| e.to_string()),
                    }
                    .map(|_| (None, None))
                } else {
                    match model {
                        Model::Ocr(m) => ocr::install_reserved(
                            &OcrConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|(installed, files)| (Some(installed), Some(files)))
                        .map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::setup_reserved(
                            &PiiConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|files| (None, Some(files)))
                        .map_err(|e| e.to_string()),
                    }
                };
                let (result, checked_files) = match result {
                    Ok((installed, files)) => (Ok(installed), files),
                    Err(error) => (Err(error), None),
                };
                SetupCompletion::remember(
                    model,
                    remove,
                    &progress,
                    result,
                    model_state,
                    checked_files,
                    &paths,
                )
            }
        });
        cx.spawn(async move |this, cx| {
            let completion = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_setup(model, completion, cx);
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.working.is_some() {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
                {
                    continue;
                }
                break;
            }
        })
        .detach();
        cx.notify();
    }
    fn finish_setup(&mut self, model: Model, completion: SetupCompletion, cx: &mut Context<Self>) {
        let SetupCompletion {
            mut result,
            mut status,
            model_state,
        } = completion;
        let index = Self::index(model);
        if status == Status::Ready
            && model_state.status(model, &Files::read(&self.model_files()[index]))
                != Some(Status::Ready)
        {
            status = Status::Unknown;
            result = Err("Model files changed. Retry setup.".into());
        }
        // Runtimes are shared, so any setup can change every model's size.
        self.pending = Model::ALL.map(models::pending).to_vec();
        self.statuses[index] = status.clone();
        if let Some(rows) = &mut self.row_statuses {
            rows[index] = status;
        }
        self.model_state = model_state;
        if let Model::Ocr(m) = model {
            self.ocr_installations.retain(|i| i.config.model != m);
            if let Ok(Some(i)) = &result {
                self.ocr_installations.push(i.clone());
            }
        }
        cx.emit(Event::Finished(model, result.clone()));
        self.error = result.err();
        self.working = None;
        self.progress = None;
        cx.notify();
    }
    /// Settings → Updates: detection (ADR 0038) and, on macOS, installation
    /// after an explicit click (ADR 0040).
    fn updates_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let updates = self.draft.updates;
        let state = updater::state(cx);
        let prerelease_build = updater::is_prerelease_build();
        let installs = !matches!(
            state.eligibility,
            Some(Err(updater::Ineligible::Unsupported))
        );
        let status = match &state.install {
            updater::Install::Downloading { version, .. } => format!(
                "Downloading mdoc {version}… {}%",
                state.percent().unwrap_or(0)
            ),
            updater::Install::Ready { version, .. } => {
                format!(
                    "mdoc {version} is ready. It replaces this version when mdoc restarts or quits."
                )
            }
            updater::Install::Failed { version, message } => {
                format!("Couldn't update to mdoc {version}. {message}")
            }
            updater::Install::Idle if state.checking => "Checking for updates…".to_string(),
            updater::Install::Idle => match &state.available {
                Some(update) => format!(
                    "mdoc {} is available. You have {}.",
                    update.version,
                    env!("CARGO_PKG_VERSION")
                ),
                None if state.checked => {
                    format!("mdoc {} is the latest version.", env!("CARGO_PKG_VERSION"))
                }
                None => format!("You have mdoc {}.", env!("CARGO_PKG_VERSION")),
            },
        };
        // Why this copy can't install the available update, if it can't.
        let blocked = state
            .available
            .as_ref()
            .and_then(|_| state.installable().err())
            .filter(|why| !why.is_empty() && installs);
        let can_install = state.installable().is_ok()
            && matches!(
                state.install,
                updater::Install::Idle | updater::Install::Failed { .. }
            );
        let notes = state.available.as_ref().map(|u| {
            let text = u.notes.trim();
            match text.char_indices().nth(400) {
                Some((end, _)) => format!("{}…", &text[..end]),
                None => text.to_owned(),
            }
        });
        let release = state
            .available
            .as_ref()
            .map(|u| u.html_url.clone())
            .or_else(|| match &state.install {
                updater::Install::Failed { version, .. } => Some(updater::release_url(version)),
                _ => None,
            });
        let busy = matches!(state.install, updater::Install::Downloading { .. });
        let actions = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap_2()
            .child(self.switch(
                "updates-automatic",
                "Check when mdoc starts",
                updates.check_automatically,
                true,
                |p| p.updates.check_automatically = !p.updates.check_automatically,
                theme,
                cx,
            ))
            .when(!busy, |v| {
                v.child(
                    self.scrolled_quiet("updates-check", "Check now", theme, !state.checking, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "updates-check".into()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            updater::spawn_check(this.draft.updates.include_prereleases, false, cx)
                        })),
                )
            })
            .when(can_install, |v| {
                v.child(
                    self.scrolled_control(
                        "updates-install",
                        if matches!(state.install, updater::Install::Failed { .. }) {
                            "Try again"
                        } else {
                            "Download and install"
                        },
                        theme,
                        true,
                        cx,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "updates-install".into())
                    })
                    .on_click(|_, _, cx| updater::start_install(cx)),
                )
            })
            .when(busy, |v| {
                v.child(
                    self.scrolled_quiet("updates-cancel", "Cancel", theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "updates-cancel".into()))
                        .on_click(|_, _, cx| updater::cancel_install(cx)),
                )
            })
            .when(
                matches!(state.install, updater::Install::Ready { .. }),
                |v| {
                    v.child(
                        self.scrolled_control("updates-restart", "Restart now", theme, true, cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "updates-restart".into())
                            })
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::RestartToUpdate))),
                    )
                },
            )
            .when_some(release, |v, url| {
                v.child(
                    self.scrolled_control("updates-view", "View release ↗", theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "updates-view".into()))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_3()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child("Automatic updates")
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .child("Check when mdoc starts"),
                            )
                            .child(
                                div()
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "updates-status".into())
                                    })
                                    .text_size(px(11.))
                                    .text_color(p.header_muted)
                                    .child(status),
                            ),
                    )
                    .child(div().max_w(px(260.)).child(actions)),
            )
            .when_some(
                match &state.install {
                    updater::Install::Downloading {
                        received, total, ..
                    } => Some((*received, *total)),
                    _ => None,
                },
                |v, (received, total)| v.child(ui::progress_bar(received, total, theme)),
            )
            .when_some(blocked, |v, why| {
                v.child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(why),
                )
            })
            .when_some(notes.filter(|n| !n.is_empty()), |v, notes| {
                v.child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(notes),
                )
            })
            .child(self.setting_row(
                "Include pre-releases",
                if prerelease_build {
                    "This build always includes pre-releases."
                } else {
                    "Also offer versions published for testing."
                },
                self.switch(
                    "updates-prereleases",
                    "Include pre-releases",
                    updates.include_prereleases || prerelease_build,
                    !prerelease_build,
                    |p| p.updates.include_prereleases = !p.updates.include_prereleases,
                    theme,
                    cx,
                ),
                theme,
            ))
            .into_any_element()
    }
    fn section_note(&self, text: &str, model: Model) -> String {
        match self.pending[Self::index(model)].runtime {
            0 => text.to_owned(),
            runtime => format!(
                "{text} The first download adds a {} MB shared runtime.",
                model_download::megabytes(runtime)
            ),
        }
    }
    fn row(&self, model: Model, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = theme.search_accent();
        let index = Self::index(model);
        let status = self.row_status(model);
        let pending = self.pending[index];
        let selected = match model {
            Model::Ocr(m) => self.draft.ocr.model == m,
            Model::Pii(m) => self.draft.pseudonymization.model == m,
        };
        let selectable = !matches!(self.statuses[index], Status::Unavailable(_));
        let idle = self.working.is_none() && !model_work::busy();
        let working = self.working == Some(model);
        let checking = self.row_checking(model);
        let missing = matches!(status, Status::Missing)
            || (matches!(status, Status::Unknown) && !checking && pending.total() > 0);
        let removable = idle && !matches!(self.statuses[index], Status::Missing | Status::Unknown);
        let bytes = match model {
            Model::Ocr(m) => ocr::manifest::for_model(m)
                .artifacts
                .iter()
                .map(|a| a.size)
                .sum::<u64>(),
            Model::Pii(m) => mdoc_detect::manifest_for(m)
                .files
                .iter()
                .map(|a| a.bytes)
                .sum::<u64>(),
        };
        // A missing model's size sits on its Download button.
        let status_label = if checking {
            "Checking…".to_owned()
        } else {
            status_label(status)
        };
        let download_label = format!(
            "Download · {} MB",
            model_download::megabytes(if pending.model > 0 {
                pending.model
            } else {
                bytes
            })
        );
        let note = (missing && selected && !working).then_some("Downloads on first use");
        div().id(("settings-choice", index)).flex().flex_col().min_w_0()
            .border_b_1().border_color(palette.border)
            .when(selected, |v| v.bg(gpui::Hsla { a: 0.05, ..accent }))
            .child(div().flex().flex_wrap().items_center().gap_2().p_2()
                .child(crate::ui::focus_ring(("select-model", index), theme).role(gpui::Role::RadioButton).aria_label(models::title(model)).aria_toggled(if selected { gpui::Toggled::True } else { gpui::Toggled::False }).key_context("UiControl").tab_index(0).tab_stop(selectable).flex().flex_1().flex_basis(px(220.)).min_w(px(200.)).items_center().gap_3()
                    .when(selectable, |v| v.cursor_pointer())
                    .when(!selectable, |v| v.opacity(0.5))
                    .child(div().flex().flex_col().flex_1().min_w_0().gap_1()
                        .child(div().flex().items_center().gap_2().min_w_0()
                            .child(div().text_size(px(13.)).text_ellipsis().child(models::title(model)))
                            .when(model.recommended(), |v| v.child(div().flex_shrink_0().px_1().rounded_sm().text_size(px(10.)).text_color(accent).border_1().border_color(gpui::Hsla { a: 0.5, ..accent }).child("Recommended"))))
                        .child(div().text_size(px(11.)).text_color(palette.header_muted).child(match note {
                            Some(note) => format!("{} · {} MB · {status_label} · {note}", model.summary(), model_download::megabytes(bytes)),
                            None => format!("{} · {} MB · {status_label}", model.summary(), model_download::megabytes(bytes)),
                        })))
                    .child(div().text_color(if selected { accent } else { palette.header_muted }).child(if selected { "●" } else { "○" }))
                    .when(cfg!(test) && model == Model::Pii(PiiModel::Fp32), |v| v.debug_selector(|| "settings-select-fp32".into()))
                    .map(|v| self.reveal(("select-model", index), v, selectable, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if selectable && !selected {
                            this.select(model, cx);
                        }
                    })))
                .child(div().flex().flex_shrink_0().items_center().gap_2()
                    .when(working, |v| v.child(self.scrolled_quiet(("cancel-model", index), "Cancel", theme, !self.cancelling(), cx)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_setup(cx)))))
                    .when(missing && !working, |v| v.child(self.scrolled_control(("download-model", index), download_label, theme, idle && ocr::SUPPORTED, cx)
                        .on_click(cx.listener(move |this, _, _, cx| { if idle && ocr::SUPPORTED { this.setup(model, false, cx); } }))))
                    .when(matches!(status, Status::Damaged(_)) && !working, |v| v.child(self.scrolled_control(("repair-now", index), "Repair", theme, idle && ocr::SUPPORTED, cx)
                        .on_click(cx.listener(move |this, _, _, cx| { if idle && ocr::SUPPORTED { this.setup(model, false, cx); } }))))
                    .when(!missing && !working && !checking, |v| v.child(self.scrolled_quiet(("remove-model", index), "Remove…", theme, removable, cx).on_click(cx.listener(move |_, _, window, cx| {
                        if !removable { return; }
                        let prompt = window.prompt(PromptLevel::Warning, &format!("Remove {} model files?", model.name()), Some("Shared runtimes and documents are retained."), &["Remove", "Cancel"], cx);
                        cx.spawn(async move |this, cx| { if prompt.await.ok() == Some(0) { let _ = this.update(cx, |this, cx| this.setup(model, true, cx)); } }).detach();
                    }))))
                    .child(self.scrolled_quiet(("model-details", index), if self.details == Some(model) { "Less ↑" } else { "Details ↓" }, theme, true, cx)
                        .when(cfg!(test) && model == Model::Pii(PiiModel::Fp16), |v| v.debug_selector(|| "settings-details-fp16".into()))
                        .on_click(cx.listener(move |this, _, _, cx| { this.details = if this.details == Some(model) { None } else { Some(model) }; cx.notify(); })))))
            .child(div().px_2().pb_2().child(ui::model_attribution(("model-attribution", index), model, theme)))
            .when_some(self.progress_of(model), |v, state| v.child(div().px_2().pb_3().child(ui::setup_progress(("model-progress", index), &state, self.cancelling(), theme))))
            .when(self.details == Some(model), |v| {
                let (revision, path) = match model {
                    Model::Ocr(m) => (ocr::manifest::for_model(m).revision.to_string(), ocr::root().map(|root| ocr::model_root(&root, m)).map_err(|e| e.to_string())),
                    Model::Pii(m) => (mdoc_detect::manifest_for(m).revision, mdoc_detect::root_for(m).map_err(|e| e.to_string())),
                };
                let evidence = match model { Model::Ocr(m) => m.evidence(), Model::Pii(m) => m.evidence() };
                let (_, license) = models::source_and_license(model);
                let mut details = div().flex().flex_col().gap_2().px_2().pb_3().min_w_0().text_size(px(11.)).text_color(palette.header_muted)
                    .child(div().text_color(palette.header_fg).child(format!("{} · {} MB", model.name(), model_download::megabytes(bytes))))
                    .child(evidence)
                    .when_some(match status { Status::Damaged(e) | Status::Unavailable(e) => Some(e.clone()), _ => None }, |v, reason| v.child(reason));
                if let Model::Pii(m) = model {
                    let manifest = mdoc_detect::manifest_for(m);
                    let export_url = format!("https://huggingface.co/{}/tree/{}", manifest.repository, manifest.revision);
                    details = details.child(m.description()).child(format!("Languages: {}", m.languages()))
                        .child(div().flex().flex_wrap().gap_2()
                            .child(self.scrolled_quiet(("settings-hf", index * 2), "Model card ↗", theme, true, cx).on_click(move |_, _, cx| cx.open_url(m.hugging_face_url())))
                            .child(self.scrolled_quiet(("settings-hf", index * 2 + 1), "Pinned files ↗", theme, true, cx).on_click(move |_, _, cx| cx.open_url(&export_url))));
                }
                details = details.child(format!("Revision: {revision}\nLicenses: {license}\nStorage: {}", path.map(|p| p.display().to_string()).unwrap_or_else(|e| e)))
                    .child(match model {
                        Model::Ocr(_) => "CPU · ONNX Runtime 1.27.0 · PDFium native-v7988\nFixed upstream precision · converter-managed threads\nLimits: 256 MiB PDF · 4-page batches · 256 MiB bitmap/page",
                        Model::Pii(_) => "CPU · ONNX Runtime 1.27.0 · 4 threads\nLimits: 2 MiB source · 512 tokens/window · 120 s cooperative deadline",
                    })
                    .child(div().flex().flex_wrap().gap_2()
                        .when(!missing, |v| v.child(self.scrolled_quiet(("repair-model", index), "Repair", theme, idle && ocr::SUPPORTED, cx)
                            .on_click(cx.listener(move |this, _, _, cx| { if idle && ocr::SUPPORTED { this.setup(model, false, cx); } }))))
);
                v.child(details)
            }).into_any_element()
    }
}
impl gpui::Focusable for Panel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Panel {
    fn appearance_section(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        self.setting_row(
            "Appearance",
            "Follow the system or choose a theme.",
            div()
                .map(|v| {
                    #[cfg(all(
                        debug_assertions,
                        feature = "headless-renders",
                        any(target_os = "macos", target_os = "windows")
                    ))]
                    {
                        let bounds = self.appearance_bounds.clone();
                        v.on_children_prepainted(move |children, _, _| {
                            bounds.set(children.try_into().ok())
                        })
                    }
                    #[cfg(not(all(
                        debug_assertions,
                        feature = "headless-renders",
                        any(target_os = "macos", target_os = "windows")
                    )))]
                    {
                        v
                    }
                })
                .id("settings-theme")
                .role(gpui::Role::RadioGroup)
                .aria_label("Theme")
                .flex()
                .gap_2()
                .children(
                    [
                        (ThemeChoice::System, "System", "settings-theme-system"),
                        (ThemeChoice::Light, "Light", "settings-theme-light"),
                        (ThemeChoice::Dark, "Dark", "settings-theme-dark"),
                    ]
                    .into_iter()
                    .map(|(choice, label, id)| {
                        let selected = self.theme_choice == choice;
                        self.scrolled_control(id, "", theme, true, cx)
                            .flex_1()
                            .flex()
                            .justify_center()
                            .role(gpui::Role::RadioButton)
                            .aria_label(label)
                            .aria_toggled(if selected {
                                gpui::Toggled::True
                            } else {
                                gpui::Toggled::False
                            })
                            .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                            .child(
                                div()
                                    .when(cfg!(test) && selected, |v| {
                                        v.debug_selector(move || format!("{id}-selected"))
                                    })
                                    .child(label),
                            )
                            .when(selected, |v| {
                                v.border_color(theme.search_accent())
                                    .bg(theme.sidebar_selected())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.theme_revision += 1;
                                this.reset_theme = None;
                                cx.emit(Event::ThemeSelected(choice))
                            }))
                    }),
                ),
            theme,
        )
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.category_render(window, cx)
    }
}

#[cfg(test)]
mod tests;
