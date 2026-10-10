use crate::{
    markdown_search::SearchInput,
    model_download::{Pending, Progress},
    settings::*,
    style::Theme,
    *,
};
use gpui::{AnyElement, FocusHandle, MouseButton, actions};
use std::time::Duration;

actions!(
    model_settings,
    [CloseSettings, NextSettingsField, PreviousSettingsField]
);
pub enum Event {
    Applied,
    Checked,
    Compare(Model),
    Finished(Model, Result<Option<ocr::Installed>, String>),
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
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    checks: u64,
    refresh_watch: Option<gpui::Task<()>>,
    check_pending: bool,
    checking: bool,
    scroll: gpui::ScrollHandle,
    controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
}
impl gpui::EventEmitter<Event> for Panel {}
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseSettings, Some("ModelSettings")),
        KeyBinding::new("tab", NextSettingsField, Some("ModelSettings")),
        KeyBinding::new("shift-tab", PreviousSettingsField, Some("ModelSettings")),
    ]);
}
fn field(value: &str, cx: &mut Context<Panel>) -> Entity<SearchInput> {
    cx.new(|cx| {
        let mut input = SearchInput::new(cx)
            .with_key_context("SettingsInput")
            .with_placeholder("0–1");
        input.set_value(value.into(), cx);
        input
    })
}
pub fn control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    ui::action_control(id, label, theme, enabled, false)
}
fn quiet_control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    div()
        .id(id)
        .key_context("UiControl")
        .tab_index(0)
        .tab_stop(enabled)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .focus_visible(|s| {
            s.bg(theme.pdf_style().placeholder_bg)
                .text_color(theme.search_accent())
        })
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
        control: gpui::Stateful<gpui::Div>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
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
    ) -> gpui::Stateful<gpui::Div> {
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
    ) -> gpui::Stateful<gpui::Div> {
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
            pending: Model::ALL.map(Model::pending).to_vec(),
            threshold: field(&draft.pseudonymization.threshold.to_string(), cx),
            confidence: field(&draft.ocr.minimum_confidence.to_string(), cx),
            draft,
            error: None,
            applying: false,
            working: None,
            progress: None,
            advanced: false,
            details: None,
            theme,
            focus: cx.focus_handle(),
            previous: None,
            checks: 0,
            refresh_watch: None,
            check_pending: false,
            checking: false,
            scroll: gpui::ScrollHandle::new(),
            controls: Default::default(),
        }
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        self.error = self.shared.borrow().error.clone();
        self.update_fields(cx);
        self.previous = window.focused(cx);
        self.open = true;
        window.focus(&self.focus, cx);
        self.refresh(cx);
        self.refresh_watch = Some(cx.spawn(async move |this, cx| {
            let mut previous = model_work::description();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let Ok(open) = this.update(cx, |this, cx| {
                    if this.check_pending && !model_work::busy() {
                        this.refresh(cx);
                    }
                    let current = model_work::description();
                    if current != previous {
                        previous = current;
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
    /// Open Settings at one feature's models, e.g. from a setup card.
    pub fn show_section(&mut self, model: Model, window: &mut Window, cx: &mut Context<Self>) {
        self.show(window, cx);
        self.scroll
            .scroll_to_item(if matches!(model, Model::Ocr(_)) { 0 } else { 1 });
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
            let pending = Model::ALL.map(Model::pending).to_vec();
            let statuses = Model::ALL
                .into_iter()
                .map(|model| {
                    let result = match model {
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
                        }),
                        Model::Pii(m) => pii::detector::check_reserved(m),
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
        let parsed = (|| {
            self.draft.ocr.minimum_confidence = self
                .confidence
                .read(cx)
                .value()
                .parse()
                .map_err(|_| "OCR confidence must be a number from 0 to 1.")?;
            self.draft.pseudonymization.threshold = self
                .threshold
                .read(cx)
                .value()
                .parse()
                .map_err(|_| "Detection threshold must be a number from 0 to 1.")?;
            self.draft.validate()
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
                        Model::Ocr(m) => ocr::remove_model(m),
                        Model::Pii(m) => pii::detector::remove_model(m),
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
                        .map(Some),
                        Model::Pii(m) => pii::detector::setup_config(
                            &PiiConfig {
                                model: m,
                                ..Default::default()
                            },
                            &progress,
                        )
                        .map(|_| None),
                    }
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let index = Self::index(model);
                // Runtimes are shared, so any setup can change every model's size.
                this.pending = Model::ALL.map(Model::pending).to_vec();
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
            Model::Ocr(m) => m.manifest().artifacts.iter().map(|a| a.size).sum::<u64>(),
            Model::Pii(m) => pii::detector::manifest_for(m)
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
                .child(div().id(("select-model", index)).role(gpui::Role::Button).aria_label(model.title()).aria_toggled(if selected { gpui::Toggled::True } else { gpui::Toggled::False }).focus_visible(|s| s.bg(palette.placeholder_bg)).key_context("UiControl").tab_index(0).tab_stop(selectable).flex().flex_1().flex_basis(px(220.)).min_w(px(200.)).items_center().gap_3()
                    .when(selectable, |v| v.cursor_pointer())
                    .when(!selectable, |v| v.opacity(0.5))
                    .child(div().text_color(if selected { accent } else { palette.header_muted }).child(if selected { "●" } else { "○" }))
                    .child(div().flex().flex_col().flex_1().min_w_0().gap_1()
                        .child(div().flex().items_center().gap_2().min_w_0()
                            .child(div().text_size(px(13.)).text_ellipsis().child(model.title()))
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
            .when_some(self.progress_of(model), |v, state| v.child(div().px_3().pb_3().child(ui::setup_progress(("model-progress", index), &state, self.cancelling(), theme))))
            .when(self.details == Some(model), |v| {
                let (revision, path) = match model {
                    Model::Ocr(m) => (m.manifest().revision.to_string(), ocr::root().map(|root| ocr::model_root(&root, m))),
                    Model::Pii(m) => (pii::detector::manifest_for(m).revision, pii::detector::root_for(m)),
                };
                let evidence = match model { Model::Ocr(m) => m.evidence(), Model::Pii(m) => m.evidence() };
                let license = match model { Model::Ocr(_) => "Apache-2.0", Model::Pii(m) => m.license() };
                let mut details = div().flex().flex_col().gap_2().px_3().pb_3().min_w_0().text_size(px(11.)).text_color(palette.header_muted)
                    .child(div().text_color(palette.header_fg).child(format!("{} · {} MB", model.name(), model_download::megabytes(bytes))))
                    .child(evidence)
                    .when_some(match status { Status::Damaged(e) | Status::Unavailable(e) => Some(e.clone()), _ => None }, |v, reason| v.child(reason));
                if let Model::Pii(m) = model {
                    let manifest = pii::detector::manifest_for(m);
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
        div().id("settings-overlay").absolute().inset_0().p_4().flex().items_center().justify_center().occlude().bg(gpui::Hsla { a: 0.35, ..p.bg })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(ui::panel("settings-dialog", theme).when(cfg!(test), |v| v.debug_selector(|| "settings-dialog".into()))
                .key_context("ModelSettings UiPanel").track_focus(&self.focus).tab_group().tab_stop(false).w(px(600.)).max_w_full()
                .max_h((window.viewport_size().height - px(32.)).max(px(160.)))
                .flex().flex_col().rounded_lg().shadow_lg().bg(p.bg).text_color(p.header_fg).text_size(px(13.))
                .border_1().border_color(p.border)
                .on_action(cx.listener(|this, _: &CloseSettings, w, cx| this.close(w, cx)))
                .on_action(|_: &New, _, cx| cx.stop_propagation())
                .on_action(|_: &Open, _, cx| cx.stop_propagation())
                .on_action(|_: &Close, _, cx| cx.stop_propagation())
                .on_action(|_: &NextTab, _, cx| cx.stop_propagation())
                .on_action(|_: &PreviousTab, _, cx| cx.stop_propagation())
                .on_action(|_: &Save, _, cx| cx.stop_propagation())
                .on_action(|_: &SaveAs, _, cx| cx.stop_propagation())
                .on_action(|_: &Import, _, cx| cx.stop_propagation())
                .on_action(cx.listener(|this, _: &NextSettingsField, w, cx| {
                    ui::cycle(w, cx, Some(&this.focus), false);
                }))
                .on_action(cx.listener(|this, _: &PreviousSettingsField, w, cx| {
                    ui::cycle(w, cx, Some(&this.focus), true);
                }))
                .on_action(cx.listener(|this, _: &ui::NextControl, w, cx| { ui::cycle(w, cx, Some(&this.focus), false); cx.stop_propagation(); }))
                .on_action(cx.listener(|this, _: &ui::PreviousControl, w, cx| { ui::cycle(w, cx, Some(&this.focus), true); cx.stop_propagation(); }))
                .child(div().px_4().pt_4().pb_3().flex().flex_col().gap_1().flex_shrink_0()
                    .child(div().text_size(px(18.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Settings"))
                    .child(div().text_size(px(12.)).text_color(p.header_muted).child("Models run on this computer. Choices apply to your next run.")))
                .child(div().id("settings-scroll").when(cfg!(test), |v| v.debug_selector(|| "settings-scroll".into())).track_scroll(&self.scroll).overflow_y_scroll().min_h_0().flex_1().px_4().pb_3()
                    .child(div().flex().flex_col().gap_2()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Text recognition"))
                        .child(div().text_size(px(11.)).text_color(p.header_muted).child(self.section_note("Reads scanned pages. English is supported now; more languages will follow.", Model::Ocr(self.draft.ocr.model))))
                        .children(OcrModel::ALL.into_iter().map(|m| self.row(Model::Ocr(m), cx))))
                    .child(div().flex().flex_col().gap_2().mt_4()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Pseudonymization"))
                        .child(div().text_size(px(11.)).text_color(p.header_muted).child(self.section_note("Detection can miss names and identifiers — review the whole document before sharing.", Model::Pii(self.draft.pseudonymization.model))))
                        .children(PiiModel::ALL.into_iter().map(|m| self.row(Model::Pii(m), cx))))
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
                                .on_click(cx.listener(move |this, _, _, cx| { if changed && !this.applying { this.apply(cx); } })))))))
                .when_some(self.error.clone(), |v, e| v.child(div().px_4().pb_3().text_size(px(12.)).text_color(style::markdown_style(theme).alert_warning).child(e)))
                // Another app job (a scan or comparison) holds the model slot.
                .when_some(model_work::description().filter(|_| self.working.is_none() && !self.checking), |v, work| v.child(ui::activity("model-work-activity", work, theme).px_4().pb_2()))
                .child(div().flex().flex_wrap().gap_2().items_center().flex_shrink_0().px_4().py_3().border_t_1().border_color(p.border)
                    .child(quiet_control("reset-settings", "Reset defaults", theme, !self.applying).when(cfg!(test), |v| v.debug_selector(|| "reset-settings".into())).on_click(cx.listener(|this, _, _, cx| this.reset(cx))))
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
    #[gpui::test]
    fn keyboard_focus_reveals_choices_in_small_scrolling_panel(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        cx.update(ui::bind_keys);
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(Store::new(), Rc::new(Cell::new(Theme::default())), cx)
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
    fn apply_is_explicit_and_close_discards_draft(cx: &mut TestAppContext) {
        let shared = Store::new();
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
        let shared = Store::new();
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
        let shared = Store::new();
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
