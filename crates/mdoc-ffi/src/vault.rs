//! `Vault`: the mobile apps' handle on the encrypted Vault (`mdoc_vault`,
//! ADR 0125, #1091): Projects and Documents by id, Unfiled, moves with
//! undo, renames, deletes, and lists from the manifest's summaries. It
//! stands beside `Library`, which the apps keep using until they switch
//! (ROADMAP Vault & Projects items 9 and 12); `DocumentSession` opens and
//! saves its documents (`open_from_vault`, `save_into_vault`).
//!
//! The app passes the Vault's folder and key in; the core never picks a
//! path nor asks the OS for a key (Decision 21). The apps keep it in a
//! sibling `vault` / `vault-dev` folder beside their old storage, chosen
//! by the same release-or-dev check, never inside the old cases or library
//! folder.
//!
//! AI Replies are saved, listed, restored and deleted here too
//! ([`replies`]); `ReplyVault` is the Restore extension's handle, which
//! writes nothing but new replies.
mod originals;
mod replies;

pub use originals::OriginalFile;
pub use replies::{ReplyInfo, ReplyOwner, ReplyVault, SavedReply, SharedFrom};

use crate::{
    CoreError,
    document_session::{Category, DocumentOrigin, DocumentPhase},
};
use mdoc_vault::{Colour, DocumentSummary, MoveUndo, Moved, NewProject, Project, VaultError};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(uniffi::Object)]
pub struct Vault {
    store: mdoc_vault::Vault,
    /// One check and write at a time through this handle: a document's
    /// save reads its record and Project and writes them while no move,
    /// rename or delete runs in between.
    writes: Mutex<()>,
    /// Moves Undo can still take back, and where text was shared from.
    undo: Mutex<Undo>,
}

/// Moves to undo, by the handle `DocumentMove::undo` gives, each with the
/// share count when it was made; and the last share from each place
/// (a Project, or Unfiled for `None`).
#[derive(Default)]
struct Undo {
    count: u64,
    moves: HashMap<u64, (MoveUndo, u64)>,
    shared: HashMap<Option<String>, u64>,
}

/// The file family a Document was made from (#1091, source kind); named
/// apart from both apps' own `SourceKind`, which Swift's tests would find
/// ambiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DocumentSourceKind {
    Pdf,
    Word,
    Spreadsheet,
    Presentation,
    Ebook,
    Image,
    /// Camera pages.
    Scan,
    Markdown,
    /// Plain text, and pasted and new documents: the origin tells them
    /// apart.
    Text,
}

/// A Project as lists and its page show it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    /// Its colour's hue in degrees (0..360): one of the eight swatches.
    pub hue: u32,
    pub pinned: bool,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub updated: u64,
    /// Its documents.
    pub documents: u64,
}

/// A Vault document as lists show it, from the manifest without opening
/// it: its review's counts and phase as of its last save. Nothing in it
/// names a person beyond the title.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DocumentInfo {
    pub id: String,
    pub title: String,
    /// `None`: Unfiled.
    pub project_id: Option<String>,
    pub source_kind: DocumentSourceKind,
    pub origin: DocumentOrigin,
    pub exclude_from_ai: bool,
    pub phase: DocumentPhase,
    pub scanned: bool,
    /// Proposals left, as the snapshot counts them.
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub updated: u64,
    /// It keeps its Source Original (`Vault::original`).
    pub has_original: bool,
}

/// One of a Project's names, for its `Names` segment.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectName {
    pub alias: String,
    pub category: Category,
    /// The primary wording first, the one restore puts back.
    pub originals: Vec<String>,
}

/// A Project page's data: the Project, its documents (most recently
/// updated first) and its names that still restore.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectPage {
    pub project: ProjectInfo,
    pub documents: Vec<DocumentInfo>,
    pub names: Vec<ProjectName>,
}

/// What deleting a Project does with its documents; the app asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ProjectDocuments {
    /// Each becomes Unfiled with the names its text uses and its replies;
    /// the Project's own AI Replies are deleted with it.
    MoveToUnfiled,
    Delete,
}

