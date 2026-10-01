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
mod ocr;
#[cfg(test)]
mod perf_tests;
mod preview;
mod session_store;
mod style;
mod tabs;
#[cfg(test)]
mod ui_tests;

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
        RunOcr,
        ExtractNative,
        DismissImportWarning,
        Save,
        SaveAs,
        CopyMarkdown,
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
    New,
    Open(PathBuf),
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
    fn label(&self) -> &'static str {
        match self {
            Self::Checking => "Checking OCR…",
            Self::Missing => "Set up OCR",
            Self::Installing => "Setting up OCR…",
            Self::Ready(_) => "OCR ready",
            Self::Failed(_) => "Retry OCR setup",
            Self::Unsupported => "OCR unavailable on this platform",
        }
    }
}

struct Workspace {
    focus: gpui::FocusHandle,
    owner: Option<(u64, gpui::WeakEntity<tabs::Tabs>)>,
    active: bool,
    dirty_cached: bool,
    loading: bool,
    load_generation: u64,
    unavailable: bool,
    source_only: bool,
    auto_convert_pending: bool,
    automatic_import: bool,
    generated_unedited: bool,
    blank_disposable: bool,
    ocr_required: Option<Vec<u32>>,
    import_permit: Option<Arc<import_session::ImportPermit>>,
    conversion_source: Option<PathBuf>,
    import_busy: Arc<AtomicBool>,
    import_cancel: Arc<AtomicBool>,
    theme: Rc<Cell<Theme>>,
    editor: Entity<EditorState>,
    images: images::ImageCache,
    session: document_session::DocumentSession,
    preview: preview::PreviewState,
    scroll: ScrollHandle,
    error: Option<String>,
    copy_feedback: Option<gpui::Task<()>>,
    expanded_notice: Option<&'static str>,
    ocr_notice_dismissed: bool,
    setup_error_dismissed: bool,
    prompting: bool,
    job: import_session::ImportSession,
    ocr_state: OcrState,
    markdown_search: Entity<markdown_search::SearchInput>,
    search: search_session::SearchSession,
    _subscription: Subscription,
    _markdown_search_subscription: Subscription,
}

