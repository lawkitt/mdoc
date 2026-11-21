//! Loaded preview and pending replacement have independent lifetimes.
//! Failure preserves the displayed preview; committing releases old PDF data
//! before deleting its temporary DOCX backing file.
use crate::{
    comment_rail::CommentRail,
    document_view::{Background, DocumentView, PasswordPrompt},
    markdown_search,
    style::Theme,
    workspace,
};
use gpui::{App, Context, Entity, Focusable, Subscription, Window, actions, prelude::*};
use gpui_pdf::{HeaderToggle, PdfSource, PdfView, ViewOptions};
use mdoc_import::{
    comments::Comment,
    docx_preview::{self, PreviewError},
    import::{self, Source, SourceStamp},
};
use mdoc_store::{session_store, settings};
use pdf_inspector::PdfAnnotation;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
actions!(mdoc, [RetryPreview]);

/// DOCX previews are rendered locally and only approximate Word's layout;
/// the Markdown is converted from the file itself.
pub(crate) const DOCX_NOTE: &str = "Approximate preview: the layout may differ from Word. The Markdown is converted from the file itself.";

/// The DOCX worker's reader for the comment marks in the PDF it rendered
/// (`docx_preview::MarkReader`): the anchors' rectangles, read through gpui-pdf.
pub(crate) fn read_comment_marks(pdf: &Path) -> Result<Vec<docx_preview::LinkMark>, String> {
    let bytes = std::fs::read(pdf).map_err(|e| e.to_string())?;
    let doc = gpui_pdf::parse(Arc::new(bytes)).map_err(|e| format!("{e:?}"))?;
    Ok(gpui_pdf::link_marks(&doc)
        .into_iter()
        .map(|m| docx_preview::LinkMark {
            page: m.page,
            rect: docx_preview::AnchorRect {
                x: m.x,
                y: m.y,
                w: m.w,
                h: m.h,
            },
            target: m.target,
        })
        .collect())
}

/// Give the preview's find bar a text input, so typing goes through the
/// text-input path and IME works.
pub(crate) fn attach_search_field(
    pdf: &mut PdfView,
    cx: &mut Context<PdfView>,
) -> Entity<markdown_search::SearchInput> {
    let accent = pdf.focus_accent();
    let field = cx.new(|cx| {
        markdown_search::SearchInput::new(cx)
            .with_focus_accent(accent)
            .with_key_context("PreviewSearch")
            .with_placeholder("Find…")
            .with_label("Find in preview")
    });
    let focus = field.read(cx).focus_handle(cx);
    pdf.set_search_field(field.clone(), focus, |field| field.value().to_owned(), cx);
    field
}

/// What an import read from a source's comments or annotations, and when.
#[derive(Clone)]
struct ImportedComments {
    source: PathBuf,
    stamp: SourceStamp,
    comments: Arc<Vec<Comment>>,
    annotations: Arc<Vec<PdfAnnotation>>,
    /// A DOCX's bytes as its import read them, within the preview's limit:
    /// its preview renders these, not the file.
    docx: Option<Source>,
}

impl ImportedComments {
    fn unchanged(&self) -> bool {
        SourceStamp::of(&self.source) == Some(self.stamp)
    }

    /// The comments, and a DOCX's bytes, while the file is still the one
    /// the import read; a changed file's cards come from its own reading,
    /// as its anchors do.
    fn current(self) -> Option<(Arc<Vec<Comment>>, Option<Source>)> {
        self.unchanged().then_some((self.comments, self.docx))
    }

    /// A PDF's annotations, likewise.
    fn current_annotations(self) -> Option<Arc<Vec<PdfAnnotation>>> {
        self.unchanged().then_some(self.annotations)
    }
}

struct PendingPreview {
    pdf: Entity<PdfView>,
    docx: Option<docx_preview::DocxPreview>,
    _subscription: Subscription,
}

/// The preview pane's message when the source can't be read, for example
/// because it moved; Retry can succeed once it is back.
pub(super) const UNREADABLE_SOURCE: &str =
    "mdoc could not read this file. Check that it is still there.";

/// The preview pane's message for a format it can't show.
pub(super) const NO_PREVIEW: &str = "No preview for this format. mdoc previews PDF and DOCX files.";

#[derive(Default)]
pub(super) struct PreviewState {
    split_ratio: Option<f32>,
    visible: bool,
    queued: bool,
    /// Committed attachment; source is the current request/retry target.
    attachment: Option<PathBuf>,
    restore_position: Option<(usize, f32, session_store::PreviewFit)>,
    pdf: Option<Entity<PdfView>>,
    docx: Option<docx_preview::DocxPreview>,
    /// The DOCX's comments or the PDF's annotations beside its page, when
    /// it has any.
    comments: Option<Entity<CommentRail>>,
    /// The comments this tab's import read from its source, with the
    /// source's stamp then, so the cards show what the left-out line
    /// counted; a Source shown without an import here, or changed since,
    /// reads its own.
    imported_comments: Option<ImportedComments>,
    /// The shown PDF's annotations that carry text: the import's while the
    /// file is unchanged, else its own reading.
    annotations: Option<(PathBuf, Arc<Vec<PdfAnnotation>>)>,
    /// Reads `annotations` beside the page's own load.
    annotation_read: Option<gpui::Task<()>>,
    /// Reads a locked PDF's bytes for its page, which opens them in memory
    /// with the tab's password.
    locked_read: Option<gpui::Task<()>>,
    /// The shown PDF is locked and the tab has no password that opens it:
    /// the Original asks on its own card (#1024 decision 6).
    locked: Option<PasswordPrompt>,
    /// A password typed on the Original's card, tried by the current load:
    /// the tab keeps it once the page opens with it.
    trial: Option<import::Password>,
    /// A PDF whose annotations could not be read: its page keeps its own
    /// drawing of them, since no rail shows them.
    /// Whether the read had a password: a locked PDF read without one
    /// reads again once the tab has its password.
    unread_annotations: Option<(PathBuf, bool)>,
    pending: Option<PendingPreview>,
    /// The current request shows the source as a PDF, whatever its name
    /// (an import found a PDF in a `.docx`): its annotations' rail shows
    /// its comments.
    pdf_source: bool,
    message: Option<String>,
    loading: bool,
    retryable: bool,
    /// The last request failed. Unlike `retryable`, this includes failures
    /// another attempt can't fix; the sidebar's warning follows it.
    failed: bool,
    source: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    generation: u64,
    subscription: Option<Subscription>,
}

