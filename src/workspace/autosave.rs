//! Documents save themselves: a tab's text, once typing pauses, goes to the
//! Vault through one writer, and a new document is created by its first
//! save. An Unfiled document keeps its own names with it. A closed tab's
//! last text is still written, and a quit waits for the writer. Projects,
//! moves and deletes go through the same writer.
use super::{
    Workspace,
    document_writer::{DocumentWriter, Target},
};
use gpui::{AppContext as _, Context, Window};
use mdoc_pii::{restore::ExportedIdentity, saved::SavedReview};
use mdoc_store::{
    document::Document,
    document_id::{DocumentId, VaultId},
};
use mdoc_vault::{DocumentOrigin, MoveUndo, NewDocument, ProjectDocuments, SourceKind};

/// One autosave, of the text a document showed when it was queued.
pub(super) enum Write {
    Create(NewDocument),
    Vault {
        markdown: String,
        /// A new document's title follows its first line.
        title: Option<String>,
        /// Its review, whose ranges are in `markdown`.
        review: Option<Box<SavedReview>>,
        /// A conversion's notes, kept with its original.
        notes: Option<Notes>,
        /// Its names, kept with it while it is Unfiled.
        names: Box<Names>,
        /// Scanned, and no proposals left: its text is Ready.
        ready: bool,
    },
    /// The Project the target names, as the window holds it then.
    Project,
    /// A move or a delete, of this document.
    Op(VaultId, Op),
}

/// A move or a delete, run at the head of the writer's queue.
pub(super) enum Op {
    /// Into a Project, or Unfiled for `None`.
    Move { to: Option<String> },
    /// Into a new Project with this name, its categories and dismissed
    /// merge hints.
    New {
        name: String,
        skip: Vec<String>,
        dismissed: Vec<(String, String)>,
    },
    /// Back where a move took it from; `made`: the Project it created.
    Undo {
        undo: MoveUndo,
        made: Option<String>,
    },
    Delete {
        project: String,
        documents: ProjectDocuments,
        members: Vec<VaultId>,
    },
}

/// A document's names as its review holds them.
pub(super) struct Names {
    identities: Vec<ExportedIdentity>,
    kept: Vec<(String, String)>,
    not_names: Vec<(String, String)>,
}

/// A write as it runs, off the main thread.
enum Job {
    Create(NewDocument),
    Vault {
        markdown: String,
        title: Option<String>,
        review: Option<Box<SavedReview>>,
        notes: Option<Notes>,
        names: Box<Names>,
        ready: bool,
    },
    Project(Box<mdoc_vault::Project>),
}

/// What a conversion learned of its source.
pub(super) struct Notes {
    warning: Option<String>,
    left_out: Option<String>,
    /// It took a password to open.
    password_protected: bool,
}

/// How a write ended.
enum Wrote {
    Created(VaultId, String, bool),
    Saved,
}

pub(super) type Writer = DocumentWriter<Write>;

/// A document's title from its first line: Markdown markers dropped, at
/// most 80 characters, "Untitled" when it has no words.
pub(super) fn first_line_title(text: &str) -> String {
    let line = text
        .lines()
        .map(|line| {
            line.trim_start_matches(['#', '>', '-', '*', '+', ' ', '\t'])
                .trim()
        })
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let title: String = line.chars().take(80).collect();
    if title.is_empty() {
        "Untitled".into()
    } else {
        title
    }
}

