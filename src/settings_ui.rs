use crate::{
    document_view::{Save, SaveAs},
    markdown_search::SearchInput,
    models,
    style::{self, Theme},
    ui, updater,
    workspace::{Close, Import, New, NextTab, Open, PreviousTab},
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, KeyBinding, MouseButton, PromptLevel,
    Window, actions, div, prelude::*, px,
};
use mdoc_import::{
    model_download::{self, Pending, Progress},
    model_work, ocr,
};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings::{
    Model, OcrConfig, OcrModel, PiiConfig, PiiModel, Preferences, Shared, SpellingConfig,
    UpdatesConfig, save,
};
use std::{cell::Cell, rc::Rc, sync::atomic::Ordering, time::Duration};

pub(crate) mod dictionary;

actions!(
    model_settings,
    [CloseSettings, NextSettingsField, PreviousSettingsField]
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
}
#[derive(Clone, Debug)]
pub enum Status {
    Unknown,
    Missing,
    Ready,
    Damaged(String),
    Unavailable(String),
}
impl Status {
    fn label(&self) -> String {
        match self {
            Self::Unknown => "Not checked".into(),
            Self::Missing => "Not downloaded".into(),
            Self::Ready => "Installed".into(),
            Self::Damaged(_) => "Repair needed".into(),
            Self::Unavailable(_) => "Unavailable".into(),
        }
    }
}
pub struct Panel {
    pub open: bool,
    pub shared: Shared,
    pub statuses: Vec<Status>,
    pub ocr_installations: Vec<ocr::Installed>,
    /// Bytes each model in `Model::ALL` still needs, measured cheaply by size.
    pub pending: Vec<Pending>,
    pub draft: Preferences,
    pub threshold: Entity<SearchInput>,
    pub confidence: Entity<SearchInput>,
    pub error: Option<String>,
    pub applying: bool,
    pub working: Option<Model>,
    progress: Option<Progress>,
    pub advanced: bool,
    pub details: Option<Model>,
    theme: Rc<Cell<Theme>>,
    /// The window's current choice, projected into this control.
    theme_choice: ThemeChoice,
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
        Self {
            open: false,
            shared,
            statuses: vec![Status::Unknown; Model::ALL.len()],
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
            applying: false,
            working: None,
            progress: None,
            advanced: false,
            details: None,
            theme,
            theme_choice: ThemeChoice::default(),
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
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        self.error = self.shared.borrow().error.as_ref().map(ToString::to_string);
        self.update_fields(cx);
        self.previous = window.focused(cx);
        self.open = true;
        window.focus(&self.focus, cx);
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
        self.theme_choice = choice;
        // The resolved theme may change even while System stays selected.
        cx.notify();
    }
    /// Open Settings at one feature's models, e.g. from a setup card.
    pub fn show_section(&mut self, model: Model, window: &mut Window, cx: &mut Context<Self>) {
        self.show(window, cx);
        self.scroll
            .scroll_to_item(if matches!(model, Model::Ocr(_)) { 1 } else { 2 });
        cx.notify();
    }
    /// Open Settings at Updates, the last section (the update banner).
    pub fn show_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show(window, cx);
        self.scroll.scroll_to_bottom();
        cx.notify();
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
        if self.applying || matches!(self.statuses[Self::index(model)], Status::Unavailable(_)) {
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
        // Unsaved numeric text is discarded; saved choices already apply.
        self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        self.update_fields(cx);
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
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.working.is_some() || model_work::busy() {
            self.check_pending = true;
            return;
        }
        let Ok(permit) = model_work::Permit::acquire_for("checking installed models") else {
            return;
        };
        self.checks += 1;
        self.check_pending = false;
        self.checking = true;
        let generation = self.checks;
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let mut installed = Vec::new();
            let pending = Model::ALL.map(models::pending).to_vec();
            let statuses = Model::ALL
                .into_iter()
                .map(|model| {
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
                    if !ocr::SUPPORTED {
                        Status::Unavailable(
                            "Automatic inference supports Apple Silicon macOS and Windows x64."
                                .into(),
                        )
                    } else {
                        match result {
                            Ok(true) => Status::Ready,
                            Ok(false) => Status::Missing,
                            Err(e) => Status::Unavailable(e),
                        }
                    }
                })
                .collect::<Vec<_>>();
            (statuses, installed, pending)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.checks == generation && this.working.is_none() {
                    this.checking = false;
                    this.statuses = result.0;
                    this.ocr_installations = result.1;
                    this.pending = result.2;
                    cx.emit(Event::Checked);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Save the advanced numeric fields together with the current choices.
    pub fn apply(&mut self, cx: &mut Context<Self>) {
        if self.applying {
            return;
        }
        let parsed =
            (|| {
                self.draft.ocr.minimum_confidence = self
                    .confidence
                    .read(cx)
                    .value()
                    .parse()
                    .map_err(|_| "OCR confidence must be a number from 0 to 1.".to_string())?;
                self.draft.pseudonymization.threshold =
                    self.threshold.read(cx).value().parse().map_err(|_| {
                        "Detection threshold must be a number from 0 to 1.".to_string()
                    })?;
                self.draft.validate().map_err(|e| e.to_string())
            })();
        if let Err(e) = parsed {
            self.error = Some(e);
            cx.notify();
            return;
        }
        self.persist(cx);
    }
    /// Restore and save every default, including the recommended models.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        if self.applying {
            return;
        }
        self.draft = Preferences::default();
        self.update_fields(cx);
        self.error = None;
        self.persist(cx);
    }
    /// Atomically save `draft`. A failure restores the previous choices.
    fn persist(&mut self, cx: &mut Context<Self>) {
        let prefs = self.draft.clone();
        let path = self.shared.borrow().path.clone();
        self.applying = true;
        let task = cx.background_executor().spawn({
            let prefs = prefs.clone();
            async move { path.map_or(Ok(()), |path| save(&path, &prefs)) }
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
                        cx.emit(Event::Applied);
                    }
                    Err(e) => {
                        let previous = this.shared.borrow().current.clone();
                        if let Some(previous) = previous {
                            this.draft.ocr.model = previous.ocr.model;
                            this.draft.ocr.dpi = previous.ocr.dpi;
                            this.draft.pseudonymization.model = previous.pseudonymization.model;
                            this.draft.spelling = previous.spelling;
                            this.draft.updates = previous.updates;
                        }
                        this.error = Some(format!(
                            "Could not save settings: {e}. Previous defaults remain active."
                        ))
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
        self.checks += 1;
        self.working = Some(model);
        self.error = None;
        let progress = Progress::default();
        progress.state.lock().unwrap().planned = if remove {
            0
        } else {
            self.pending[Self::index(model)].total()
        };
        self.progress = Some(progress.clone());
        let task = cx.background_executor().spawn({
            let progress = progress.clone();
            async move {
                if remove {
                    match model {
                        Model::Ocr(m) => ocr::remove_model(m).map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::remove_model(m).map_err(|e| e.to_string()),
                    }
                    .map(|_| None)
                } else {
                    match model {
                        Model::Ocr(m) => ocr::install_config(
                            &OcrConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(Some)
                        .map_err(|e| e.to_string()),
                        Model::Pii(m) => mdoc_detect::setup_config(
                            &PiiConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|_| None)
                        .map_err(|e| e.to_string()),
                    }
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let index = Self::index(model);
                // Runtimes are shared, so any setup can change every model's size.
                this.pending = Model::ALL.map(models::pending).to_vec();
                this.statuses[index] = match &result {
                    Ok(_) if remove => Status::Missing,
                    Ok(_) => Status::Ready,
                    Err(_) if progress.cancel.load(Ordering::Relaxed) => Status::Unknown,
                    Err(e) if progress.state.lock().unwrap().phase == "Checking runtime" => {
                        Status::Unavailable(e.clone())
                    }
                    Err(e) => Status::Damaged(e.clone()),
                };
                if let Model::Ocr(m) = model {
                    this.ocr_installations.retain(|i| i.config.model != m);
                    if let Ok(Some(i)) = &result {
                        this.ocr_installations.push(i.clone());
                    }
                }
                cx.emit(Event::Finished(model, result.clone()));
                this.error = result.err();
                this.working = None;
                this.progress = None;
                cx.notify();
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
    /// A section's description, plus the shared runtime it still needs.
    /// Spelling choices save at once, like model choices (ADR 0036).
    fn spelling_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let spelling = self.draft.spelling;
        let toggle = |id: &'static str,
                      label: &'static str,
                      on: bool,
                      enabled: bool,
                      flip: fn(&mut SpellingConfig),
                      cx: &mut Context<Self>| {
            let enabled = enabled && !self.applying;
            self.scrolled_control(id, label, theme, enabled, cx)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .when(on, |v| v.border_color(accent).text_color(accent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled && !this.applying {
                        flip(&mut this.draft.spelling);
                        this.persist(cx);
                    }
                }))
        };
        let on = spelling.enabled;
        div().flex().flex_col().gap_2().mt_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Spelling"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child("Flags likely typos and recognition errors in the Markdown. Code, links and detected names are skipped."))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(toggle("spelling-enabled", "Check spelling", on, true, |s| s.enabled = !s.enabled, cx))
                .child(toggle("spelling-english", "English", spelling.english, on, |s| s.english = !s.english, cx))
                .child(toggle("spelling-russian", "Russian", spelling.russian, on, |s| s.russian = !s.russian, cx)))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(toggle("spelling-caps", "Check ALL-CAPS words", spelling.check_all_caps, on, |s| s.check_all_caps = !s.check_all_caps, cx))
                .child(toggle("spelling-digits", "Check words with digits", spelling.check_digits, on, |s| s.check_digits = !s.check_digits, cx)))
            .child(self.dictionary_summary(theme, cx))
            .into_any_element()
    }
    /// Settings → Saved cases (ADR 0041).
    fn cases_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let on = self.draft.cases.ask_before_closing;
        let enabled = !self.applying;
        div().flex().flex_col().gap_2().mt_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Saved cases"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child(if cfg!(target_os = "macos") {
                "A case keeps which original each alias stands for, encrypted on this computer, so AI replies can be restored later. Open cases from File → Open Case…"
            } else {
                // No menu bar outside macOS: Open Case lives on Home and in the palette.
                "A case keeps which original each alias stands for, encrypted on this computer, so AI replies can be restored later. Open cases from Home or the command palette (Ctrl+K)."
            }))
            .child(div().flex().flex_wrap().items_center().gap_2().child(
                self.scrolled_control("cases-ask", "Ask before closing an unsaved mapping", theme, enabled, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "cases-ask".into()))
                    .aria_toggled(if on { gpui::Toggled::True } else { gpui::Toggled::False })
                    .when(on, |v| v.border_color(accent).text_color(accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if enabled && !this.applying {
                            this.draft.cases.ask_before_closing = !this.draft.cases.ask_before_closing;
                            this.persist(cx);
                        }
                    })),
            ))
            .into_any_element()
    }
    /// Settings → Recent files (ADR 0048): paths only; clearing forgets them.
    fn recents_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        div().flex().flex_col().gap_2().mt_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Recent files"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child(
                "Home and the command palette list files you opened recently: their names and folders, never their contents.",
            ))
            .child(div().flex().flex_wrap().items_center().gap_2().child(
                self.scrolled_quiet("recents-clear", "Clear recent files", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "recents-clear".into()))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::ClearRecents))),
            ))
            .into_any_element()
    }
    /// Settings → Updates: detection (ADR 0038) and, on macOS, installation
    /// after an explicit click (ADR 0040).
    fn updates_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let updates = self.draft.updates;
        let state = updater::state(cx);
        let prerelease_build = updater::is_prerelease_build();
        let toggle = |id: &'static str,
                      label: &'static str,
                      on: bool,
                      enabled: bool,
                      flip: fn(&mut UpdatesConfig),
                      cx: &mut Context<Self>| {
            let enabled = enabled && !self.applying;
            self.scrolled_control(id, label, theme, enabled, cx)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .when(on, |v| v.border_color(accent).text_color(accent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled && !this.applying {
                        flip(&mut this.draft.updates);
                        this.persist(cx);
                        if this.draft.updates.check_automatically {
                            updater::spawn_check(this.draft.updates.include_prereleases, true, cx);
                        }
                    }
                }))
        };
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
        div().flex().flex_col().gap_2().mt_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Updates"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child(if installs {
                "mdoc asks GitHub whether a newer version exists. It sends no document data, and downloads an update only when you click Download and install."
            } else {
                "mdoc asks GitHub whether a newer version exists. It sends no document data and never downloads or installs anything."
            }))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(toggle("updates-automatic", "Check when mdoc starts", updates.check_automatically, true, |u| u.check_automatically = !u.check_automatically, cx))
                .child(toggle("updates-prereleases", "Include pre-releases", updates.include_prereleases || prerelease_build, !prerelease_build, |u| u.include_prereleases = !u.include_prereleases, cx)))
            .when(prerelease_build, |v| v.child(div().text_size(px(11.)).text_color(p.header_muted).child("This is a pre-release build, so pre-releases are always included.")))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(div().when(cfg!(test), |v| v.debug_selector(|| "updates-status".into())).flex_1().child(status))
                .when(!busy, |v| v.child(self.scrolled_quiet("updates-check", "Check now", theme, !state.checking, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-check".into()))
                    .on_click(cx.listener(|this, _, _, cx| updater::spawn_check(this.draft.updates.include_prereleases, false, cx)))))
                .when(can_install, |v| v.child(self.scrolled_control("updates-install", if matches!(state.install, updater::Install::Failed { .. }) { "Try again" } else { "Download and install" }, theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-install".into()))
                    .border_color(accent).text_color(accent)
                    .on_click(|_, _, cx| updater::start_install(cx))))
                .when(busy, |v| v.child(self.scrolled_quiet("updates-cancel", "Cancel", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-cancel".into()))
                    .on_click(|_, _, cx| updater::cancel_install(cx))))
                .when(matches!(state.install, updater::Install::Ready { .. }), |v| v.child(self.scrolled_control("updates-restart", "Restart now", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-restart".into()))
                    .border_color(accent).text_color(accent)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::RestartToUpdate)))))
                .when_some(release, |v, url| v.child(self.scrolled_control("updates-view", "View release ↗", theme, true, cx)
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-view".into()))
                    .on_click(move |_, _, cx| cx.open_url(&url)))))
            .when_some(match &state.install { updater::Install::Downloading { received, total, .. } => Some((*received, *total)), _ => None }, |v, (received, total)| v.child(ui::progress_bar(received, total, theme)))
            .when_some(blocked, |v, why| v.child(div().text_size(px(11.)).text_color(p.header_muted).child(why)))
            .when_some(notes.filter(|n| !n.is_empty()), |v, notes| v.child(div().text_size(px(11.)).text_color(p.header_muted).child(notes)))
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
        let status = &self.statuses[index];
        let pending = self.pending[index];
        let selected = match model {
            Model::Ocr(m) => self.draft.ocr.model == m,
            Model::Pii(m) => self.draft.pseudonymization.model == m,
        };
        let selectable = !matches!(status, Status::Unavailable(_)) && !self.applying;
        let idle = self.working.is_none() && !model_work::busy();
        let working = self.working == Some(model);
        let checking = self.checking && matches!(status, Status::Unknown);
        let missing = matches!(status, Status::Missing)
            || (matches!(status, Status::Unknown) && !checking && pending.total() > 0);
        let removable = idle && !matches!(status, Status::Missing | Status::Unknown);
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
            status.label()
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
            .border_1().border_color(if selected { accent } else { palette.border }).rounded_md()
            .when(selected, |v| v.bg(gpui::Hsla { a: 0.05, ..accent }))
            .child(div().flex().flex_wrap().items_center().gap_2().p_2()
                .child(crate::ui::focus_ring(("select-model", index), theme).role(gpui::Role::RadioButton).aria_label(models::title(model)).aria_toggled(if selected { gpui::Toggled::True } else { gpui::Toggled::False }).key_context("UiControl").tab_index(0).tab_stop(selectable).flex().flex_1().flex_basis(px(220.)).min_w(px(200.)).items_center().gap_3()
                    .when(selectable, |v| v.cursor_pointer())
                    .when(!selectable, |v| v.opacity(0.5))
                    .child(div().text_color(if selected { accent } else { palette.header_muted }).child(if selected { "●" } else { "○" }))
                    .child(div().flex().flex_col().flex_1().min_w_0().gap_1()
                        .child(div().flex().items_center().gap_2().min_w_0()
                            .child(div().text_size(px(13.)).text_ellipsis().child(models::title(model)))
                            .when(model.recommended(), |v| v.child(div().flex_shrink_0().px_1().rounded_sm().text_size(px(10.)).text_color(accent).border_1().border_color(gpui::Hsla { a: 0.5, ..accent }).child("Recommended"))))
                        .child(div().text_size(px(11.)).text_color(palette.header_muted).child(match note {
                            Some(note) => format!("{} · {note}", model.summary()),
                            None => model.summary().to_owned(),
                        })))
                    .when(cfg!(test) && model == Model::Pii(PiiModel::Fp32), |v| v.debug_selector(|| "settings-select-fp32".into()))
                    .map(|v| self.reveal(("select-model", index), v, selectable, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if selectable && !selected {
                            this.select(model, cx);
                        }
                    })))
                .child(div().flex().flex_shrink_0().items_center().gap_2()
                    .when(!working && !missing, |v| v.child(div().text_size(px(11.)).text_color(palette.header_muted).child(status_label)))
                    .when(working, |v| v.child(self.scrolled_quiet(("cancel-model", index), "Cancel", theme, !self.cancelling(), cx)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_setup(cx)))))
                    .when(missing && !working, |v| v.child(self.scrolled_control(("download-model", index), download_label, theme, idle && ocr::SUPPORTED, cx)
                        .on_click(cx.listener(move |this, _, _, cx| { if idle && ocr::SUPPORTED { this.setup(model, false, cx); } }))))
                    .when(matches!(status, Status::Damaged(_)) && !working, |v| v.child(self.scrolled_control(("repair-now", index), "Repair", theme, idle && ocr::SUPPORTED, cx)
                        .on_click(cx.listener(move |this, _, _, cx| { if idle && ocr::SUPPORTED { this.setup(model, false, cx); } }))))
                    .child(self.scrolled_quiet(("model-details", index), if self.details == Some(model) { "Less ↑" } else { "Details ↓" }, theme, true, cx)
                        .when(cfg!(test) && model == Model::Pii(PiiModel::Fp16), |v| v.debug_selector(|| "settings-details-fp16".into()))
                        .on_click(cx.listener(move |this, _, _, cx| { this.details = if this.details == Some(model) { None } else { Some(model) }; cx.notify(); })))))
            .child(div().px_3().pb_2().child(ui::model_attribution(("model-attribution", index), model, theme)))
            .when_some(self.progress_of(model), |v, state| v.child(div().px_3().pb_3().child(ui::setup_progress(("model-progress", index), &state, self.cancelling(), theme))))
            .when(self.details == Some(model), |v| {
                let (revision, path) = match model {
                    Model::Ocr(m) => (ocr::manifest::for_model(m).revision.to_string(), ocr::root().map(|root| ocr::model_root(&root, m)).map_err(|e| e.to_string())),
                    Model::Pii(m) => (mdoc_detect::manifest_for(m).revision, mdoc_detect::root_for(m).map_err(|e| e.to_string())),
                };
                let evidence = match model { Model::Ocr(m) => m.evidence(), Model::Pii(m) => m.evidence() };
                let (_, license) = models::source_and_license(model);
                let mut details = div().flex().flex_col().gap_2().px_3().pb_3().min_w_0().text_size(px(11.)).text_color(palette.header_muted)
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
                        .child(self.scrolled_quiet(("remove-model", index), "Remove model…", theme, removable, cx).on_click(cx.listener(move |_, _, window, cx| {
                            if !removable { return; }
                            let prompt = window.prompt(PromptLevel::Warning, &format!("Remove {} model files?", model.name()), Some("Shared runtimes and documents are retained."), &["Remove", "Cancel"], cx);
                            cx.spawn(async move |this, cx| { if prompt.await.ok() == Some(0) { let _ = this.update(cx, |this, cx| this.setup(model, true, cx)); } }).detach();
                        }))));
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
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mb_4()
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Appearance"),
            )
            .child(
                div()
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
                                .aria_label(style::theme_label(choice))
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
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(Event::ThemeSelected(choice))
                                }))
                        }),
                    ),
            )
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        // Choices save at once; only typed numbers wait for Save.
        let changed = self.shared.borrow().current.as_ref().is_none_or(|current| {
            self.confidence.read(cx).value().parse::<f32>().ok()
                != Some(current.ocr.minimum_confidence)
                || self.threshold.read(cx).value().parse::<f32>().ok()
                    != Some(current.pseudonymization.threshold)
        });
        let ocr_note = self.section_note(
            "Reads scanned pages in English and Russian.",
            Model::Ocr(self.draft.ocr.model),
        );
        div().id("settings-overlay").absolute().inset_0().p_4().flex().items_center().justify_center().occlude().bg(gpui::Hsla { a: 0.35, ..p.bg })
            // A press on the backdrop closes Settings like other overlays;
            // presses inside the dialog stop before reaching it.
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| { cx.stop_propagation(); this.close(w, cx) }))
            .child(ui::panel("settings-dialog", theme).when(cfg!(test), |v| v.debug_selector(|| "settings-dialog".into()))
                // Announced as a dialog named for what it shows (ADR 0064).
                .role(gpui::Role::Dialog).aria_label(if self.dictionary.is_some() { "Your dictionary" } else { "Settings" })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .key_context("ModelSettings UiPanel").track_focus(&self.focus).tab_group().tab_stop(false).w(px(600.)).max_w_full()
                .max_h((window.viewport_size().height - px(32.)).max(px(160.)))
                .flex().flex_col().rounded_lg().shadow_lg().bg(p.bg).text_color(p.header_fg).text_size(px(13.))
                .border_1().border_color(p.border)
                .on_action(cx.listener(|this, _: &CloseSettings, w, cx| if this.dictionary.is_some() { this.close_dictionary(w, cx) } else { this.close(w, cx) }))
                .map(block_document_commands)
                .on_action(cx.listener(|this, _: &NextSettingsField, w, cx| {
                    ui::cycle(w, cx, Some(&this.focus), false);
                }))
                .on_action(cx.listener(|this, _: &PreviousSettingsField, w, cx| {
                    ui::cycle(w, cx, Some(&this.focus), true);
                }))
                .on_action(cx.listener(|this, _: &ui::NextControl, w, cx| { ui::cycle(w, cx, Some(&this.focus), false); cx.stop_propagation(); }))
                .on_action(cx.listener(|this, _: &ui::PreviousControl, w, cx| { ui::cycle(w, cx, Some(&this.focus), true); cx.stop_propagation(); }))
                .when(self.dictionary.is_some(), |v| v.h(px(560.)).child(div().px_4().pt_4().pb_3().flex().flex_col().gap_1().flex_shrink_0()
                    .child(div().flex().items_center().gap_2()
                        .child(quiet_control("close-dictionary", "← Settings", theme, true).when(cfg!(test), |v| v.debug_selector(|| "close-dictionary".into())).on_click(cx.listener(|this, _, w, cx| this.close_dictionary(w, cx))))
                        .child(div().text_size(px(18.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Your dictionary")))
                    .child(div().text_size(px(12.)).text_color(p.header_muted).child("Spellcheck accepts these words in every document. Exact spellings; a lowercase word also accepts its capitalized form."))))
                .when(self.dictionary.is_some(), |v| v.child(self.dictionary_body(theme, cx)))
                .when(self.dictionary.is_none(), |v| v.child(div().px_4().pt_4().pb_3().flex().flex_col().gap_1().flex_shrink_0()
                    .child(div().text_size(px(18.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Settings"))
                    .child(div().text_size(px(12.)).text_color(p.header_muted).child("Appearance applies immediately. Model choices apply to your next run."))))
                .when(self.dictionary.is_none(), |v| v.child(div().id("settings-scroll").when(cfg!(test), |v| v.debug_selector(|| "settings-scroll".into())).track_scroll(&self.scroll).overflow_y_scroll().min_h_0().flex_1().px_4().pb_3()
                    .child(self.appearance_section(theme, cx))
                    .child(div().flex().flex_col().gap_2()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Text recognition"))
                        .child(div().text_size(px(11.)).text_color(p.header_muted).child(ocr_note.clone())
                            .when(cfg!(test), |v| v.debug_selector(move || ocr_note.clone())))
                        .child(div().id("ocr-models").role(gpui::Role::RadioGroup).aria_label("Text recognition").flex().flex_col().gap_2()
                            .children(OcrModel::ALL.into_iter().map(|m| self.row(Model::Ocr(m), cx)))))
                    .child(div().flex().flex_col().gap_2().mt_4()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Pseudonymization"))
                        .child(div().text_size(px(11.)).text_color(p.header_muted).child(self.section_note("Detection can miss names and identifiers — review the whole document before sharing.", Model::Pii(self.draft.pseudonymization.model))))
                        .child(div().id("pii-models").role(gpui::Role::RadioGroup).aria_label("Pseudonymization").flex().flex_col().gap_2()
                            .children(PiiModel::ALL.into_iter().map(|m| self.row(Model::Pii(m), cx)))))
                    .child(self.scrolled_quiet("advanced-settings", if self.advanced { "Advanced ↑" } else { "Advanced ↓" }, theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "settings-advanced".into()))
                        .mt_3().on_click(cx.listener(|this, _, _, cx| { this.advanced = !this.advanced; cx.notify(); })))
                    .when(self.advanced, |v| v.child(div().flex().flex_col().gap_3().mt_2().p_3().rounded_md().bg(theme.sidebar_bg())
                        .child(div().flex().flex_wrap().gap_2()
                            .child(self.scrolled_quiet("compare-ocr", "Compare text recognition", theme, true, cx).on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Compare(Model::Ocr(OcrModel::Cyrillic))))))
                            .child(self.scrolled_quiet("compare-pii", "Compare pseudonymization", theme, true, cx).on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Compare(Model::Pii(PiiModel::Fp16)))))))
                        .child(div().flex().flex_wrap().items_center().gap_2()
                            .child(div().w(px(172.)).child("OCR resolution"))
                            .children([150, 200, 300].map(|dpi| self.scrolled_control(("dpi", dpi as usize), format!("{dpi} DPI"), theme, !self.applying, cx)
                                .when(self.draft.ocr.dpi == dpi, |v| v.border_color(accent).text_color(accent))
                                .on_click(cx.listener(move |this, _, _, cx| { if !this.applying && this.draft.ocr.dpi != dpi { this.draft.ocr.dpi = dpi; this.persist(cx); } })))))
                        .child(div().flex().flex_wrap().items_center().gap_2()
                            .child(div().w(px(172.)).child("OCR minimum confidence"))
                            .child(div().id("settings-confidence-field").w(px(90.)).border_1().rounded_md().border_color(p.border).bg(p.bg).px_2().py_1().child(self.confidence.clone()).map(|v| ui::reveal_focus(v, self.confidence.read(cx).focus_handle(cx), self.scroll.clone()))))
                        .child(div().flex().flex_wrap().items_center().gap_2()
                            .child(div().w(px(172.)).child("Detection threshold"))
                            .child(div().id("settings-threshold-field").w(px(90.)).border_1().rounded_md().border_color(p.border).bg(p.bg).px_2().py_1().child(self.threshold.clone()).map(|v| ui::reveal_focus(v, self.threshold.read(cx).focus_handle(cx), self.scroll.clone()))))
                        .child(div().flex().flex_wrap().items_center().gap_2()
                            .child(div().flex_1().text_size(px(11.)).text_color(p.header_muted).child("Values range from 0 to 1. Higher thresholds return fewer candidates."))
                            .child(self.scrolled_control("apply-settings", if self.applying { "Saving…" } else { "Save" }, theme, !self.applying && changed, cx)
                                .when(cfg!(test), |v| v.debug_selector(|| "settings-apply".into()))
                                .on_click(cx.listener(move |this, _, _, cx| { if changed && !this.applying { this.apply(cx); } }))))))
                    .child(self.spelling_section(theme, cx))
                    .child(self.cases_section(theme, cx))
                    .child(self.recents_section(theme, cx))
                    .child(self.updates_section(theme, cx))))
                .when_some(self.error.clone(), |v, e| v.child(div().px_4().pb_3().text_size(px(12.)).text_color(style::markdown_style(theme).alert_warning).child(e)))
                // Another app job (a scan or comparison) holds the model slot.
                .when_some(self.other_work.filter(|_| self.working.is_none() && !self.checking), |v, work| v.child(ui::activity("model-work-activity", work, theme).px_4().pb_2()))
                .child(div().flex().flex_wrap().gap_2().items_center().flex_shrink_0().px_4().py_3().border_t_1().border_color(p.border)
                    .when(self.dictionary.is_none(), |v| v.child(quiet_control("reset-settings", "Reset defaults", theme, !self.applying).when(cfg!(test), |v| v.debug_selector(|| "reset-settings".into())).on_click(cx.listener(|this, _, _, cx| this.reset(cx)))))
                    .child(div().flex_1())
                    .child(ui::primary_button("close-settings", "Done", theme, true)
                        .when(cfg!(test), |v| v.debug_selector(|| "settings-close".into()))
                        .on_click(cx.listener(|this, _, w, cx| this.close(w, cx)))))
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use gpui::size;
    use mdoc_store::settings::Store;
    #[gpui::test]
    fn text_recognition_note_matches_the_offered_languages(cx: &mut TestAppContext) {
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
        });
        cx.simulate_resize(size(px(800.), px(2400.)));
        for model in OcrModel::ALL {
            panel.update(cx, |panel, cx| {
                panel.open = true;
                panel.draft.ocr.model = model;
                panel.pending.fill(Default::default());
                panel.statuses.fill(Status::Missing);
                cx.notify();
            });
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            assert!(
                cx.debug_bounds("Reads scanned pages in English and Russian.")
                    .is_some()
            );
        }
        assert_eq!(ocr::title(OcrModel::V6Small), "English");
        assert_eq!(ocr::title(OcrModel::Cyrillic), "English & Russian");
    }

    #[gpui::test]
    fn all_download_choices_show_source_and_license_without_details(cx: &mut TestAppContext) {
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
        });
        cx.simulate_resize(size(px(800.), px(2400.)));
        panel.update(cx, |panel, cx| {
            panel.open = true;
            panel.statuses.fill(Status::Missing);
            assert!(panel.details.is_none());
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        for model in Model::ALL {
            let label = format!("{}: {}", model.name(), models::attribution_label(model));
            assert!(
                cx.debug_bounds(Box::leak(label.clone().into_boxed_str()))
                    .is_some(),
                "{label}"
            );
        }
    }
    #[gpui::test]
    fn keyboard_focus_reveals_choices_in_small_scrolling_panel(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        cx.update(ui::bind_keys);
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
        });
        cx.simulate_resize(size(px(640.), px(480.)));
        panel.update_in(cx, |p, window, cx| {
            p.open = true;
            p.statuses.fill(Status::Ready);
            window.focus(&p.focus, cx);
            cx.notify();
        });
        let mut reached = false;
        for _ in 0..12 {
            cx.simulate_keystrokes("tab");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
            }
            reached = cx.update(|window, cx| {
                panel
                    .read(cx)
                    .controls
                    .borrow()
                    .get(&gpui::ElementId::from(("select-model", 3usize)))
                    .is_some_and(|focus| focus.is_focused(window))
            });
            if reached {
                break;
            }
        }
        assert!(reached, "Tab must visit the last model choice");
        let choice = cx.debug_bounds("settings-select-fp32").unwrap();
        let body = cx.debug_bounds("settings-scroll").unwrap();
        assert!(
            choice.top() >= body.top() && choice.bottom() <= body.bottom(),
            "choice={choice:?}, body={body:?}"
        );
    }

    #[gpui::test]
    fn scrolling_keeps_the_entire_focus_ring_clear_of_the_footer(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        cx.update(ui::bind_keys);
        for theme in [Theme::Light, Theme::Dark] {
            let (panel, cx) = cx.add_window_view(|_, cx| {
                Panel::new(Store::in_memory(), Rc::new(Cell::new(theme)), cx)
            });
            cx.simulate_resize(size(px(640.), px(480.)));
            panel.update_in(cx, |p, window, cx| {
                p.open = true;
                p.statuses.fill(Status::Ready);
                window.focus(&p.focus, cx);
                cx.notify();
            });
            for (id, selector, key) in [
                ("advanced-settings", "settings-advanced", "tab"),
                ("cases-ask", "cases-ask", "tab"),
                ("advanced-settings", "settings-advanced", "shift-tab"),
            ] {
                let mut reached = false;
                for _ in 0..40 {
                    cx.simulate_keystrokes(key);
                    for _ in 0..3 {
                        cx.update(|window, cx| {
                            window.refresh();
                            window.draw(cx).clear(cx);
                        });
                    }
                    reached = cx.update(|window, cx| {
                        panel
                            .read(cx)
                            .controls
                            .borrow()
                            .get(&gpui::ElementId::from(id))
                            .is_some_and(|focus| focus.is_focused(window))
                    });
                    if reached {
                        break;
                    }
                }
                assert!(reached, "{key} reaches {id} in {theme:?}");
                let ring = cx.debug_bounds(selector).unwrap().dilate(px(4.));
                let viewport = cx.debug_bounds("settings-scroll").unwrap();
                assert!(
                    ring.top() >= viewport.top() && ring.bottom() <= viewport.bottom(),
                    "{id}: ring={ring:?}, viewport={viewport:?}"
                );
            }
        }
    }

    #[gpui::test]
    fn update_choices_save_immediately_and_an_update_offers_its_release(cx: &mut TestAppContext) {
        let shared = Store::in_memory();
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
        });
        // Tall enough that the Updates section needs no scrolling.
        cx.simulate_resize(size(px(800.), px(2400.)));
        // Not shown: opening would check models through the global job slot.
        panel.update(cx, |panel, cx| {
            panel.open = true;
            cx.notify();
        });
        cx.run_until_parked();
        let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), gpui::Modifiers::none());
            cx.run_until_parked();
        };
        assert!(cx.debug_bounds("updates-view").is_none());

        click(cx, "updates-automatic");
        let saved = shared.borrow().snapshot().unwrap().updates;
        assert!(!saved.check_automatically);
        click(cx, "updates-prereleases");
        let saved = shared.borrow().snapshot().unwrap().updates;
        // A pre-release build always includes pre-releases; the switch is inert.
        assert_eq!(saved.include_prereleases, !updater::is_prerelease_build());
        // Turning the check back on starts one; tests never reach the network.
        click(cx, "updates-automatic");
        assert!(
            shared
                .borrow()
                .snapshot()
                .unwrap()
                .updates
                .check_automatically
        );
        cx.update(|_, cx| assert!(!updater::state(cx).checking));

        let set = |cx: &mut gpui::VisualTestContext, state: updater::UpdateState| {
            cx.update(|_, cx| {
                cx.set_global(state);
                cx.refresh_windows();
            });
            cx.run_until_parked();
        };
        let found = updater::UpdateState {
            checked: true,
            available: Some(updater::test_update("9.0.0")),
            ..Default::default()
        };
        // Until this copy is known to be eligible, only the release page.
        set(cx, found.clone());
        assert!(cx.debug_bounds("updates-view").is_some());
        assert!(cx.debug_bounds("updates-install").is_none());
        // An eligible copy offers the install; a download can be cancelled.
        set(
            cx,
            updater::UpdateState {
                eligibility: Some(Ok(())),
                ..found.clone()
            },
        );
        assert!(cx.debug_bounds("updates-install").is_some());
        set(
            cx,
            updater::UpdateState {
                eligibility: Some(Ok(())),
                install: updater::Install::Downloading {
                    version: "9.0.0".into(),
                    received: 10,
                    total: 100,
                    progress: updater::Progress::default(),
                },
                ..found.clone()
            },
        );
        assert!(cx.debug_bounds("updates-install").is_none());
        click(cx, "updates-cancel");
        cx.update(|_, cx| assert!(matches!(updater::state(cx).install, updater::Install::Idle)));
        // A staged update offers Restart now, which the workspace carries out.
        set(
            cx,
            updater::UpdateState {
                eligibility: Some(Ok(())),
                install: updater::Install::Ready {
                    version: "9.0.0".into(),
                    staged: Default::default(),
                },
                ..found
            },
        );
        let restarted = Rc::new(Cell::new(false));
        let seen = restarted.clone();
        let _subscription = cx.update(|_, cx| {
            cx.subscribe(&panel, move |_, event, _| {
                if matches!(event, Event::RestartToUpdate) {
                    seen.set(true);
                }
            })
        });
        click(cx, "updates-restart");
        assert!(restarted.get());
    }
    #[gpui::test]
    fn apply_is_explicit_and_close_discards_draft(cx: &mut TestAppContext) {
        let shared = Store::in_memory();
        let theme = Rc::new(Cell::new(Theme::default()));
        let (panel, cx) = cx.add_window_view(|_, cx| Panel::new(shared.clone(), theme, cx));
        panel.update_in(cx, |panel, w, cx| {
            panel.show(w, cx);
            panel.draft.ocr.dpi = 300;
            panel
                .threshold
                .update(cx, |i, cx| i.set_value("0.3".into(), cx));
            assert_eq!(shared.borrow().snapshot().unwrap().ocr.dpi, 150);
            panel.close(w, cx);
            panel.show(w, cx);
            assert_eq!(panel.draft.ocr.dpi, 150);
            assert_eq!(panel.threshold.read(cx).value(), "0.5");
            panel.draft.ocr.dpi = 200;
            panel.apply(cx);
        });
        cx.run_until_parked();
        assert_eq!(shared.borrow().snapshot().unwrap().ocr.dpi, 200);
    }
    #[gpui::test]
    fn model_choices_save_immediately_and_failures_revert(cx: &mut TestAppContext) {
        let shared = Store::in_memory();
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
        });
        // Not shown: opening would check models through the global job slot.
        panel.update(cx, |panel, cx| panel.select(Model::Pii(PiiModel::Fp32), cx));
        cx.run_until_parked();
        let current = shared.borrow().snapshot().unwrap();
        assert_eq!(current.pseudonymization.model, PiiModel::Fp32);
        // A selected missing model downloads on first use; nothing starts now.
        panel.update(cx, |panel, _| {
            assert!(panel.working.is_none());
            assert!(panel.needs_setup(Model::Pii(PiiModel::Fp32)));
        });

        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("not-a-directory");
        std::fs::write(&invalid, b"blocked").unwrap();
        shared.borrow_mut().path = Some(invalid.join("settings.json"));
        panel.update(cx, |panel, cx| {
            panel.select(Model::Ocr(OcrModel::Cyrillic), cx)
        });
        cx.run_until_parked();
        assert_eq!(
            shared.borrow().snapshot().unwrap().ocr.model,
            OcrModel::V6Small
        );
        panel.update(cx, |panel, _| {
            assert_eq!(panel.draft.ocr.model, OcrModel::V6Small);
            assert!(panel.error.as_ref().unwrap().contains("Previous defaults"));
        });

        shared.borrow_mut().path = None;
        panel.update(cx, |panel, cx| panel.reset(cx));
        cx.run_until_parked();
        assert_eq!(shared.borrow().snapshot().unwrap(), Preferences::default());
    }
    #[gpui::test]
    fn failed_apply_preserves_active_defaults(cx: &mut TestAppContext) {
        let shared = Store::in_memory();
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("not-a-directory");
        std::fs::write(&invalid, b"blocked").unwrap();
        shared.borrow_mut().path = Some(invalid.join("settings.json"));
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
        });
        panel.update(cx, |panel, cx| {
            panel.draft.ocr.dpi = 300;
            panel.apply(cx);
        });
        cx.run_until_parked();
        assert_eq!(shared.borrow().snapshot().unwrap().ocr.dpi, 150);
        panel.update(cx, |panel, _| {
            assert!(panel.error.as_ref().unwrap().contains("Previous defaults"))
        });
        panel.update(cx, |panel, cx| {
            panel
                .threshold
                .update(cx, |i, cx| i.set_value("NaN".into(), cx));
            panel.apply(cx);
        });
        panel.update(cx, |p, _| assert!(p.error.is_some()));
        assert_eq!(
            shared
                .borrow()
                .snapshot()
                .unwrap()
                .pseudonymization
                .threshold,
            0.5
        );
    }
}