impl PreviewState {
    pub(super) fn split_ratio(&self) -> Option<f32> {
        self.split_ratio
    }

    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    /// A DOCX job waits for the preview worker.
    pub(super) fn queued(&self) -> bool {
        self.queued
    }

    /// The committed attachment; `source` is the current request.
    pub(super) fn attachment(&self) -> Option<&PathBuf> {
        self.attachment.as_ref()
    }

    pub(super) fn source(&self) -> Option<&PathBuf> {
        self.source.as_ref()
    }

    pub(super) fn pdf(&self) -> Option<&Entity<PdfView>> {
        self.pdf.as_ref()
    }

    #[cfg(test)]
    pub(super) fn docx(&self) -> Option<&docx_preview::DocxPreview> {
        self.docx.as_ref()
    }

    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn comment_rail(&self) -> Option<&Entity<CommentRail>> {
        self.comments.as_ref()
    }

    /// The comments show beside the page now: the "comments" F6 region.
    pub(super) fn comments_shown(&self, cx: &App) -> bool {
        self.comments
            .as_ref()
            .is_some_and(|rail| rail.read(cx).shown())
    }

    /// A rendering of the source shows its comments: a DOCX or a PDF whose
    /// preview has not failed.
    pub(super) fn renders_comments(&self) -> bool {
        !self.failed
            && (self.pdf_source
                || self
                    .source
                    .as_ref()
                    .and_then(|p| p.extension())
                    .is_some_and(|e| {
                        e.eq_ignore_ascii_case("docx") || e.eq_ignore_ascii_case("pdf")
                    }))
    }

    /// An import of `source` read `comments` (office documents) or
    /// `annotations` (PDF): its rendering's cards while the file is
    /// unchanged, and a DOCX's page too.
    pub(super) fn imported(
        &mut self,
        source: &Source,
        comments: Vec<Comment>,
        annotations: Vec<PdfAnnotation>,
    ) {
        if self
            .unread_annotations
            .as_ref()
            .is_some_and(|(path, _)| *path == source.path)
        {
            self.unread_annotations = None;
        }
        // Past its limit the preview refuses the file, so none are kept;
        // a locked file's unlocked bytes stay with the tab's unlock, their
        // one holder.
        let docx = (source
            .path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
            && !source.unlocked()
            && source.bytes().len() as u64 <= docx_preview::MAX_INPUT_BYTES)
            .then(|| source.clone());
        self.imported_comments = source.stamp.map(|stamp| ImportedComments {
            source: source.path.clone(),
            stamp,
            comments: Arc::new(comments),
            annotations: Arc::new(annotations),
            docx,
        });
    }

    /// The PDF shown has other annotations than an import of `source`
    /// read: the file changed since it loaded.
    pub(super) fn shows_other_annotations(
        &self,
        source: &Path,
        annotations: &[PdfAnnotation],
    ) -> bool {
        self.pdf_source
            && self
                .annotations
                .as_ref()
                .is_some_and(|(path, shown)| path == source && shown.as_slice() != annotations)
    }

    /// A PDF source's annotations are in: the rail beside its pages once
    /// they have loaded too. True when the rail appeared.
    pub(super) fn attach_pdf_comments(
        &mut self,
        theme: Rc<Cell<Theme>>,
        region: gpui::FocusHandle,
        window: &mut Window,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        if self.comments.is_some() || self.docx.is_some() {
            return false;
        }
        let (Some(pdf), Some(source)) = (self.pdf.as_ref(), self.attachment.as_ref()) else {
            return false;
        };
        let Some(annotations) = self
            .annotations
            .as_ref()
            .filter(|(path, annotations)| path == source && !annotations.is_empty())
            .map(|(_, annotations)| annotations.clone())
        else {
            return false;
        };
        let frames = pdf.read(cx).page_frames();
        let (comments, anchors) = crate::pdf_comments::rail_input(&annotations, &frames);
        let pdf = pdf.clone();
        self.comments = Some(
            cx.new(|cx| CommentRail::new(&comments, &anchors, &pdf, theme, region, window, cx)),
        );
        true
    }

    /// The comments an import here read from `source`, if any.
    fn imported_comments(&self, source: &Path) -> Option<ImportedComments> {
        self.imported_comments
            .as_ref()
            .filter(|known| known.source == source)
            .cloned()
    }

    /// Passages of hidden text in the DOCX shown.
    fn hidden_text(&self) -> usize {
        self.docx.as_ref().map_or(0, |docx| docx.hidden_text)
    }

    /// The pane's one plain message, if any.
    pub(super) fn message(&self) -> Option<&String> {
        self.message.as_ref()
    }

    pub(super) fn loading(&self) -> bool {
        self.loading
    }

    /// Another attempt can succeed, so Retry shows.
    pub(super) fn retryable(&self) -> bool {
        self.retryable
    }

