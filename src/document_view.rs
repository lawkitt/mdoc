//! The document view: one tab's Markdown editor, source preview, search,
//! import/OCR state and pseudonymization review. The window-level
//! [`crate::workspace::Workspace`] owns these views.
use super::*;

mod case;
mod chrome;

#[derive(Clone)]
pub(crate) enum Next {
    Import(import::Imported),
    Close,
    /// Saved first so the new case lists the document (#75); `close` closes
    /// the tab once the case is written.
    CreateCase {
        close: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum OcrState {
    Checking,
    Missing,
    Installing,
    Ready(ocr::Installed),
    Failed(String),
    Unsupported,
}

impl OcrState {
    pub(crate) fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

pub(crate) struct DocumentView {
    pub(crate) focus: gpui::FocusHandle,
    /// F6 regions of the document (ADR 0062); the editor and the
    /// replacements panel bring their own handles.
    pub(crate) toolbar_region: gpui::FocusHandle,
    pub(crate) preview_region: gpui::FocusHandle,
    pub(crate) comments_region: gpui::FocusHandle,
    /// The toolbar's More button, which its menu returns focus to.
    pub(crate) more_focus: gpui::FocusHandle,
    /// The open More menu, which holds focus and Tab (ADR 0062).
    pub(crate) more_menu_focus: gpui::FocusHandle,
    /// The More button's bounds, so a press on it is left to its toggle.
    more_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Whether the last frame drew the Markdown and the original panes: a
    /// narrow window shows one of them (ADR 0062 skips the hidden one).
    panes_drawn: std::cell::Cell<(bool, bool)>,
    pub(crate) owner: (u64, gpui::WeakEntity<workspace::Workspace>),
    pub(crate) active: bool,
    pub(crate) dirty_cached: bool,
    pub(crate) loading: bool,
    pub(crate) load_generation: u64,
    pub(crate) unavailable: bool,
    pub(crate) source_only: bool,
    pub(crate) auto_convert_pending: bool,
    pub(crate) automatic_import: bool,
    pub(crate) generated_unedited: bool,
    pub(crate) blank_disposable: bool,
    /// External files over the empty page: whether any is supported (ADR 0028).
    pub(crate) file_drag: Option<bool>,
    pub(crate) ocr_required: Option<Vec<u32>>,
    pub(crate) import_permit: Option<Arc<import_session::ImportPermit>>,
    pub(crate) conversion_source: Option<PathBuf>,
    pub(crate) import_busy: Arc<AtomicBool>,
    pub(crate) import_cancel: Arc<AtomicBool>,
    pub(crate) preferences: settings::Shared,
    pub(crate) model_panel: Entity<settings_ui::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) title: style::TitleBar,
    pub(crate) editor: Entity<EditorState>,
    pub(crate) images: images::ImageCache,
    pub(crate) session: document_session::DocumentSession,
    pub(crate) pii: pii::ui::ReviewUi,
    pub(crate) spelling: spelling::SpellState,
    pub(crate) preview: preview::PreviewState,
    pub(crate) scroll: ScrollHandle,
    pub(crate) error: Option<String>,
    pub(crate) copy_feedback: Option<gpui::Task<()>>,
    /// The header's ⋯ menu (ADR 0048).
    pub(crate) more_open: bool,
    /// Text zoom of this document's Markdown, not saved (ADR 0048).
    pub(crate) zoom: f32,
    /// Inspector (ADR 0048 variant C): the Share tab is showing.
    pub(crate) inspector_share: bool,
    /// The running copy feedback belongs to Copy for AI, not Copy Markdown.
    pub(crate) copied_for_ai: bool,
    /// The saved case this tab's mapping belongs to (ADR 0041).
    pub(crate) case: Option<cases::CaseLink>,
    pub(crate) case_save: Option<gpui::Task<()>>,
    /// The case's identities, for the panel's "in case" section.
    pub(crate) case_names: Vec<pii::restore::ExportedIdentity>,
    pub(crate) case_names_open: bool,
    pub(crate) case_term: Option<Entity<markdown_search::SearchInput>>,
    /// The result of Pseudonymize Clipboard, until dismissed.
    pub(crate) clipboard_notice: Option<String>,
    /// The before/after view replaces the editor while open.
    pub(crate) compare: Option<pii::ui::CompareState>,
    /// The floating view pill fades when idle (README media decision 9).
    pub(crate) switcher: pii::ui::SwitcherRest,
    /// Categories this tab's case does not detect.
    pub(crate) case_skip: std::collections::HashSet<pii::Category>,
    pub(crate) view_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(crate) split_dragging: bool,
    pub(crate) original_selected: bool,
    pub(crate) ocr_notice_dismissed: bool,
    pub(crate) setup_error_dismissed: bool,
    /// The card's primary action takes focus once when OCR consent appears.
    pub(crate) card_focus: gpui::FocusHandle,
    pub(crate) focus_card: bool,
    pub(crate) prompting: bool,
    pub(crate) job: import_session::ImportSession,
    pub(crate) ocr_state: OcrState,
    /// This document's last OCR run recognized nothing; OCR itself still works.
    pub(crate) ocr_error: Option<String>,
    /// Setup or a run of this document's OCR model failed, when that is not
    /// the Settings model (whose state `ocr_state` shares with every tab).
    pub(crate) ocr_failure: Option<String>,
    /// What detection saw when the card appeared (ADR 0065).
    pub(crate) recognition_signals: Option<recognition::Signals>,
    /// Recognition languages picked on the card, for this document only.
    pub(crate) recognition_pick: Option<recognition::Choice>,
    /// Keyboard input source languages; tests inject their own.
    pub(crate) input_languages: fn() -> Vec<recognition::Language>,
    /// The model this tab is setting up, and whether it put the shared
    /// `ocr_state` into Installing (it must replace that state on finish).
    pub(crate) ocr_setup: Option<(settings::OcrModel, bool)>,
    pub(crate) markdown_search: Entity<markdown_search::SearchInput>,
    /// The Replace row's field; its text stays with the tab (ADR 0063).
    pub(crate) markdown_replace: Entity<markdown_search::SearchInput>,
    pub(crate) search: search_session::SearchSession,
    pub(crate) ocr_setup_subscription: Option<Subscription>,
    pub(crate) _subscription: Subscription,
    pub(crate) _markdown_search_subscription: Subscription,
}

/// The system file dialog could not open. On Linux GPUI uses the
/// xdg-desktop-portal file chooser, which minimal systems and WSL lack.
pub(crate) fn chooser_unavailable(error: &impl std::fmt::Display) -> String {
    let hint = if cfg!(target_os = "linux") {
        " Install xdg-desktop-portal with a backend such as xdg-desktop-portal-gtk, \
         or open files from the command line (mdoc path/to/file) or by dropping them."
    } else {
        ""
    };
    format!("The file dialog could not open: {error}.{hint}")
}

pub(crate) struct DocumentViewDependencies {
    pub(crate) owner: (u64, gpui::WeakEntity<workspace::Workspace>),
    pub(crate) preferences: settings::Shared,
    pub(crate) model_panel: Entity<settings_ui::Panel>,
    pub(crate) theme: Rc<Cell<Theme>>,
    pub(crate) import_busy: Arc<AtomicBool>,
    pub(crate) ocr: OcrState,
    pub(crate) title: style::TitleBar,
    /// Keyboard input source languages (`recognition::input_source_languages`).
    pub(crate) input_languages: fn() -> Vec<recognition::Language>,
}

/// The shared OCR state for the Settings model from what Settings already
/// knows, without starting a check (ADR 0066).
pub(crate) fn known_ocr_state(
    preferences: &settings::Shared,
    panel: &settings_ui::Panel,
) -> OcrState {
    let Ok(config) = preferences.borrow().snapshot().map(|p| p.ocr) else {
        return OcrState::Missing;
    };
    if let Some(mut installed) = panel
        .ocr_installations
        .iter()
        .find(|i| i.config.model == config.model)
        .cloned()
    {
        installed.config = config;
        return OcrState::Ready(installed);
    }
    match &panel.statuses[settings_ui::Panel::index(settings::Model::Ocr(config.model))] {
        settings_ui::Status::Damaged(e) | settings_ui::Status::Unavailable(e) => {
            OcrState::Failed(e.clone())
        }
        _ => OcrState::Missing,
    }
}

impl DocumentView {
    pub(crate) fn new(
        dependencies: DocumentViewDependencies,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown_search = cx.new(markdown_search::SearchInput::new);
        let markdown_replace = cx.new(|cx| {
            markdown_search::SearchInput::new(cx)
                .with_key_context("MarkdownReplace")
                .with_placeholder("Replace with")
        });
        let scroll = ScrollHandle::new();
        let editor = cx.new(|cx| {
            // Ghost text at the caret; the empty page centers the rest (ADR 0028).
            let mut editor = EditorState::new(window, cx).with_placeholder("Type here…");
            editor.set_markdown_style(style::markdown_style(dependencies.theme.get()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
            spelling::install_editor_hooks(&mut editor, dependencies.preferences.clone());
            editor.set_autoscroll(scroll.clone());
            editor
        });
        let images = images::install(&editor, Document::default().directory(), cx);
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Transaction(transaction) => this.pii_transaction(transaction, cx),
                EditorEvent::Changed => {
                    this.copy_feedback = None;
                    this.pii_edited(cx);
                    this.schedule_spellcheck(true, cx);
                    this.schedule_case_save(cx);
                    this.generated_unedited = false;
                    this.blank_disposable = false;
                    this.dirty_cached = this.dirty(cx);
                    this.refresh_markdown_search(SearchRefresh::DocumentEdit, false, window, cx);
                    cx.notify();
                }
                EditorEvent::ActivateAnnotations(ids) => {
                    this.choose_annotations(ids.clone(), window, cx)
                }
                EditorEvent::ActivateAnnotation(id) => this.activate_annotation(*id, window, cx),
                EditorEvent::MiddleClickAnnotation(id, modifiers) => {
                    this.middle_click_mention(*id, *modifiers, false, window, cx)
                }
                EditorEvent::SelectionChanged => this.sync_selection_action(cx),
                EditorEvent::DiagnosticAction(action, word) => {
                    this.spelling_action(*action, word.clone(), cx)
                }
                EditorEvent::SelectionAction => {
                    this.add_pii_candidate(&PiiAddCandidate, window, cx)
                }
                // A hovered replacement shows its tooltip (ADR 0048).
                EditorEvent::HoverAnnotation(_) => cx.notify(),
                EditorEvent::OpenLink(src) => {
                    if src.starts_with("https://")
                        || src.starts_with("http://")
                        || src.starts_with("mailto:")
                    {
                        cx.open_url(src);
                    } else if let Some(path) = document::local_path(src, &this.save_directory()) {
                        cx.emit(workspace::TabEvent::Open(vec![path]));
                    }
                }
            });
        let markdown_search_subscription = cx.subscribe_in(
            &markdown_search,
            window,
            |this, _, event, window, cx| match event {
                markdown_search::SearchInputEvent::Changed if this.search.open => {
                    this.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
                }
                _ => {}
            },
        );
        Self {
            focus: cx.focus_handle(),
            toolbar_region: cx.focus_handle(),
            preview_region: cx.focus_handle(),
            comments_region: cx.focus_handle(),
            more_focus: cx.focus_handle().tab_stop(true),
            more_menu_focus: cx.focus_handle(),
            more_bounds: Default::default(),
            panes_drawn: std::cell::Cell::new((true, false)),
            owner: dependencies.owner,
            active: true,
            dirty_cached: false,
            loading: false,
            load_generation: 0,
            unavailable: false,
            source_only: false,
            auto_convert_pending: false,
            automatic_import: false,
            generated_unedited: false,
            blank_disposable: false,
            file_drag: None,
            ocr_required: None,
            import_permit: None,
            conversion_source: None,
            import_busy: dependencies.import_busy,
            import_cancel: Arc::new(AtomicBool::new(false)),
            preferences: dependencies.preferences,
            model_panel: dependencies.model_panel,
            theme: dependencies.theme,
            title: dependencies.title,
            editor,
            images,
            session: document_session::DocumentSession::default(),
            pii: pii::ui::ReviewUi::new(cx),
            spelling: spelling::SpellState::default(),
            preview: preview::PreviewState::default(),
            scroll,
            error: None,
            copy_feedback: None,
            more_open: false,
            zoom: 1.,
            inspector_share: false,
            copied_for_ai: false,
            case: None,
            case_save: None,
            case_names: Vec::new(),
            case_names_open: false,
            case_term: None,
            clipboard_notice: None,
            compare: None,
            switcher: Default::default(),
            case_skip: Default::default(),
            view_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            split_dragging: false,
            original_selected: false,
            ocr_notice_dismissed: false,
            card_focus: cx.focus_handle(),
            focus_card: false,
            setup_error_dismissed: false,
            prompting: false,
            job: import_session::ImportSession::default(),
            ocr_state: dependencies.ocr,
            ocr_error: None,
            ocr_failure: None,
            recognition_signals: None,
            recognition_pick: None,
            input_languages: dependencies.input_languages,
            ocr_setup: None,
            markdown_search,
            markdown_replace,
            search: search_session::SearchSession::default(),
            ocr_setup_subscription: None,
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(workspace::TabEvent::ToggleTheme);
    }

    pub(crate) fn replace_images(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.images.release(window, cx);
        self.images = images::install(&self.editor, directory, cx);
    }

    pub(crate) fn dirty(&self, cx: &App) -> bool {
        if self.loading || self.unavailable || self.source_only || self.generated_unedited {
            return false;
        }
        self.session.dirty(self.editor.read(cx).text())
            || (self.session.document.path.is_none() && self.preview.attachment.is_some())
    }

    /// A Markdown tab with no content and no source shows the empty page (ADR 0028).
    fn shows_empty_page(&self, cx: &App) -> bool {
        !self.loading
            && !self.unavailable
            && !self.source_only
            && self.session.source.is_none()
            && self.preview.source.is_none()
            && self.preview.attachment.is_none()
            && self.editor.read(cx).text().is_empty()
    }

    pub(crate) fn can_copy_markdown(&self) -> bool {
        !self.loading && !self.unavailable && !self.source_only
    }

    pub(crate) fn copy_markdown(
        &mut self,
        _: &CopyMarkdown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            self.editor.read(cx).text().to_owned(),
        ));
        self.copied_for_ai = false;
        self.show_copy_feedback(cx);
    }
    pub(crate) fn show_copy_feedback(&mut self, cx: &mut Context<Self>) {
        // Replacing the task restarts feedback; typing and document transitions cancel it.
        self.copy_feedback = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copy_feedback = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Replacements are applied or the review is open in this tab.
    pub(crate) fn pseudonymized(&self) -> bool {
        self.pii.reviewing || !self.pii.review.applied().is_empty()
    }
    /// The Markdown came from OCR.
    pub(crate) fn recognized(&self) -> bool {
        self.session.ocr_configuration.is_some()
    }

    /// The OCR model chosen in Settings, whose readiness `ocr_state` shares.
    fn settings_ocr_model(&self) -> Option<settings::OcrModel> {
        self.preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| p.ocr.model)
    }

    /// This document's recognition languages: the card's pick, else the
    /// detected default with its reason (ADR 0065).
    pub(crate) fn recognition_choice(&self) -> recognition::Choice {
        if let Some(pick) = &self.recognition_pick {
            return pick.clone();
        }
        self.recognition_default()
    }

    fn recognition_default(&self) -> recognition::Choice {
        let fallback = self.settings_ocr_model().unwrap_or_default();
        recognition::detect(
            &self.recognition_signals.clone().unwrap_or_default(),
            &settings::OcrModel::offered(),
            fallback.languages(),
        )
    }

    /// The bundle that reads this document's recognition languages.
    pub(crate) fn recognition_model(&self) -> settings::OcrModel {
        settings::OcrModel::plan(&self.recognition_choice().languages).0
    }

    /// Gather detection signals for `path` once its card appears.
    fn observe_recognition(&mut self, path: &std::path::Path, text: Vec<recognition::Language>) {
        self.recognition_signals = Some(recognition::Signals {
            text,
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            input_sources: (self.input_languages)(),
        });
    }

    /// Pick recognition languages on the card. The pick holds until the
    /// document converts; picking the detected list again keeps its
    /// explanation, frozen so a later Settings change cannot move it.
    pub(crate) fn pick_recognition(
        &mut self,
        languages: Vec<recognition::Language>,
        cx: &mut Context<Self>,
    ) {
        if self.job.busy() || self.ocr_state.busy() {
            return;
        }
        let detected = self.recognition_default();
        self.recognition_pick = Some(if detected.languages == languages {
            detected
        } else {
            recognition::Choice::picked(languages)
        });
        self.ocr_failure = None;
        cx.notify();
    }

    /// The shared state for the Settings model from what Settings already
    /// knows. Never starts a check: that would take the model slot from a
    /// run about to resume.
    fn known_settings_ocr_state(&self, cx: &App) -> OcrState {
        known_ocr_state(&self.preferences, self.model_panel.read(cx))
    }

    /// A checked installation of `model`: the shared one for the Settings
    /// model, else one Settings checked or set up. None after a failure of
    /// that model, so Retry verifies the installation again.
    fn installed_for(&self, model: settings::OcrModel, cx: &App) -> Option<ocr::Installed> {
        if self.settings_ocr_model() == Some(model) {
            match &self.ocr_state {
                OcrState::Ready(installed) if installed.config.model == model => {
                    return Some(installed.clone());
                }
                OcrState::Failed(_) => return None,
                _ => {}
            }
        } else if self.ocr_failure.is_some() {
            return None;
        }
        self.model_panel
            .read(cx)
            .ocr_installations
            .iter()
            .find(|i| i.config.model == model)
            .cloned()
    }

    /// Zoom the Markdown text between 50% and 200% in 5% steps.
    pub(crate) fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let zoom = ((zoom * 20.).round() / 20.).clamp(0.5, 2.);
        // Show the pill so keyboard zoom reports its level.
        self.wake_switcher(cx);
        if zoom != self.zoom {
            self.zoom = zoom;
            cx.notify();
        }
    }

