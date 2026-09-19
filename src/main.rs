//! A file-based Markdown editor with side-by-side PDF and DOCX preview.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod comment_panel;
mod document;
mod docx_preview;
mod images;
mod import;
mod markdown_search;
mod style;
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
        DismissImportWarning,
        Save,
        SaveAs,
        Close,
        ClosePdf,
        RetryPreview,
        ToggleTheme,
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

struct PendingPreview {
    pdf: Entity<PdfView>,
    docx: Option<docx_preview::DocxPreview>,
    _subscription: Subscription,
}

struct Workspace {
    theme: Rc<Cell<Theme>>,
    editor: Entity<EditorState>,
    document: Document,
    pdf: Option<Entity<PdfView>>,
    docx_preview: Option<docx_preview::DocxPreview>,
    comment_panel: Option<Entity<comment_panel::CommentPanel>>,
    pending_preview: Option<PendingPreview>,
    scroll: ScrollHandle,
    error: Option<String>,
    preview_message: Option<String>,
    preview_loading: bool,
    preview_retryable: bool,
    preview_source: Option<PathBuf>,
    preview_cancel: Option<Arc<AtomicBool>>,
    prompting: bool,
    importing: bool,
    document_generation: u64,
    preview_generation: u64,
    pending_import: Option<(u64, Result<import::Imported, String>)>,
    import_source: Option<PathBuf>,
    import_warning: Option<String>,
    markdown_search: Entity<markdown_search::SearchInput>,
    markdown_search_open: bool,
    markdown_search_match_case: bool,
    markdown_search_index: Option<SearchIndex>,
    markdown_search_source: String,
    markdown_search_matches: Vec<SearchMatch>,
    markdown_search_active: Option<usize>,
    markdown_search_anchor: Option<usize>,
    markdown_search_revision: u64,
    markdown_search_task: Option<gpui::Task<()>>,
    _subscription: Subscription,
    _markdown_search_subscription: Subscription,
    pdf_subscription: Option<Subscription>,
}

