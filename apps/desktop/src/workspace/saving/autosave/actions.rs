//! Autosave's actions: a tab's text and notes saved once typing pauses, new
//! documents created by their first write, and the writer's jobs that write
//! them to the Vault one at a time.

use super::state::{
    Captured, Failed, Job, Names, Notes, Op, Ran, Save, TabWriteError, Write, Wrote,
    first_line_title,
};
use crate::workspace::saving::text;
use crate::workspace::{Workspace, saving::document_writer::Target};
use gpui::{AppContext as _, Context, Window};
use mdoc_pii::saved::SavedReview;
use mdoc_store::{
    document::Document,
    document_id::{DocumentId, VaultId},
};
use mdoc_vault::{DocumentOrigin, NewDocument, ReviewCounts, SourceKind};

/// Whether `view`'s review is Ready where its last write said it was not,
/// or the other way round: a write is due though nothing else changed.
pub(in crate::workspace) fn ready_unwritten(
    view: &crate::document::DocumentView,
    cx: &gpui::App,
) -> bool {
    crate::workspace::shelf_source::ready(view, cx) != view.session.document.ready
}

/// The save of `view`'s Vault document showing `text`.
pub(super) fn vault_save(
    view: &crate::document::DocumentView,
    text: String,
    review: Option<SavedReview>,
    cx: &gpui::App,
) -> Save {
    let names = view.pii.review();
    // Proposals left as the window counts them (`remaining`), so Home and
    // the Project page say Needs review once the view is gone.
    let counts = review.as_ref().map(|_| ReviewCounts {
        proposed: names.remaining() as u64,
        replaced: names.applied().len() as u64,
        kept: names.kept_mentions().len() as u64,
    });
    Save {
        title: view
            .session
            .follows_first_line
            .then(|| first_line_title(&text)),
        markdown: text,
        review,
        notes: view.session.source.is_some().then(|| Notes {
            warning: view.session.warning.clone(),
            left_out: view.session.left_out.clone(),
            password_protected: view.import.unlock().is_some(),
        }),
        names: Names {
            identities: names.export_identities(),
            kept: names.kept_pairs(),
            not_names: names.not_a_name_pairs(),
        },
        ready: crate::workspace::shelf_source::ready(view, cx),
        edits_unscanned: names.edits_unscanned(),
        scanned: view.session.document.scanned || !view.pii.scans().is_empty(),
        counts,
    }
}