impl Workspace {
    #[cfg(test)]
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_ocr(None, window, cx)
    }

    fn new_with_ocr(ocr: Option<OcrState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let markdown_search = cx.new(markdown_search::SearchInput::new);
        let editor = cx.new(|cx| {
            let mut editor =
                EditorState::new(window, cx).with_placeholder("Start writing Markdown…");
            editor.set_markdown_style(style::markdown_style(Theme::default()), cx);
            editor.set_block_chip_provider(|src| {
                gpui_pdf::is_pdf(src).then(|| src.to_owned().into())
            });
            editor
        });
        let images = images::install(&editor, Document::default().directory(), cx);
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Changed => {
                    this.copy_feedback = None;
                    this.generated_unedited = false;
                    this.blank_disposable = false;
                    this.dirty_cached = this.dirty(cx);
                    this.update_title(window, cx);
                    this.refresh_markdown_search(SearchRefresh::DocumentEdit, false, window, cx);
                    cx.notify();
                }
                EditorEvent::OpenLink(src) => {
                    if src.starts_with("https://")
                        || src.starts_with("http://")
                        || src.starts_with("mailto:")
                    {
                        cx.open_url(src);
                    } else if let Some(path) = document::local_path(src, &this.save_directory()) {
                        if this.owner.is_some() {
                            cx.emit(tabs::TabEvent::Open(vec![path]));
                        } else {
                            this.open_path(path, window, cx);
                        }
                    }
                }
                _ => {}
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
        if ocr.is_none() {
            let task = cx.background_executor().spawn(async { ocr::check() });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.ocr_state = match result {
                        Ok(Some(installed)) => OcrState::Ready(installed),
                        Ok(None) if ocr::SUPPORTED => OcrState::Missing,
                        Ok(None) => OcrState::Unsupported,
                        Err(error) => OcrState::Failed(error),
                    };
                    cx.notify();
                });
            })
            .detach();
        }
        Self {
            focus: cx.focus_handle(),
            owner: None,
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
            ocr_required: None,
            import_permit: None,
            conversion_source: None,
            import_busy: Arc::new(AtomicBool::new(false)),
            import_cancel: Arc::new(AtomicBool::new(false)),
            theme: Rc::new(Cell::new(Theme::default())),
            editor,
            images,
            session: document_session::DocumentSession::default(),
            preview: preview::PreviewState::default(),
            scroll: ScrollHandle::new(),
            error: None,
            copy_feedback: None,
            expanded_notice: None,
            ocr_notice_dismissed: false,
            setup_error_dismissed: false,
            prompting: false,
            job: import_session::ImportSession::default(),
            ocr_state: ocr.unwrap_or(OcrState::Checking),
            markdown_search,
            search: search_session::SearchSession::default(),
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        if self.owner.is_some() {
            cx.emit(tabs::TabEvent::ToggleTheme);
            return;
        }
        let theme = self.theme.get().toggle();
        self.theme.set(theme);
        self.editor.update(cx, |editor, cx| {
            editor.set_markdown_style(style::markdown_style(theme), cx)
        });
        if let Some(pdf) = &self.preview.pdf {
            pdf.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
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
        if self.owner.is_some() {
            match &next {
                Next::New => {
                    cx.emit(tabs::TabEvent::New);
                    return;
                }
                Next::Open(path) => {
                    cx.emit(tabs::TabEvent::Open(vec![path.clone()]));
                    return;
                }
                _ => {}
            }
        }
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
                if self.owner.is_some() {
                    cx.emit(tabs::TabEvent::CloseResolved);
                    return;
                }
                self.session.replace(Document::default());
                self.reset_markdown_search(cx);
                self.close_preview(window, cx);
                window.remove_window();
            }
            Next::New => {
                self.session.replace(Document::default());
                self.replace_images(self.session.document.directory(), window, cx);
                self.editor.update(cx, |editor, cx| editor.set_text("", cx));
                self.reset_markdown_search(cx);
                self.error = None;
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.update_title(window, cx);
            }
            Next::Open(path) => match Document::open(path) {
                Ok(document) => {
                    self.session.replace(document);
                    self.editor.update(cx, |editor, cx| {
                        editor.set_text(self.session.document.saved.clone(), cx)
                    });
                    self.reset_markdown_search(cx);
                    self.replace_images(self.session.document.directory(), window, cx);
                    self.error = None;
                    self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    self.update_title(window, cx);
                }
                Err(error) => self.error = Some(format!("Could not open document: {error}")),
            },
            Next::Import(imported) => {
                self.source_only = false;
                self.generated_unedited = self.automatic_import;
                self.ocr_required = None;
                let retain_preview = self.preview.source.as_ref() == Some(&imported.source)
                    && (self.preview.pdf.is_some() || self.preview.loading);
                self.session
                    .import(imported.source.clone(), imported.warning);
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
        let installed = match &self.ocr_state {
            OcrState::Ready(installed) if allow_ocr && !skip_ocr => Some(installed.clone()),
            _ => None,
        };
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
            if let Some((_, owner)) = owner {
                let _ = owner.update_in(cx, |owner, window, cx| owner.refresh_import(window, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .owner
            .as_ref()
            .and_then(|(_, owner)| owner.upgrade())
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
        self.setup_error_dismissed = false;
        self.ocr_state = OcrState::Installing;
        cx.emit(tabs::TabEvent::OcrState(self.ocr_state.clone()));
        self.error = None;
        let owner = self.owner.clone();
        let task = cx.background_executor().spawn(async { ocr::install() });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            // Installation belongs to the application even if its initiating
            // tab has closed. Only the import continuation belongs to the tab.
            if let Some((_, owner)) = owner {
                let state = match &result {
                    Ok(installed) => OcrState::Ready(installed.clone()),
                    Err(error) => OcrState::Failed(error.clone()),
                };
                let _ = owner.update(cx, |owner, cx| owner.set_ocr(state, cx));
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_ocr_setup(result, window, cx)
            });
        })
        .detach();
        cx.notify();
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
        if skip || matches!(self.ocr_state, OcrState::Ready(_)) {
            self.start_import_mode(path, skip, window, cx);
        } else if ocr::SUPPORTED {
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
        let expanded = self.expanded_notice == Some(id);
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
                    .child(div().text_color(palette.header_fg).child(title))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(palette.header_muted)
                            .child(detail.clone()),
                    )
                    .when(id == "preview-notice" && self.preview.retryable, |v| {
                        v.child(button("Retry", RetryPreview, theme))
                    })
                    .child(
                        div()
                            .id((id, 0usize))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .hover(|v| v.bg(palette.placeholder_bg))
                            .child(if expanded { "Less" } else { "Details" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.expanded_notice = if expanded { None } else { Some(id) };
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id((id, 1usize))
                            .aria_label("Dismiss notice")
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
            .when(expanded, |v| {
                v.child(
                    div()
                        .id((id, 2usize))
                        .max_h(px(120.))
                        .overflow_y_scroll()
                        .px_3()
                        .pb_2()
                        .child(detail),
                )
            })
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
            self.request(Next::Open(path), window, cx);
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
                    if this.owner.is_some() {
                        cx.emit(tabs::TabEvent::Open(paths));
                    } else {
                        for path in paths {
                            this.open_path(path, window, cx);
                        }
                    }
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
        if let Some((id, owner)) = &self.owner {
            let identity = session_store::identity(&path);
            let other = owner
                .upgrade()
                .and_then(|owner| owner.read(cx).find_path(&identity, Some(*id), cx));
            if let Some(other) = other {
                cx.emit(tabs::TabEvent::SaveConflict(other));
                cx.emit(tabs::TabEvent::CloseCancelled);
                return;
            }
        }
        let path = if self.owner.is_some() {
            session_store::identity(&path)
        } else {
            path
        };
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

fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> impl IntoElement {
    div()
        .id(label)
        .when(cfg!(test), |view| view.debug_selector(move || label.into()))
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .hover(move |s| s.bg(theme.pdf_style().placeholder_bg))
        .child(label)
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
                    .child(count),
            )
            .child(
                div()
                    .id("markdown-search-previous")
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
                "Choose an action in the Markdown pane to continue.".into(),
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
        let ocr_pages = self.ocr_required.as_ref().map(|pages| {
            pages
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        });
        let ocr_disabled =
            self.job.busy() || self.import_busy.load(Ordering::Relaxed) || self.ocr_state.busy();
        div().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            // Toolbar clicks must dispatch inside this workspace, not the outer tab shell.
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(|this, _: &TogglePreview, window, cx| this.toggle_preview(window, cx)))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(Self::copy_markdown))
            .on_action(cx.listener(Self::setup_ocr))
            .on_action(cx.listener(|this, _: &RunOcr, window, cx| this.ocr_action(false, window, cx)))
            .on_action(cx.listener(|this, _: &ExtractNative, window, cx| this.ocr_action(true, window, cx)))
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| { this.session.warning = None; cx.notify(); }))
            .on_action(cx.listener(|this, _: &New, window, cx| this.request(Next::New, window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| { if this.owner.is_some() { cx.emit(tabs::TabEvent::CloseRequested); } else { this.request(Next::Close, window, cx); } }))
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| { this.toggle_preview(window, cx); if this.source_only { window.focus(&this.focus, cx); } else { window.focus(&this.editor.read(cx).focus_handle(cx), cx); } cx.notify(); }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .child(div().flex().flex_wrap().items_center().gap_2().p_2().text_size(px(13.)).border_b_1().border_color(palette.border)
                .child(button("New", New, theme)).child(button("Open…", Open, theme))
                .when(!self.source_only, |bar| bar.child(button("Save", Save, theme)).child(button("Save As…", SaveAs, theme)))
                .when(!self.source_only, |bar| bar.child(if self.can_copy_markdown() {
                    button("Copy Markdown", CopyMarkdown, theme).into_any_element()
                } else { div().px_3().py_1().opacity(0.5).child("Copy Markdown").into_any_element() })
                    .when(self.copy_feedback.is_some(), |bar| bar.child(div().text_color(palette.header_muted).child("Copied"))))
                .when((self.source_only && self.ocr_required.is_none() && !self.auto_convert_pending) || self.job.busy(), |bar| bar.child(if self.job.busy() { div().child(if self.ocr_state.busy() { "Waiting for OCR setup…" } else if self.job.recognizing() { "Recognizing text…" } else { "Converting…" }).into_any_element() } else if self.import_busy.load(Ordering::Relaxed) || self.ocr_state.busy() { div().opacity(0.5).child("Convert to Markdown").into_any_element() } else { button("Convert to Markdown", Import, theme).into_any_element() }))
                .child(if matches!(self.ocr_state, OcrState::Missing | OcrState::Failed(_)) {
                    button(self.ocr_state.label(), SetupOcr, theme).into_any_element()
                } else { div().opacity(0.65).child(self.ocr_state.label()).into_any_element() })
                .child(div().flex_1())
                .child(button(theme.toggle_label(), ToggleTheme, theme))
                .child(if self.dirty_cached { "Unsaved changes" } else if self.source_only { "Source preview" } else { "Markdown · WYSIWYG" })
                .when(self.preview.loading, |bar| bar.child(div().child("Preparing preview…")))
                .when(self.preview.pdf.is_some() || self.preview.source.is_some(), |bar| bar.child(button(if self.preview.visible { "Close Preview" } else { "Show Preview" }, TogglePreview, theme))))
            .children(import_notice).children(ocr_notice)
            .child(div().flex().flex_1().min_h_0()
                .when(!self.source_only, |row| row.child(div().flex().flex_1().min_w_0().h_full().flex().flex_col()
                    .when(self.search.open, |column| column.child(search_bar))
                    .child(if self.loading || self.unavailable {
                        div().p_6().child(if self.loading { "Loading Markdown…" } else { "Markdown unavailable" })
                            .when(self.unavailable, |view| view.child(button("Retry", RetryDocument, theme))).into_any_element()
                    } else { div().relative().flex_1().min_w_0().min_h_0()
                        .child(div().id("document-scroll").size_full().overflow_y_scroll().track_scroll(&self.scroll).p_6()
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify())).child(self.editor.clone()))
                        .child(markdown_scrollbar).into_any_element() })))
                .when(self.source_only, |row| row.child(div().flex_1().min_w_0().p_6().flex().flex_col().justify_center().gap_2()
                    .child(div().text_size(px(18.)).child(self.display_name()))
                    .child(div().text_color(palette.header_muted).child(if self.job.busy() {
                        if self.ocr_state.busy() { "Setting up text recognition…" } else if self.job.recognizing() { "Recognizing text…" } else { "Converting to Markdown…" }
                    } else if self.ocr_required.is_some() { "Text recognition required" }
                    else if self.auto_convert_pending { "Waiting to convert…" }
                    else if self.error.is_some() { "Conversion could not be completed" } else { "Convert this document to begin editing" }))
                    .when_some(ocr_pages, |v, pages| v
                        .child(div().text_size(px(13.)).text_color(palette.header_muted).child(if pages.is_empty() { "Recognition needs your attention.".into() } else { format!("Pages {pages} need OCR. The original remains available on the right.") }))
                        .when(ocr::SUPPORTED && !matches!(self.ocr_state, OcrState::Ready(_)), |v| v.child(div().text_size(px(12.)).text_color(palette.header_muted)
                            .child(format!("Setup downloads about {} MB once. Recognition runs locally on this device.", ocr::download_megabytes()))))
                        .child(div().flex().flex_wrap().gap_2().text_size(px(13.)).when(!ocr_disabled, |v| v
                            .when(ocr::SUPPORTED, |v| v.child(button(if matches!(self.ocr_state, OcrState::Ready(_)) { "Run OCR" } else { "Set up OCR" }, RunOcr, theme)))
                            .child(button("Extract native text only", ExtractNative, theme))))
                        .when(!ocr::SUPPORTED, |v| v.child(div().text_size(px(12.)).text_color(palette.header_muted).child("Local OCR is unavailable on this platform."))))))
                .when(self.source_only && self.preview.visible && self.preview.pdf.is_none(), |row| row.child(div().w_1_2().h_full().border_l_1().border_color(palette.border)
                    .flex().items_center().justify_center().text_color(palette.header_muted)
                    .child(if self.preview.loading { "Preparing preview…" } else { "Preview unavailable" })))
                .when_some(self.preview.pdf.clone().filter(|_| self.preview.visible), |row, pdf| row.child(div().w_1_2().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))
                    .when_some(self.preview.comment_panel.clone(), |pane, comments| pane.child(comments)))))
            .children(preview_notice).children(setup_notice).children(error_notice)
    }
}

fn main() {
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
        bind_markdown_search_keys(cx);
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
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