impl Workspace {
    /// Tab `id`'s document changed: save what it shows now.
    pub(super) fn autosave(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return;
        };
        let view = view.read(cx);
        let text = view.editor.read(cx).text().to_string();
        if let Some(document) = view.session.document.id() {
            let review = document.vault().and_then(|_| view.saved_review());
            let target = Target::Document(document.clone());
            // A move rewrites it: it loads again first.
            if let DocumentId::Vault(id) = &document
                && self.moving.contains(id)
            {
                return;
            }
            // Unchanged since the last completed write, and none running: a
            // running one may hold text this view has since undone.
            if !view.unsaved(cx)
                && review == view.session.document.review
                && !self.document_writer.has(&target)
            {
                return;
            }
            let DocumentId::Vault(_) = document else {
                return;
            };
            let write = vault_write(view, text, review, cx);
            self.queue_write(target, write, window, cx);
            return;
        }
        // A new or pasted document is created by its first keystroke; a
        // conversion, an AI reply and an empty tab are not documents yet.
        let creatable = view.lifecycle.ready()
            && !view.lifecycle.source_only()
            && view.session.source.is_none()
            && view.preview.source().is_none()
            && view.preview.attachment().is_none()
            && view.pii.restore().is_none()
            && !text.trim().is_empty();
        if !creatable || self.vault.vault().is_none() || self.document_writer.has(&Target::Tab(id))
        {
            return;
        }
        let new = NewDocument {
            project: None,
            title: first_line_title(&text),
            source_kind: SourceKind::Text,
            origin: if view.session.pasted {
                DocumentOrigin::Ordinary
            } else {
                DocumentOrigin::New
            },
            markdown: text,
        };
        self.queue_write(Target::Tab(id), Write::Create(new), window, cx);
    }

    /// Save every document whose text was not written yet: Save in every
    /// tab, before a quit.
    pub(super) fn autosave_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id()).collect();
        for id in ids {
            self.autosave(id, window, cx);
        }
    }

    pub(super) fn queue_write(
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

    fn run_write(
        &mut self,
        (target, write): (Target, Write),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let write = match write {
            // Moves and deletes run now, with nothing else writing.
            Write::Op(document, op) => {
                match op {
                    Op::Delete {
                        project,
                        documents,
                        members,
                    } => self.run_delete(project, documents, members, window, cx),
                    op => self.run_op(document, op, window, cx),
                }
                self.advance_writer(window, cx);
                return;
            }
            Write::Project => {
                let project = match &target {
                    Target::Project(id) => self.projects.project(id).cloned(),
                    _ => None,
                };
                match project {
                    Some(project) => Job::Project(Box::new(project)),
                    // Deleted since.
                    None => {
                        self.advance_writer(window, cx);
                        return;
                    }
                }
            }
            Write::Create(new) => Job::Create(new),
            Write::Vault {
                markdown,
                title,
                review,
                notes,
                names,
                ready,
            } => Job::Vault {
                markdown,
                title,
                review,
                notes,
                names,
                ready,
            },
        };
        let vault = self.vault.vault().cloned();
        let written = match &write {
            Job::Create(new) => new.markdown.clone(),
            Job::Vault { markdown, .. } => markdown.clone(),
            Job::Project(_) => String::new(),
        };
        let title = match &write {
            Job::Create(new) => Some(new.title.clone()),
            Job::Vault { title, .. } => title.clone(),
            Job::Project(_) => None,
        };
        let review = match &write {
            Job::Vault { review, .. } => review.as_deref().cloned(),
            _ => None,
        };
        let job = target.clone();
        let task = cx.background_executor().spawn(async move {
            let no_vault = || "Documents are unavailable.".to_string();
            match (job, write) {
                (Target::Tab(_), Job::Create(new)) => {
                    let follows = new.origin == DocumentOrigin::New;
                    let vault = vault.ok_or_else(no_vault)?;
                    let created = vault.create_document(new).map_err(|e| e.to_string())?;
                    let id =
                        VaultId::parse(&created.id).ok_or_else(|| "Invalid id.".to_string())?;
                    Ok(Wrote::Created(id, created.markdown, follows))
                }
                (Target::Document(DocumentId::Vault(id)), job @ Job::Vault { .. }) => {
                    let vault = vault.ok_or_else(no_vault)?;
                    save_vault(&vault, &id, job).map(|()| Wrote::Saved)
                }
                (Target::Project(id), Job::Project(project)) => {
                    let vault = vault.ok_or_else(no_vault)?;
                    match vault.update_project(&id, |stored| {
                        super::projects_ui::keep_fields(stored, &project)
                    }) {
                        Ok(_) | Err(mdoc_vault::VaultError::NotFound { .. }) => Ok(Wrote::Saved),
                        Err(error) => Err(error.to_string()),
                    }
                }
                _ => Err("This document can't be saved.".into()),
            }
        });
        let entity = cx.entity().downgrade();
        let handle = self.window_handle;
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = cx.update_window(handle, |_, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.write_finished(target, (written, review), title, result, window, cx);
                });
            });
        })
        .detach();
    }

    fn write_finished(
        &mut self,
        target: Target,
        (written, review): (String, Option<SavedReview>),
        title: Option<String>,
        result: Result<Wrote, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (target, result) {
            (Target::Tab(tab), Ok(Wrote::Created(id, markdown, follows))) => {
                // A closed tab's document is in the Vault all the same.
                if let Some(view) = self.tabs.get(tab).and_then(|tab| tab.view().cloned()) {
                    let document = DocumentId::Vault(id.clone());
                    view.update(cx, |view, cx| {
                        if view.session.document.id().is_none() {
                            view.session.document = Document::in_vault(id, markdown);
                            view.session.name = title;
                            view.session.follows_first_line = follows;
                            cx.notify();
                        }
                    });
                    self.tabs.set_document(tab, Some(document));
                    // Typing went on while it was created.
                    self.autosave(tab, window, cx);
                }
            }
            (Target::Project(_) | Target::Op(_), Ok(_)) => {}
            (Target::Document(document), Ok(_)) => {
                let tabs: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
                    .collect();
                for (tab, view) in tabs {
                    let member = view.update(cx, |view, cx| {
                        let shows = view.session.document.is(&document);
                        if shows {
                            view.session.document.saved = written.clone();
                            view.session.document.review = review.clone();
                            view.session.write_due = false;
                            if let Some(title) = &title {
                                view.session.name = Some(title.clone());
                            }
                            cx.notify();
                        }
                        shows && view.project.link().is_some()
                    });
                    // A Not a name its saved text no longer restores leaves
                    // the Project now.
                    if member {
                        self.project_changed(tab, cx);
                    }
                }
            }
            (target, Err(error)) => {
                // The edit stays in its tab: a close or quit waiting on it stops.
                let tab = match &target {
                    Target::Tab(tab) => Some(*tab),
                    Target::Document(document) => self.find_document(document, None, cx),
                    Target::Project(_) | Target::Op(_) => None,
                };
                if let Some(tab) = tab {
                    self.closing_after_write.remove(&tab);
                }
                if self.quit_flow.abandon() {
                    Self::quit_cancelled(cx);
                    crate::menu_bar::reveal_if_hidden(window, cx);
                }
                if let Target::Project(id) = &target {
                    // Written again by the next write of it, or the next quit.
                    self.project_ui.mark_unwritten(id);
                    let name =
                        (self.projects.project(id)).map_or_else(String::new, |p| p.name.clone());
                    self.notices
                        .post(format!("Could not save the Project “{name}”: {error}"));
                } else {
                    let name = match &target {
                        Target::Tab(tab) => self.tabs.get(*tab).and_then(|tab| tab.view().cloned()),
                        Target::Document(document) => self
                            .find_document(document, None, cx)
                            .and_then(|tab| self.tabs.get(tab)?.view().cloned()),
                        Target::Project(_) | Target::Op(_) => None,
                    }
                    .map(|view| view.read(cx).display_name());
                    self.notices.post(match name {
                        Some(name) => format!("Could not save “{name}”: {error}"),
                        None => format!("Could not save a closed document: {error}"),
                    });
                }
            }
            (Target::Tab(_), Ok(Wrote::Saved)) => {}
        }
        self.advance_writer(window, cx);
    }

    /// The running write ended: start the next one, or finish a quit
    /// waiting for them, and close the tabs that waited.
    fn advance_writer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.document_writer.finished() {
            self.run_write(next, window, cx);
        } else if self.quit_flow.finishing() {
            self.finish_quit(window, cx);
        }
        // Closes that waited for their writes go now.
        let closing: Vec<u64> = self.closing_after_write.iter().copied().collect();
        for tab in closing {
            if !self.write_pending_for(tab, cx) {
                self.closing_after_write.remove(&tab);
                self.remove(tab, window, cx);
            }
        }
        cx.notify();
    }

    /// Write tab `id`'s Vault document as it shows now, at once and on this
    /// thread: a move or delete at the head of the queue reads it next.
    /// `Ok` when there is nothing to write.
    pub(super) fn write_tab_now(
        &mut self,
        id: u64,
        vault: &mdoc_vault::Vault,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(view) = self.tabs.get(id).and_then(|tab| tab.view().cloned()) else {
            return Ok(());
        };
        let (document, write) = {
            let view = view.read(cx);
            let Some(DocumentId::Vault(document)) = view.session.document.id() else {
                return Ok(());
            };
            if view.lifecycle.loading() || view.lifecycle.source_only() {
                return Ok(());
            }
            let text = view.editor.read(cx).text().to_string();
            let review = view.saved_review();
            (document, vault_write(view, text, review, cx))
        };
        let Write::Vault {
            markdown,
            title,
            review,
            notes,
            names,
            ready,
        } = write
        else {
            return Ok(());
        };
        let saved_review = review.as_deref().cloned();
        let job = Job::Vault {
            markdown: markdown.clone(),
            title: title.clone(),
            review,
            notes,
            names,
            ready,
        };
        save_vault(vault, &document, job)?;
        view.update(cx, |view, cx| {
            view.session.document.saved = markdown;
            view.session.document.review = saved_review;
            view.session.write_due = false;
            if let Some(title) = title {
                view.session.name = Some(title);
            }
            cx.notify();
        });
        Ok(())
    }

    /// A write of tab `id`'s document, or of the document it is becoming,
    /// runs or waits.
    pub(super) fn write_pending_for(&self, id: u64, cx: &gpui::App) -> bool {
        let document = (self.tabs.get(id))
            .and_then(|tab| tab.view())
            .and_then(|view| view.read(cx).session.document.id());
        self.document_writer.has(&Target::Tab(id))
            || document
                .is_some_and(|document| self.document_writer.has(&Target::Document(document)))
    }
}

