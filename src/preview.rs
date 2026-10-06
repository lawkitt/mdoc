//! Loaded preview and pending replacement have independent lifetimes.
//! Failure preserves the displayed preview; committing releases old PDF data
//! before deleting its temporary DOCX backing file.
use super::*;

struct PendingPreview {
    pdf: Entity<PdfView>,
    docx: Option<docx_preview::DocxPreview>,
    _subscription: Subscription,
}

#[derive(Default)]
pub(super) struct PreviewState {
    pub split_ratio: Option<f32>,
    pub visible: bool,
    pub queued: bool,
    /// Committed attachment; source is the current request/retry target.
    pub attachment: Option<PathBuf>,
    pub restore_position: Option<(usize, f32, session_store::PreviewFit)>,
    pub pdf: Option<Entity<PdfView>>,
    pub docx: Option<docx_preview::DocxPreview>,
    pub comment_panel: Option<Entity<comment_panel::CommentPanel>>,
    pending: Option<PendingPreview>,
    pub message: Option<String>,
    pub loading: bool,
    pub retryable: bool,
    pub source: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    pub generation: u64,
    subscription: Option<Subscription>,
}

impl PreviewState {
    fn cancel_job(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn invalidate(&mut self) {
        self.cancel_job();
        self.pending = None;
        self.generation = self.generation.wrapping_add(1);
        self.message = None;
        self.retryable = false;
        self.queued = false;
    }

    fn begin(&mut self, path: PathBuf) -> u64 {
        if self.attachment.as_ref() != Some(&path) {
            self.restore_position = None;
        }
        self.invalidate();
        self.source = Some(path);
        self.loading = true;
        self.visible = true;
        self.generation
    }

    fn is_current(&self, generation: u64) -> bool {
        self.loading && self.generation == generation
    }

    fn fail(&mut self, generation: u64, message: String) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.cancel_job();
        self.pending = None;
        self.loading = false;
        self.queued = false;
        self.retryable = true;
        self.message = Some(message);
        true
    }

    fn accept(
        &mut self,
        generation: u64,
        theme: Rc<Cell<Theme>>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        let Some(pending) = self.pending.take() else {
            return false;
        };
        self.release_loaded(window, cx);
        self.cancel = None;
        self.loading = false;
        self.queued = false;
        self.attachment = self.source.clone();
        self.retryable = false;
        self.message = pending
            .docx
            .as_ref()
            .and_then(|d| (!d.warnings.is_empty()).then(|| d.warnings.join(" ")));
        self.comment_panel = pending
            .docx
            .as_ref()
            .filter(|d| !d.comments.is_empty())
            .map(|d| cx.new(|_| comment_panel::CommentPanel::new(d.comments.clone(), theme)));
        self.docx = pending.docx;
        if let Some(source) = self.source.as_ref() {
            let name = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            pending.pdf.update(cx, |pdf, cx| {
                pdf.set_display_name(format!("Original · {name}"), cx)
            });
        }
        self.subscription = Some(cx.observe(&pending.pdf, |_, _, cx| cx.notify()));
        if let Some((page, zoom, fit)) = self.restore_position.take() {
            pending.pdf.update(cx, |pdf, cx| {
                pdf.set_zoom(zoom, cx);
                match fit {
                    session_store::PreviewFit::Width => pdf.fit_width(cx),
                    session_store::PreviewFit::Page => pdf.fit_page(cx),
                    session_store::PreviewFit::Manual => {}
                }
                pdf.go_to_page(page, cx);
            });
        }
        self.pdf = Some(pending.pdf);
        true
    }

    fn release_loaded(&mut self, window: &mut Window, cx: &mut App) {
        self.subscription = None;
        if let Some(pdf) = self.pdf.take() {
            pdf.update(cx, |pdf, cx| pdf.release(window, cx));
        }
        self.docx = None;
        self.comment_panel = None;
    }

    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.invalidate();
        self.loading = false;
        self.source = None;
        self.attachment = None;
        self.visible = false;
        self.queued = false;
        self.release_loaded(window, cx);
    }
}

impl Drop for PreviewState {
    fn drop(&mut self) {
        self.cancel_job();
    }
}