    pub(crate) fn display_name(&self) -> String {
        if self.source_only {
            self.preview
                .source
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Source".into())
        } else {
            self.session.display_name()
        }
    }

    pub(crate) fn open_source(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.source_only = true;
        self.preview.source = Some(path.clone());
        self.preview.attachment = Some(path.clone());
        self.preview.visible = true;
        self.open_path(path, window, cx);
        if self.active {
            window.focus(&self.focus, cx);
        }
    }

    /// The window title while this tab shows; the workspace sets it.
    pub(crate) fn window_title(&self) -> String {
        format!(
            "{}{} — mdoc",
            if self.dirty_cached { "• " } else { "" },
            self.display_name()
        )
    }

    pub(crate) fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        if !self.dirty(cx) {
            self.proceed(next, window, cx);
            return;
        }
        self.prompting = true;
        // Save is the default, Discard ⌘D / Alt+D, Cancel Esc (ADR 0064).
        let question = format!("Save changes to “{}”?", self.display_name());
        let answer = window.prompt(
            PromptLevel::Warning,
            &question,
            Some("Unsaved changes will be lost if you discard them."),
            &[
                gpui::PromptButton::new("Save"),
                gpui::PromptButton::new("Discard"),
                gpui::PromptButton::cancel("Cancel"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                match answer {
                    Some(0) => this.save(false, Some(next), window, cx),
                    Some(1) => this.proceed(next, window, cx),
                    _ => {
                        cx.emit(workspace::TabEvent::CloseCancelled);
                    }
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_feedback = None;
        match next {
            Next::Close => {
                cx.emit(workspace::TabEvent::CloseResolved);
                return;
            }
            Next::CreateCase { close } => {
                cx.emit(workspace::TabEvent::CreateCase { close });
                return;
            }
            Next::Import(imported) => {
                self.source_only = false;
                self.generated_unedited = self.automatic_import;
                self.ocr_required = None;
                self.ocr_error = None;
                self.ocr_failure = None;
                self.recognition_signals = None;
                self.recognition_pick = None;
                let retain_preview = self.preview.source.as_ref() == Some(&imported.source)
                    && (self.preview.pdf.is_some() || self.preview.loading);
                self.session
                    .import(imported.source.clone(), imported.warning);
                self.session.ocr_configuration = imported.ocr_configuration;
                self.reset_pii(cx);
                if !retain_preview {
                    self.close_preview(window, cx);
                }
                self.replace_images(self.save_directory(), window, cx);
                self.editor
                    .update(cx, |editor, cx| editor.set_text(imported.markdown, cx));
                self.dirty_cached = self.dirty(cx);
                self.reset_markdown_search(cx);
                if self.active {
                    window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                }
                self.error = None;
                if !retain_preview {
                    self.preview.message = None;
                }
                if retain_preview {
                    self.preview.visible = true;
                } else if imported.is_pdf {
                    self.open_pdf(imported.source, window, cx);
                } else if imported.is_docx {
                    self.open_docx(imported.source, window, cx);
                } else {
                    self.preview.attachment = Some(imported.source.clone());
                    self.preview.source = Some(imported.source);
                    self.preview.message =
                        Some("Source preview unavailable for this imported format.".into());
                }
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
            }
        }
        cx.notify();
    }

    pub(crate) fn import(&mut self, _: &Import, window: &mut Window, cx: &mut Context<Self>) {
        if !self.source_only
            || self.prompting
            || self.job.busy()
            || self.ocr_state.busy()
            || self.import_busy.load(Ordering::Relaxed)
        {
            return;
        }
        self.automatic_import = false;
        self.auto_convert_pending = false;
        if let Some(path) = self.preview.source.clone() {
            self.start_import(path, window, cx);
        }
    }

    pub(crate) fn start_import(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_import_run(path, false, false, window, cx);
    }

    fn start_import_mode(
        &mut self,
        path: PathBuf,
        skip_ocr: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_import_run(path, skip_ocr, !skip_ocr, window, cx);
    }

    fn start_import_run(
        &mut self,
        path: PathBuf,
        skip_ocr: bool,
        allow_ocr: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_import_with(path, skip_ocr, allow_ocr, None, window, cx);
    }

    /// `ready` is the installation a finished setup checked, with the
    /// configuration captured when it started.
    fn start_import_with(
        &mut self,
        path: PathBuf,
        skip_ocr: bool,
        allow_ocr: bool,
        ready: Option<ocr::Installed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if allow_ocr && self.ocr_state.busy() {
            return;
        }
        // Settings' resolution and confidence, this document's model.
        let config = if allow_ocr && !skip_ocr {
            let snapshot = self.preferences.borrow().snapshot();
            match snapshot {
                Ok(p) => Some(settings::OcrConfig {
                    model: self.recognition_model(),
                    ..p.ocr
                }),
                Err(e) => {
                    self.error = Some(e);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let installed = match ready {
            Some(ready) if allow_ocr && !skip_ocr => Some(ready),
            _ => config.and_then(|config| {
                self.installed_for(config.model, cx).map(|mut installed| {
                    installed.config = config;
                    installed
                })
            }),
        };
        if allow_ocr && installed.is_none() {
            self.error = Some(
                "Selected OCR model is not ready. Open Settings to download or repair it.".into(),
            );
            cx.notify();
            return;
        }
        if self.job.busy() {
            return;
        }
        if self.import_permit.is_none() {
            self.import_permit = import_session::ImportPermit::acquire(&self.import_busy);
        }
        let Some(permit) = self.import_permit.clone() else {
            return;
        };
        if !self.job.begin(installed.is_some()) {
            return;
        }
        self.conversion_source = Some(path.clone());
        cx.emit(workspace::TabEvent::ImportState);
        let cancel = self.import_cancel.clone();
        let owner = self.owner.clone();
        self.error = None;
        self.ocr_error = None;
        let generation = self.session.generation;
        let task = cx.background_executor().spawn(async move {
            let result = import::prepare_cancellable(&path, installed.as_ref(), skip_ocr, &cancel);
            drop(permit);
            result
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.job.complete(generation, result);
                this.resume_import(window, cx);
            });
            let (_, owner) = owner;
            let _ = owner.update_in(cx, |owner, window, cx| owner.refresh_import(window, cx));
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .owner
            .1
            .upgrade()
            .is_some_and(|owner| owner.read(cx).resolving_close())
        {
            return;
        }
        let generation = self.session.generation;
        let Some(completion) = self.job.take_ready(generation, self.prompting) else {
            return;
        };
        if let import_session::ImportCompletion::Ready(result) = completion {
            match result {
                Ok(imported) => {
                    self.request(Next::Import(imported), window, cx);
                }
                Err(import::ImportError::OcrFailed(error)) => {
                    if self.settings_ocr_model() == Some(self.recognition_model()) {
                        self.ocr_state = OcrState::Failed(error.clone());
                        cx.emit(workspace::TabEvent::OcrState(self.ocr_state.clone()));
                    } else {
                        self.ocr_failure = Some(error.clone());
                    }
                    self.error = Some(error);
                    if let Some(path) = self.conversion_source.clone() {
                        self.await_ocr(path, cx);
                    }
                }
                Err(import::ImportError::OcrNoText(error)) => {
                    self.ocr_error = Some(error.clone());
                    self.error = Some(error);
                    if let Some(path) = self.conversion_source.clone() {
                        self.await_ocr(path, cx);
                    }
                }
                Err(import::ImportError::Message(error)) => self.error = Some(error),
                Err(import::ImportError::NeedsOcr(path, pages, text)) => {
                    self.observe_recognition(&path, text);
                    self.ocr_required = Some(pages);
                    self.await_ocr(path, cx)
                }
            }
        }
        if !self.job.busy() {
            self.import_permit = None;
            cx.emit(workspace::TabEvent::ImportState);
        }
        // A stale completion also clears the busy indicator.
        cx.notify();
    }

    fn setup_ocr(&mut self, _: &SetupOcr, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting
            || self.job.busy()
            || self.ocr_state.busy()
            || self.import_busy.load(Ordering::Relaxed)
            || !ocr::SUPPORTED
        {
            return;
        }
        if self.installed_for(self.recognition_model(), cx).is_some() {
            return;
        }
        self.begin_ocr_setup(window, cx);
    }

    fn begin_ocr_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ocr_state.busy() {
            return;
        }
        let snapshot = self.preferences.borrow().snapshot();
        let config = match snapshot {
            Ok(p) => settings::OcrConfig {
                model: self.recognition_model(),
                ..p.ocr
            },
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        let shared = self.settings_ocr_model() == Some(config.model);
        self.ocr_failure = None;
        self.ocr_setup = None;
        let panel = self.model_panel.clone();
        // The inline consent continuation remains document-owned. Settings downloads never create one.
        if self.job.has_ocr_continuation() {
            let config = config.clone();
            let subscription =
                cx.subscribe_in(&panel, window, move |this, _, event, window, cx| {
                    if let settings_ui::Event::Finished(settings::Model::Ocr(model), result) = event
                        && *model == config.model
                        && this.job.has_ocr_continuation()
                    {
                        match result {
                            Ok(Some(installed)) => {
                                let mut installed = installed.clone();
                                installed.config = config.clone();
                                this.finish_ocr_setup(Ok(installed), window, cx);
                            }
                            Ok(None) => {}
                            Err(e) => this.finish_ocr_setup(Err(e.clone()), window, cx),
                        }
                    }
                });
            self.ocr_setup_subscription = Some(subscription);
        }
        let started = panel.update(cx, |panel, cx| {
            panel.setup(settings::Model::Ocr(config.model), false, cx);
            panel.working == Some(settings::Model::Ocr(config.model))
        });
        if started {
            self.ocr_setup = Some((config.model, shared));
            // Another model's setup shows only in this card's progress.
            if shared {
                self.ocr_state = OcrState::Installing;
                cx.emit(workspace::TabEvent::OcrState(self.ocr_state.clone()));
            }
            cx.notify();
        } else if self.job.has_ocr_continuation() {
            self.finish_ocr_setup(
                Err("Another model job is running. Retry when it finishes.".into()),
                window,
                cx,
            );
        }
    }

    pub(crate) fn finish_ocr_setup(
        &mut self,
        result: Result<ocr::Installed, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (model, claimed) = self.ocr_setup.take().unwrap_or_else(|| {
            let model = match &result {
                Ok(installed) => installed.config.model,
                Err(_) => self.recognition_model(),
            };
            (model, self.settings_ocr_model() == Some(model))
        });
        // Only the Settings model's outcome is shared with every tab; the
        // Settings panel keeps any other model's installation. Settings may
        // have changed during setup: a tab that set the shared state to
        // Installing replaces it either way.
        let shared = self.settings_ocr_model() == Some(model);
        match &result {
            Ok(installed) if shared => self.ocr_state = OcrState::Ready(installed.clone()),
            Err(error) if shared => self.ocr_state = OcrState::Failed(error.clone()),
            Err(error) => self.ocr_failure = Some(error.clone()),
            Ok(_) => {}
        }
        if claimed && !shared {
            self.ocr_state = self.known_settings_ocr_state(cx);
        }
        match result {
            Ok(installed) => {
                if let Some(path) = self.job.resume_after_setup(self.session.generation) {
                    self.start_import_with(path, false, true, Some(installed), window, cx);
                }
            }
            Err(_) => {
                self.job.finish();
                if let Some(path) = self.job.resume_after_setup(self.session.generation) {
                    self.await_ocr(path, cx);
                }
            }
        }
        if !self.job.busy() {
            self.import_permit = None;
        }
        if shared || claimed {
            cx.emit(workspace::TabEvent::OcrState(self.ocr_state.clone()));
        }
        cx.emit(workspace::TabEvent::ImportState);
        cx.notify();
    }

    // Consent lives in the source pane. Waiting for a decision must not hold
    // the global conversion slot or interrupt another tab with a modal dialog.
    fn await_ocr(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.recognition_signals.is_none() {
            self.observe_recognition(&path, Vec::new());
        }
        self.conversion_source = Some(path);
        self.ocr_required.get_or_insert_with(Vec::new);
        self.ocr_notice_dismissed = false;
        self.focus_card = true;
        self.job.finish();
        self.import_permit = None;
        cx.emit(workspace::TabEvent::ImportState);
        cx.notify();
    }

    pub(crate) fn ocr_action(&mut self, skip: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.job.busy()
            || self.import_busy.load(Ordering::Relaxed)
            || self.ocr_state.busy()
            || self.ocr_required.is_none()
        {
            return;
        }
        let Some(path) = self.conversion_source.clone() else {
            return;
        };
        self.automatic_import = false;
        if !skip && self.recognition_pick.is_none() {
            // Running OCR keeps this choice: a Settings change during
            // setup or recognition no longer moves it.
            self.recognition_pick = Some(self.recognition_default());
        }
        if skip {
            self.start_import_mode(path, skip, window, cx);
        } else if self.installed_for(self.recognition_model(), cx).is_some() {
            self.start_import_mode(path, false, window, cx);
        } else if ocr::SUPPORTED {
            if model_work::busy() || self.model_panel.read(cx).working.is_some() {
                self.error = Some("Another model job is running. Retry when it finishes.".into());
                cx.notify();
                return;
            }
            self.import_permit = import_session::ImportPermit::acquire(&self.import_busy);
            if self.import_permit.is_none() {
                return;
            }
            self.job.defer_for_setup(self.session.generation, path);
            self.begin_ocr_setup(window, cx);
        }
    }

    pub(crate) fn try_auto_convert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.auto_convert_pending
            || self.job.busy()
            || self.import_busy.load(Ordering::Relaxed)
            || self.import_cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let Some(path) = self.preview.source.clone() else {
            return;
        };
        self.automatic_import = true;
        self.auto_convert_pending = false;
        self.start_import(path, window, cx);
    }

    /// The Markdown pane before conversion: one card that changes in place
    /// through conversion, OCR consent, setup and recognition (ADR 0026).
    fn conversion_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let choice = self.recognition_choice();
        let (ocr_model, unreadable) = settings::OcrModel::plan(&choice.languages);
        let model = Some(settings::Model::Ocr(ocr_model));
        let shared = self.settings_ocr_model() == Some(ocr_model);
        let ready = self.installed_for(ocr_model, cx).is_some();
        let panel = self.model_panel.read(cx);
        let progress = model.and_then(|m| panel.progress_of(m));
        let cancelling = panel.cancelling();
        let pending = model
            .map(|m| panel.pending[settings_ui::Panel::index(m)])
            .unwrap_or_default();
        let installing =
            progress.is_some() || (shared && matches!(self.ocr_state, OcrState::Installing));
        let card = ui::state_card("conversion-card", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "conversion-card".into())
            })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .text_ellipsis()
                    .child(self.display_name()),
            );
        let Some(pages) = self.ocr_required.clone() else {
            let (title, busy) = if self.job.busy() {
                (
                    if self.job.recognizing() {
                        "Recognizing text…"
                    } else {
                        "Converting to Markdown…"
                    },
                    true,
                )
            } else if self.auto_convert_pending {
                ("Waiting to convert…", true)
            } else if self.error.is_some() {
                ("Conversion could not be completed", false)
            } else {
                ("Convert this document to begin editing", false)
            };
            return card
                .child(if busy {
                    ui::activity("conversion-activity", title, theme)
                        .text_size(px(14.))
                        .into_any_element()
                } else {
                    ui::card_title(title).into_any_element()
                })
                .when(self.error.is_some() && !busy, |v| {
                    v.child(ui::card_text(
                        "The notice below explains what went wrong.",
                        theme,
                    ))
                })
                .into_any_element();
        };
        let count = pages.len();
        let card = card
            .child(ui::card_title(if count == 0 { "Text recognition needs your attention" } else { "Some pages need text recognition" }))
            .when(count > 0, |v| v.child(ui::card_text(format!(
                "{count} {} no usable text ({}). Recognition reads {} on this computer; the original stays in the preview.",
                if count == 1 { "page has" } else { "pages have" },
                ui::page_ranges(&pages, 8),
                if count == 1 { "it" } else { "them" },
            ), theme)));
        if !ocr::SUPPORTED {
            return card
                .child(ui::card_text(
                    "Local OCR is unavailable on this platform.",
                    theme,
                ))
                .child(
                    ui::card_actions().child(
                        ui::primary_button("extract-native", "Use native text only", theme, true)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ExtractNative), cx)
                            }),
                    ),
                )
                .into_any_element();
        }
        if self.job.recognizing() {
            return card
                .child(ui::activity(
                    "recognition-activity",
                    "Recognizing text…",
                    theme,
                ))
                .into_any_element();
        }
        if installing {
            return card
                .child(match &progress {
                    Some(state) => {
                        ui::setup_progress("ocr-setup-progress", state, cancelling, theme)
                            .into_any_element()
                    }
                    None => {
                        ui::activity("ocr-setup-progress", "Preparing…", theme).into_any_element()
                    }
                })
                .child(
                    ui::card_actions().child(
                        settings_ui::control(
                            "cancel-ocr-setup",
                            "Cancel",
                            theme,
                            progress.is_some() && !cancelling,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "cancel-ocr-setup".into())
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model_panel
                                .update(cx, |panel, cx| panel.cancel_setup(cx))
                        })),
                    ),
                )
                .into_any_element();
        }
        let disabled =
            self.job.busy() || self.import_busy.load(Ordering::Relaxed) || self.ocr_state.busy();
        let failed = match &self.ocr_state {
            OcrState::Failed(error) if shared => Some(error.clone()),
            _ => self.ocr_failure.clone().or_else(|| self.ocr_error.clone()),
        };
        let primary = if ready && self.ocr_error.is_none() {
            "Run OCR"
        } else if failed.is_some() {
            "Retry"
        } else if pending.total() == 0 {
            // Installed but not yet checked: setup only verifies it.
            "Run OCR"
        } else {
            "Download & recognize"
        };
        if std::mem::take(&mut self.focus_card)
            && (self.focus.contains_focused(window, cx) || window.focused(cx).is_none())
        {
            let focus = self.card_focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        let name = model
            .map(|m| m.short_name())
            .unwrap_or_else(|| "OCR".into());
        // The engine says what it cannot read (ADR 0065).
        let hint = if unreadable.is_empty() {
            choice.reason.clone()
        } else {
            format!(
                "{}. {} OCR cannot read {}.",
                choice.reason,
                ocr_model.title(),
                recognition::label(&unreadable)
            )
        };
        let accent = theme.search_accent();
        let picker = div()
            .id("recognition-languages")
            .role(gpui::Role::RadioGroup)
            .aria_label("Recognition languages")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(
                div()
                    .mr_1()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child("Recognize"),
            )
            .children(settings::OcrModel::offered().into_iter().enumerate().map(
                |(index, languages)| {
                    let selected = choice.languages == languages;
                    let label = recognition::label(languages);
                    let selector = format!("recognize {label}");
                    div()
                        .id(("recognition-language", index))
                        .role(gpui::Role::RadioButton)
                        .aria_label(label.clone())
                        .aria_toggled(if selected {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .key_context("UiControl")
                        .tab_index(0)
                        .tab_stop(!disabled)
                        .px_2()
                        .py(px(2.))
                        .rounded_md()
                        .border_1()
                        .border_color(if selected { accent } else { palette.border })
                        .when(selected, |v| v.bg(gpui::Hsla { a: 0.08, ..accent }))
                        .text_size(px(12.))
                        .text_color(if selected { accent } else { palette.header_fg })
                        .focus_visible(move |s| s.bg(palette.placeholder_bg))
                        .when(!disabled, |v| v.cursor_pointer())
                        .when(disabled, |v| v.opacity(0.45))
                        .child(label)
                        .when(cfg!(test), move |v| v.debug_selector(move || selector))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !disabled {
                                this.pick_recognition(languages.to_vec(), cx);
                            }
                        }))
                },
            ));
        card.child(picker)
            .child(ui::card_hint(hint, theme).when(cfg!(test), |v| {
                v.debug_selector(|| "ocr-language-hint".into())
            }))
            .when_some(failed, |v, error| v.child(ui::card_error(error, theme)))
            .child(
                ui::card_actions()
                    .child(
                        ui::primary_button("run-ocr", primary, theme, !disabled)
                            .track_focus(&self.card_focus)
                            .when(cfg!(test), move |v| {
                                v.debug_selector(move || primary.into())
                            })
                            .on_click(move |_, window, cx| {
                                if !disabled {
                                    window.dispatch_action(Box::new(RunOcr), cx)
                                }
                            }),
                    )
                    .child(
                        settings_ui::control(
                            "extract-native",
                            "Use native text only",
                            theme,
                            !disabled,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "Use native text only".into())
                        })
                        .on_click(move |_, window, cx| {
                            if !disabled {
                                window.dispatch_action(Box::new(ExtractNative), cx)
                            }
                        }),
                    )
                    .child(
                        ui::link_button("choose-ocr-model", "Choose model…", theme, true)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "choose-ocr-model".into())
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(model) = model {
                                    this.model_panel.update(cx, |panel, cx| {
                                        panel.show_section(model, window, cx)
                                    });
                                }
                            })),
                    ),
            )
            .child(ui::card_note(
                if ready || pending.total() == 0 {
                    format!("{name} · runs on this computer")
                } else {
                    format!(
                        "{name} · {}, once · then runs on this computer",
                        pending.download_label()
                    )
                },
                theme,
            ))
            .into_any_element()
    }

    fn notice(
        &self,
        id: &'static str,
        title: &'static str,
        detail: String,
        failure: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let accent = if failure {
            style::markdown_style(theme).alert_caution
        } else {
            style::markdown_style(theme).alert_warning
        };
        div()
            .id(id)
            .flex()
            .flex_col()
            .flex_shrink_0()
            .text_size(px(12.))
            .bg(gpui::Hsla { a: 0.07, ..accent })
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        div()
                            .text_color(accent)
                            .child(if failure { "!" } else { "ⓘ" }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(palette.header_fg)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(palette.header_muted)
                            .child(detail),
                    )
                    .when(id == "preview-notice" && self.preview.retryable, |v| {
                        v.child(button("Retry", RetryPreview, theme))
                    })
                    .when(id == "ocr-notice", |v| {
                        v.child(ui::control("show-markdown", "Show", theme, true).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.original_selected = false;
                                cx.notify();
                            }),
                        ))
                    })
                    .child(
                        div()
                            .id((id, 1usize))
                            .role(gpui::Role::Button)
                            .focus_visible(|s| s.bg(palette.placeholder_bg))
                            .key_context("UiControl")
                            .tab_index(0)
                            .aria_label("Dismiss notice")
                            .flex_shrink_0()
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .hover(|v| v.bg(palette.placeholder_bg))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match id {
                                    "import-notice" => this.session.warning = None,
                                    "ocr-notice" => this.ocr_notice_dismissed = true,
                                    "preview-notice" => this.preview.message = None,
                                    "setup-notice" => this.setup_error_dismissed = true,
                                    "clipboard-notice" => this.clipboard_notice = None,
                                    _ => this.error = None,
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn save_directory(&self) -> PathBuf {
        self.session.directory()
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            self.open_pdf(path, window, cx);
            self.error = None;
            cx.notify();
        } else if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("docx"))
        {
            self.open_docx(path, window, cx);
            cx.notify();
        } else if document::is_markdown(&path) {
            cx.emit(workspace::TabEvent::Open(vec![path]));
        } else if self.source_only {
            let generation = self.begin_preview(path.clone());
            let task = cx.background_executor().spawn(async move {
                std::fs::File::open(path)
                    .and_then(|file| file.metadata())
                    .map(|_| ())
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.preview.generation != generation {
                        return;
                    }
                    this.preview.loading = false;
                    this.preview.retryable = result.is_err();
                    this.preview.message = Some(match result {
                        Ok(()) => "Preview unavailable for this format.".into(),
                        Err(error) => format!("Could not open source: {error}"),
                    });
                    cx.notify();
                });
            })
            .detach();
            cx.notify();
        } else {
            self.error = Some("Choose a Markdown or supported source document.".into());
            cx.notify();
        }
    }

    pub(crate) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        self.prompting = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open one or more files".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                match result {
                    Ok(Ok(Some(paths))) => cx.emit(workspace::TabEvent::Open(paths)),
                    Ok(Err(error)) => {
                        this.error = Some(chooser_unavailable(&error));
                        cx.notify();
                    }
                    _ => {}
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn save(
        &mut self,
        save_as: bool,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompting || self.loading || self.unavailable || self.source_only {
            cx.emit(workspace::TabEvent::CloseCancelled);
            return;
        }
        if !save_as && let Some(path) = self.session.document.path.clone() {
            self.write(path, next, window, cx);
            return;
        }
        self.prompting = true;
        let name = self.session.suggested_name();
        let path = cx.prompt_for_new_path(&self.save_directory(), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(mut path))) = result {
                    if path.extension().is_none() {
                        path.set_extension("md");
                    }
                    if !document::is_markdown(&path) {
                        this.error = Some(
                            "Save Markdown with a .md, .markdown, .mdown, or .txt extension."
                                .into(),
                        );
                        cx.emit(workspace::TabEvent::CloseCancelled);
                        cx.notify();
                        this.resume_import(window, cx);
                        return;
                    }
                    this.write(path, next, window, cx);
                } else {
                    if let Ok(Err(error)) = &result {
                        this.error = Some(chooser_unavailable(error));
                        cx.notify();
                    }
                    cx.emit(workspace::TabEvent::CloseCancelled);
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn write(
        &mut self,
        path: PathBuf,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = session_store::identity(&path);
        let (id, owner) = &self.owner;
        let other = owner
            .upgrade()
            .and_then(|owner| owner.read(cx).find_path(&path, Some(*id), cx));
        if let Some(other) = other {
            cx.emit(workspace::TabEvent::SaveConflict(other));
            cx.emit(workspace::TabEvent::CloseCancelled);
            return;
        }
        let old_directory = self.save_directory();
        match self.session.save(path, self.editor.read(cx).text()) {
            Ok(()) => {
                self.generated_unedited = false;
                self.blank_disposable = false;
                self.dirty_cached = false;
                cx.emit(workspace::TabEvent::Saved);
                if old_directory != self.session.document.directory() {
                    self.replace_images(self.session.document.directory(), window, cx);
                }
                self.error = None;
                if let Some(next) = next {
                    self.proceed(next, window, cx);
                }
            }
            Err(error) => {
                self.error = Some(format!("Could not save: {error}"));
                cx.emit(workspace::TabEvent::CloseCancelled);
            }
        }
        cx.notify();
    }
}

