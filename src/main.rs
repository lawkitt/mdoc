//! A file-based Markdown editor with side-by-side PDF and DOCX preview.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod comment_panel;
mod document;
mod document_session;
mod docx_preview;
mod images;
mod import;
mod import_session;
mod markdown_search;
mod search_session;
use search_session::SearchRefresh;
mod comparison;
mod comparison_ui;
mod model_download;
mod model_work;
mod ocr;
#[cfg(test)]
mod perf_tests;
mod pii;
mod preview;
mod session_store;
mod settings;
mod settings_ui;
mod style;
mod tabs;
mod ui;
#[cfg(test)]
mod ui_tests;
mod workspace_ui;

use document::Document;
use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyBinding, Menu, MenuItem, PathPromptOptions,
    PromptLevel, ScrollHandle, Subscription, Window, WindowBounds, WindowOptions, actions, div,
    prelude::*, px, size,
};
use gpui_pdf::PdfView;
use mdoc_editor::{EditorEvent, EditorState, SearchIndex, SearchMatch};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use style::Theme;

actions!(
    mdoc,
    [
        New,
        Open,
        Import,
        SetupOcr,
        Settings,
        RunOcr,
        ExtractNative,
        DismissImportWarning,
        Save,
        SaveAs,
        CopyMarkdown,
        Pseudonymize,
        PiiAddCandidate,
        PiiReviewCandidate,
        PiiNextCandidate,
        PiiPreviousCandidate,
        PiiConfirm,
        PiiApplyAll,
        PiiNextChoice,
        PiiPreviousChoice,
        PiiOpenChoice,
        PiiClosePopup,
        Close,
        ClosePdf,
        RetryPreview,
        ToggleTheme,
        ToggleSidebar,
        NextTab,
        PreviousTab,
        Quit,
        TogglePreview,
        RetryDocument,
        FindMarkdown,
        FindNextMarkdown,
        FindPreviousMarkdown,
        CloseMarkdownSearch,
        ToggleMarkdownMatchCase,
    ]
);

#[derive(Clone)]
enum Next {
    Import(import::Imported),
    Close,
}

#[derive(Clone, Debug)]
enum OcrState {
    Checking,
    Missing,
    Installing,
    Ready(ocr::Installed),
    Failed(String),
    Unsupported,
}

