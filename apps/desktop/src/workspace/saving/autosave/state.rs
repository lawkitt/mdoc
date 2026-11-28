//! Autosave's state: the writes a job carries (a tab's save, its notes, a
//! Project, the user lists, a move), what a job ran to and what it wrote,
//! and a new document's title from its first line. No GPUI.

use crate::workspace::saving::document_writer::DocumentWriter;
use crate::workspace::saving::text;
use mdoc_pii::{restore::ExportedIdentity, saved::SavedReview};
use mdoc_store::document_id::VaultId;
use mdoc_vault::{MoveUndo, NewDocument, ProjectDocuments, ReviewCounts};

/// One job of the writer: what to do. What an editor shows is read when
/// the job reaches the head of the queue, never when it is queued.
pub(in crate::workspace) enum Write {
    /// A new document's first save, of what its tab shows then.
    Create,
    /// A Vault document's text, review and names, as its tab shows them.
    Save,
    /// The Project the target names, as the window holds it.
    Project,
    /// A move or a delete, of this document.
    Op(VaultId, Op),
    /// An AI reply to a tab's document.
    Reply(crate::workspace::replies::ReplySave),
    /// A document's private notes, as its tab holds them.
    Notes,
    /// Promote to Document of this saved AI reply, then open it.
    Promote(String),
    /// Delete this saved AI reply.
    DeleteReply(String),
    /// Export Project into the folder the user chose.
    Export(crate::workspace::projects::export_project::Export),
    /// Publish files sealed beside the queue, as documents.
    Import(crate::workspace::saving::intake::Staged),
    /// The user-wide lists, as Settings last changed them.
    UserLists(mdoc_store::lists::Lists),
    /// A dropped folder's new Project, for the sheet that asked.
    FolderProject(crate::workspace::projects::folder_sheet::FolderProject),
    /// New Project: an empty Project, then its page.
    NewProject(crate::workspace::projects::new_project::NewProjectJob),
    /// The last job: every editor, the Projects and the session are
    /// written, then the window closes.
    Quit,
}

/// A move or a delete, run at the head of the writer's queue.
pub(in crate::workspace) enum Op {
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
    /// Unfiled documents, with their originals, names and replies.
    DeleteDocuments { documents: Vec<VaultId> },
}

/// Whether a job ended, or holds the head of the queue until it does: a
/// write off the main thread, a reload of what it wrote, a deferred quit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum Ran {
    Ended,
    Holds,
}

/// A document's names as its review holds them.
pub(in crate::workspace) struct Names {
    pub(super) identities: Vec<ExportedIdentity>,
    pub(super) kept: Vec<(String, String)>,
    pub(super) not_names: Vec<(String, String)>,
}

/// What a job writes of one tab's document, read at the head of the
/// queue: the window's copy of its Project when its names wait to be
/// written, then its text, review and names when they changed.
pub(in crate::workspace) struct Captured {
    pub(super) document: VaultId,
    /// Written first: the names the text uses.
    pub(super) project: Option<Box<mdoc_vault::Project>>,
    pub(super) save: Option<Box<Save>>,
}

impl Captured {
    pub(super) fn writes(&self) -> bool {
        self.project.is_some() || self.save.is_some()
    }
}

/// A document's text as its tab showed it.
pub(in crate::workspace) struct Save {
    pub(super) markdown: String,
    /// A new document's title follows its first line.
    pub(super) title: Option<String>,
    /// Its review, whose ranges are in `markdown`.
    pub(super) review: Option<SavedReview>,
    /// A conversion's notes, kept with its original.
    pub(super) notes: Option<Notes>,
    /// Its names, kept with it while it is Unfiled.
    pub(super) names: Names,
    /// Scanned, and no proposals left: its text is Ready.
    pub(super) ready: bool,
    /// An edit changed paragraphs not scanned since: saved unscanned, so
    /// a document left mid-edit reopens owing a scan of all of it.
    pub(super) edits_unscanned: bool,
    /// All of its text was scanned before: this tab scanned it, or it
    /// opened scanned.
    pub(super) scanned: bool,
    /// What its review counts now, for lists without a live view.
    pub(super) counts: Option<ReviewCounts>,
}

/// Why a tab's document was not written at the head of a job.
#[derive(Debug)]
pub(in crate::workspace) enum TabWriteError {
    /// Its names could not join its Project's.
    Names(String),
    /// The Vault refused the write.
    Write(String),
}

impl std::fmt::Display for TabWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Names(error) => f.write_str(&text::names_not_kept(error)),
            Self::Write(error) => f.write_str(&text::text_not_saved(error)),
        }
    }
}

/// A write as it runs, off the main thread.
pub(super) enum Job {
    Create(NewDocument),
    Save(Box<Captured>),
    Project(Box<mdoc_vault::Project>),
}

/// What a conversion learned of its source.
pub(in crate::workspace) struct Notes {
    pub(super) warning: Option<String>,
    pub(super) left_out: Option<String>,
    /// It took a password to open.
    pub(super) password_protected: bool,
}

/// How a write ended.
pub(super) enum Wrote {
    Created {
        id: VaultId,
        markdown: String,
        follows: bool,
        title: String,
    },
    Saved(Box<Captured>),
    Project,
}

/// A write that failed, with what it had captured.
pub(super) struct Failed {
    pub(super) error: String,
    pub(super) captured: Option<Box<Captured>>,
}

pub(in crate::workspace) type Writer = DocumentWriter<Write>;

/// A document's title from its first line: Markdown markers dropped, at
/// most 80 characters, "Untitled" when it has no words.
pub(in crate::workspace) fn first_line_title(text: &str) -> String {
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
        text::UNTITLED.into()
    } else {
        title
    }
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
