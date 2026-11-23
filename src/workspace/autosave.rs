//! Documents save themselves: a tab's text, once typing pauses, goes to the
//! Vault (or, for a case's Markdown file until cases move to Projects, to
//! that file) through one writer, and a new document is created by its
//! first save. A closed tab's last text is still written, and a quit waits
//! for the writer.
use super::{
    Workspace,
    document_writer::{DocumentWriter, Target},
};
use gpui::{Context, Window};
use mdoc_pii::saved::SavedReview;
use mdoc_store::{
    document::Document,
    document_id::{DocumentId, VaultId},
};
use mdoc_vault::{DocumentOrigin, NewDocument, SourceKind};
use std::path::PathBuf;

/// One autosave, of the text a document showed when it was queued.
pub(super) enum Write {
    Create(NewDocument),
    Vault {
        markdown: String,
        /// A new document's title follows its first line.
        title: Option<String>,
        /// Its review, whose ranges are in `markdown`.
        review: Option<Box<SavedReview>>,
        /// A conversion's warning and left-out line, kept with its original.
        notes: Option<(Option<String>, Option<String>)>,
    },
    File {
        text: String,
        /// What the file held when it was last read or written.
        saved: String,
    },
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
            // A case's Markdown file keeps its review in the case.
            let review = document.vault().and_then(|_| view.saved_review());
            if !view.unsaved(cx) && review == view.session.document.review {
                return;
            }
            let write = match document {
                DocumentId::Vault(_) => Write::Vault {
                    title: view
                        .session
                        .follows_first_line
                        .then(|| first_line_title(&text)),
                    markdown: text,
                    review: review.map(Box::new),
                    notes: view
                        .session
                        .source
                        .is_some()
                        .then(|| (view.session.warning.clone(), view.session.left_out.clone())),
                },
                DocumentId::File(_) => Write::File {
                    text,
                    saved: view.session.document.saved.clone(),
                },
            };
            self.queue_write(Target::Document(document), write, window, cx);
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

    fn queue_write(
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
        let vault = self.vault.vault().cloned();
        let written = match &write {
            Write::Create(new) => new.markdown.clone(),
            Write::Vault { markdown, .. } => markdown.clone(),
            Write::File { text, .. } => text.clone(),
        };
        let title = match &write {
            Write::Create(new) => Some(new.title.clone()),
            Write::Vault { title, .. } => title.clone(),
            Write::File { .. } => None,
        };
        let review = match &write {
            Write::Vault { review, .. } => review.as_deref().cloned(),
            _ => None,
        };
        let job = target.clone();
        let task = cx.background_executor().spawn(async move {
            let no_vault = || "Documents are unavailable.".to_string();
            match (job, write) {
                (Target::Tab(_), Write::Create(new)) => {
                    let follows = new.origin == DocumentOrigin::New;
                    let vault = vault.ok_or_else(no_vault)?;
                    let created = vault.create_document(new).map_err(|e| e.to_string())?;
                    let id =
                        VaultId::parse(&created.id).ok_or_else(|| "Invalid id.".to_string())?;
                    Ok(Wrote::Created(id, created.markdown, follows))
                }
                (
                    Target::Document(DocumentId::Vault(id)),
                    Write::Vault {
                        markdown,
                        title,
                        review,
                        notes,
                    },
                ) => {
                    let vault = vault.ok_or_else(no_vault)?;
                    vault
                        .update_document(id.as_str(), |document| {
                            document.markdown = markdown;
                            document.review = review.map(|review| *review);
                            if let (Some((warning, left_out)), Some(original)) =
                                (notes, document.original.as_mut())
                            {
                                original.info.import_warning = warning;
                                original.info.left_out = left_out;
                            }
                            if let Some(title) = title {
                                document.title = title;
                            }
                        })
                        .map(|_| Wrote::Saved)
                        .map_err(|e| e.to_string())
                }
                (Target::Document(DocumentId::File(path)), Write::File { text, saved }) => {
                    save_file(path, &text, saved).map(|()| Wrote::Saved)
                }
                _ => Err("This document can't be saved.".into()),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.write_finished(target, (written, review), title, result, window, cx);
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
            (Target::Document(document), Ok(_)) => {
                let tabs: Vec<_> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| Some((tab.id(), tab.view()?.clone())))
                    .collect();
                for (tab, view) in tabs {
                    let shows = view.update(cx, |view, cx| {
                        let shows = view.session.document.is(&document);
                        if shows {
                            view.session.document.saved = written.clone();
                            view.session.document.review = review.clone();
                            if let Some(title) = &title {
                                view.session.name = Some(title.clone());
                            }
                            cx.notify();
                        }
                        shows
                    });
                    // A case member's saved text is what its case and the
                    // shelf go by.
                    if shows && document.file().is_some() {
                        self.case_changed(tab, cx);
                        self.record_review(tab, cx);
                    }
                }
            }
            (target, Err(error)) => {
                let name = match &target {
                    Target::Tab(tab) => self.tabs.get(*tab).and_then(|tab| tab.view().cloned()),
                    Target::Document(document) => self
                        .find_document(document, None, cx)
                        .and_then(|tab| self.tabs.get(tab)?.view().cloned()),
                }
                .map(|view| view.read(cx).display_name());
                self.notices.post(match name {
                    Some(name) => format!("Could not save “{name}”: {error}"),
                    None => format!("Could not save a closed document: {error}"),
                });
            }
            (Target::Tab(_), Ok(Wrote::Saved)) => {}
        }
        if let Some(next) = self.document_writer.finished() {
            self.run_write(next, window, cx);
        } else if self.quit_flow.finishing() {
            self.finish_quit(window, cx);
        }
        cx.notify();
    }
}

/// Write a case's Markdown file, unless it changed on disk since it was
/// read: then the edit stays in the tab and the user is told.
fn save_file(path: PathBuf, text: &str, saved: String) -> Result<(), String> {
    let mut document = Document {
        vault: None,
        path: Some(path.clone()),
        saved,
        review: None,
    };
    document.save(path, text).map_err(|e| e.to_string())
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