/// Where a move left a document.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DocumentMove {
    /// Its text was rewritten in the aliases it has there.
    pub document: DocumentInfo,
    /// `None`: Unfiled.
    pub project: Option<ProjectInfo>,
    /// The Project it left, which keeps its names; `None`: it was Unfiled.
    pub left: Option<ProjectInfo>,
    /// For `undo_move`, while this handle lives.
    pub undo: u64,
}

/// Whether a document may leave for an AI now (#1091 Decision 19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Readiness {
    Ready,
    /// Never saved while Ready: not scanned, or proposals were left.
    NotReviewed,
    /// Its text is not the text it saved while Ready.
    ChangedSinceReview,
    /// Its text still holds this many names its Project knows (its own
    /// names while Unfiled) by an original wording.
    NamesNotReplaced {
        count: u64,
    },
}

#[uniffi::export]
impl Vault {
    /// The Vault in folder `root`, sealed with `key` (32 bytes), created
    /// if needed. A folder sealed with another key is refused, and nothing
    /// is written to it.
    #[uniffi::constructor]
    pub fn open(root: String, key: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let key = vault_key(&key)?;
        Ok(Arc::new(Self {
            store: mdoc_vault::Vault::open(root, &key)?,
            writes: Mutex::new(()),
            undo: Mutex::default(),
        }))
    }

    /// Take in what another handle wrote (the Restore extension's
    /// replies); true when anything changed.
    pub fn refresh(&self) -> Result<bool, CoreError> {
        Ok(self.store.refresh()?)
    }

    /// Object files that do not open: damaged, or sealed with another key.
    pub fn unreadable(&self) -> u64 {
        self.store.damaged().len() as u64
    }

    /// Every Project, pinned first, then most recently updated.
    pub fn projects(&self) -> Vec<ProjectInfo> {
        project_infos(&self.store)
    }

    pub fn project(&self, id: String) -> Result<ProjectInfo, CoreError> {
        Ok(self.project_info(&self.store.project(&id)?))
    }

    /// A Project page: the Project, its documents and its names.
    pub fn project_page(&self, id: String) -> Result<ProjectPage, CoreError> {
        let project = self.store.project(&id)?;
        let names = (project.mapping.identities.iter())
            .filter_map(|identity| identity.exported())
            .map(|identity| ProjectName {
                alias: identity.alias,
                category: identity.category.into(),
                originals: identity.originals.iter().map(|o| o.to_string()).collect(),
            })
            .collect();
        Ok(ProjectPage {
            documents: self.infos(self.store.documents_in(Some(&id))),
            project: self.project_info(&project),
            names,
        })
    }

    pub fn create_project(&self, name: String) -> Result<ProjectInfo, CoreError> {
        let name = project_name(&name)?;
        let _writes = self.lock_writes();
        let project = self.store.create_project(NewProject {
            name,
            ..NewProject::default()
        })?;
        Ok(self.project_info(&project))
    }

    /// Rename Project `id`; its documents keep their titles.
    pub fn rename_project(&self, id: String, name: String) -> Result<ProjectInfo, CoreError> {
        let name = project_name(&name)?;
        let _writes = self.lock_writes();
        let project = self
            .store
            .update_project(&id, |project| project.name = name)?;
        Ok(self.project_info(&project))
    }

    /// Delete Project `id`, first moving its documents to Unfiled, which
    /// deletes the Project's own AI Replies, or deleting them with theirs.
    /// A delete stopped part way is finished by deleting again.
    pub fn delete_project(&self, id: String, documents: ProjectDocuments) -> Result<(), CoreError> {
        let _writes = self.lock_writes();
        let documents = match documents {
            ProjectDocuments::MoveToUnfiled => mdoc_vault::ProjectDocuments::MoveToUnfiled,
            ProjectDocuments::Delete => mdoc_vault::ProjectDocuments::Delete,
        };
        let deleted = self.store.delete_project(&id, documents);
        // The ones that moved, counted again in Unfiled, even when the
        // delete stopped part way.
        for document in self.store.documents_in(None) {
            if document.counts.is_none() {
                let _ = self.recount(&document.id);
            }
        }
        Ok(deleted?)
    }