/// Write what `captured` holds: its Project first, the names its text
/// uses, then the document. Any thread.
pub(super) fn write_captured(vault: &mdoc_vault::Vault, captured: &Captured) -> Result<(), String> {
    if let Some(project) = &captured.project {
        match vault.update_project(&project.id, |stored| {
            crate::workspace::projects::keep_fields(stored, project)
        }) {
            Ok(_) | Err(mdoc_vault::VaultError::NotFound { .. }) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    match &captured.save {
        Some(save) => save_vault(vault, &captured.document, save),
        None => Ok(()),
    }
}

/// Save `save` into document `id`.
pub(super) fn save_vault(
    vault: &mdoc_vault::Vault,
    id: &VaultId,
    save: &Save,
) -> Result<(), String> {
    vault
        .update_document(id.as_str(), |document| {
            // A phone's "I checked the names myself" was of the text as it
            // was (item 48): new text was never checked.
            if document.markdown != save.markdown {
                document.names_checked_by_hand = false;
            }
            document.markdown.clone_from(&save.markdown);
            // Its text is written: it opens as text from now on.
            document.unconverted = false;
            document.review.clone_from(&save.review);
            // Counted from the review it saves; none without one.
            document.counts = save.counts;
            if let (Some(notes), Some(original)) = (&save.notes, document.original.as_mut()) {
                original.info.import_warning.clone_from(&notes.warning);
                original.info.left_out.clone_from(&notes.left_out);
                original.info.password_protected |= notes.password_protected;
            }
            if let Some(title) = &save.title {
                document.title.clone_from(title);
            }
            // In a Project its names are the Project's.
            if document.project.is_none() {
                crate::workspace::projects::fold(
                    &mut document.mapping,
                    &save.names.identities,
                    &save.names.kept,
                    &save.names.not_names,
                    id.as_str(),
                    |_| false,
                );
            }
            // `scanned`: all of its text was scanned.
            document.scanned =
                !save.edits_unscanned && (document.scanned || save.scanned || save.ready);
            document.set_ready(save.ready);
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

impl Workspace {
    /// Tab `id`'s document changed: save what it shows once the save
    /// reaches the head of the queue.
    pub(in crate::workspace) fn autosave(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.autosave_document(id, window, cx);
        // A reply pasted in its box goes after the names it binds.
        self.queue_box_reply(id, window, cx);
        self.queue_notes(id, window, cx);
    }

    /// Queue tab `id`'s notes when they differ from what was last written:
    /// they save as its text does, through the one writer, read when the
    /// write runs.
    pub(super) fn queue_notes(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let document = {
            let view = view.read(cx);
            let Some(DocumentId::Vault(document)) = view.session.document.id() else {
                return;
            };
            if !view.notes.needs_write() || self.vault.vault().is_none() {
                return;
            }
            document
        };
        self.queue_write(Target::Notes(document), Write::Notes, window, cx);
    }

    /// Write a document's notes as a tab holds them now; every tab showing
    /// the document learns they are written.
    pub(in crate::workspace) fn run_notes_save(
        &mut self,
        document: &VaultId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.vault().cloned() else {
            return;
        };
        let target = DocumentId::Vault(document.clone());
        let Some(view) = (self.tabs.iter())
            .filter_map(|tab| tab.view())
            .find(|view| {
                let view = view.read(cx);
                view.session.document.is(&target) && view.notes.needs_write()
            })
            .cloned()
        else {
            return;
        };
        let notes = view.read(cx).notes.text().to_string();
        view.update(cx, |view, _| view.notes.queued(notes.clone()));
        match vault.set_notes(document.as_str(), notes.clone()) {
            // Deleted meanwhile: nothing to keep them with, and nothing to
            // write them again (a quit would ask forever).
            Ok(_) | Err(mdoc_vault::VaultError::NotFound { .. }) => {
                for view in self.tabs.iter().filter_map(|tab| tab.view()) {
                    view.update(cx, |view, _| {
                        if view.session.document.is(&target) {
                            view.notes.mark_saved(notes.clone());
                        }
                    });
                }
            }
            Err(error) => {
                self.notes_write_failed(&target, cx);
                let tab = self.find_document(&target, None, cx);
                if let Some(tab) = tab {
                    // The close waiting on them stops, and the view stays
                    // with its notes.
                    self.closing_after_write.remove(&tab);
                    self.unsaved.insert(tab);
                }
                if self.quit_flow.abandon() {
                    Self::quit_cancelled(cx);
                    crate::shelf::menu_bar::reveal_if_hidden(window, cx);
                }
                let name = (tab.and_then(|tab| self.tabs.get(tab)?.view().cloned()))
                    .map(|view| view.read(cx).display_name());
                self.notices.post(match name {
                    Some(name) => text::notes_not_saved(&name, &error),
                    None => text::closed_notes_not_saved(&error),
                });
            }
        }
    }

    /// The notes handed over for `target` were not written.
    pub(super) fn notes_write_failed(&mut self, target: &DocumentId, cx: &mut Context<Self>) {
        for view in self.tabs.iter().filter_map(|tab| tab.view()) {
            view.update(cx, |view, _| {
                if view.session.document.is(target) {
                    view.notes.write_failed();
                }
            });
        }
    }

    pub(super) fn autosave_document(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let view = view.read(cx);
        if let Some(document) = view.session.document.id() {
            let review = view.saved_review();
            let target = Target::Document(document.clone());
            // Unchanged since the last completed write, and none running: a
            // running one may hold text this view has since undone.
            if !view.unsaved(cx)
                && review == view.session.document.review
                && !ready_unwritten(view, cx)
                && !self.document_writer.has(&target)
                && !view.project_save_pending()
            {
                return;
            }
            self.queue_write(target, Write::Save, window, cx);
            return;
        }
        if self.new_document_of(id, cx).is_none() || self.document_writer.has(&Target::Tab(id)) {
            return;
        }
        self.queue_write(Target::Tab(id), Write::Create, window, cx);
    }

    /// The document tab `id` becomes with its first write, from what it
    /// shows now: a new or pasted document with words in it. A conversion
    /// and an empty tab are not documents yet.
    pub(super) fn new_document_of(&self, id: u64, cx: &gpui::App) -> Option<NewDocument> {
        let view = self.tabs.get(id)?.view()?.read(cx);
        let text = view.editor.read(cx).text().to_string();
        let creatable = view.session.document.id().is_none()
            && view.lifecycle.ready()
            && !view.lifecycle.source_only()
            && view.session.source.is_none()
            && view.preview.source().is_none()
            && view.preview.attachment().is_none()
            && !text.trim().is_empty();
        (creatable && self.vault.vault().is_some()).then(|| NewDocument {
            project: None,
            title: first_line_title(&text),
            source_kind: SourceKind::Text,
            origin: if view.session.pasted {
                DocumentOrigin::Ordinary
            } else {
                DocumentOrigin::New
            },
            markdown: text,
        })
    }

    /// Save every document whose text was not written yet: Save in every
    /// tab.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(in crate::workspace) fn autosave_all(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id()).collect();
        for id in ids {
            self.autosave(id, window, cx);
        }
    }

    pub(in crate::workspace) fn queue_write(
        &mut self,
        target: Target,
        write: Write,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(next) = self.document_writer.queue(target, write) {
            self.run_write(next, window, cx);
        }
    }

    /// Run the job at the head of the queue. One that ends at once lets the
    /// next one start; one that holds the head calls
    /// [`Self::advance_writer`] when it ends.
    pub(super) fn run_write(
        &mut self,
        (target, write): (Target, Write),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ran = match write {
            Write::Op(document, op) => match op {
                Op::Delete {
                    project,
                    documents,
                    members,
                } => self.run_delete(project, documents, members, window, cx),
                Op::DeleteDocuments { documents } => {
                    self.run_delete_documents(documents, window, cx);
                    Ran::Ended
                }
                op => self.run_op(document, op, window, cx),
            },
            Write::Reply(save) => {
                self.run_reply_save(save, window, cx);
                Ran::Ended
            }
            Write::Notes => {
                if let Target::Notes(document) = &target {
                    self.run_notes_save(document, window, cx);
                }
                Ran::Ended
            }
            Write::Promote(reply) => {
                self.run_promote(&reply, window, cx);
                Ran::Ended
            }
            Write::DeleteReply(reply) => {
                self.run_delete_reply(&reply, window, cx);
                Ran::Ended
            }
            Write::Export(export) => self.run_export(export, window, cx),
            Write::Import(staged) => {
                self.run_import(staged, window, cx);
                Ran::Ended
            }
            Write::UserLists(lists) => {
                self.run_user_lists(lists, cx);
                Ran::Ended
            }
            Write::FolderProject(project) => {
                self.run_new_project(project, window, cx);
                Ran::Ended
            }
            Write::NewProject(job) => {
                self.run_empty_project(job, window, cx);
                Ran::Ended
            }
            Write::Quit => {
                // In the next turn, still at the head: the quit's caller
                // returns first, and nothing can run in between.
                cx.defer_in(window, |this, window, cx| {
                    if this.run_quit(window, cx) == Ran::Ended {
                        this.advance_writer(window, cx);
                    }
                });
                Ran::Holds
            }
            write @ (Write::Create | Write::Save | Write::Project) => {
                self.run_save(target, write, window, cx)
            }
        };
        if ran == Ran::Ended {
            self.advance_writer(window, cx);
        }
    }

    /// A save off the main thread, of what the window shows now.
    pub(super) fn run_save(
        &mut self,
        target: Target,
        write: Write,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Ran {
        let job = match (&target, write) {
            (Target::Project(id), Write::Project) => {
                // Its open members' names waiting for their fold go too.
                if let Err(error) = self.fold_members(&[Some(id.clone())], cx) {
                    self.notices.post(text::project_names_not_kept(&error));
                }
                match self.projects.project(id) {
                    Some(project) => Job::Project(Box::new(project.clone())),
                    // Deleted since.
                    None => return Ran::Ended,
                }
            }
            (Target::Tab(tab), Write::Create) => match self.new_document_of(*tab, cx) {
                Some(new) => Job::Create(new),
                None => return Ran::Ended,
            },
            (Target::Document(document), Write::Save) => {
                let Some(tab) = self.find_document(document, None, cx) else {
                    return Ran::Ended;
                };
                match self.capture(tab, false, cx) {
                    Ok(Some(captured)) => {
                        // Its names are not the Vault's until this write
                        // ends: the shelf waits for them.
                        if let Some(project) = &captured.project {
                            self.projects_saving.insert(project.id.clone());
                        }
                        Job::Save(Box::new(captured))
                    }
                    Ok(None) => return Ran::Ended,
                    Err(error) => {
                        self.save_failed(&target, error, window, cx);
                        return Ran::Ended;
                    }
                }
            }
            _ => return Ran::Ended,
        };
        let vault = self.vault.vault().cloned();
        let task = cx.background_executor().spawn(async move {
            let no_vault = || text::DOCUMENTS_UNAVAILABLE.to_string();
            let Some(vault) = vault else {
                return Err(Failed {
                    error: no_vault(),
                    captured: match job {
                        Job::Save(captured) => Some(captured),
                        _ => None,
                    },
                });
            };
            match job {
                Job::Create(new) => {
                    let follows = new.origin == DocumentOrigin::New;
                    let title = new.title.clone();
                    let failed = |error: String| Failed {
                        error,
                        captured: None,
                    };
                    let created = vault
                        .create_document(new)
                        .map_err(|e| failed(e.to_string()))?;
                    let id = VaultId::parse(&created.id)
                        .ok_or_else(|| failed(text::INVALID_ID.to_string()))?;
                    Ok(Wrote::Created {
                        id,
                        markdown: created.markdown,
                        follows,
                        title,
                    })
                }
                Job::Save(captured) => match write_captured(&vault, &captured) {
                    Ok(()) => Ok(Wrote::Saved(captured)),
                    Err(error) => Err(Failed {
                        error,
                        captured: Some(captured),
                    }),
                },
                Job::Project(project) => {
                    match vault.update_project(&project.id, |stored| {
                        crate::workspace::projects::keep_fields(stored, &project)
                    }) {
                        Ok(_) | Err(mdoc_vault::VaultError::NotFound { .. }) => Ok(Wrote::Project),
                        Err(error) => Err(Failed {
                            error: error.to_string(),
                            captured: None,
                        }),
                    }
                }
            }
        });
        let entity = cx.entity().downgrade();
        let handle = self.window_handle;
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = cx.update_window(handle, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.write_finished(target, result, window, cx);
                });
            });
        })
        .detach();
        Ran::Holds
    }

    pub(super) fn write_finished(
        &mut self,
        target: Target,
        result: Result<Wrote, Failed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (target, result) {
            (
                Target::Tab(tab),
                Ok(Wrote::Created {
                    id,
                    markdown,
                    follows,
                    title,
                }),
            ) => {
                // A closed tab's document is in the Vault all the same.
                if self.took_created(tab, id, markdown, follows, title, cx) {
                    // Typing went on while it was created.
                    self.autosave(tab, window, cx);
                }
            }
            (_, Ok(Wrote::Saved(captured))) => {
                if let Some(project) = &captured.project {
                    self.projects_saving.remove(&project.id);
                }
                self.commit_captured(&captured, cx)
            }
            (_, Ok(_)) => {}
            (target, Err(Failed { error, captured })) => {
                if let Some(project) = captured.as_ref().and_then(|c| c.project.as_ref()) {
                    self.projects_saving.remove(&project.id);
                    // Written again by the next write of it, or the quit.
                    self.project_ui.mark_unwritten(&project.id);
                }
                self.save_failed(&target, error, window, cx);
            }
        }
        self.advance_writer(window, cx);
    }

    /// Tab `tab`'s first write made document `id`. Returns whether its
    /// view is still there.
    pub(super) fn took_created(
        &mut self,
        tab: u64,
        id: VaultId,
        markdown: String,
        follows: bool,
        title: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.tabs.get(tab).and_then(|tab| tab.view().cloned()) else {
            return false;
        };
        let document = DocumentId::Vault(id.clone());
        view.update(cx, |view, cx| {
            if view.session.document.id().is_none() {
                view.session.document = Document::in_vault(id.clone(), markdown);
                view.session.name = Some(title);
                view.session.follows_first_line = follows;
                cx.notify();
            }
        });
        self.tabs.set_document(tab, Some(document));
        self.unsaved.remove(&tab);
        // Back reaches it by its id, also once it is evicted.
        self.navigation.rename(
            &crate::workspace::sidebar::navigation::Stop::Tab(tab),
            crate::workspace::sidebar::navigation::Stop::Document(id),
        );
        true
    }

    /// A write into `target` failed: its edit stays in its tab, and a close
    /// or quit waiting on it stops.
    pub(super) fn save_failed(
        &mut self,
        target: &Target,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = match target {
            Target::Tab(tab) => Some(*tab),
            Target::Document(document) => self.find_document(document, None, cx),
            _ => None,
        };
        if let Some(tab) = tab {
            self.closing_after_write.remove(&tab);
            // Its edit is in no file: eviction keeps it.
            self.unsaved.insert(tab);
        }
        if self.quit_flow.abandon() {
            Self::quit_cancelled(cx);
            crate::shelf::menu_bar::reveal_if_hidden(window, cx);
        }
        if let Target::Project(id) = target {
            // Written again by the next write of it, or the next quit.
            self.project_ui.mark_unwritten(id);
            let name = (self.projects.project(id)).map_or_else(String::new, |p| p.name.clone());
            self.notices.post(text::project_not_saved(&name, &error));
            return;
        }
        let name = (tab.and_then(|tab| self.tabs.get(tab)?.view().cloned()))
            .map(|view| view.read(cx).display_name());
        self.notices.post(match name {
            Some(name) => text::not_saved(&name, &error),
            None => text::closed_not_saved(&error),
        });
    }

    /// The running job ended: start the next one, and close the tabs that
    /// waited for their writes.
    pub(in crate::workspace) fn advance_writer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(next) = self.document_writer.finished() {
            self.run_write(next, window, cx);
        }
        // A Copy for AI waiting for what it shows to be written.
        self.copy_for_ai_after_write(cx);
        // Closes that waited for their writes go now.
        let closing: Vec<u64> = self.closing_after_write.iter().copied().collect();
        for tab in closing {
            if !self.write_pending_for(tab, cx) {
                self.closing_after_write.remove(&tab);
                self.remove(tab, window, cx);
            }
        }
        // A view kept for its write may go now.
        self.evict(window, cx);
        cx.notify();
    }

    /// Read what tab `id` shows, for a job at the head of the queue (ADR
    /// 0130): its names join its Project first, which may renumber an alias
    /// the Project retired in its text, and only then its text, review and
    /// names are read. The window's copy of its Project comes along while
    /// its names wait to be written. `fold`: fold its names even when no
    /// change waits for its fold (moves, deletes, exports and the quit).
    /// `None` when there is nothing to write.
    pub(in crate::workspace) fn capture(
        &mut self,
        id: u64,
        fold: bool,
        cx: &mut Context<Self>,
    ) -> Result<Option<Captured>, String> {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return Ok(None);
        };
        let document = {
            let view = view.read(cx);
            let Some(DocumentId::Vault(document)) = view.session.document.id() else {
                return Ok(None);
            };
            if view.lifecycle.loading() || view.lifecycle.source_only() {
                return Ok(None);
            }
            document
        };
        let waiting = view.update(cx, |view, _| view.take_project_save());
        if fold || waiting {
            self.fold_into_project(id, cx)?;
        }
        let link = (view.read(cx).project.link()).map(|link| link.id.clone());
        let project = link
            .filter(|project| {
                self.project_ui.is_unwritten(project)
                    || self.document_writer.has(&Target::Project(project.clone()))
            })
            .and_then(|project| self.projects.project(&project).cloned())
            .map(Box::new);
        if let Some(project) = &project {
            self.project_ui.take_unwritten(&project.id);
        }
        let save = {
            let view = view.read(cx);
            let review = view.saved_review();
            // Unchanged since its last write: its text is the Vault's, and
            // writing it again could only undo a newer revision.
            (view.unsaved(cx)
                || review != view.session.document.review
                || ready_unwritten(view, cx))
            .then(|| {
                let text = view.editor.read(cx).text().to_string();
                Box::new(vault_save(view, text, review, cx))
            })
        };
        let captured = Captured {
            document,
            project,
            save,
        };
        Ok(captured.writes().then_some(captured))
    }

    /// What `captured` wrote is the Vault's now: every tab showing its
    /// document learns it.
    pub(super) fn commit_captured(&mut self, captured: &Captured, cx: &mut Context<Self>) {
        let Some(save) = &captured.save else {
            return;
        };
        let document = DocumentId::Vault(captured.document.clone());
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
            .collect();
        for (tab, view) in tabs {
            if !view.read(cx).session.document.is(&document) {
                continue;
            }
            self.unsaved.remove(&tab);
            let member = view.update(cx, |view, cx| {
                view.session.document.saved = save.markdown.clone();
                view.session.document.review = save.review.clone();
                view.session.document.ready = save.ready;
                view.session.document.scanned |= save.ready;
                view.session.write_due = false;
                if let Some(title) = &save.title {
                    view.session.name = Some(title.clone());
                }
                cx.notify();
                view.project.link().is_some()
            });
            // A Not a name its saved text no longer restores leaves the
            // Project now.
            if member {
                self.project_changed(tab, cx);
            }
        }
    }

    /// Write tab `id`'s Vault document as it shows now, at once and on this
    /// thread, for a job at the head of the queue: its names first, then
    /// its text. `Ok` when there is nothing to write.
    pub(in crate::workspace) fn write_tab_now(
        &mut self,
        id: u64,
        vault: &mdoc_vault::Vault,
        cx: &mut Context<Self>,
    ) -> Result<(), TabWriteError> {
        let Some(captured) = self.capture(id, true, cx).map_err(TabWriteError::Names)? else {
            return Ok(());
        };
        match write_captured(vault, &captured) {
            Ok(()) => {
                self.commit_captured(&captured, cx);
                Ok(())
            }
            Err(error) => {
                if let Some(project) = &captured.project {
                    self.project_ui.mark_unwritten(&project.id);
                }
                Err(TabWriteError::Write(error))
            }
        }
    }

    /// Create tab `id`'s document now, on this thread, for the quit.
    pub(in crate::workspace) fn create_tab_now(
        &mut self,
        id: u64,
        vault: &mdoc_vault::Vault,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(new) = self.new_document_of(id, cx) else {
            return Ok(());
        };
        let follows = new.origin == DocumentOrigin::New;
        let title = new.title.clone();
        let created = vault.create_document(new).map_err(|e| e.to_string())?;
        let document = VaultId::parse(&created.id).ok_or_else(|| text::INVALID_ID.to_string())?;
        self.took_created(id, document, created.markdown, follows, title, cx);
        Ok(())
    }

    /// A write of tab `id`'s document, or of the document it is becoming,
    /// runs or waits.
    pub(in crate::workspace) fn write_pending_for(&self, id: u64, cx: &gpui::App) -> bool {
        let document = (self.tabs.get(id))
            .and_then(|tab| tab.view())
            .and_then(|view| view.read(cx).session.document.id());
        let notes = match &document {
            Some(DocumentId::Vault(id)) => self.document_writer.has(&Target::Notes(id.clone())),
            None => false,
        };
        self.document_writer.has(&Target::Tab(id))
            || self.document_writer.has(&Target::Reply(id))
            || notes
            || document
                .is_some_and(|document| self.document_writer.has(&Target::Document(document)))
    }
}