    /// The last request failed; the sidebar's warning follows it.
    pub(super) fn failed(&self) -> bool {
        self.failed
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// A tab's preview as its session record saved it.
    pub(super) fn restore(
        &mut self,
        attachment: Option<PathBuf>,
        visible: bool,
        split_ratio: Option<f32>,
        position: Option<(usize, f32, session_store::PreviewFit)>,
    ) {
        self.source = attachment.clone();
        self.attachment = attachment;
        self.visible = visible;
        self.split_ratio = split_ratio;
        self.restore_position = position;
    }

    /// Show `path` as the tab's source before anything loads it.
    pub(super) fn show_source(&mut self, path: PathBuf) {
        self.source = Some(path.clone());
        self.attachment = Some(path);
        self.visible = true;
    }

    /// `path` has no preview: the pane says so.
    pub(super) fn no_preview(&mut self, path: PathBuf) {
        self.attachment = Some(path.clone());
        self.source = Some(path);
        self.message = Some(NO_PREVIEW.into());
    }

    /// A locked DOCX this session unlocked: no preview renders it, since
    /// that would write the unlocked document to a temporary file. It reads
    /// as the locked file's own preview failure, so the left-out line
    /// counts its comments.
    pub(super) fn unlocked_docx(&mut self, path: PathBuf) {
        self.attachment = Some(path.clone());
        self.source = Some(path);
        self.retryable = false;
        self.failed = true;
        self.message = Some(docx_message(&PreviewError::Encrypted).into());
    }

    /// A source without a preview was checked for `generation`; it is
    /// `readable` or not. False when a newer request replaced it.
    pub(super) fn checked(&mut self, generation: u64, readable: bool) -> bool {
        if self.generation != generation {
            return false;
        }
        self.loading = false;
        self.retryable = !readable;
        self.failed = !readable;
        self.message = Some(
            if readable {
                NO_PREVIEW
            } else {
                UNREADABLE_SOURCE
            }
            .into(),
        );
        true
    }

    pub(super) fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the pane; returns whether it was showing.
    pub(super) fn hide(&mut self) -> bool {
        std::mem::take(&mut self.visible)
    }

    pub(super) fn clear_message(&mut self) {
        self.message = None;
    }

    /// The split between Markdown and the original was dragged.
    pub(super) fn set_split(&mut self, ratio: f32) {
        self.split_ratio = Some(ratio.clamp(0.25, 0.75));
    }

    /// Show `source` as failed with `message`, for renders of failure states.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn show_failure(&mut self, source: PathBuf, message: String) {
        self.show_source(source);
        self.retryable = false;
        self.failed = true;
        self.message = Some(message);
    }

    #[cfg(test)]
    pub(super) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Pretend a request for `source` is loading.
    #[cfg(test)]
    pub(super) fn set_loading(&mut self, source: PathBuf) {
        self.source = Some(source);
        self.loading = true;
    }

    fn cancel_job(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn invalidate(&mut self) {
        self.cancel_job();
        self.annotation_read = None;
        self.locked_read = None;
        self.locked = None;
        self.trial = None;
        self.pending = None;
        self.generation = self.generation.wrapping_add(1);
        self.message = None;
        self.retryable = false;
        self.failed = false;
        self.queued = false;
    }

    fn begin(&mut self, path: PathBuf) -> u64 {
        if self.attachment.as_ref() != Some(&path) {
            self.restore_position = None;
        }
        self.invalidate();
        self.source = Some(path);
        self.pdf_source = false;
        self.loading = true;
        self.visible = true;
        self.generation
    }

    fn is_current(&self, generation: u64) -> bool {
        self.loading && self.generation == generation
    }

    /// The PDF is locked and the tab has no password that opens it: the
    /// Original asks for one with `prompt`, first or after a wrong one.
    fn lock(&mut self, generation: u64, prompt: PasswordPrompt) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.cancel_job();
        self.pending = None;
        self.loading = false;
        self.queued = false;
        self.retryable = false;
        self.failed = false;
        self.message = None;
        self.locked = Some(prompt);
        self.trial = None;
        true
    }

    /// The Original asks for the shown PDF's password.
    pub(super) fn locked(&self) -> Option<PasswordPrompt> {
        self.locked
    }