    /// Every document, most recently updated first.
    pub fn documents(&self) -> Vec<DocumentInfo> {
        self.infos(self.store.documents())
    }

    /// Unfiled documents, most recently updated first.
    pub fn unfiled(&self) -> Vec<DocumentInfo> {
        self.infos(self.store.documents_in(None))
    }

    pub fn document(&self, id: String) -> Result<DocumentInfo, CoreError> {
        let document = self.store.document(&id)?;
        Ok(self.info(mdoc_vault::DocumentSummary::of(&document)))
    }

    /// Rename…: document `id`'s title, trimmed. A new document becomes an
    /// ordinary one: its title stops following its first line. A document
    /// open in a `DocumentSession` is renamed there instead
    /// (`rename_in_vault`): its next save would find this change made
    /// elsewhere.
    pub fn rename_document(&self, id: String, title: String) -> Result<DocumentInfo, CoreError> {
        let title = document_title(&title)?;
        let _writes = self.lock_writes();
        let document = self.store.update_document(&id, |document| {
            document.title = title;
            if document.origin == mdoc_vault::DocumentOrigin::New {
                document.origin = mdoc_vault::DocumentOrigin::Ordinary;
            }
        })?;
        Ok(self.info(mdoc_vault::DocumentSummary::of(&document)))
    }

    /// Delete document `id`, with its original and its AI Replies.
    pub fn delete_document(&self, id: String) -> Result<(), CoreError> {
        let _writes = self.lock_writes();
        Ok(self.store.delete_document(&id)?)
    }

    /// Unfiled's multi-select delete: each of `ids`, in order; the first
    /// that fails stops it, and the ones before it stay deleted.
    pub fn delete_documents(&self, ids: Vec<String>) -> Result<(), CoreError> {
        let _writes = self.lock_writes();
        for id in ids {
            self.store.delete_document(&id)?;
        }
        Ok(())
    }

    /// Move document `id` into Project `project_id`, or Unfiled for
    /// `None`: its names line up with the ones there (a name known there
    /// takes its alias, a clashing alias the next unused number, the others
    /// there are proposed), its text is rewritten with them, and its saved
    /// review goes, as its ranges were of the old text. The Project it left
    /// keeps its names. Refused for a document already there. A document
    /// open in a `DocumentSession` is opened again afterwards.
    pub fn move_document(
        &self,
        id: String,
        project_id: Option<String>,
    ) -> Result<DocumentMove, CoreError> {
        let _writes = self.lock_writes();
        let moved = self.store.move_document(&id, project_id.as_deref())?;
        self.moved(moved)
    }

    /// `move_document` into a new Project named `name`.
    pub fn move_document_to_new_project(
        &self,
        id: String,
        name: String,
    ) -> Result<DocumentMove, CoreError> {
        let name = project_name(&name)?;
        let _writes = self.lock_writes();
        let moved = self.store.move_document_to_new_project(
            &id,
            NewProject {
                name,
                ..NewProject::default()
            },
        )?;
        self.moved(moved)
    }

    /// Undo of a move (`DocumentMove::undo`): the document moves back and
    /// its names take their old aliases there. The names the move added
    /// where it went go again where nothing there uses them, unless text
    /// was shared from there since (`text_for_ai`): a reply to it may name
    /// them.
    pub fn undo_move(&self, undo: u64) -> Result<DocumentMove, CoreError> {
        let _writes = self.lock_writes();
        let (mut back, made) = self
            .lock_undo()
            .moves
            .remove(&undo)
            .ok_or_else(|| CoreError::invalid("That move can no longer be undone."))?;
        let now = self.store.document(&back.document)?.project;
        if self
            .lock_undo()
            .shared
            .get(&now)
            .is_some_and(|at| *at > made)
        {
            back.added.clear();
        }
        let moved = self.store.undo_move(&back)?;
        self.moved(moved)
    }

    /// Whether document `id` may leave for an AI now, checked against its
    /// Project's names as they are now.
    pub fn readiness(&self, id: String) -> Result<Readiness, CoreError> {
        Ok(match self.store.readiness(&id)? {
            mdoc_vault::Readiness::Ready => Readiness::Ready,
            mdoc_vault::Readiness::NotReviewed => Readiness::NotReviewed,
            mdoc_vault::Readiness::ChangedSinceReview => Readiness::ChangedSinceReview,
            mdoc_vault::Readiness::NamesNotReplaced(count) => Readiness::NamesNotReplaced {
                count: count as u64,
            },
        })
    }