#[derive(Clone, Copy)]
enum SearchRefresh {
    Open,
    Query,
    DocumentEdit,
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.cancel_preview_job();
    }
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        images::install(&editor, Document::default().directory(), cx);
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Changed => {
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
                        this.open_path(path, window, cx);
                    }
                }
                _ => {}
            });
        let markdown_search_subscription = cx.subscribe_in(
            &markdown_search,
            window,
            |this, _, event, window, cx| match event {
                markdown_search::SearchInputEvent::Changed if this.markdown_search_open => {
                    this.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
                }
                _ => {}
            },
        );
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.dirty(cx) {
                    this.request(Next::Close, window, cx);
                    false
                } else {
                    this.close_preview(window, cx);
                    true
                }
            })
            .unwrap_or(true)
        });
        Self {
            theme: Rc::new(Cell::new(Theme::default())),
            editor,
            document: Document::default(),
            pdf: None,
            docx_preview: None,
            comment_panel: None,
            pending_preview: None,
            scroll: ScrollHandle::new(),
            error: None,
            preview_message: None,
            preview_loading: false,
            preview_retryable: false,
            preview_source: None,
            preview_cancel: None,
            prompting: false,
            importing: false,
            document_generation: 0,
            preview_generation: 0,
            pending_import: None,
            import_source: None,
            import_warning: None,
            markdown_search,
            markdown_search_open: false,
            markdown_search_match_case: false,
            markdown_search_index: None,
            markdown_search_source: String::new(),
            markdown_search_matches: Vec::new(),
            markdown_search_active: None,
            markdown_search_anchor: None,
            markdown_search_revision: 0,
            markdown_search_task: None,
            _subscription: subscription,
            _markdown_search_subscription: markdown_search_subscription,
            pdf_subscription: None,
        }
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme.get().toggle();
        self.theme.set(theme);
        self.editor.update(cx, |editor, cx| {
            editor.set_markdown_style(style::markdown_style(theme), cx)
        });
        if let Some(pdf) = &self.pdf {
            pdf.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    fn find_markdown(&mut self, _: &FindMarkdown, window: &mut Window, cx: &mut Context<Self>) {
        self.markdown_search_open = true;
        self.refresh_markdown_search(SearchRefresh::Open, true, window, cx);
        self.markdown_search
            .update(cx, |input, cx| input.select_all(cx));
        window.focus(&self.markdown_search.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn find_next_markdown(
        &mut self,
        _: &FindNextMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_markdown_search(false, window, cx);
    }

    fn find_previous_markdown(
        &mut self,
        _: &FindPreviousMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_markdown_search(true, window, cx);
    }

    fn close_markdown_search(
        &mut self,
        _: &CloseMarkdownSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.markdown_search_open {
            return;
        }
        self.markdown_search_task = None;
        self.markdown_search_anchor = None;
        self.markdown_search_open = false;
        self.markdown_search_active = None;
        self.markdown_search_matches.clear();
        self.markdown_search_revision = self.markdown_search_revision.wrapping_add(1);
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(Vec::new(), None, cx)
        });
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn toggle_markdown_match_case(
        &mut self,
        _: &ToggleMarkdownMatchCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.markdown_search_match_case = !self.markdown_search_match_case;
        if self.markdown_search_open {
            self.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
        } else {
            cx.notify();
        }
    }

    fn reset_markdown_search(&mut self, cx: &mut Context<Self>) {
        self.markdown_search_task = None;
        self.markdown_search_anchor = None;
        self.markdown_search_open = false;
        self.markdown_search_match_case = false;
        self.markdown_search_index = None;
        self.markdown_search_source.clear();
        self.markdown_search_matches.clear();
        self.markdown_search_active = None;
        self.markdown_search_revision = self.markdown_search_revision.wrapping_add(1);
        self.markdown_search.update(cx, |input, cx| input.reset(cx));
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(Vec::new(), None, cx)
        });
    }

    fn refresh_markdown_search(
        &mut self,
        reason: SearchRefresh,
        should_scroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.editor.read(cx).text().to_owned();
        if !self.markdown_search_open {
            if source != self.markdown_search_source {
                self.markdown_search_index = None;
                self.markdown_search_source.clear();
                self.markdown_search_matches.clear();
                self.markdown_search_active = None;
            }
            return;
        }

        let old_source = self.markdown_search_source.clone();
        let old_anchor = self
            .markdown_search_active
            .and_then(|index| self.markdown_search_matches.get(index))
            .and_then(search_match_start)
            .or(self.markdown_search_anchor);
        let index = if self.markdown_search_source == source {
            self.markdown_search_index.take()
        } else {
            None
        };
        let query = self.markdown_search.read(cx).value().to_owned();
        let match_case = self.markdown_search_match_case;
        let cursor = self.editor.read(cx).cursor();
        let anchor = match reason {
            SearchRefresh::DocumentEdit => old_anchor
                .map(|offset| map_edit_offset(&old_source, &source, offset))
                .unwrap_or(cursor),
            SearchRefresh::Open | SearchRefresh::Query => cursor,
        };
        self.markdown_search_task = None;
        self.markdown_search_revision = self.markdown_search_revision.wrapping_add(1);
        let revision = self.markdown_search_revision;
        self.markdown_search_anchor = Some(anchor);
        if query.is_empty() {
            self.markdown_search_index = index;
            self.markdown_search_source = source;
            self.publish_markdown_search(Vec::new(), anchor, false, window, cx);
            return;
        }
        // Multi-megabyte projection/folding takes hundreds of milliseconds in
        // debug builds. Keep that work off the event loop, with no result cap.
        if source.len() > 64 * 1024 {
            self.markdown_search_matches.clear();
            self.markdown_search_active = None;
            self.editor.update(cx, |editor, cx| {
                editor.set_search_matches(Vec::new(), None, cx)
            });
            self.markdown_search_source = source.clone();
            self.markdown_search_index = None;
            let work = cx.background_executor().spawn(async move {
                let index = index.unwrap_or_else(|| SearchIndex::from_markdown(&source));
                let matches = index.find(&query, match_case);
                (source, index, matches)
            });
            self.markdown_search_task = Some(cx.spawn_in(window, async move |this, cx| {
                let (source, index, matches) = work.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.markdown_search_open && this.markdown_search_revision == revision {
                        this.markdown_search_task = None;
                        this.markdown_search_source = source;
                        this.markdown_search_index = Some(index);
                        this.publish_markdown_search(matches, anchor, should_scroll, window, cx);
                    }
                });
            }));
            cx.notify();
            return;
        }
        let index = index.unwrap_or_else(|| SearchIndex::from_markdown(&source));
        let matches = index.find(&query, match_case);
        self.markdown_search_index = Some(index);
        self.markdown_search_source = source;
        self.publish_markdown_search(matches, anchor, should_scroll, window, cx);
    }

    fn publish_markdown_search(
        &mut self,
        matches: Vec<SearchMatch>,
        anchor: usize,
        should_scroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = if matches.is_empty() {
            None
        } else {
            first_search_match_at_or_after(&matches, anchor).or(Some(0))
        };
        self.markdown_search_anchor = active.and_then(|i| search_match_start(&matches[i]));
        self.markdown_search_matches = matches;
        self.markdown_search_active = active;
        let revision = self.markdown_search_revision;
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(
                self.markdown_search_matches.clone(),
                self.markdown_search_active,
                cx,
            )
        });
        if should_scroll && self.markdown_search_active.is_some() {
            self.schedule_markdown_search_scroll(revision, window, cx);
        }
        cx.notify();
    }

    fn step_markdown_search(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.markdown_search_open || self.markdown_search_matches.is_empty() {
            return;
        }
        let len = self.markdown_search_matches.len();
        let current = self.markdown_search_active.unwrap_or_else(|| {
            first_search_match_at_or_after(
                &self.markdown_search_matches,
                self.editor.read(cx).cursor(),
            )
            .unwrap_or(0)
        });
        self.markdown_search_active = Some(if backwards {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        });
        self.markdown_search_revision = self.markdown_search_revision.wrapping_add(1);
        let revision = self.markdown_search_revision;
        self.editor.update(cx, |editor, cx| {
            editor.set_active_search_match(self.markdown_search_active, cx)
        });
        self.schedule_markdown_search_scroll(revision, window, cx);
        cx.notify();
    }

    fn schedule_markdown_search_scroll(
        &self,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refine_markdown_search_scroll(revision, 2, window, cx);
    }

    fn refine_markdown_search_scroll(
        &self,
        revision: u64,
        remaining: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |workspace, cx| {
                if !workspace.markdown_search_open || workspace.markdown_search_revision != revision
                {
                    return;
                }
                if let Some(index) = workspace.markdown_search_active {
                    workspace.scroll_to_markdown_search(index, cx);
                    // on_next_frame runs before paint: new query highlights or
                    // focus-dependent wrapping may only acquire bounds afterward.
                    if remaining > 0 {
                        workspace.refine_markdown_search_scroll(
                            revision,
                            remaining - 1,
                            window,
                            cx,
                        );
                        cx.notify();
                    }
                }
            });
        });
    }

    fn scroll_to_markdown_search(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(match_bounds) = self.editor.read(cx).search_match_bounds(index) else {
            return;
        };
        let viewport = self.scroll.bounds();
        let margin = px(24.);
        let mut offset = self.scroll.offset();
        if match_bounds.top() < viewport.top() + margin {
            offset.y += viewport.top() + margin - match_bounds.top();
        } else if match_bounds.bottom() > viewport.bottom() - margin {
            offset.y -= match_bounds.bottom() - (viewport.bottom() - margin);
        }
        let max = self.scroll.max_offset();
        let min_y = -max.y;
        if offset.y > px(0.) {
            offset.y = px(0.);
        }
        if offset.y < min_y {
            offset.y = min_y;
        }
        if self.scroll.offset() != offset {
            self.scroll.set_offset(offset);
            cx.notify();
        }
    }

    fn dirty(&self, cx: &App) -> bool {
        (self.document.path.is_none() && self.import_source.is_some())
            || self.editor.read(cx).text() != self.document.saved
    }

    fn update_title(&self, window: &mut Window, cx: &App) {
        let suggested = self.import_source.as_ref().map(|p| p.with_extension("md"));
        let name = self
            .document
            .path
            .as_deref()
            .or(suggested.as_deref())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".into());
        window.set_window_title(&format!(
            "{}{} — mdoc",
            if self.dirty(cx) { "• " } else { "" },
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
                    _ => {}
                }
                this.resume_import(window, cx);
            });
        })
        .detach();
    }

    fn proceed(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Close => {
                self.changed_document();
                self.reset_markdown_search(cx);
                self.close_preview(window, cx);
                window.remove_window();
            }
            Next::New => {
                self.changed_document();
                self.document = Document::default();
                images::install(&self.editor, self.document.directory(), cx);
                self.editor.update(cx, |editor, cx| editor.set_text("", cx));
                self.reset_markdown_search(cx);
                self.error = None;
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.update_title(window, cx);
            }
            Next::Open(path) => match Document::open(path) {
                Ok(document) => {
                    self.changed_document();
                    self.editor
                        .update(cx, |editor, cx| editor.set_text(document.saved.clone(), cx));
                    self.reset_markdown_search(cx);
                    self.document = document;
                    images::install(&self.editor, self.document.directory(), cx);
                    self.error = None;
                    self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    self.update_title(window, cx);
                }
                Err(error) => self.error = Some(format!("Could not open document: {error}")),
            },
            Next::Import(imported) => {
                self.changed_document();
                self.close_preview(window, cx);
                self.document = Document::default();
                self.import_source = Some(imported.source.clone());
                self.import_warning = imported.warning;
                images::install(&self.editor, self.save_directory(), cx);
                self.editor
                    .update(cx, |editor, cx| editor.set_text(imported.markdown, cx));
                self.reset_markdown_search(cx);
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
                self.error = None;
                self.preview_message = None;
                if imported.is_pdf {
                    self.open_pdf(imported.source, window, cx);
                } else if imported.is_docx {
                    self.open_docx(imported.source, window, cx);
                } else {
                    self.preview_source = Some(imported.source);
                    self.preview_message =
                        Some("Source preview unavailable for this imported format.".into());
                }
                self.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.update_title(window, cx);
            }
        }
        cx.notify();
    }

    fn changed_document(&mut self) {
        self.document_generation += 1;
        self.import_source = None;
        self.import_warning = None;
    }

    fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.preview_loading = false;
        self.preview_retryable = false;
        self.preview_source = None;
        self.preview_message = None;
        self.pending_preview = None;
        self.cancel_preview_job();
        self.pdf_subscription = None;
        if let Some(pdf) = self.pdf.take() {
            pdf.update(cx, |pdf, cx| pdf.release(window, cx));
        }
        self.docx_preview = None;
        self.comment_panel = None;
    }

    fn cancel_preview_job(&mut self) {
        if let Some(cancel) = self.preview_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn import(&mut self, _: &Import, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting || self.importing {
            return;
        }
        self.prompting = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import as Markdown — PDF, Office, OpenDocument, RTF, EPUB, CSV".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(paths))) = result
                    && let Some(path) = paths.into_iter().next()
                {
                    this.start_import(path, window, cx);
                }
            });
        })
        .detach();
    }

    fn start_import(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing {
            return;
        }
        self.importing = true;
        self.error = None;
        let generation = self.document_generation;
        let task = cx
            .background_executor()
            .spawn(async move { import::convert(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending_import = Some((generation, result));
                this.resume_import(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn resume_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompting {
            return;
        }
        if let Some((generation, result)) = self.pending_import.take() {
            self.importing = false;
            if generation == self.document_generation {
                match result {
                    Ok(imported) => self.request(Next::Import(imported), window, cx),
                    Err(error) => self.error = Some(error),
                }
            }
            cx.notify();
        }
    }

    fn save_directory(&self) -> PathBuf {
        if self.document.path.is_none()
            && let Some(parent) = self.import_source.as_ref().and_then(|p| p.parent())
        {
            return parent.to_path_buf();
        }
        self.document.directory()
    }

    fn begin_preview(&mut self, path: PathBuf) -> u64 {
        self.cancel_preview_job();
        self.pending_preview = None;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.preview_message = None;
        self.preview_retryable = false;
        self.preview_source = Some(path);
        self.error = None;
        self.preview_loading = true;
        self.preview_generation
    }

    fn load_preview(
        &mut self,
        path: PathBuf,
        docx: Option<docx_preview::DocxPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = self.theme.clone();
        let fit_docx = docx.is_some();
        let pdf = cx.new(|cx| {
            PdfView::new(
                path,
                Rc::new(move || theme.get().pdf_style()),
                Rc::new(|| 1.0),
                cx,
            )
        });
        // DOCX previews always live in the side pane. Fit them to that pane before
        // the first page raster is requested; this avoids rendering an 820 px page
        // bitmap that would immediately be scaled down to roughly half that width.
        if fit_docx {
            pdf.update(cx, |pdf, cx| pdf.fit_width(cx));
        }
        let generation = self.preview_generation;
        let subscription = cx.subscribe_in(&pdf, window, move |this, pdf, _, window, cx| {
            if generation != this.preview_generation {
                return;
            }
            if let Some(error) = pdf.read(cx).load_error() {
                this.preview_message = Some(error.to_string());
            } else if pdf.read(cx).is_locked() {
                this.preview_message = Some(
                    "This PDF is password-protected. Open an unlocked copy to view it here.".into(),
                );
            } else if !pdf.read(cx).is_loaded() {
                return;
            }
            this.preview_loading = false;
            this.preview_cancel = None;
            if this.preview_message.is_some() {
                this.preview_retryable = true;
                this.pending_preview = None;
            } else if let Some(pending) = this.pending_preview.take() {
                this.pdf_subscription = None;
                if let Some(old) = this.pdf.take() {
                    old.update(cx, |pdf, cx| pdf.release(window, cx));
                }
                this.preview_message = pending
                    .docx
                    .as_ref()
                    .and_then(|d| (!d.warnings.is_empty()).then(|| d.warnings.join(" ")));
                this.comment_panel = pending
                    .docx
                    .as_ref()
                    .filter(|d| !d.comments.is_empty())
                    .map(|d| {
                        cx.new(|_| {
                            comment_panel::CommentPanel::new(d.comments.clone(), this.theme.clone())
                        })
                    });
                this.docx_preview = pending.docx;
                this.pdf_subscription = Some(cx.observe(&pending.pdf, |_, _, cx| cx.notify()));
                this.pdf = Some(pending.pdf);
            }
            cx.notify();
        });
        self.pending_preview = Some(PendingPreview {
            pdf,
            docx,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn open_pdf(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_preview(path.clone());
        self.load_preview(path, None, window, cx);
    }

    fn open_docx(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.begin_preview(path.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.preview_cancel = Some(cancel.clone());
        let task = cx
            .background_executor()
            .spawn(async move { docx_preview::render_with_cancel(&path, cancel) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if generation != this.preview_generation {
                    return;
                }
                match result {
                    Ok(preview) => {
                        this.load_preview(preview.pdf_path.clone(), Some(preview), window, cx)
                    }
                    Err(error) => {
                        this.preview_cancel = None;
                        this.preview_loading = false;
                        this.preview_retryable = true;
                        this.preview_message = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn retry_preview(&mut self, _: &RetryPreview, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_retryable
            && let Some(path) = self.preview_source.clone()
        {
            self.open_path(path, window, cx);
        }
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
        } else {
            self.error =
                Some("Choose a Markdown (.md, .markdown, .mdown, .txt), PDF, or DOCX file.".into());
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
            multiple: false,
            prompt: Some("Open Markdown, PDF, or DOCX".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if let Ok(Ok(Some(paths))) = result
                    && let Some(path) = paths.into_iter().next()
                {
                    this.open_path(path, window, cx);
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
        if self.prompting {
            return;
        }
        if !save_as && let Some(path) = self.document.path.clone() {
            self.write(path, next, window, cx);
            return;
        }
        self.prompting = true;
        let suggested = self.import_source.as_ref().map(|p| p.with_extension("md"));
        let name = suggested
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled.md");
        let path = cx.prompt_for_new_path(&self.save_directory(), Some(name));
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
                        cx.notify();
                        this.resume_import(window, cx);
                        return;
                    }
                    this.write(path, next, window, cx);
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
        if let Some(source) = &self.import_source
            && (std::path::absolute(&path).ok().as_ref() == Some(source)
                || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok()))
        {
            self.error = Some("Choose a different path to preserve the imported source.".into());
            cx.notify();
            return;
        }
        let old_directory = self.save_directory();
        match self.document.save(path, self.editor.read(cx).text()) {
            Ok(()) => {
                if old_directory != self.document.directory() {
                    images::install(&self.editor, self.document.directory(), cx);
                }
                self.error = None;
                self.update_title(window, cx);
                if let Some(next) = next {
                    self.proceed(next, window, cx);
                }
            }
            Err(error) => self.error = Some(format!("Could not save: {error}")),
        }
        cx.notify();
    }
}

fn search_match_start(search_match: &SearchMatch) -> Option<usize> {
    search_match.source.first().map(|range| range.start)
}

fn first_search_match_at_or_after(matches: &[SearchMatch], offset: usize) -> Option<usize> {
    matches.iter().position(|search_match| {
        search_match_start(search_match).is_some_and(|start| start >= offset)
    })
}

/// Map an old source offset through the smallest changed middle region. This
/// keeps the active occurrence stable across ordinary typing while remaining
/// UTF-8 safe at the common-prefix/common-suffix boundaries.
fn map_edit_offset(old: &str, new: &str, offset: usize) -> usize {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();
    let mut prefix = 0;
    while prefix < old_bytes.len()
        && prefix < new_bytes.len()
        && old_bytes[prefix] == new_bytes[prefix]
    {
        prefix += 1;
    }
    while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    let mut suffix = 0;
    while suffix < old_bytes.len().saturating_sub(prefix)
        && suffix < new_bytes.len().saturating_sub(prefix)
        && old_bytes[old_bytes.len() - 1 - suffix] == new_bytes[new_bytes.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }

    let offset = offset.min(old.len());
    if offset < prefix {
        return offset.min(new.len());
    }
    let old_changed_end = old.len().saturating_sub(suffix);
    if offset >= old_changed_end {
        return new.len().saturating_sub(old.len().saturating_sub(offset));
    }
    prefix.min(new.len())
}

fn button(label: &'static str, action: impl gpui::Action, theme: Theme) -> impl IntoElement {
    div()
        .id(label)
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
        } else if self.markdown_search_task.is_some() {
            "Searching…".to_owned()
        } else if self.markdown_search_matches.is_empty() {
            "0 matches".to_owned()
        } else {
            format!(
                "{} / {}",
                self.markdown_search_active.map_or(0, |index| index + 1),
                self.markdown_search_matches.len()
            )
        };
        let navigation_disabled = self.markdown_search_matches.is_empty();
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
                    .aria_label(if self.markdown_search_match_case {
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
                    .when(self.markdown_search_match_case, |view| {
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
        div().size_full().flex().flex_col().bg(palette.bg).text_color(palette.header_fg).text_size(px(16.))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::import))
            .on_action(cx.listener(|this, _: &DismissImportWarning, _, cx| { this.import_warning = None; cx.notify(); }))
            .on_action(cx.listener(|this, _: &New, window, cx| this.request(Next::New, window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, None, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, None, window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| this.request(Next::Close, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePdf, window, cx| { this.close_preview(window, cx); window.focus(&this.editor.read(cx).focus_handle(cx), cx); cx.notify(); }))
            .on_action(cx.listener(Self::find_markdown))
            .on_action(cx.listener(Self::find_next_markdown))
            .on_action(cx.listener(Self::find_previous_markdown))
            .on_action(cx.listener(Self::close_markdown_search))
            .on_action(cx.listener(Self::retry_preview))
            .child(div().flex().items_center().gap_2().p_2().text_size(px(13.)).border_b_1().border_color(palette.border)
                .child(button("New", New, theme)).child(button("Open…", Open, theme)).child(button("Save", Save, theme)).child(button("Save As…", SaveAs, theme))
                .child(if self.importing { div().child("Converting…").into_any_element() } else { button("Import as Markdown…", Import, theme).into_any_element() })
                .child(div().flex_1())
                .child(button(theme.toggle_label(), ToggleTheme, theme))
                .child(if self.dirty(cx) { "Unsaved changes" } else { "Markdown · WYSIWYG" })
                .when(self.preview_loading, |bar| bar.child(div().child("Preparing preview…")))
                .when(self.pdf.is_some() || self.preview_source.is_some(), |bar| bar.child(button("Close Preview", ClosePdf, theme))))
            .when_some(self.import_warning.clone(), |view, warning| view.child(div().flex().items_center().gap_2().p_2().border_b_1().border_color(palette.border)
                .child(div().id("import-warning").flex_1().min_w_0().max_h(px(96.)).overflow_y_scroll().child(warning))
                .child(button("Dismiss", DismissImportWarning, theme))))
            .child(div().flex().flex_1().min_h_0()
                .child(div().flex().flex_1().min_w_0().h_full().flex().flex_col()
                    .when(self.markdown_search_open, |column| column.child(search_bar))
                    .child(div().id("document-scroll").flex_1().min_w_0().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).p_6().child(self.editor.clone())))
                .when_some(self.pdf.clone(), |row, pdf| row.child(div().w_1_2().min_w_0().h_full().flex().flex_col().border_l_1().border_color(palette.border)
                    .child(div().flex_1().min_h_0().child(if pdf.read(cx).is_locked() { div().p_6().child("This PDF is password-protected. Open an unlocked copy to view it here.").into_any_element() } else { pdf.into_any_element() }))
                    .when_some(self.comment_panel.clone(), |pane, comments| pane.child(comments)))))
            .when_some(self.preview_message.clone(), |view, message| view.child(div().flex().items_center().gap_2().p_2().bg(theme.error_bg()).child(div().flex_1().child(format!("{}: {message}", self.preview_source.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Preview".into())))).when(self.preview_retryable, |bar| bar.child(button("Retry", RetryPreview, theme)))))
            .when_some(self.error.clone(), |view, error| view.child(div().p_2().bg(theme.error_bg()).child(error)))
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
    let initial = args.get(1).cloned().map(PathBuf::from);
    let application = gpui_platform::application();
    let open_context = Rc::new(RefCell::new(
        None::<(gpui::WindowHandle<Workspace>, gpui::AsyncApp)>,
    ));
    let pending_url = Rc::new(RefCell::new(None::<PathBuf>));
    application.on_open_urls({
        let open_context = open_context.clone();
        let pending_url = pending_url.clone();
        move |urls| {
            // This app has one document window. The last file in an OS open
            // request follows the same latest-preview-wins policy as Open.
            if let Some(path) = urls
                .iter()
                .filter_map(|url| url::Url::parse(url).ok()?.to_file_path().ok())
                .next_back()
            {
                if let Some((window, cx)) = open_context.borrow_mut().as_mut() {
                    let _ = window.update(cx, |workspace, window, cx| {
                        workspace.open_path(path, window, cx)
                    });
                } else {
                    *pending_url.borrow_mut() = Some(path);
                }
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
            KeyBinding::new(&format!("{modifier}-q"), Close, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::action("Import as Markdown…", Import),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::separator(),
                MenuItem::action("Quit", Close),
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
                        let mut workspace = Workspace::new(window, cx);
                        workspace.update_title(window, cx);
                        if let Some(path) = initial {
                            workspace.open_path(path, window, cx);
                        }
                        workspace
                    })
                },
            )
            .expect("Could not open the editor window");
        if let Some(path) = pending_url.borrow_mut().take() {
            let _ = handle.update(cx, |workspace, window, cx| {
                workspace.open_path(path, window, cx)
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
