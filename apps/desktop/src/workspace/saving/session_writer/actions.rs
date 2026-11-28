//! The session writer's actions: taking the snapshot, the checkpoint, and
//! carrying out a write or its failure.

use super::SessionEffect;
use crate::workspace::saving::text;
use crate::workspace::{Workspace, saving};
use gpui::{App, Context, PromptLevel, Window};
use mdoc_store::{session_store, session_store::Session};

impl Workspace {
    pub(in crate::workspace) fn snapshot(&self, cx: &App) -> Session {
        let mut session = Session {
            sidebar_visible: self.sidebar.visible(),
            sidebar_choice: self.sidebar.choice(),
            theme_choice: self.theme_choice,
            expanded_projects: self.tree.expanded_ids(),
            ..Session::default()
        };
        for tab in &self.tabs {
            let mut record = tab.record().clone();
            if let Some(view) = tab.view() {
                let view = view.read(cx);
                record.source_only =
                    view.lifecycle.source_only() || view.lifecycle.generated_unedited();
                record.blank = !record.source_only
                    && !record.names_document()
                    && view.preview.source().is_none()
                    && view.editor.read(cx).text().is_empty();
                record.blank_disposable = record.blank && view.lifecycle.blank_disposable();
                // A source tab keeps the name it shows, so after a restart it
                // reads the same before and after it is first opened.
                record.name = if record.blank {
                    view.session.name.clone()
                } else if record.source_only {
                    Some(view.display_name())
                } else {
                    None
                };
                if let Some(document) = view.session.document.id() {
                    record.set_document(document);
                }
                if !view.lifecycle.loading() && !view.lifecycle.unavailable() {
                    record.caret = view.editor.read(cx).cursor();
                    record.scroll_y = f32::from(view.scroll.offset().y);
                }
                record.attachment = view
                    .preview
                    .attachment()
                    .cloned()
                    .or_else(|| view.preview.source().cloned());
                // A Vault tab's Source is a viewing copy, made again on load.
                if record.document.is_some() {
                    record.attachment = None;
                    record.source_only = false;
                    record.blank = false;
                    record.blank_disposable = false;
                }
                record.preview_visible = view.preview.visible();
                record.preview_split = view.preview.split_ratio();
                if let Some(pdf) = view.preview.pdf() {
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
            // A Vault tab is its id: no title, ever.
            if record.document.is_some() {
                record.name = None;
            }
            if !record.names_document() && !record.source_only && !record.blank {
                continue;
            }
            if tab.id() == self.tabs.active() {
                session.active = session.tabs.len();
            }
            session.tabs.push(record);
        }
        session
    }

    pub(in crate::workspace) fn checkpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready || !self.session_writer.enabled() {
            return;
        }
        let snapshot = self.snapshot(cx);
        let effect = self.session_writer.checkpoint(snapshot);
        self.carry_out_session(effect, window, cx);
    }

    pub(in crate::workspace) fn carry_out_session(
        &mut self,
        effect: Option<SessionEffect<Session>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(SessionEffect::Write { path, snapshot }) = effect else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let result = session_store::save(&path, &snapshot);
            (snapshot, result)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (snapshot, result) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => {
                        let effect = this.session_writer.saved(snapshot);
                        this.carry_out_session(effect, window, cx);
                    }
                    Err(error) => {
                        this.session_writer.failed();
                        this.session_failed(error.to_string(), window, cx);
                    }
                }
                // A quit waiting at the head for this write goes on.
                if !this.session_writer.writing()
                    && std::mem::take(&mut this.quit_waits_for_session)
                    && this.run_quit(window, cx) == saving::autosave::Ran::Ended
                {
                    this.advance_writer(window, cx);
                }
            });
        })
        .detach();
    }

    /// The tab list could not be saved; during a quit, ask whether to quit
    /// with the previous one.
    pub(in crate::workspace) fn session_failed(
        &mut self,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notices.post(text::session_not_saved(&error));
        if self.quit_flow.abandon() {
            // A window hidden in the menu bar shows to ask.
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
            let answer = window.prompt(
                PromptLevel::Warning,
                text::TAB_LIST_NOT_SAVED,
                Some(text::TAB_LIST_DETAIL),
                &[crate::ui::text::CANCEL, text::QUIT_WITHOUT_SESSION],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if answer.await.ok() == Some(1) {
                    let _ = this.update_in(cx, |this, window, cx| {
                        // The previous tab list stays.
                        this.session_writer.set_path(None);
                        this.queue_quit(window, cx);
                    });
                } else {
                    let _ = this.update(cx, |_, cx| Self::quit_cancelled(cx));
                }
            })
            .detach();
        }
        cx.notify();
    }
}