    /// Copy for AI's text of document `id` as saved, refused unless it is
    /// Ready now and not excluded from AI; nothing of it is stored.
    pub fn text_for_ai(&self, id: String) -> Result<String, CoreError> {
        let text = self.store.text_for_ai(&id)?;
        let place = self.store.document(&id)?.project;
        let mut undo = self.lock_undo();
        undo.count += 1;
        let count = undo.count;
        undo.shared.insert(place, count);
        Ok(text)
    }
}

impl Vault {
    pub(crate) fn store(&self) -> &mdoc_vault::Vault {
        &self.store
    }

    /// Held across a check and the writes it decides; never across a
    /// suspension, nor while waiting for a session's document lock.
    pub(crate) fn lock_writes(&self) -> MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_undo(&self) -> MutexGuard<'_, Undo> {
        self.undo.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn project_info(&self, project: &Project) -> ProjectInfo {
        ProjectInfo {
            documents: self.store.documents_in(Some(&project.id)).len() as u64,
            ..summary_info(&mdoc_vault::ProjectSummary::of(project))
        }
    }

    pub(crate) fn infos(&self, summaries: Vec<DocumentSummary>) -> Vec<DocumentInfo> {
        document_infos(&self.store, summaries)
    }

    fn info(&self, summary: DocumentSummary) -> DocumentInfo {
        document_info(&self.store, summary)
    }

    fn count(&self, id: &str) -> Result<crate::document_session::Counts, CoreError> {
        count(&self.store, id)
    }

    /// Record a moved document's counts, which the move cleared, so lists
    /// read them from the manifest again. The caller holds `writes`.
    fn recount(&self, id: &str) -> Result<mdoc_vault::Document, CoreError> {
        let counts = self.count(id)?;
        Ok(self
            .store
            .update_document(id, |document| document.counts = Some(counts.into()))?)
    }

    /// What a move tells the app; the caller holds `writes`.
    fn moved(&self, moved: Moved) -> Result<DocumentMove, CoreError> {
        let document = match self.recount(&moved.document.id) {
            Ok(document) => document,
            Err(_) => moved.document,
        };
        let mut undo = self.lock_undo();
        undo.count += 1;
        let handle = undo.count;
        undo.moves.insert(handle, (moved.undo, handle));
        drop(undo);
        Ok(DocumentMove {
            document: self.info(mdoc_vault::DocumentSummary::of(&document)),
            project: moved.project.as_ref().map(|p| self.project_info(p)),
            left: moved.left.as_ref().map(|p| self.project_info(p)),
            undo: handle,
        })
    }
}

/// What lists read of a Vault handle: the app's (`mdoc_vault::Vault`) or
/// the Restore extension's (`mdoc_vault::ReplyHandle`).
pub(crate) trait Reads {
    fn projects(&self) -> Vec<mdoc_vault::ProjectSummary>;
    fn documents(&self) -> Vec<DocumentSummary>;
    fn project(&self, id: &str) -> Result<Project, VaultError>;
    fn document(&self, id: &str) -> Result<mdoc_vault::Document, VaultError>;
}

macro_rules! reads {
    ($handle:ty) => {
        impl Reads for $handle {
            fn projects(&self) -> Vec<mdoc_vault::ProjectSummary> {
                <$handle>::projects(self)
            }
            fn documents(&self) -> Vec<DocumentSummary> {
                <$handle>::documents(self)
            }
            fn project(&self, id: &str) -> Result<Project, VaultError> {
                <$handle>::project(self, id)
            }
            fn document(&self, id: &str) -> Result<mdoc_vault::Document, VaultError> {
                <$handle>::document(self, id)
            }
        }
    };
}
reads!(mdoc_vault::Vault);
reads!(mdoc_vault::ReplyHandle);