    /// Ends the current request with one plain `message`; Retry shows only
    /// when `retryable`, that is, when another attempt can succeed.
    fn fail(&mut self, generation: u64, message: String, retryable: bool) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.cancel_job();
        self.pending = None;
        self.loading = false;
        self.queued = false;
        self.retryable = retryable;
        self.failed = true;
        self.message = Some(message);
        // A password the failed load was trying proved nothing.
        self.trial = None;
        true
    }

    fn accept(
        &mut self,
        generation: u64,
        theme: Rc<Cell<Theme>>,
        region: gpui::FocusHandle,
        window: &mut Window,
        cx: &mut Context<DocumentView>,
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
        self.failed = false;
        self.message = pending
            .docx
            .as_ref()
            .and_then(|d| (!d.warnings.is_empty()).then(|| d.warnings.join(" ")));
        self.comments = pending
            .docx
            .as_ref()
            .filter(|d| !d.comments.is_empty())
            .map(|d| {
                cx.new(|cx| {
                    CommentRail::new(
                        &d.comments,
                        &d.anchors,
                        &pending.pdf,
                        theme.clone(),
                        region.clone(),
                        window,
                        cx,
                    )
                })
            });
        self.docx = pending.docx;
        if let Some(source) = self.source.as_ref() {
            let name = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let note = self.docx.is_some().then_some(DOCX_NOTE.into());
            pending.pdf.update(cx, |pdf, cx| {
                pdf.set_display_name(format!("Source · {name}"), cx);
                // An expected limitation, not a notice (README media decision 9).
                pdf.set_source_note(note, cx);
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
        self.attach_pdf_comments(theme, region, window, cx);
        true
    }

    fn release_loaded(&mut self, window: &mut Window, cx: &mut App) {
        self.subscription = None;
        if let Some(pdf) = self.pdf.take() {
            pdf.update(cx, |pdf, cx| pdf.release(window, cx));
        }
        self.docx = None;
        self.comments = None;
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

/// The Source pane's app-wide display choices (Settings' General choices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceDisplay {
    /// "💬 N": comments beside the page.
    pub comments: bool,
    /// "Hidden text (N)": Word's hidden text drawn in place.
    pub hidden_text: bool,
}

impl DocumentView {
    /// The Source header's "💬 N" and "Hidden text (N)", and the rail,
    /// follow Show comments and Show hidden text.
    pub(crate) fn sync_source_controls(&mut self, cx: &mut Context<Self>) {
        let display = self.source_display();
        let Some(pdf) = self.preview.pdf().cloned() else {
            return;
        };
        let comments = self.preview.comments.clone();
        if let Some(rail) = &comments {
            rail.update(cx, |rail, cx| rail.set_shown(display.comments, cx));
        }
        let count = comments.map_or(0, |rail| rail.read(cx).comment_count());
        let hidden = self.preview.hidden_text();
        let owner = cx.entity().downgrade();
        let toggle = move |change: SourceDisplay| {
            let owner = owner.clone();
            Rc::new(move |_: &mut Window, cx: &mut App| {
                let _ = owner.update(cx, |_, cx| {
                    cx.emit(workspace::TabEvent::SourceDisplay(change));
                });
            }) as gpui_pdf::ActionFn
        };
        let mut toggles = Vec::new();
        if count > 0 {
            toggles.push(HeaderToggle {
                id: "source-comments",
                label: format!("💬 {count}").into(),
                description: if display.comments {
                    "Hide comments"
                } else {
                    "Show comments"
                }
                .into(),
                on: display.comments,
                action: toggle(SourceDisplay {
                    comments: !display.comments,
                    ..display
                }),
            });
        }
        if hidden > 0 {
            toggles.push(HeaderToggle {
                id: "source-hidden-text",
                label: format!("Hidden text ({hidden})").into(),
                description: if display.hidden_text {
                    "Hide hidden text"
                } else {
                    "Show hidden text"
                }
                .into(),
                on: display.hidden_text,
                action: toggle(SourceDisplay {
                    hidden_text: !display.hidden_text,
                    ..display
                }),
            });
        }
        pdf.update(cx, |pdf, cx| pdf.set_header_toggles(toggles, cx));
    }

    /// Show comments or Show hidden text changed, here or in another tab:
    /// the rail follows, and a DOCX with hidden text is drawn again in the
    /// same place.
    pub(crate) fn source_display_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_source_controls(cx);
        let display = self.source_display();
        let redraw = self.preview.docx.as_ref().is_some_and(|docx| {
            docx.hidden_text > 0 && docx.options.show_hidden_text != display.hidden_text
        });
        if !redraw || self.preview.loading {
            return;
        }
        if let Some(pdf) = self.preview.pdf.as_ref() {
            let pdf = pdf.read(cx);
            let (page, zoom) = pdf.reading_position();
            let fit = match pdf.fit_mode() {
                Some(gpui_pdf::FitMode::Width) => session_store::PreviewFit::Width,
                Some(gpui_pdf::FitMode::Page) => session_store::PreviewFit::Page,
                None => session_store::PreviewFit::Manual,
            };
            self.preview.restore_position = Some((page, zoom, fit));
        }
        if let Some(path) = self.preview.attachment.clone() {
            self.open_docx(path, window, cx);
        }
    }

    /// Next comment: scroll to it and focus its card.
    pub(super) fn next_comment(
        &mut self,
        _: &crate::comment_rail::NextComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(rail) = self.preview.comments.clone() {
            rail.update(cx, |rail, cx| rail.step(true, window, cx));
        }
    }

    /// Previous comment: scroll to it and focus its card.
    pub(super) fn previous_comment(
        &mut self,
        _: &crate::comment_rail::PreviousComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(rail) = self.preview.comments.clone() {
            rail.update(cx, |rail, cx| rail.step(false, window, cx));
        }
    }

    /// The left-out line: what the Markdown leaves out, comments counted
    /// only while no rendering shows them.
    pub(crate) fn left_out(&self) -> Option<String> {
        let comments = self
            .session
            .comments_left_out
            .clone()
            .filter(|_| !self.preview.renders_comments());
        let lines: Vec<String> = [self.session.left_out.clone(), comments]
            .into_iter()
            .flatten()
            .collect();
        (!lines.is_empty()).then(|| lines.join(" "))
    }

    /// Show comments and Show hidden text, as Settings holds them now.
    pub(crate) fn source_display(&self) -> SourceDisplay {
        let defaults = settings::GeneralConfig::default();
        let general = self
            .preferences
            .borrow()
            .current
            .as_ref()
            .map_or(defaults, |p| p.general);
        SourceDisplay {
            comments: general.show_comments,
            hidden_text: general.show_hidden_text,
        }
    }

    pub(super) fn toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.visible = !self.preview.visible;
        if !self.preview.visible && self.active && self.lifecycle.ready() {
            if self.lifecycle.source_only() {
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
        self.load_preview_source(path.clone(), PdfSource::Path(path), docx, window, cx);
    }

    /// [`Self::load_preview`] drawing `source`, which may be bytes in memory
    /// with a password, for the document at `path`.
    pub(super) fn load_preview_source(
        &mut self,
        path: PathBuf,
        source: PdfSource,
        docx: Option<docx_preview::DocxPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = self.theme.clone();
        let with_password = matches!(
            source,
            PdfSource::Bytes {
                password: Some(_),
                ..
            }
        );
        // The page draws its comments itself only when this load's read of
        // them, with or without its password, failed.
        let hide = self.preview.unread_annotations != Some((path.clone(), with_password));
        let pdf = cx.new(|cx| {
            PdfView::with_options(
                source,
                Rc::new(move || theme.get().pdf_style()),
                Rc::new(|| 1.0),
                // The rail shows a PDF's comment annotations; the page
                // doesn't draw them again, unless they couldn't be read.
                ViewOptions {
                    hide_comment_annotations: hide,
                },
                cx,
            )
        });
        // Both source PDFs and converted DOCX previews follow the pane width.
        // Sticky fit responds to sidebar/window resizing until manually zoomed.
        let activity_theme = self.theme.clone();
        pdf.update(cx, |pdf, cx| {
            pdf.set_loading_indicator(Rc::new(move |label| {
                crate::ui::activity(label.clone(), label, activity_theme.get()).into_any_element()
            }));
            pdf.fit_width(cx);
            attach_search_field(pdf, cx);
        });
        let owner = cx.entity().downgrade();
        pdf.update(cx, |pdf, _| {
            pdf.set_on_close(Rc::new(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    if this.preview.visible {
                        this.toggle_preview(window, cx);
                    }
                });
            }))
        });
        let generation = self.preview.generation;
        let subscription = cx.subscribe_in(&pdf, window, move |this, pdf, _, window, cx| {
            if !this.preview.is_current(generation) {
                return;
            }
            if pdf.read(cx).is_unsupported_encryption() {
                this.preview.fail(
                    generation,
                    "This PDF uses encryption mdoc can’t open, so it can’t be previewed.".into(),
                    false,
                );
            } else if pdf.read(cx).load_error().is_some() {
                let retryable = pdf.read(cx).load_error_retryable();
                let message = if retryable {
                    UNREADABLE_SOURCE
                } else {
                    "This PDF is damaged or incomplete, so it can't be previewed."
                };
                this.preview.fail(generation, message.into(), retryable);
            } else if pdf.read(cx).is_locked()
                && !with_password
                && let Some(password) = this.pdf_password(&path)
            {
                // The tab has a password now that it lacked when this load
                // began (an import proved it meanwhile): load with that.
                this.open_locked_pdf(path.clone(), password, false, window, cx);
                return;
            } else if pdf.read(cx).is_locked() {
                // A password tried here and refused is a wrong one.
                let prompt = if with_password {
                    PasswordPrompt::Wrong
                } else {
                    PasswordPrompt::Asking
                };
                this.preview.lock(generation, prompt);
            } else if pdf.read(cx).is_loaded() {
                // A password typed on the Original's card opened it: the tab
                // keeps it, as it keeps one its import was unlocked with.
                if let Some(password) = this.preview.trial.take() {
                    this.import.keep_password(import::Unlock::Password {
                        path: path.clone(),
                        password,
                    });
                }
                let region = this.comments_region.clone();
                if this
                    .preview
                    .accept(generation, this.theme.clone(), region, window, cx)
                {
                    // A switch made while this rendering ran draws again.
                    this.source_display_changed(window, cx);
                }
            } else {
                return;
            }
            let dirty = this.dirty(cx);
            this.lifecycle.set_dirty(dirty);
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
        self.preview.pdf_source = true;
        let password = self.pdf_password(&path);
        self.read_annotations(path.clone(), password.clone(), window, cx);
        match password {
            Some(password) => self.load_locked_pdf(path, password, window, cx),
            None => self.load_preview(path, None, window, cx),
        }
    }

    /// The tab's password of the locked PDF at `path`, which every call
    /// that opens it takes (#1024 decision 6).
    pub(crate) fn pdf_password(&self, path: &Path) -> Option<import::Password> {
        let unlock = self.import.unlock()?;
        unlock.opens(path).then(|| unlock.password().cloned())?
    }

    /// Unlock on the Original's card: the field's text opens the locked PDF
    /// shown, in memory, and is wiped from the field; the tab keeps it once
    /// the page opens with it.
    pub(crate) fn unlock_original(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .preview
            .source
            .clone()
            .filter(|_| self.preview.locked().is_some())
        else {
            return;
        };
        let password = self
            .password_field
            .update(cx, |field, cx| field.take_secret(cx));
        if password.is_empty() {
            return;
        }
        // Another password: annotations a wrong one couldn't read, it may.
        if self
            .preview
            .unread_annotations
            .as_ref()
            .is_some_and(|(unread, with_password)| *unread == path && *with_password)
        {
            self.preview.unread_annotations = None;
        }
        self.open_locked_pdf(path, import::Password::from(password), true, window, cx);
    }

    /// [`Self::open_pdf`] of the locked PDF at `path` with `password`: the
    /// tab's, or one typed on the Original's card (`trial`), which the tab
    /// keeps once the page opens with it.
    fn open_locked_pdf(
        &mut self,
        path: PathBuf,
        password: import::Password,
        trial: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_preview(path.clone());
        self.preview.pdf_source = true;
        if trial {
            self.preview.trial = Some(password.clone());
        }
        self.read_annotations(path.clone(), Some(password.clone()), window, cx);
        self.load_locked_pdf(path, password, window, cx);
    }

    /// The page of the locked PDF at `path`: its bytes read off-thread, then
    /// opened in memory with `password`. Nothing decrypted is written; the
    /// view decrypts again on each load.
    fn load_locked_pdf(
        &mut self,
        path: PathBuf,
        password: import::Password,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self.preview.generation;
        let read = cx.background_executor().spawn({
            let path = path.clone();
            async move { Source::read(&path) }
        });
        self.preview.locked_read = Some(cx.spawn_in(window, async move |this, cx| {
            let read = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.preview.is_current(generation) {
                    return;
                }
                match read {
                    Ok(source) => {
                        let source = PdfSource::Bytes {
                            bytes: Arc::new(source.shared_bytes()).into(),
                            password: Some(Arc::new(password) as gpui_pdf::Password),
                        };
                        this.load_preview_source(path, source, None, window, cx);
                    }
                    Err(_) => {
                        this.preview
                            .fail(generation, UNREADABLE_SOURCE.into(), true);
                    }
                }
                cx.notify();
            });
        }));
    }

    /// The shown PDF's annotations, beside the page's own load: the
    /// import's while the file is unchanged, else read as the import
    /// would, with the tab's `password` of a locked one. Whichever of the
    /// two ends last brings the rail.
    fn read_annotations(
        &mut self,
        path: PathBuf,
        password: Option<import::Password>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let read_with = password.is_some();
        if self.preview.unread_annotations == Some((path.clone(), read_with)) {
            return;
        }
        let reopen_with = password.clone();
        let known = self.preview.imported_comments(&path);
        let read = cx.background_executor().spawn({
            let path = path.clone();
            async move {
                known
                    .and_then(ImportedComments::current_annotations)
                    .or_else(|| {
                        let source = Source::read(&path).ok()?;
                        import::source_annotations(&source, password.as_ref()).map(Arc::new)
                    })
            }
        });
        self.preview.annotation_read = Some(cx.spawn_in(window, async move |this, cx| {
            let annotations = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.preview.source.as_ref() != Some(&path) {
                    return;
                }
                let Some(annotations) = annotations else {
                    // No rail can show them, so the page draws them
                    // itself: load it again with its own drawing.
                    if let Some(task) = this.preview.annotation_read.take() {
                        task.detach();
                    }
                    this.preview.unread_annotations = Some((path.clone(), read_with));
                    // Again with the password it was read with, so a typed
                    // one still meets the page.
                    match reopen_with {
                        Some(password) => {
                            let trial = this.preview.trial.is_some();
                            this.open_locked_pdf(path, password, trial, window, cx);
                        }
                        None => this.open_pdf(path, window, cx),
                    }
                    return;
                };
                // Read after all: the marker of an earlier failed read goes.
                if this
                    .preview
                    .unread_annotations
                    .as_ref()
                    .is_some_and(|(unread, _)| *unread == path)
                {
                    this.preview.unread_annotations = None;
                }
                this.preview.annotations = Some((path, annotations));
                let (theme, region) = (this.theme.clone(), this.comments_region.clone());
                if this.preview.attach_pdf_comments(theme, region, window, cx) {
                    this.source_display_changed(window, cx);
                }
                cx.notify();
            });
        }));
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
        cx.emit(workspace::TabEvent::Docx(workspace::DocxEvent::Queue {
            path,
            generation,
            cancel,
        }));
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
        let options = docx_preview::RenderOptions {
            show_hidden_text: self.source_display().hidden_text,
        };
        let known = self.preview.imported_comments(&path);
        let unlocked = self.unlocked_source(&path);
        let task = cx.background_executor().spawn(async move {
            let (comments, held) = known.and_then(ImportedComments::current).unzip();
            let source = match held
                .flatten()
                .or_else(|| unlocked.filter(Source::is_current))
            {
                Some(source) => source,
                None => docx_preview::read_source(&path)?,
            };
            // Headless UI tests cannot launch the test harness as a worker.
            if cfg!(test) {
                docx_preview::render_in_process(
                    &source,
                    cancel,
                    read_comment_marks,
                    options,
                    comments,
                )
            } else {
                docx_preview::render_with_cancel(&source, cancel, options, comments)
            }
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            // The worker is free before this preview loads, as the window
            // learns it first.
            let _ = this.update(cx, |_, cx| {
                cx.emit(workspace::TabEvent::Docx(workspace::DocxEvent::Finished {
                    generation,
                }))
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
                        this.preview.fail(
                            generation,
                            docx_message(&error).into(),
                            error.retryable(),
                        );
                    }
                }
                cx.notify();
            });
            Background::Docx { generation }
        });
        self.keep_background(task);
        cx.notify();
    }

    /// The tab's unlocked bytes of the locked file at `path`: a reload or a
    /// redraw of its preview renders these, never asking again (#1024
    /// decision 6). The render refuses them until it runs in memory.
    pub(crate) fn unlocked_source(&self, path: &Path) -> Option<Source> {
        let unlock = self.import.unlock()?;
        match unlock {
            import::Unlock::Unlocked(source) if source.path == path => Some(source.clone()),
            _ => None,
        }
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

/// The one sentence the preview pane shows for a failed DOCX preview. The
/// error's `Display` text keeps the technical detail for tests and diagnostics.
pub(crate) fn docx_message(error: &PreviewError) -> &'static str {
    match error {
        PreviewError::InputTooLarge => {
            "This DOCX is too large to preview. Previews take files up to 50 MB."
        }
        PreviewError::MacrosNotSupported => "DOCX files with macros can't be previewed.",
        // The preview renders to a temporary file, which would hold the
        // unlocked document.
        PreviewError::Encrypted => {
            "Password-protected DOCX files aren’t previewed, so no unlocked copy is written to disk."
        }
        PreviewError::TrackedChangesNotSupported => {
            "DOCX files with tracked changes can't be previewed. Accept or reject the changes in Word first."
        }
        PreviewError::NestedArchive => {
            "This DOCX contains other packed files, so it can't be previewed."
        }
        PreviewError::Package(_) => "This DOCX is damaged or incomplete, so it can't be previewed.",
        PreviewError::Render(_) => "The preview could not be drawn.",
        PreviewError::Rejected => "The preview can't draw this DOCX.",
        PreviewError::Timeout => "The preview took longer than 30 seconds and was stopped.",
        PreviewError::Cancelled => "The preview was cancelled.",
        PreviewError::TemporaryStorage(_) => {
            "mdoc could not create temporary files for the preview."
        }
        PreviewError::Read(_) => "mdoc could not read this file. Check that it is still there.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Importing a locked PDF and drawing its Original (#1024): what
    /// `a_locked_pdf_leaves_the_temporary_folder_empty` runs in a process
    /// of its own, whose temporary folder is an empty one.
    #[test]
    #[ignore = "run by a_locked_pdf_leaves_the_temporary_folder_empty"]
    fn import_and_draw_a_locked_pdf() {
        let path = PathBuf::from(std::env::var_os("MDOC_LOCKED_PDF").expect("the parent's file"));
        let password = import::Password::new("Petrov-2026".into());
        let mut unlock = Some(import::Unlock::Password {
            path: path.clone(),
            password: password.clone(),
        });
        let imported =
            import::prepare_unlocking(&path, &mut unlock, None, true, &AtomicBool::new(false))
                .unwrap();
        assert!(imported.markdown.contains("agreement"));
        assert!(!imported.annotations.is_empty());
        let source = Source::read(&path).unwrap();
        assert!(import::source_annotations(&source, Some(&password)).is_some());
        let bytes: gpui_pdf::PdfData = Arc::new(source.shared_bytes()).into();
        let doc = gpui_pdf::parse_with_password(bytes, Some(password.expose())).unwrap();
        for page in 0..gpui_pdf::page_dims(&doc).len() {
            gpui_pdf::page_luma(&doc, page, 1.0).unwrap();
        }
    }

    /// Nothing imported, decrypted or rendered from a locked PDF reaches
    /// disk (#1024): with its temporary folder pointed at an empty one, a
    /// process imports the PDF with its password, reads its annotations and
    /// draws every page of its Original from memory, and leaves the folder
    /// empty.
    #[test]
    fn a_locked_pdf_leaves_the_temporary_folder_empty() {
        let dir = tempfile::tempdir().unwrap();
        let temporary = dir.path().join("temporary");
        std::fs::create_dir(&temporary).unwrap();
        let documents = dir.path().join("documents");
        std::fs::create_dir(&documents).unwrap();
        let path = documents.join("locked.pdf");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pdf-source/comments-aes256.pdf"),
            &path,
        )
        .unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "preview::tests::import_and_draw_a_locked_pdf",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .env("MDOC_LOCKED_PDF", &path)
            .env("TMP", &temporary)
            .env("TEMP", &temporary)
            .env("TMPDIR", &temporary)
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&child.stdout);
        // It ran, and passed: a renamed test would run nothing and succeed.
        assert!(
            child.status.success() && output.contains("test result: ok. 1 passed"),
            "{output}{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&documents).unwrap().count(), 1);
    }

    #[test]
    fn newer_request_cancels_old_job_and_rejects_stale_failure() {
        let mut preview = PreviewState::default();
        let old = preview.begin("old.docx".into());
        let cancel = Arc::new(AtomicBool::new(false));
        preview.cancel = Some(cancel.clone());
        let current = preview.begin("new.docx".into());
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!preview.fail(old, "stale".into(), true));
        assert!(preview.is_current(current));
        assert!(preview.message.is_none());
        assert_eq!(preview.source, Some("new.docx".into()));
        assert!(preview.fail(current, "retry me".into(), true));
        assert!(preview.retryable);
        assert!(!preview.loading);
        assert!(!preview.fail(current, "duplicate".into(), true));
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

    #[test]
    fn only_a_docx_or_pdf_rendering_that_drew_shows_comments() {
        let mut preview = PreviewState::default();
        assert!(!preview.renders_comments());
        let generation = preview.begin("a.docx".into());
        assert!(preview.renders_comments());
        assert!(preview.fail(generation, "refused".into(), false));
        assert!(!preview.renders_comments());
        let generation = preview.begin("a.PDF".into());
        assert!(preview.renders_comments());
        assert!(preview.fail(generation, "damaged".into(), false));
        assert!(!preview.renders_comments());
        preview.begin("a.odt".into());
        assert!(!preview.renders_comments());
        // A PDF under another name, shown as a PDF.
        preview.pdf_source = true;
        assert!(preview.renders_comments());
    }

    /// A PDF named `.docx` previews as a PDF, whose rail shows its
    /// annotations, so the left-out line leaves them out.
    #[gpui::test]
    fn a_pdf_named_docx_shows_its_annotations(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("contract.docx");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/import/text_annotations.pdf"),
            &source,
        )
        .unwrap();
        let imported = crate::test_support::convert(&source).unwrap();
        assert!(imported.is_pdf);
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update_in(cx, |app, window, cx| {
            app.proceed(
                crate::document_view::Next::Import(Box::new(imported)),
                window,
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert_eq!(app.preview.source(), Some(&source));
            assert!(app.preview.renders_comments());
            assert_eq!(app.left_out(), None);
            let rail = app.preview.comment_rail().expect("the annotations' rail");
            // Eight annotations, one a reply.
            assert_eq!(rail.read(cx).count(), 7);
        });
    }

    /// A PDF shown with other annotations than its import read changed
    /// since it loaded, so the import loads it again.
    #[test]
    fn a_changed_pdf_shows_other_annotations() {
        let mut preview = PreviewState::default();
        let source = PathBuf::from("a.pdf");
        let note = |text: &str| PdfAnnotation {
            page: 1,
            kind: pdf_inspector::AnnotationKind::Text,
            author: None,
            date: None,
            text: text.into(),
            rect: pdf_inspector::AnnotationRect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
            quads: Vec::new(),
            in_reply_to: None,
            resolved: false,
            marked_text: None,
        };
        preview.annotations = Some((source.clone(), Arc::new(vec![note("old")])));
        assert!(
            !preview.shows_other_annotations(&source, &[note("new")]),
            "not a PDF Source"
        );
        preview.pdf_source = true;
        assert!(preview.shows_other_annotations(&source, &[note("new")]));
        assert!(preview.shows_other_annotations(&source, &[]));
        assert!(!preview.shows_other_annotations(&source, &[note("old")]));
        assert!(!preview.shows_other_annotations(Path::new("b.pdf"), &[]));
    }

    /// A PDF whose annotations could not be read keeps its own drawing of
    /// them, and no rail.
    #[gpui::test]
    fn an_unread_pdf_keeps_its_own_annotations(cx: &mut gpui::TestAppContext) {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-source/comments.pdf");
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update_in(cx, |app, window, cx| {
            app.preview.unread_annotations = Some((source.clone(), false));
            app.open_pdf(source.clone(), window, cx);
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            let pdf = app.preview.pdf().expect("the page loads").read(cx);
            assert!(!pdf.options().hide_comment_annotations);
            assert!(app.preview.comment_rail().is_none());
        });
        // Read once its import has: the rail, and the page leaves them out.
        app.update_in(cx, |app, window, cx| {
            let read = Source::read(&source).unwrap();
            let annotations = mdoc_import::import::source_annotations(&read, None).unwrap();
            app.preview.imported(&read, Vec::new(), annotations);
            app.open_pdf(source, window, cx);
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let app = app.read(cx);
            let pdf = app.preview.pdf().unwrap().read(cx);
            assert!(pdf.options().hide_comment_annotations);
            assert_eq!(app.preview.comment_rail().unwrap().read(cx).count(), 8);
        });
    }

    /// The import's comments are a DOCX's cards only while the file is the
    /// one it read; a file saved over since is read again.
    #[test]
    fn imported_comments_hold_while_the_source_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.docx");
        std::fs::write(&source, b"first").unwrap();
        let mut preview = PreviewState::default();
        let comment = Comment {
            id: "0".into(),
            ..Comment::default()
        };
        preview.imported(
            &Source::read(&source).unwrap(),
            vec![comment.clone()],
            Vec::new(),
        );
        assert!(preview.imported_comments(Path::new("other.docx")).is_none());
        let known = preview.imported_comments(&source).unwrap();
        assert_eq!(known.clone().current().unwrap().0.as_slice(), [comment]);
        assert!(known.clone().current_annotations().unwrap().is_empty());
        std::fs::write(&source, b"saved over").unwrap();
        assert!(known.clone().current().is_none());
        assert!(known.current_annotations().is_none());
        // An import whose file it could not stat offers nothing to reuse.
        preview.imported(
            &Source::new(source.clone(), None, Vec::new()),
            Vec::new(),
            Vec::new(),
        );
        assert!(preview.imported_comments(&source).is_none());
    }

    /// The left-out line in the document header: the counts, with the
    /// comments only while no rendering of the source shows them.
    #[gpui::test]
    fn the_left_out_line_counts_comments_only_without_a_rendering(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update(cx, |app, cx| {
            app.session.left_out = Some("1 picture not imported.".into());
            app.session.comments_left_out = Some("2 comments not shown.".into());
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                app.read(cx).left_out().as_deref(),
                Some("1 picture not imported. 2 comments not shown.")
            );
        });
        assert!(cx.debug_bounds("left-out").is_some());
        app.update(cx, |app, _| {
            app.preview.begin("contract.docx".into());
        });
        cx.update(|_, cx| {
            assert_eq!(
                app.read(cx).left_out().as_deref(),
                Some("1 picture not imported.")
            );
        });
        app.update(cx, |app, _| {
            app.session.left_out = None;
        });
        cx.update(|_, cx| assert_eq!(app.read(cx).left_out(), None));
    }

    #[gpui::test]
    fn stale_accept_cannot_consume_newer_pending_preview(cx: &mut gpui::TestAppContext) {
        let (app, cx) = crate::document_view::tests::boot(cx);
        app.update_in(cx, |app, window, cx| {
            let old = app.begin_preview("old.pdf".into());
            let current = app.begin_preview("new.pdf".into());
            app.load_preview("new.pdf".into(), None, window, cx);
            let pending = app.preview.pending.as_ref().unwrap().pdf.entity_id();
            let region = app.comments_region.clone();
            assert!(
                !app.preview
                    .accept(old, app.theme.clone(), region, window, cx)
            );
            assert_eq!(
                app.preview.pending.as_ref().unwrap().pdf.entity_id(),
                pending
            );
            assert!(app.preview.is_current(current));
            app.close_preview(window, cx);
            let region = app.comments_region.clone();
            assert!(
                !app.preview
                    .accept(current, app.theme.clone(), region, window, cx)
            );
            assert!(app.preview.pending.is_none());
        });
    }
}