impl Workspace {
    pub(super) fn toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.visible = !self.preview.visible;
        if !self.preview.visible && self.active && !self.loading && !self.unavailable {
            if self.source_only {
                window.focus(&self.focus, cx);
            } else {
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            }
        }
        if self.preview.visible
            && self.preview.pdf.is_none()
            && !self.preview.loading
            && let Some(path) = self
                .preview
                .attachment
                .clone()
                .or(self.preview.source.clone())
        {
            self.open_path(path, window, cx);
        }
        cx.notify();
    }

    pub(super) fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.close(window, cx);
    }

    pub(super) fn begin_preview(&mut self, path: PathBuf) -> u64 {
        self.error = None;
        self.preview.begin(path)
    }

    pub(super) fn load_preview(
        &mut self,
        path: PathBuf,
        docx: Option<docx_preview::DocxPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = self.theme.clone();
        let pdf = cx.new(|cx| {
            PdfView::new(
                path,
                Rc::new(move || theme.get().pdf_style()),
                Rc::new(|| 1.0),
                cx,
            )
        });
        // Both source PDFs and converted DOCX previews follow the pane width.
        // Sticky fit responds to sidebar/window resizing until manually zoomed.
        pdf.update(cx, |pdf, cx| pdf.fit_width(cx));
        let generation = self.preview.generation;
        let subscription = cx.subscribe_in(&pdf, window, move |this, pdf, _, window, cx| {
            if !this.preview.is_current(generation) {
                return;
            }
            if let Some(error) = pdf.read(cx).load_error() {
                this.preview.fail(generation, error.to_string());
            } else if pdf.read(cx).is_locked() {
                this.preview.fail(
                    generation,
                    "This PDF is password-protected. Open an unlocked copy to view it here.".into(),
                );
            } else if pdf.read(cx).is_loaded() {
                this.preview
                    .accept(generation, this.theme.clone(), window, cx);
            } else {
                return;
            }
            this.dirty_cached = this.dirty(cx);
            this.update_title(window, cx);
            cx.notify();
        });
        self.preview.pending = Some(PendingPreview {
            pdf,
            docx,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn open_pdf(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_preview(path.clone());
        self.load_preview(path, None, window, cx);
    }

    pub(super) fn open_docx(
        &mut self,
        path: PathBuf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self.begin_preview(path.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.preview.cancel = Some(cancel.clone());
        self.preview.queued = true;
        cx.emit(tabs::TabEvent::Docx {
            path,
            generation,
            cancel,
        });
        cx.notify();
    }

    pub(super) fn run_docx(
        &mut self,
        path: PathBuf,
        generation: u64,
        cancel: Arc<AtomicBool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.queued = false;
        let owner = self.owner.clone();
        let task = cx
            .background_executor()
            .spawn(async move { docx_preview::render_with_cancel(&path, cancel) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let (id, owner) = owner;
            let _ = owner.update_in(cx, |owner, window, cx| {
                owner.docx_finished(id, generation, window, cx)
            });
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.preview.is_current(generation) {
                    return;
                }
                match result {
                    Ok(preview) => {
                        this.load_preview(preview.pdf_path.clone(), Some(preview), window, cx)
                    }
                    Err(error) => {
                        this.preview.fail(generation, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn retry_preview(
        &mut self,
        _: &RetryPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview.retryable
            && let Some(path) = self.preview.source.clone()
        {
            self.open_path(path, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_request_cancels_old_job_and_rejects_stale_failure() {
        let mut preview = PreviewState::default();
        let old = preview.begin("old.docx".into());
        let cancel = Arc::new(AtomicBool::new(false));
        preview.cancel = Some(cancel.clone());
        let current = preview.begin("new.docx".into());
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!preview.fail(old, "stale".into()));
        assert!(preview.is_current(current));
        assert!(preview.message.is_none());
        assert_eq!(preview.source, Some("new.docx".into()));
        assert!(preview.fail(current, "retry me".into()));
        assert!(preview.retryable);
        assert!(!preview.loading);
        assert!(!preview.fail(current, "duplicate".into()));
        assert_eq!(preview.message.as_deref(), Some("retry me"));
    }

    #[test]
    fn dropping_preview_cancels_conversion() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut preview = PreviewState::default();
        preview.cancel = Some(cancel.clone());
        drop(preview);
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[gpui::test]
    fn stale_accept_cannot_consume_newer_pending_preview(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::ui_tests::boot(cx);
        app.update_in(cx, |app, window, cx| {
            let old = app.begin_preview("old.pdf".into());
            let current = app.begin_preview("new.pdf".into());
            app.load_preview("new.pdf".into(), None, window, cx);
            let pending = app.preview.pending.as_ref().unwrap().pdf.entity_id();
            assert!(!app.preview.accept(old, app.theme.clone(), window, cx));
            assert_eq!(
                app.preview.pending.as_ref().unwrap().pdf.entity_id(),
                pending
            );
            assert!(app.preview.is_current(current));
            app.close_preview(window, cx);
            assert!(!app.preview.accept(current, app.theme.clone(), window, cx));
            assert!(app.preview.pending.is_none());
        });
    }
}