/// The key as the app passes it: 32 bytes.
fn vault_key(key: &[u8]) -> Result<[u8; 32], CoreError> {
    key.try_into()
        .map_err(|_| CoreError::invalid("The Vault key must be 32 bytes."))
}

/// Every Project with its count of documents, pinned first, then most
/// recently updated.
fn project_infos(store: &impl Reads) -> Vec<ProjectInfo> {
    let documents = store.documents();
    (store.projects().iter())
        .map(|project| ProjectInfo {
            documents: (documents.iter())
                .filter(|d| d.project.as_deref() == Some(project.id.as_str()))
                .count() as u64,
            ..summary_info(project)
        })
        .collect()
}

fn document_infos(store: &impl Reads, summaries: Vec<DocumentSummary>) -> Vec<DocumentInfo> {
    summaries
        .into_iter()
        .map(|s| document_info(store, s))
        .collect()
}

/// As lists show it: the counts its last save recorded, or, when none
/// did, as opening it would count them.
fn document_info(store: &impl Reads, summary: DocumentSummary) -> DocumentInfo {
    let counts: crate::document_session::Counts = match summary.counts {
        Some(counts) => counts.into(),
        None => count(store, &summary.id).unwrap_or_default(),
    };
    DocumentInfo {
        phase: DocumentPhase::of(summary.scanned, counts.proposed),
        id: summary.id,
        title: summary.title,
        project_id: summary.project,
        source_kind: summary.source_kind.into(),
        origin: summary.origin.into(),
        exclude_from_ai: summary.exclude_from_ai,
        scanned: summary.scanned,
        proposed: counts.proposed,
        replaced: counts.replaced,
        kept: counts.kept,
        created: summary.created,
        updated: summary.updated,
        has_original: summary.source_sha256.is_some(),
    }
}

/// Document `id` as opening it counts it.
fn count(store: &impl Reads, id: &str) -> Result<crate::document_session::Counts, CoreError> {
    let document = store.document(id)?;
    let project = match &document.project {
        Some(project) => Some(store.project(project)?),
        None => None,
    };
    Ok(crate::document_session::vault_counts(
        &document,
        project.as_ref(),
    ))
}

fn summary_info(project: &mdoc_vault::ProjectSummary) -> ProjectInfo {
    ProjectInfo {
        id: project.id.clone(),
        name: project.name.clone(),
        hue: project
            .colour
            .unwrap_or_else(|| Colour::for_id(&project.id))
            .hue(),
        pinned: project.pinned,
        created: project.created,
        updated: project.updated,
        documents: 0,
    }
}

/// A Project name as typed, trimmed; a blank one is refused.
fn project_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::invalid("A Project needs a name."));
    }
    Ok(name.into())
}

/// A document title as typed, trimmed; a blank one is refused.
pub(crate) fn document_title(title: &str) -> Result<String, CoreError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CoreError::invalid("A document needs a title."));
    }
    Ok(title.into())
}

impl From<mdoc_vault::SourceKind> for DocumentSourceKind {
    fn from(kind: mdoc_vault::SourceKind) -> Self {
        use mdoc_vault::SourceKind as K;
        match kind {
            K::Pdf => Self::Pdf,
            K::Word => Self::Word,
            K::Spreadsheet => Self::Spreadsheet,
            K::Presentation => Self::Presentation,
            K::Ebook => Self::Ebook,
            K::Image => Self::Image,
            K::Scan => Self::Scan,
            K::Markdown => Self::Markdown,
            K::Text => Self::Text,
        }
    }
}

impl From<DocumentSourceKind> for mdoc_vault::SourceKind {
    fn from(kind: DocumentSourceKind) -> Self {
        match kind {
            DocumentSourceKind::Pdf => Self::Pdf,
            DocumentSourceKind::Word => Self::Word,
            DocumentSourceKind::Spreadsheet => Self::Spreadsheet,
            DocumentSourceKind::Presentation => Self::Presentation,
            DocumentSourceKind::Ebook => Self::Ebook,
            DocumentSourceKind::Image => Self::Image,
            DocumentSourceKind::Scan => Self::Scan,
            DocumentSourceKind::Markdown => Self::Markdown,
            DocumentSourceKind::Text => Self::Text,
        }
    }
}
