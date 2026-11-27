//! The tab list's saves to `session.json`: one write at a time,
//! the latest snapshot waiting behind it, and none when nothing changed
//! since the last save. Generic over the snapshot, so the queue rules run
//! without a window or a disk; the workspace view takes snapshots, runs the
//! writes in the background and reports how they ended.
use super::text;
use crate::workspace::{Workspace, saving};
use gpui::{App, Context, PromptLevel, Window};
use mdoc_store::{session_store, session_store::Session};

use std::path::PathBuf;

/// What the workspace view carries out for a session-writer transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum SessionEffect<S> {
    /// Save `snapshot` to `path` in the background, then call `saved` or
    /// `failed`.
    Write { path: PathBuf, snapshot: S },
}

pub(in crate::workspace) struct SessionWriter<S> {
    /// Where the session lives; none in tests, which never save.
    path: Option<PathBuf>,
    /// What the last successful write saved.
    persisted: Option<S>,
    writing: bool,
    /// The newest snapshot not yet written.
    pending: Option<S>,
}

impl<S: Clone + PartialEq> SessionWriter<S> {
    pub(in crate::workspace) fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            persisted: None,
            writing: false,
            pending: None,
        }
    }

    pub(in crate::workspace) fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Save to `path` from now on.
    pub(in crate::workspace) fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    /// Whether checkpoints save at all.
    pub(in crate::workspace) fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// A write is running.
    pub(in crate::workspace) fn writing(&self) -> bool {
        self.writing
    }

    /// The tab list now looks like `snapshot`. Unchanged since the last
    /// save, with no write running, it is not written again.
    pub(in crate::workspace) fn checkpoint(&mut self, snapshot: S) -> Option<SessionEffect<S>> {
        if self.persisted.as_ref() == Some(&snapshot) && !self.writing {
            return None;
        }
        self.pending = Some(snapshot);
        self.next()
    }

    /// Start writing the waiting snapshot unless a write is running.
    fn next(&mut self) -> Option<SessionEffect<S>> {
        if self.writing {
            return None;
        }
        let snapshot = self.pending.take()?;
        let path = self.path.clone()?;
        self.writing = true;
        Some(SessionEffect::Write { path, snapshot })
    }

    /// The write of `snapshot` finished; a newer snapshot that differs from
    /// it is written next.
    pub(in crate::workspace) fn saved(&mut self, snapshot: S) -> Option<SessionEffect<S>> {
        self.writing = false;
        self.persisted = Some(snapshot);
        if self.pending == self.persisted {
            self.pending = None;
        }
        self.next()
    }

    /// The write failed. The waiting snapshot is dropped; the next
    /// checkpoint tries again.
    pub(in crate::workspace) fn failed(&mut self) {
        self.writing = false;
        self.pending = None;
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn write(snapshot: u8) -> Option<SessionEffect<u8>> {
        Some(SessionEffect::Write {
            path: "/profile/session.json".into(),
            snapshot,
        })
    }

    #[test]
    fn one_write_runs_at_a_time_and_only_the_newest_waits() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert!(writer.enabled());
        assert_eq!(writer.checkpoint(1), write(1));
        assert!(writer.writing());
        assert_eq!(writer.checkpoint(2), None);
        assert_eq!(writer.checkpoint(3), None);
        assert_eq!(writer.saved(1), write(3));
        assert_eq!(writer.saved(3), None);
        assert!(!writer.writing());
        // Unchanged since the last save: nothing to write.
        assert_eq!(writer.checkpoint(3), None);
        assert_eq!(writer.checkpoint(4), write(4));
    }

    #[test]
    fn a_snapshot_equal_to_the_one_just_saved_is_not_written_again() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
        // While writing, even an unchanged snapshot waits: the write may fail.
        assert_eq!(writer.checkpoint(1), None);
        assert_eq!(writer.saved(1), None);
        assert!(!writer.writing());
    }

    #[test]
    fn a_failed_write_drops_the_waiting_snapshot() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
        assert_eq!(writer.checkpoint(2), None);
        writer.failed();
        assert!(!writer.writing());
        assert_eq!(writer.checkpoint(2), write(2));
    }

    #[test]
    fn without_a_path_nothing_is_written() {
        let mut writer = SessionWriter::<u8>::new(None);
        assert!(!writer.enabled());
        assert_eq!(writer.checkpoint(1), None);
        assert!(!writer.writing());
        writer.set_path(Some("/profile/session.json".into()));
        assert_eq!(writer.path(), Some(&"/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
    }
}