/// The autosave of `view`'s Vault document showing `text`.
fn vault_write(
    view: &crate::document_view::DocumentView,
    text: String,
    review: Option<SavedReview>,
    cx: &gpui::App,
) -> Write {
    let names = view.pii.review();
    Write::Vault {
        title: view
            .session
            .follows_first_line
            .then(|| first_line_title(&text)),
        markdown: text,
        review: review.map(Box::new),
        notes: view.session.source.is_some().then(|| Notes {
            warning: view.session.warning.clone(),
            left_out: view.session.left_out.clone(),
            password_protected: view.import.unlock().is_some(),
        }),
        names: Box::new(Names {
            identities: names.export_identities(),
            kept: names.kept_pairs(),
            not_names: names.not_a_name_pairs(),
        }),
        ready: super::shelf_source::ready(view, cx),
    }
}

/// Save `job` (a `Job::Vault`) into document `id`.
fn save_vault(vault: &mdoc_vault::Vault, id: &VaultId, job: Job) -> Result<(), String> {
    let Job::Vault {
        markdown,
        title,
        review,
        notes,
        names,
        ready,
    } = job
    else {
        return Ok(());
    };
    vault
        .update_document(id.as_str(), |document| {
            document.markdown = markdown;
            // Its text is written: it opens as text from now on.
            document.unconverted = false;
            document.review = review.map(|review| *review);
            // Counted from the review it replaces: lists count it again.
            document.counts = None;
            if let (Some(notes), Some(original)) = (notes, document.original.as_mut()) {
                original.info.import_warning = notes.warning;
                original.info.left_out = notes.left_out;
                original.info.password_protected |= notes.password_protected;
            }
            if let Some(title) = title {
                document.title = title;
            }
            // In a Project its names are the Project's.
            if document.project.is_none() {
                super::projects_ui::fold(
                    &mut document.mapping,
                    &names.identities,
                    &names.kept,
                    &names.not_names,
                    id.as_str(),
                    |_| false,
                );
            }
            document.scanned |= ready;
            document.set_ready(ready);
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_first_line_with_words() {
        assert_eq!(
            first_line_title("\n\n# Lease agreement\nText"),
            "Lease agreement"
        );
        assert_eq!(first_line_title("> - quoted"), "quoted");
        assert_eq!(first_line_title("   \n#\n"), "Untitled");
        assert_eq!(first_line_title(&"я".repeat(100)).chars().count(), 80);
    }
}