pub(crate) fn button(
    label: &'static str,
    action: impl gpui::Action,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    ui::control(label, label, theme, true)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

/// A find-bar control: a focusable button named for assistive technology,
/// whose tooltip shows its shortcut.
fn search_bar_button(
    id: &'static str,
    content: impl IntoElement,
    label: &'static str,
    shortcut: &'static str,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let palette = theme.pdf_style();
    div()
        .id(id)
        .role(gpui::Role::Button)
        .key_context("UiControl")
        .tab_index(0)
        .focus_visible(|s| s.bg(palette.placeholder_bg))
        .aria_label(label)
        .aria_keyshortcuts(shortcut)
        .tooltip(style::tooltip(format!("{label}  {shortcut}"), theme))
        .flex_shrink_0()
        .rounded_md()
        .px_2()
        .h(px(28.))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_color(palette.header_muted)
        .hover(|view| view.bg(palette.placeholder_bg))
        .child(content)
}

/// The macOS or the Windows/Linux spelling of a shortcut.
const fn chord(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

fn toggled(on: bool) -> gpui::Toggled {
    if on {
        gpui::Toggled::True
    } else {
        gpui::Toggled::False
    }
}

pub(crate) fn bind_markdown_search_keys(cx: &mut App) {
    let mac = cfg!(target_os = "macos");
    let modifier = if mac { "cmd" } else { "ctrl" };
    let find = &format!("{modifier}-f");
    let next = &format!("{modifier}-g");
    let previous = &format!("{modifier}-shift-g");
    // Find and Replace keys follow Zed's defaults (ADR 0063).
    let replace = if mac { "cmd-alt-f" } else { "ctrl-h" };
    let replace_all = if mac { "cmd-enter" } else { "ctrl-alt-enter" };
    let match_case = if mac { "cmd-alt-c" } else { "alt-c" };
    let whole_word = if mac { "cmd-alt-w" } else { "alt-w" };
    let fields = ["MarkdownSearch", "MarkdownReplace"];
    let mut bindings = vec![
        KeyBinding::new(find, FindMarkdown, Some("Editor")),
        KeyBinding::new(find, FindMarkdown, None),
        KeyBinding::new(replace, FindReplaceMarkdown, Some("Editor")),
        KeyBinding::new(replace, FindReplaceMarkdown, None),
        KeyBinding::new(next, FindNextMarkdown, Some("Editor")),
        KeyBinding::new(previous, FindPreviousMarkdown, Some("Editor")),
        KeyBinding::new("enter", FindNextMarkdown, Some("MarkdownSearch")),
        KeyBinding::new("shift-enter", FindPreviousMarkdown, Some("MarkdownSearch")),
        KeyBinding::new("enter", ReplaceNextMarkdown, Some("MarkdownReplace")),
    ];
    for field in fields {
        bindings.extend([
            KeyBinding::new(find, FindMarkdown, Some(field)),
            KeyBinding::new(replace, FindReplaceMarkdown, Some(field)),
            KeyBinding::new(next, FindNextMarkdown, Some(field)),
            KeyBinding::new(previous, FindPreviousMarkdown, Some(field)),
            KeyBinding::new(replace_all, ReplaceAllMarkdown, Some(field)),
            KeyBinding::new(match_case, ToggleMarkdownMatchCase, Some(field)),
            KeyBinding::new(whole_word, ToggleMarkdownWholeWord, Some(field)),
            KeyBinding::new("escape", CloseMarkdownSearch, Some(field)),
            // Undo in the find bar undoes the document, Replace all included.
            KeyBinding::new(&format!("{modifier}-z"), mdoc_editor::Undo, Some(field)),
            KeyBinding::new(
                &format!("{modifier}-shift-z"),
                mdoc_editor::Redo,
                Some(field),
            ),
        ]);
        if !mac {
            bindings.push(KeyBinding::new("ctrl-y", mdoc_editor::Redo, Some(field)));
        }
    }
    cx.bind_keys(bindings);
}

/// One zoom step (ADR 0048).
pub(crate) const ZOOM_STEP: f32 = 0.05;
/// The widest the Markdown reading column gets at 100% (ADR 0048).
const READING_WIDTH: f32 = 760.;

impl DocumentView {
    /// The document's F6 regions in order, each with whether it is a single
    /// focus target (the editor) rather than a group of controls (ADR 0062).
    pub(crate) fn focus_regions(&self, cx: &App) -> Vec<(gpui::FocusHandle, bool)> {
        let (markdown, original) = self.panes_drawn.get();
        let mut regions = vec![(self.toolbar_region.clone(), false)];
        if markdown
            && !self.source_only
            && !self.loading
            && !self.unavailable
            && self.compare.is_none()
        {
            regions.push((self.editor.read(cx).focus_handle(cx), true));
        }
        if self.replacements_open() && self.can_copy_markdown() {
            regions.push((self.replacements_focus(), false));
        }
        if self.preview.visible && original {
            regions.push((self.preview_region.clone(), false));
            if self.preview.comment_panel.is_some() {
                regions.push((self.comments_region.clone(), false));
            }
        }
        regions
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The before/after view follows the applied replacements (ADR 0042).
        self.refresh_compare(window, cx);
        self.release_closed_popup_focus(window, cx);
        // Derived display state; set_outlines only notifies on change.
        self.sync_scope_outlines(cx);
        // Loads and Settings changes reach spellcheck here (no-op otherwise).
        self.ensure_spellcheck(cx);
        // A finished or abandoned OS drag leaves no feedback behind.
        if !cx.has_active_drag() {
            self.file_drag = None;
        }
        let empty_page = self.shows_empty_page(cx);
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let search_focused = self
            .markdown_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let query = self.markdown_search.read(cx).value().to_owned();
        let count = if query.is_empty() {
            "0 / 0".to_owned()
        } else if self.search.task.is_some() {
            "Searching…".to_owned()
        } else if self.search.matches.is_empty() {
            "0 matches".to_owned()
        } else {
            format!(
                "{} / {}",
                self.search.active.map_or(0, |index| index + 1),
                self.search.matches.len()
            )
        };
        let navigation_disabled = self.search.matches.is_empty();
        let replace_focused = self
            .markdown_replace
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let field = |id: &'static str, focused: bool| {
            div()
                .id(id)
                .flex()
                .flex_1()
                .min_w_0()
                .h(px(28.))
                .px_2()
                .items_center()
                .bg(palette.bg)
                .border_1()
                .border_color(if focused {
                    theme.search_accent()
                } else {
                    palette.border
                })
                .rounded_md()
        };
        let replace_toggle = search_bar_button(
            "markdown-search-replace-toggle",
            if self.search.replace_open {
                "⌄"
            } else {
                "›"
            },
            "Replace",
            chord("⌥⌘F", "Ctrl+H"),
            theme,
        )
        .w(px(20.))
        .aria_expanded(self.search.replace_open)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_markdown_replace(window, cx)));
        let match_case = search_bar_button(
            "markdown-search-match-case",
            "Aa",
            "Match case",
            chord("⌥⌘C", "Alt+C"),
            theme,
        )
        .aria_toggled(toggled(self.search.match_case))
        .when(self.search.match_case, |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_match_case(&ToggleMarkdownMatchCase, window, cx)
        }));
        let whole_word = search_bar_button(
            "markdown-search-whole-word",
            div().underline().child("ab"),
            "Whole word",
            chord("⌥⌘W", "Alt+W"),
            theme,
        )
        .aria_toggled(toggled(self.search.whole_word))
        .when(self.search.whole_word, |view| {
            view.bg(palette.placeholder_bg)
                .text_color(theme.search_accent())
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.toggle_markdown_whole_word(&ToggleMarkdownWholeWord, window, cx)
        }));
        let count = div()
            .id("markdown-search-count")
            .flex_shrink_0()
            .px_2()
            .text_size(px(12.))
            .text_color(palette.header_muted)
            .child(if self.search.task.is_some() {
                ui::activity("search-activity", count, theme).into_any_element()
            } else if let Some(status) = self.search.status.clone() {
                // Replace results are announced (ADR 0063).
                div()
                    .id("markdown-replace-status")
                    .role(gpui::Role::Status)
                    .aria_label(status.clone())
                    .child(status)
                    .into_any_element()
            } else {
                div().child(count).into_any_element()
            });
        let previous = search_bar_button(
            "markdown-search-previous",
            "↑",
            "Previous match",
            chord("⇧⌘G", "Shift+Ctrl+G"),
            theme,
        )
        .w(px(28.))
        .justify_center()
        .when(navigation_disabled, |view| {
            view.text_color(palette.header_muted)
        })
        .on_click(cx.listener(|this, _, window, cx| {
            this.find_previous_markdown(&FindPreviousMarkdown, window, cx)
        }));
        let next =
            search_bar_button(
                "markdown-search-next",
                "↓",
                "Next match",
                chord("⌘G", "Ctrl+G"),
                theme,
            )
            .w(px(28.))
            .justify_center()
            .when(navigation_disabled, |view| {
                view.text_color(palette.header_muted)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.find_next_markdown(&FindNextMarkdown, window, cx)
            }));
        let close = search_bar_button(
            "markdown-search-close",
            "×",
            "Close Markdown search",
            "Esc",
            theme,
        )
        .w(px(28.))
        .justify_center()
        .on_click(cx.listener(|this, _, window, cx| {
            this.close_markdown_search(&CloseMarkdownSearch, window, cx)
        }));
        let replace_row = self.search.replace_open.then(|| {
            div()
                .id("markdown-replace-row")
                .flex()
                .items_center()
                .gap_1()
                .pl(px(24.))
                .child(
                    field("markdown-replace-input", replace_focused)
                        .aria_label("Replace with")
                        .child(self.markdown_replace.clone()),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-next",
                        "Replace",
                        "Replace",
                        "Enter",
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_next_markdown(&ReplaceNextMarkdown, window, cx)
                    })),
                )
                .child(
                    search_bar_button(
                        "markdown-replace-all",
                        "Replace All",
                        "Replace all",
                        chord("⌘Enter", "Ctrl+Alt+Enter"),
                        theme,
                    )
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.replace_all_markdown(&ReplaceAllMarkdown, window, cx)
                    })),
                )
        });
        let search_bar = div()
            .id("markdown-search-bar")
            .on_action(cx.listener(|this, action: &mdoc_editor::Undo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.undo(action, window, cx))
            }))
            .on_action(cx.listener(|this, action: &mdoc_editor::Redo, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.redo(action, window, cx))
            }))
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .text_size(px(13.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(palette.border)
            .bg(palette.bg)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(replace_toggle)
                    .child(
                        field("markdown-search-input", search_focused)
                            .aria_label("Search Markdown")
                            .child(self.markdown_search.clone()),
                    )
                    .child(match_case)
                    .child(whole_word)
                    .child(count)
                    .child(previous)
                    .child(next)
                    .child(close),
            )
            .children(replace_row);
        let markdown_scrollbar = gpui_pdf::scrollbar::overlay_scrollbar(
            "markdown-scrollbar",
            &self.scroll,
            false,
            palette.header_muted,
            cx,
        );
        let import_notice = self
            .session
            .warning
            .clone()
            .map(|detail| self.notice("import-notice", "Review conversion", detail, false, cx));
        let ocr_notice = (self.ocr_required.is_some() && !self.ocr_notice_dismissed).then(|| {
            self.notice(
                "ocr-notice",
                "Text recognition required",
                "Some pages need text recognition.".into(),
                false,
                cx,
            )
        });
        let preview_notice = self.preview.message.clone().map(|detail| {
            self.notice(
                "preview-notice",
                "Preview",
                detail,
                self.preview.retryable,
                cx,
            )
        });
        let setup_notice = match &self.ocr_state {
            OcrState::Failed(error) if !self.setup_error_dismissed => {
                Some(self.notice("setup-notice", "OCR setup", error.clone(), true, cx))
            }
            _ => None,
        };
        let clipboard_notice = self
            .clipboard_notice
            .clone()
            .map(|detail| self.notice("clipboard-notice", "Clipboard", detail, false, cx));
        let error_notice = self
            .error
            .clone()
            .map(|detail| self.notice("error-notice", "Needs attention", detail, true, cx));
        let width = self.chrome_width(window);
        let content_width = (width - f32::from(self.replacements_panel_width(window))).max(1.);
        let narrow_preview = self.preview.visible && content_width < 620.;
        let show_original = self.preview.visible && (!narrow_preview || self.original_selected);
        let show_markdown = !narrow_preview || !self.original_selected;
        self.panes_drawn.set((show_markdown, show_original));
        let minimum = (280. / content_width.max(620.)).min(0.5);
        let split = self
            .preview
            .split_ratio
            .unwrap_or(0.5)
            .clamp(minimum, 1. - minimum);
        let measured_bounds = self.view_bounds.clone();
        let measured_owner = cx.entity().downgrade();
        let detached = self.title.detached.get();
        let body = div().relative().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .child(gpui::canvas(move |bounds, _, cx| { if measured_bounds.replace(bounds) != bounds { let _ = measured_owner.update(cx, |_, cx| cx.notify()); } }, |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                // Repaint so a replacement tooltip goes when the pointer leaves it.
                if this.editor.read(cx).hovered_annotation().is_some() { cx.notify(); }
                if event.pressed_button != Some(gpui::MouseButton::Left) { this.split_dragging = false; }
                if this.split_dragging {
                    let bounds = this.view_bounds.get();
                    let panel = this.replacements_panel_width(window);
                    let width = f32::from(bounds.size.width - panel).max(1.);
                    let ratio = f32::from(event.position.x - bounds.left() - panel) / width;
                    this.preview.split_ratio = Some(ratio.clamp(0.25, 0.75));
                    cx.notify();
                }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = false; cx.notify(); }))
            // While the empty page shows, the whole document view accepts OS file drops.
            .when(empty_page, |v| v
                .on_drag_move(cx.listener(|this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let over = event.bounds.contains(&event.event.position);
                    let state = over.then(|| event.drag(cx).paths().iter().any(|path| import::droppable(path)));
                    if this.file_drag != state { this.file_drag = state; cx.notify(); }
                }))
                .can_drop(|drag, _, _| drag.downcast_ref::<gpui::ExternalPaths>().is_some_and(|drag| drag.paths().iter().any(|path| import::droppable(path))))
                .on_drop(cx.listener(|this, drag: &gpui::ExternalPaths, window, cx| {
                    this.file_drag = None;
                    workspace::activate_for_drop(window, cx);
                    cx.emit(workspace::TabEvent::Open(drag.paths().to_vec()));
                    cx.notify();
                })))
            .when(!detached, |v| v.child(div().track_focus(&self.toolbar_region).key_context("ToolbarRegion").child(self.header(window, cx))))
            .when(!self.source_only && self.job.busy(), |v| v.child(
                ui::activity("document-work-activity", if matches!(self.ocr_state, OcrState::Installing) || self.job.has_ocr_continuation() { "Setting up text recognition…" } else if self.job.recognizing() { "Recognizing text…" } else { "Converting to Markdown…" }, theme)
                    .px_3().py_2().border_b_1().border_color(palette.border)))
            .when_some(self.session.ocr_configuration.clone(),|v,config|v.child(div().px_3().py_1().text_size(px(11.)).text_color(palette.header_muted).child(format!("OCR result: {} · {} DPI · minimum confidence {} · Force",config.model.name(),config.dpi,config.minimum_confidence))))
            .children(import_notice).children(clipboard_notice).children(self.restore_summary(cx)).children(ocr_notice.filter(|_| !show_markdown))
            .when(narrow_preview, |v| v.child(self.pane_switch(cx)))
            .child(div().flex().flex_1().min_h_0()
                .when(!self.source_only && show_markdown, |row| row.child(div().relative().flex().flex_1().min_w_0().h_full().flex().flex_col().text_size(px(16. * self.zoom))
                    .child({ let column = self.switcher.column.clone(); gpui::canvas(move |bounds, _, _| column.set(bounds), |_, _, _, _| {}).absolute().inset_0() })
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.switcher_pointer(event.position, cx)))
                    .when(self.search.open, |column| column.child(search_bar))
                    .child(if self.loading || self.unavailable {
                        div().p_6().child(if self.loading { ui::activity("markdown-loading", "Loading Markdown…", theme).into_any_element() } else { div().child("Markdown unavailable").into_any_element() })
                            .when(self.unavailable, |view| view.child(button("Retry", RetryDocument, theme))).into_any_element()
                    } else if let Some(compare) = self.compare_view(content_width, cx) {
                        compare
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        // Absolute, so the column's intrinsic sizing never measures the
                        // editor: each probe width (search-bar min-content, max content,
                        // 0 px) shaped it again, and a changing one missed every cache.
                        .child(div().id("document-scroll").absolute().inset_0().overflow_y_scroll().track_scroll(&self.scroll).p_6().pb(px(72.))
                            .on_scroll_wheel(cx.listener(|this, _, _, cx| { this.wake_switcher(cx); cx.notify(); }))
                            // A centred reading column (ADR 0048, from variant B).
                            .child(div().relative().w_full().max_w(px(READING_WIDTH * self.zoom)).mx_auto().child(self.editor.clone())
                                // The mention bar scrolls with its mention (ADR 0055).
                                .children(self.mention_popup_layer(window, cx))))
                        .child(markdown_scrollbar).children((empty_page && self.file_drag.is_some()).then(|| self.empty_page(cx))).into_any_element() })
                    .children(self.view_switcher(window, cx).filter(|_| !self.loading && !self.unavailable))))
                // Replacements sit beside the text they describe, before Original.
                .children(self.replacements_panel(window, cx))
                .when(self.source_only && show_markdown, |row| row.child(div().id("conversion-pane").flex_1().min_w_0().p_6().flex().items_center().justify_center().overflow_y_scroll()
                    .child(self.conversion_card(window, cx))))
                .when(self.preview.visible && !narrow_preview, |row| row.child(div().id("preview-divider").when(cfg!(test), |v| v.debug_selector(|| "preview-divider".into())).w(px(6.)).h_full().flex_shrink_0().cursor(gpui::CursorStyle::ResizeLeftRight).bg(palette.border)
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = true; cx.notify(); }))))
                .when(show_original && self.preview.pdf.is_none(), |row| row.child(div().track_focus(&self.preview_region).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).h_full().border_l_1().border_color(palette.border)
                    .flex().flex_col().items_center().justify_center().text_color(palette.header_muted)
                    .child(if self.preview.loading { ui::activity("preview-activity", "Preparing preview…", theme).into_any_element() } else { div().child(if self.preview.queued { "Waiting to prepare preview…" } else { "Preview unavailable" }).into_any_element() }).when_some(self.preview.message.clone(), |v, message| v.child(self.notice("preview-notice", "Preview", message, self.preview.retryable, cx)))))
                .when_some(self.preview.pdf.clone().filter(|_| show_original), |row, pdf| row.child(div().track_focus(&self.preview_region).when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).flex_shrink_0().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))
                    .when_some(self.preview.comment_panel.clone(), |pane, comments| pane.child(div().track_focus(&self.comments_region).child(comments)))
                    .children(preview_notice)))
)
            .children(setup_notice.filter(|_| !(show_markdown && self.source_only && self.ocr_required.is_some()))).children(error_notice)
            // The word popup anchors to the editor, hidden under before/after.
            .children(self.pii_popup(cx).filter(|_| self.compare.is_none()))
            .children(self.replacement_tip(window, cx).filter(|_| self.compare.is_none()));
        // Sidebar collapsed: the header joins the title row and the document
        // sits in its own card below it (ADR 0048).
        let content = if detached {
            div()
                .size_full()
                .flex()
                .flex_col()
                .text_color(palette.header_fg)
                .child(
                    div()
                        .track_focus(&self.toolbar_region)
                        .key_context("ToolbarRegion")
                        .child(self.header(window, cx)),
                )
                .child(
                    ui::card_surface(div().flex_1().min_h_0().ml(px(style::CARD_MARGIN)), theme)
                        .child(body),
                )
                .into_any_element()
        } else {
            body.into_any_element()
        };
        // Both header layouts share the document's focus and action scope.
        div()
            .size_full()
            .flex()
            .flex_col()
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| {
                if this.compare.is_none() {
                    this.toggle_preview(window, cx)
                }
            }))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(Self::copy_markdown))
            .on_action(cx.listener(Self::copy_for_ai))
            .on_action(cx.listener(Self::paste_ai_response))
            .on_action(cx.listener(|this, _: &crate::ToggleCompare, window, cx| {
                this.toggle_compare(window, cx)
            }))
            .on_action(cx.listener(Self::pseudonymize_clipboard))
            .on_action(cx.listener(Self::pseudonymize))
            .on_action(cx.listener(Self::add_pii_candidate))
            .on_action(cx.listener(Self::apply_all_pii))
            .on_action(cx.listener(|this, _: &PiiReviewCandidate, window, cx| {
                this.step_pii_highlight(false, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiNextCandidate, window, cx| {
                this.step_pii_highlight(false, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PiiPreviousCandidate, window, cx| {
                this.step_pii_highlight(true, false, window, cx)
            }))
            .on_action(cx.listener(Self::setup_ocr))
            .on_action(
                cx.listener(|this, _: &RunOcr, window, cx| this.ocr_action(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ExtractNative, window, cx| {
                    this.ocr_action(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| {
                this.session.warning = None;
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &New, _, cx| cx.emit(workspace::TabEvent::New)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(
                cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)),
            )
            .on_action(
                cx.listener(|_, _: &Close, _, cx| cx.emit(workspace::TabEvent::CloseRequested)),
            )
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| {
                this.toggle_preview(window, cx);
                if this.source_only {
                    window.focus(&this.focus, cx);
                } else {
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_replace_markdown))
            .on_action(cx.listener(Self::replace_next_markdown))
            .on_action(cx.listener(Self::replace_all_markdown))
            .on_action(cx.listener(Self::toggle_markdown_match_case))
            .on_action(cx.listener(Self::toggle_markdown_whole_word))
            .on_action(cx.listener(Self::next_misspelling))
            .on_action(cx.listener(Self::previous_misspelling))
            .on_action(cx.listener(Self::toggle_document_spelling))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .on_action(
                cx.listener(|this, _: &ZoomIn, _, cx| this.set_zoom(this.zoom + ZOOM_STEP, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ZoomOut, _, cx| this.set_zoom(this.zoom - ZOOM_STEP, cx)),
            )
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.set_zoom(1., cx)))
            .child(content)
    }
}