impl OcrState {
    fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

struct Workspace {
    focus: gpui::FocusHandle,
    owner: (u64, gpui::WeakEntity<tabs::Tabs>),
    active: bool,
    dirty_cached: bool,
    loading: bool,
    load_generation: u64,
    unavailable: bool,
    settings_focus: gpui::FocusHandle,
    source_only: bool,
    auto_convert_pending: bool,
    automatic_import: bool,
    generated_unedited: bool,
    blank_disposable: bool,
    /// External files over the empty page: whether any is supported (ADR 0028).
    file_drag: Option<bool>,
    ocr_required: Option<Vec<u32>>,
    import_permit: Option<Arc<import_session::ImportPermit>>,
    conversion_source: Option<PathBuf>,
    import_busy: Arc<AtomicBool>,
    import_cancel: Arc<AtomicBool>,
    preferences: settings::Shared,
    model_panel: Entity<settings_ui::Panel>,
    theme: Rc<Cell<Theme>>,
    editor: Entity<EditorState>,
    images: images::ImageCache,
    session: document_session::DocumentSession,
    pii: pii::ui::ReviewUi,
    preview: preview::PreviewState,
    scroll: ScrollHandle,
    error: Option<String>,
    copy_feedback: Option<gpui::Task<()>>,
    workspace_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    split_dragging: bool,
    original_selected: bool,
    ocr_notice_dismissed: bool,
    setup_error_dismissed: bool,
    /// The card's primary action takes focus once when OCR consent appears.
    card_focus: gpui::FocusHandle,
    focus_card: bool,
    prompting: bool,
    job: import_session::ImportSession,
    ocr_state: OcrState,
    markdown_search: Entity<markdown_search::SearchInput>,
    search: search_session::SearchSession,
    ocr_setup_subscription: Option<Subscription>,
    _subscription: Subscription,
    _markdown_search_subscription: Subscription,
}

struct WorkspaceDependencies {
    owner: (u64, gpui::WeakEntity<tabs::Tabs>),
    preferences: settings::Shared,
    model_panel: Entity<settings_ui::Panel>,
    theme: Rc<Cell<Theme>>,
    import_busy: Arc<AtomicBool>,
    ocr: OcrState,
}

impl Workspace {
    fn new(
        dependencies: WorkspaceDependencies,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown_search = cx.new(markdown_search::SearchInput::new);
        let editor = cx.new(|cx| {
            // Ghost text at the caret; the empty page centers the rest (ADR 0028).
            let mut editor = EditorState::new(window, cx).with_placeholder("Type here…");
            editor.set_markdown_style(style::markdown_style(dependencies.theme.get()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
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
                    this.generated_unedited = false;
                    this.blank_disposable = false;
                    this.dirty_cached = this.dirty(cx);
                    this.update_title(window, cx);
                    this.refresh_markdown_search(SearchRefresh::DocumentEdit, false, window, cx);
                    cx.notify();
                }
                EditorEvent::ActivateAnnotations(ids) => {
                    this.choose_annotations(ids.clone(), window, cx)
                }
                EditorEvent::ActivateAnnotation(id) => this.activate_annotation(*id, window, cx),
                EditorEvent::SelectionChanged => this.sync_selection_action(cx),
                EditorEvent::SelectionAction => {
                    this.add_pii_candidate(&PiiAddCandidate, window, cx)
                }
                EditorEvent::OpenLink(src) => {
                    if src.starts_with("https://")
                        || src.starts_with("http://")
                        || src.starts_with("mailto:")
                    {
                        cx.open_url(src);
                    } else if let Some(path) = document::local_path(src, &this.save_directory()) {
                        cx.emit(tabs::TabEvent::Open(vec![path]));
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
            owner: dependencies.owner,
            active: true,
            dirty_cached: false,
            loading: false,
            load_generation: 0,
            unavailable: false,
            settings_focus: cx.focus_handle(),
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
            editor,
            images,
            session: document_session::DocumentSession::default(),
            pii: pii::ui::ReviewUi::new(cx),
            preview: preview::PreviewState::default(),
            scroll: ScrollHandle::new(),
            error: None,
            copy_feedback: None,
            workspace_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            split_dragging: false,
            original_selected: false,
            ocr_notice_dismissed: false,
            card_focus: cx.focus_handle(),
            focus_card: false,
            setup_error_dismissed: false,
            prompting: false,
            job: import_session::ImportSession::default(),
            ocr_state: dependencies.ocr,
            markdown_search,
            search: search_session::SearchSession::default(),
            ocr_setup_subscription: None,
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(tabs::TabEvent::ToggleTheme);
    }

    fn replace_images(&mut self, directory: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.images.release(window, cx);
        self.images = images::install(&self.editor, directory, cx);
    }

    fn dirty(&self, cx: &App) -> bool {
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

    fn can_copy_markdown(&self) -> bool {
        !self.loading && !self.unavailable && !self.source_only
    }

    fn copy_markdown(&mut self, _: &CopyMarkdown, _: &mut Window, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            self.editor.read(cx).text().to_owned(),
        ));
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

    fn display_name(&self) -> String {
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

    fn open_source(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.source_only = true;
        self.preview.source = Some(path.clone());
        self.preview.attachment = Some(path.clone());
        self.preview.visible = true;
        self.open_path(path, window, cx);
        if self.active {
            window.focus(&self.focus, cx);
        }
        self.update_title(window, cx);
    }

    fn update_title(&self, window: &mut Window, _cx: &App) {
        if !self.active {
            return;
        }
        let name = self.display_name();
        window.set_window_title(&format!(
            "{}{} — mdoc",
            if self.dirty_cached { "• " } else { "" },
            name
        ));
    }

    fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        if !self.dirty(cx) {
            self.proceed(next, window, cx);
            return;
        }
        self.prompting = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes to this document?",
            Some("Unsaved changes will be lost if you discard them."),
            &["Save", "Cancel", "Discard"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                match answer {
                    Some(0) => this.save(false, Some(next), window, cx),
                    Some(2) => this.proceed(next, window, cx),
                    _ => {
                        cx.emit(tabs::TabEvent::CloseCancelled);
                    }
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_feedback = None;
        match next {
            Next::Close => {
                cx.emit(tabs::TabEvent::CloseResolved);
                return;
            }
            Next::Import(imported) => {
                self.source_only = false;
                self.generated_unedited = self.automatic_import;
                self.ocr_required = None;
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
                self.update_title(window, cx);
            }
        }
        cx.notify();
    }

    fn import(&mut self, _: &Import, window: &mut Window, cx: &mut Context<Self>) {
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

    fn start_import(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
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
        if allow_ocr && self.ocr_state.busy() {
            return;
        }
        let config = if allow_ocr && !skip_ocr {
            match self.preferences.borrow().snapshot() {
                Ok(p) => Some(p.ocr),
                Err(e) => {
                    self.error = Some(e);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let installed = match &self.ocr_state {
            OcrState::Ready(installed)
                if allow_ocr
                    && !skip_ocr
                    && config
                        .as_ref()
                        .is_some_and(|c| c.model == installed.config.model) =>
            {
                let mut installed = installed.clone();
                installed.config = config.clone().unwrap();
                Some(installed)
            }
            _ => None,
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
        cx.emit(tabs::TabEvent::ImportState);
        let cancel = self.import_cancel.clone();
        let owner = self.owner.clone();
        self.error = None;
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

    fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    self.ocr_state = OcrState::Failed(error.clone());
                    cx.emit(tabs::TabEvent::OcrState(self.ocr_state.clone()));
                    self.error = Some(error);
                    if let Some(path) = self.conversion_source.clone() {
                        self.await_ocr(path, cx);
                    }
                }
                Err(import::ImportError::Message(error)) => self.error = Some(error),
                Err(import::ImportError::NeedsOcr(path, pages)) => {
                    self.ocr_required = Some(pages);
                    self.await_ocr(path, cx)
                }
            }
        }
        if !self.job.busy() {
            self.import_permit = None;
            cx.emit(tabs::TabEvent::ImportState);
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
        if matches!(self.ocr_state, OcrState::Ready(_)) {
            return;
        }
        self.begin_ocr_setup(window, cx);
    }

    fn begin_ocr_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ocr_state.busy() {
            return;
        }
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.ocr,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
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
            self.ocr_state = OcrState::Installing;
            cx.emit(tabs::TabEvent::OcrState(self.ocr_state.clone()));
            cx.notify();
        } else if self.job.has_ocr_continuation() {
            self.finish_ocr_setup(
                Err("Another model job is running. Retry when it finishes.".into()),
                window,
                cx,
            );
        }
    }

    fn finish_ocr_setup(
        &mut self,
        result: Result<ocr::Installed, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(installed) => {
                self.ocr_state = OcrState::Ready(installed);
                if let Some(path) = self.job.resume_after_setup(self.session.generation) {
                    self.start_import_mode(path, false, window, cx);
                }
            }
            Err(error) => {
                self.ocr_state = OcrState::Failed(error);
                self.job.finish();
                if let Some(path) = self.job.resume_after_setup(self.session.generation) {
                    self.await_ocr(path, cx);
                }
            }
        }
        if !self.job.busy() {
            self.import_permit = None;
        }
        cx.emit(tabs::TabEvent::OcrState(self.ocr_state.clone()));
        cx.emit(tabs::TabEvent::ImportState);
        cx.notify();
    }

    // Consent lives in the source pane. Waiting for a decision must not hold
    // the global conversion slot or interrupt another tab with a modal dialog.
    fn await_ocr(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.conversion_source = Some(path);
        self.ocr_required.get_or_insert_with(Vec::new);
        self.ocr_notice_dismissed = false;
        self.focus_card = true;
        self.job.finish();
        self.import_permit = None;
        cx.emit(tabs::TabEvent::ImportState);
        cx.notify();
    }

    fn ocr_action(&mut self, skip: bool, window: &mut Window, cx: &mut Context<Self>) {
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
        if skip {
            self.start_import_mode(path, skip, window, cx);
        } else if matches!(self.ocr_state, OcrState::Ready(_)) {
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

    fn try_auto_convert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let model = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| settings::Model::Ocr(p.ocr.model));
        let panel = self.model_panel.read(cx);
        let progress = model.and_then(|m| panel.progress_of(m));
        let cancelling = panel.cancelling();
        let pending = model
            .map(|m| panel.pending[settings_ui::Panel::index(m)].total())
            .unwrap_or(0);
        let ready = matches!(self.ocr_state, OcrState::Ready(_));
        let installing = progress.is_some() || matches!(self.ocr_state, OcrState::Installing);
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
                        "The notice above explains what went wrong.",
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
            OcrState::Failed(error) => Some(error.clone()),
            _ => None,
        };
        let primary = if ready {
            "Run OCR"
        } else if failed.is_some() {
            "Retry"
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
        let language = match model {
            Some(settings::Model::Ocr(m)) => Some(m.languages()),
            _ => None,
        };
        card.when_some(failed, |v, error| v.child(ui::card_error(error, theme)))
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
            .when_some(language, |v, languages| {
                v.child(ui::card_hint(
                    format!(
                        "This model reads {languages}. If the document is in another language, choose a model that supports it."
                    ),
                    theme,
                )
                .when(cfg!(test), |v| v.debug_selector(|| "ocr-language-hint".into())))
            })
            .child(ui::card_note(
                if ready || pending == 0 {
                    format!("{name} · runs on this computer")
                } else {
                    format!(
                        "{name} · {} MB download, once · then runs on this computer",
                        model_download::megabytes(pending)
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
                                    _ => this.error = None,
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn save_directory(&self) -> PathBuf {
        self.session.directory()
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
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
            cx.emit(tabs::TabEvent::Open(vec![path]));
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

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
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
                if let Ok(Ok(Some(paths))) = result {
                    cx.emit(tabs::TabEvent::Open(paths));
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn save(
        &mut self,
        save_as: bool,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompting || self.loading || self.unavailable || self.source_only {
            cx.emit(tabs::TabEvent::CloseCancelled);
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
                        cx.emit(tabs::TabEvent::CloseCancelled);
                        cx.notify();
                        this.resume_import(window, cx);
                        return;
                    }
                    this.write(path, next, window, cx);
                } else {
                    cx.emit(tabs::TabEvent::CloseCancelled);
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn write(
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
            cx.emit(tabs::TabEvent::SaveConflict(other));
            cx.emit(tabs::TabEvent::CloseCancelled);
            return;
        }
        let old_directory = self.save_directory();
        match self.session.save(path, self.editor.read(cx).text()) {
            Ok(()) => {
                self.generated_unedited = false;
                self.blank_disposable = false;
                self.dirty_cached = false;
                cx.emit(tabs::TabEvent::Saved);
                if old_directory != self.session.document.directory() {
                    self.replace_images(self.session.document.directory(), window, cx);
                }
                self.error = None;
                self.update_title(window, cx);
                if let Some(next) = next {
                    self.proceed(next, window, cx);
                }
            }
            Err(error) => {
                self.error = Some(format!("Could not save: {error}"));
                cx.emit(tabs::TabEvent::CloseCancelled);
            }
        }
        cx.notify();
    }
}

fn button(
    label: &'static str,
    action: impl gpui::Action,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    ui::control(label, label, theme, true)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

fn bind_markdown_search_keys(cx: &mut App) {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new(&format!("{modifier}-f"), FindMarkdown, Some("Editor")),
        KeyBinding::new(
            &format!("{modifier}-f"),
            FindMarkdown,
            Some("MarkdownSearch"),
        ),
        KeyBinding::new(&format!("{modifier}-f"), FindMarkdown, None),
        KeyBinding::new(&format!("{modifier}-g"), FindNextMarkdown, Some("Editor")),
        KeyBinding::new(
            &format!("{modifier}-g"),
            FindNextMarkdown,
            Some("MarkdownSearch"),
        ),
        KeyBinding::new(
            &format!("{modifier}-shift-g"),
            FindPreviousMarkdown,
            Some("Editor"),
        ),
        KeyBinding::new(
            &format!("{modifier}-shift-g"),
            FindPreviousMarkdown,
            Some("MarkdownSearch"),
        ),
        KeyBinding::new("enter", FindNextMarkdown, Some("MarkdownSearch")),
        KeyBinding::new("shift-enter", FindPreviousMarkdown, Some("MarkdownSearch")),
        KeyBinding::new("escape", CloseMarkdownSearch, Some("MarkdownSearch")),
    ]);
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Derived display state; set_outlines only notifies on change.
        self.sync_scope_outlines(cx);
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
        let search_bar = div()
            .id("markdown-search-bar")
            .flex()
            .items_center()
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
                    .id("markdown-search-input")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .aria_label("Search Markdown")
                    .px_2()
                    .items_center()
                    .bg(palette.bg)
                    .border_1()
                    .border_color(if search_focused {
                        theme.search_accent()
                    } else {
                        palette.border
                    })
                    .rounded_md()
                    .child(self.markdown_search.clone()),
            )
            .child(
                div()
                    .id("markdown-search-match-case")
                    .role(gpui::Role::Button)
                    .key_context("UiControl")
                    .tab_index(0)
                    .focus_visible(|s| s.bg(palette.placeholder_bg))
                    .aria_label(if self.search.match_case {
                        "Match case: on"
                    } else {
                        "Match case: off"
                    })
                    .flex_shrink_0()
                    .rounded_md()
                    .px_2()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .text_color(palette.header_muted)
                    .hover(|view| view.bg(palette.placeholder_bg))
                    .when(self.search.match_case, |view| {
                        view.bg(palette.placeholder_bg)
                            .text_color(theme.search_accent())
                    })
                    .child("Aa")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_markdown_match_case(&ToggleMarkdownMatchCase, window, cx)
                    })),
            )
            .child(
                div()
                    .id("markdown-search-count")
                    .flex_shrink_0()
                    .px_2()
                    .text_size(px(12.))
                    .text_color(palette.header_muted)
                    .child(if self.search.task.is_some() {
                        ui::activity("search-activity", count, theme).into_any_element()
                    } else {
                        div().child(count).into_any_element()
                    }),
            )
            .child(
                div()
                    .id("markdown-search-previous")
                    .role(gpui::Role::Button)
                    .key_context("UiControl")
                    .tab_index(0)
                    .focus_visible(|s| s.bg(palette.placeholder_bg))
                    .aria_label("Previous match")
                    .w(px(28.))
                    .h(px(28.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|view| view.bg(palette.placeholder_bg))
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .child("↑")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.find_previous_markdown(&FindPreviousMarkdown, window, cx)
                    })),
            )
            .child(
                div()
                    .id("markdown-search-next")
                    .role(gpui::Role::Button)
                    .key_context("UiControl")
                    .tab_index(0)
                    .focus_visible(|s| s.bg(palette.placeholder_bg))
                    .aria_label("Next match")
                    .w(px(28.))
                    .h(px(28.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|view| view.bg(palette.placeholder_bg))
                    .when(navigation_disabled, |view| {
                        view.text_color(palette.header_muted)
                    })
                    .child("↓")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.find_next_markdown(&FindNextMarkdown, window, cx)
                    })),
            )
            .child(
                div()
                    .id("markdown-search-close")
                    .role(gpui::Role::Button)
                    .key_context("UiControl")
                    .tab_index(0)
                    .focus_visible(|s| s.bg(palette.placeholder_bg))
                    .aria_label("Close Markdown search")
                    .w(px(28.))
                    .h(px(28.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|view| view.bg(palette.placeholder_bg))
                    .child("×")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_markdown_search(&CloseMarkdownSearch, window, cx)
                    })),
            );
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
        let error_notice = self
            .error
            .clone()
            .map(|detail| self.notice("error-notice", "Needs attention", detail, true, cx));
        let width = self.chrome_width(window);
        let content_width = (width - f32::from(self.replacements_panel_width(window))).max(1.);
        let narrow_preview = self.preview.visible && content_width < 620.;
        let show_original = self.preview.visible && (!narrow_preview || self.original_selected);
        let show_markdown = !narrow_preview || !self.original_selected;
        let minimum = (280. / content_width.max(620.)).min(0.5);
        let split = self
            .preview
            .split_ratio
            .unwrap_or(0.5)
            .clamp(minimum, 1. - minimum);
        let measured_bounds = self.workspace_bounds.clone();
        let measured_owner = cx.entity().downgrade();
        div().relative().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .child(gpui::canvas(move |bounds, _, cx| { if measured_bounds.replace(bounds) != bounds { let _ = measured_owner.update(cx, |_, cx| cx.notify()); } }, |_, _, _, _| {}).absolute().inset_0())
            .on_action(cx.listener(|_, _: &ui::NextControl, window, cx| ui::cycle(window, cx, None, false)))
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| ui::cycle(window, cx, None, true)))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                if event.pressed_button != Some(gpui::MouseButton::Left) { this.split_dragging = false; }
                if this.split_dragging {
                    let bounds = this.workspace_bounds.get();
                    let panel = this.replacements_panel_width(window);
                    let width = f32::from(bounds.size.width - panel).max(1.);
                    let ratio = f32::from(event.position.x - bounds.left() - panel) / width;
                    this.preview.split_ratio = Some(ratio.clamp(0.25, 0.75));
                    cx.notify();
                }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = false; cx.notify(); }))
            // While the empty page shows, the whole workspace accepts OS file drops.
            .when(empty_page, |v| v
                .on_drag_move(cx.listener(|this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let over = event.bounds.contains(&event.event.position);
                    let state = over.then(|| event.drag(cx).paths().iter().any(|path| import::supported_extension(path)));
                    if this.file_drag != state { this.file_drag = state; cx.notify(); }
                }))
                .can_drop(|drag, _, _| drag.downcast_ref::<gpui::ExternalPaths>().is_some_and(|drag| drag.paths().iter().any(|path| import::supported_extension(path))))
                .on_drop(cx.listener(|this, drag: &gpui::ExternalPaths, _, cx| {
                    this.file_drag = None;
                    cx.emit(tabs::TabEvent::Open(drag.paths().to_vec()));
                    cx.notify();
                })))
            // Toolbar clicks must dispatch inside this workspace, not the outer tab shell.
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| this.toggle_preview(window, cx)))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(Self::copy_markdown))
            .on_action(cx.listener(Self::pseudonymize))
            .on_action(cx.listener(Self::add_pii_candidate))
            .on_action(cx.listener(Self::apply_all_pii))
            .on_action(cx.listener(|this, _: &PiiReviewCandidate, window, cx| this.step_pii_highlight(false, true, window, cx)))
            .on_action(cx.listener(|this, _: &PiiNextCandidate, window, cx| this.step_pii_highlight(false, false, window, cx)))
            .on_action(cx.listener(|this, _: &PiiPreviousCandidate, window, cx| this.step_pii_highlight(true, false, window, cx)))
            .on_action(cx.listener(Self::setup_ocr))
            .on_action(cx.listener(|this, _: &RunOcr, window, cx| this.ocr_action(false, window, cx)))
            .on_action(cx.listener(|this, _: &ExtractNative, window, cx| this.ocr_action(true, window, cx)))
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| { this.session.warning = None; cx.notify(); }))
            .on_action(cx.listener(|_, _: &New, _, cx| cx.emit(tabs::TabEvent::New)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)))
            .on_action(cx.listener(|_, _: &Close, _, cx| cx.emit(tabs::TabEvent::CloseRequested)))
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| { this.toggle_preview(window, cx); if this.source_only { window.focus(&this.focus, cx); } else { window.focus(&this.editor.read(cx).focus_handle(cx), cx); } cx.notify(); }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .child(self.toolbar(window, cx))
            .when(!self.source_only && self.job.busy(), |v| v.child(
                ui::activity("document-work-activity", if matches!(self.ocr_state, OcrState::Installing) { "Setting up text recognition…" } else if self.job.recognizing() { "Recognizing text…" } else { "Converting to Markdown…" }, theme)
                    .px_3().py_2().border_b_1().border_color(palette.border)))
            .when_some(self.session.ocr_configuration.clone(),|v,config|v.child(div().px_3().py_1().text_size(px(11.)).text_color(palette.header_muted).child(format!("OCR result: {} · {} DPI · minimum confidence {} · Force",config.model.name(),config.dpi,config.minimum_confidence))))
            .children(import_notice).children(ocr_notice.filter(|_| !show_markdown))
            .when(narrow_preview, |v| v.child(self.pane_switch(cx)))
            .child(div().flex().flex_1().min_h_0()
                .when(!self.source_only && show_markdown, |row| row.child(div().flex().flex_1().min_w_0().h_full().flex().flex_col()
                    .when(self.search.open, |column| column.child(search_bar))
                    .child(if self.loading || self.unavailable {
                        div().p_6().child(if self.loading { ui::activity("markdown-loading", "Loading Markdown…", theme).into_any_element() } else { div().child("Markdown unavailable").into_any_element() })
                            .when(self.unavailable, |view| view.child(button("Retry", RetryDocument, theme))).into_any_element()
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        .child(div().id("document-scroll").size_full().overflow_y_scroll().track_scroll(&self.scroll).p_6()
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify())).child(self.editor.clone()))
                        .child(markdown_scrollbar).children(empty_page.then(|| self.empty_page(cx))).into_any_element() })))
                // Replacements sit beside the text they describe, before Original.
                .children(self.replacements_panel(window, cx))
                .when(self.source_only && show_markdown, |row| row.child(div().id("conversion-pane").flex_1().min_w_0().p_6().flex().items_center().justify_center().overflow_y_scroll()
                    .child(self.conversion_card(window, cx))))
                .when(self.preview.visible && !narrow_preview, |row| row.child(div().id("preview-divider").when(cfg!(test), |v| v.debug_selector(|| "preview-divider".into())).w(px(6.)).h_full().flex_shrink_0().cursor(gpui::CursorStyle::ResizeLeftRight).bg(palette.border)
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| { this.split_dragging = true; cx.notify(); }))))
                .when(show_original && self.preview.pdf.is_none(), |row| row.child(div().when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).h_full().border_l_1().border_color(palette.border)
                    .flex().flex_col().items_center().justify_center().text_color(palette.header_muted)
                    .child(if self.preview.loading { ui::activity("preview-activity", "Preparing preview…", theme).into_any_element() } else { div().child(if self.preview.queued { "Waiting to prepare preview…" } else { "Preview unavailable" }).into_any_element() }).when_some(self.preview.message.clone(), |v, message| v.child(self.notice("preview-notice", "Preview", message, self.preview.retryable, cx)))))
                .when_some(self.preview.pdf.clone().filter(|_| show_original), |row, pdf| row.child(div().when(!narrow_preview, |v| v.w(px(content_width * (1. - split)))).when(narrow_preview, |v| v.flex_1()).flex_shrink_0().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))
                    .when_some(self.preview.comment_panel.clone(), |pane, comments| pane.child(comments))
                    .children(preview_notice)))
)
            .children(setup_notice.filter(|_| !(show_markdown && self.source_only && self.ocr_required.is_some()))).children(error_notice)
            .children(self.pii_popup(window, cx))
    }
}

fn main() {
    // The pinned engine chooses providers through this variable. Set it once,
    // before starting GPUI, executors or native worker threads; never mutate it
    // while the application is running. Experimental inference is CPU only.
    unsafe {
        std::env::set_var("GLINER2_DEVICE", "cpu");
    }
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--mdoc-docx-worker") {
        let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
            eprintln!("missing DOCX worker input or output");
            std::process::exit(2);
        };
        let output = PathBuf::from(output);
        if let Err(error) = docx_preview::run_worker(&PathBuf::from(input), &output) {
            let _ = std::fs::write(output.with_extension("error"), error.to_string());
            std::process::exit(1);
        }
        return;
    }
    let initial: Vec<_> = args.into_iter().skip(1).map(PathBuf::from).collect();
    let application = gpui_platform::application();
    let open_context = Rc::new(RefCell::new(
        None::<(gpui::WindowHandle<tabs::Tabs>, gpui::AsyncApp)>,
    ));
    let pending_url = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    application.on_open_urls({
        let open_context = open_context.clone();
        let pending_url = pending_url.clone();
        move |urls| {
            let paths = urls
                .iter()
                .filter_map(|url| url::Url::parse(url).ok()?.to_file_path().ok())
                .collect();
            if let Some((window, cx)) = open_context.borrow_mut().as_mut() {
                let _ = window.update(cx, |workspace, window, cx| {
                    workspace.open_paths(paths, window, cx)
                });
            } else {
                pending_url.borrow_mut().extend(paths);
            }
        }
    });
    application.run(move |cx: &mut App| {
        cx.on_app_quit(|cx| {
            model_work::shutdown();
            let executor = cx.background_executor().clone();
            async move {
                executor
                    .spawn(async {
                        docx_preview::shutdown_workers();
                    })
                    .await
            }
        })
        .detach();
        mdoc_editor::bind_keys(cx);
        markdown_search::bind_keys(cx);
        pii::ui::bind_keys(cx);
        settings_ui::bind_keys(cx);
        ui::bind_keys(cx);
        cx.bind_keys([KeyBinding::new(
            "escape",
            comparison_ui::CloseComparison,
            Some("ModelComparison"),
        )]);
        bind_markdown_search_keys(cx);
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
            KeyBinding::new(&format!("{modifier}-,"), Settings, None),
            KeyBinding::new(&format!("{modifier}-n"), New, None),
            KeyBinding::new(&format!("{modifier}-o"), Open, None),
            KeyBinding::new(&format!("{modifier}-shift-i"), Import, None),
            KeyBinding::new(&format!("{modifier}-s"), Save, None),
            KeyBinding::new(&format!("{modifier}-shift-s"), SaveAs, None),
            KeyBinding::new(&format!("{modifier}-w"), Close, None),
            KeyBinding::new(&format!("{modifier}-q"), Quit, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::action("Convert to Markdown", Import),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::action("Copy Markdown", CopyMarkdown),
                MenuItem::action("Pseudonymize", Pseudonymize),
                MenuItem::action("Settings", Settings),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        }]);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let mut workspace = tabs::Tabs::new(window, cx);
                        workspace.open_paths(initial, window, cx);
                        workspace
                    })
                },
            )
            .expect("Could not open the editor window");
        let pending = std::mem::take(&mut *pending_url.borrow_mut());
        if !pending.is_empty() {
            let _ = handle.update(cx, |workspace, window, cx| {
                workspace.open_paths(pending, window, cx)
            });
        }
        *open_context.borrow_mut() = Some((handle, cx.to_async()));
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
