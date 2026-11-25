//! Moving a Document into or out of a Project, and deleting a Project
//! (#1091 Decisions 4 and 9), by Document id.
use super::{Vault, WriteLock};
use crate::{
    VaultError, is_id,
    mapping::{Identity, Mapping},
    records::{Document, NewProject, Project, Reply, ReplyOwner, now},
};
use mdoc_pii::{
    Review,
    restore::{ExportedIdentity, RestoreMap, plan_join},
};

/// What a move did.
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
    /// The Document as saved in its new place, its text in the aliases it
    /// has there.
    pub document: Document,
    /// The Project it is in now; `None`: Unfiled.
    pub project: Option<Project>,
    /// The Project it left, as stored after it left; `None` when it was
    /// Unfiled. It keeps its identities, so replies to text already sent
    /// still restore with it.
    pub left: Option<Project>,
    /// What [`Vault::undo_move`] takes to move it back.
    pub undo: MoveUndo,
}

/// How to move a Document back where a move took it from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveUndo {
    pub document: String,
    /// The Project it left; `None`: Unfiled.
    pub project: Option<String>,
    /// Its own mapping when it left Unfiled, so its names take their old
    /// aliases again; empty when it left a Project.
    pub mapping: Mapping,
    /// Ids of the identities the move added where it went. Moving back
    /// takes out those nothing there uses; a caller clears them when text
    /// shared from there since may name them.
    pub added: Vec<String>,
}

/// What deleting a Project does with its Documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectDocuments {
    /// Each becomes Unfiled with the names its text uses.
    MoveToUnfiled,
    Delete,
}

/// Where a move goes.
enum Target<'a> {
    Project(&'a str),
    /// A new Project, set up before its first write.
    New(NewProject, Box<dyn FnOnce(&mut Project) + 'a>),
    /// Unfiled, starting from this mapping of its own.
    Unfiled(Mapping),
}

/// Where a move goes, loaded.
enum Landing {
    Project(Project),
    Unfiled(Mapping),
}

impl Landing {
    fn mapping(&self) -> &Mapping {
        match self {
            Self::Project(project) => &project.mapping,
            Self::Unfiled(mapping) => mapping,
        }
    }
}

impl Vault {
    /// Move a Document into Project `to`, or Unfiled for `None`. Its names
    /// line up with the mapping it joins: a name known there takes its
    /// alias, a clashing alias the next unused number, the other names
    /// there are proposed, and its text is rewritten with the new aliases.
    /// The Project it left keeps its identities. Refused for a Document
    /// already there.
    ///
    /// The Project joined is saved first, then the Document, then the
    /// Project left; a step that fails puts the earlier ones back, and a
    /// crash between them leaves at most unused names in the Project
    /// joined. A copy of the Document saved after the move is skipped as
    /// stale: open it again.
    pub fn move_document(&self, document: &str, to: Option<&str>) -> Result<Moved, VaultError> {
        let lock = self.write_lock()?;
        let target = match to {
            Some(id) => Target::Project(id),
            None => Target::Unfiled(Mapping::default()),
        };
        self.move_locked(&lock, document, target, &[])
    }

    /// [`Self::move_document`] into a new Project. A move that fails leaves
    /// no new Project behind; a crash may leave it, empty.
    pub fn move_document_to_new_project(
        &self,
        document: &str,
        new: NewProject,
    ) -> Result<Moved, VaultError> {
        self.move_document_to_new_project_with(document, new, |_| {})
    }

    /// [`Self::move_document_to_new_project`], the new Project set up by
    /// `setup` (its categories, its dismissed hints) before its first
    /// write: one move, so a failure leaves neither the Project nor the
    /// move behind.
    pub fn move_document_to_new_project_with(
        &self,
        document: &str,
        new: NewProject,
        setup: impl FnOnce(&mut Project),
    ) -> Result<Moved, VaultError> {
        let lock = self.write_lock()?;
        self.move_locked(&lock, document, Target::New(new, Box::new(setup)), &[])
    }

    /// Undo a move: the Document moves back as [`Self::move_document`]
    /// moves it, its names taking their old aliases there (a name added
    /// meanwhile takes a fresh number). From where it leaves now, the
    /// identities the move added go again unless another Document there
    /// still uses their alias.
    pub fn undo_move(&self, undo: &MoveUndo) -> Result<Moved, VaultError> {
        let lock = self.write_lock()?;
        let target = match &undo.project {
            Some(id) => Target::Project(id),
            None => Target::Unfiled(undo.mapping.clone()),
        };
        self.move_locked(&lock, &undo.document, target, &undo.added)
    }

    /// Delete a Project, first moving its Documents to Unfiled or deleting
    /// them, then its own AI Replies; a Document's replies follow it. Later
    /// saves of it by this handle are skipped. A crash part way leaves the
    /// Project with the Documents and replies not yet done; deleting again
    /// finishes. A Document that cannot be read for now refuses the delete,
    /// as it may be in the Project; one that never will (another key,
    /// damaged) is passed over.
    pub fn delete_project(&self, id: &str, documents: ProjectDocuments) -> Result<(), VaultError> {
        self.delete_project_of(id, documents, None).map(|_| ())
    }

    /// [`Self::delete_project`], only while the Project's Documents are
    /// still `members`, the ones the user was asked about: compared under
    /// the write lock, so a Document filed in meanwhile is never deleted
    /// unasked. `Ok(false)` when they differ, and nothing is changed.
    pub fn delete_project_with(
        &self,
        id: &str,
        documents: ProjectDocuments,
        members: &[String],
    ) -> Result<bool, VaultError> {
        self.delete_project_of(id, documents, Some(members))
    }

    fn delete_project_of(
        &self,
        id: &str,
        documents: ProjectDocuments,
        members: Option<&[String]>,
    ) -> Result<bool, VaultError> {
        if !is_id(id) {
            return Err(VaultError::InvalidId);
        }
        let lock = self.write_lock()?;
        self.load_record::<Project>(id)?;
        self.refresh()?;
        let unopened: Vec<String> =
            (self.state().manifest.documents.damaged.keys().cloned()).collect();
        for document in unopened {
            match self.load_record::<Document>(&document) {
                Err(VaultError::Io(error)) => return Err(VaultError::Io(error)),
                _ => continue,
            }
        }
        if let Some(members) = members {
            let mut now: Vec<String> = (self.documents_in(Some(id)).into_iter())
                .map(|summary| summary.id)
                .collect();
            let mut asked = members.to_vec();
            now.sort();
            asked.sort();
            if now != asked {
                return Ok(false);
            }
        }
        // Its AI activity first: a failure there leaves the Project whole.
        self.delete_activity_locked(&lock, id)?;
        for document in self.documents_in(Some(id)) {
            match documents {
                ProjectDocuments::MoveToUnfiled => {
                    let target = Target::Unfiled(Mapping::default());
                    self.move_locked(&lock, &document.id, target, &[])?;
                }
                ProjectDocuments::Delete => self.delete_document_locked(&lock, &document.id)?,
            }
        }
        // Its own replies go with its mapping; its Documents' follow them.
        self.delete_replies_of(&lock, &ReplyOwner::Project(id.to_string()))?;
        self.check_alive()?;
        super::remove_sealed(&crate::manifest::project_path(&self.root, id))?;
        self.forget_deleted::<Project>(id)?;
        Ok(true)
    }

    fn move_locked(
        &self,
        lock: &WriteLock<'_>,
        document: &str,
        target: Target<'_>,
        take_back: &[String],
    ) -> Result<Moved, VaultError> {
        if !is_id(document) {
            return Err(VaultError::InvalidId);
        }
        let stored: Document = self.load_record(document)?;
        self.next_revision(&stored)?;
        let from: Option<Project> = match &stored.project {
            Some(id) => match self.load_record(id) {
                Err(VaultError::NotFound { .. }) => None,
                loaded => Some(loaded?),
            },
            None => None,
        };
        let (to, made) = match target {
            Target::Project(id) if stored.project.as_deref() == Some(id) => {
                return Err(VaultError::AlreadyThere);
            }
            Target::Project(id) => (Landing::Project(self.load_record(id)?), None),
            Target::New(new, setup) => {
                let project = self.create_project_locked(lock, new, setup)?;
                (Landing::Project(project.clone()), Some(project.id))
            }
            Target::Unfiled(_) if stored.project.is_none() => return Err(VaultError::AlreadyThere),
            Target::Unfiled(mapping) => (Landing::Unfiled(mapping), None),
        };
        let moved = self.move_into(lock, &stored, from, &to, take_back);
        if moved.is_err()
            && self.check_alive().is_ok()
            && let Some(made) = made
        {
            let _ = super::remove_sealed(&crate::manifest::project_path(&self.root, &made));
            let _ = self.forget_deleted::<Project>(&made);
        }
        moved
    }

    /// The move's writes, each putting the earlier ones back when it fails.
    fn move_into(
        &self,
        lock: &WriteLock<'_>,
        stored: &Document,
        from: Option<Project>,
        to: &Landing,
        take_back: &[String],
    ) -> Result<Moved, VaultError> {
        let from_mapping = from.as_ref().map_or(&stored.mapping, |p| &p.mapping);
        let (text, exported, ids) =
            realias(stored, from_mapping, to.mapping()).map_err(VaultError::Names)?;
        // Its replies follow it, so the names they are bound to come along
        // even where its text no longer uses them.
        let bound = self.reply_identities(&stored.id, from_mapping, &ids)?;
        let mut added = Vec::new();
        let mut gather = |before: &Mapping, after: &Mapping| {
            added = (after.identities.iter())
                .filter(|i| before.identity(&i.id).is_none())
                .map(|i| i.id.clone())
                .collect();
        };
        let (joined, local) = match to {
            Landing::Project(project) => {
                let joined = self.update_locked::<Project>(lock, &project.id, |joined| {
                    let before = joined.mapping.clone();
                    joined.mapping.merge_with_ids(&exported, &ids, None);
                    joined.mapping.adopt(&bound);
                    gather(&before, &joined.mapping);
                })?;
                (Some(joined), Mapping::default())
            }
            Landing::Unfiled(mapping) => {
                let mut local = mapping.clone();
                local.merge_with_ids(&exported, &ids, None);
                local.adopt(&bound);
                gather(mapping, &local);
                (None, local)
            }
        };
        // The Project joined as it was, to put back when a later step fails.
        let before = match to {
            Landing::Project(project) => Some(project),
            Landing::Unfiled(_) => None,
        };
        let mut moved = stored.clone();
        moved.markdown = text;
        moved.mapping = local;
        moved.project = joined.as_ref().map(|p| p.id.clone());
        // Readiness was of the text in the mapping it left.
        moved.ready_fingerprint = None;
        // The review's ranges and identities were of that text and mapping
        // too: the next open scans again.
        moved.review = None;
        moved.counts = None;
        // Its text is still to be made, though the move writes it.
        moved.unconverted = stored.pending_conversion();
        let moved = match self.place_document(lock, moved) {
            Ok(moved) => moved,
            Err(error) => {
                if let Some(before) = before {
                    let _ = self.put_back(lock, before);
                }
                return Err(error);
            }
        };
        let left = match from {
            Some(from) => match self.drop_unused(lock, &from, &stored.id, take_back) {
                Ok(left) => Some(left),
                Err(error) => {
                    let _ = self.place_document(lock, stored.clone());
                    if let Some(before) = before {
                        let _ = self.put_back(lock, before);
                    }
                    return Err(error);
                }
            },
            None => None,
        };
        Ok(Moved {
            undo: MoveUndo {
                document: stored.id.clone(),
                project: stored.project.clone(),
                mapping: stored.mapping.clone(),
                added,
            },
            document: moved,
            project: joined,
            left,
        })
    }

    /// Save `document` over its file as the next revision, with the Project
    /// and mapping it carries: only a move changes those.
    fn place_document(
        &self,
        lock: &WriteLock<'_>,
        mut document: Document,
    ) -> Result<Document, VaultError> {
        let stored: Document = self.load_record(&document.id)?;
        let project = document.project.take();
        let mapping = std::mem::take(&mut document.mapping);
        crate::manifest::Record::settle(
            &mut document,
            &stored,
            self.next_revision(&stored)?,
            now(),
        );
        document.project = project;
        document.mapping = mapping;
        self.write_record(lock, &document)?;
        Ok(document)
    }

    /// Save `before` over its Project as stored now: a later step failed.
    fn put_back(&self, lock: &WriteLock<'_>, before: &Project) -> Result<Project, VaultError> {
        self.update_locked::<Project>(lock, &before.id, |project| {
            *project = before.clone();
        })
    }

    /// Take out of `project` the identities named by `ids` (a move added
    /// them) that nothing there uses: no other Document's text holds their
    /// alias and no reply restoring there is bound to them. A Document or
    /// reply that cannot be read counts as using them, and so does one that
    /// does not open, as it may be the Project's. The Project as stored
    /// after.
    fn drop_unused(
        &self,
        lock: &WriteLock<'_>,
        project: &Project,
        leaving: &str,
        ids: &[String],
    ) -> Result<Project, VaultError> {
        let candidates: Vec<(String, ExportedIdentity)> = (project.mapping.identities.iter())
            .filter(|i| ids.contains(&i.id))
            .filter_map(|i| Some((i.id.clone(), i.exported()?)))
            .collect();
        if candidates.is_empty() {
            return Ok(project.clone());
        }
        let map = RestoreMap::new(candidates.iter().map(|(_, e)| e.clone()).collect());
        let mut used = vec![false; candidates.len()];
        self.refresh()?;
        if !self.state().manifest.documents.damaged.is_empty() {
            return Ok(project.clone());
        }
        let members: Vec<String> = (self.documents_in(Some(&project.id)).into_iter())
            .map(|summary| summary.id)
            .filter(|id| id != leaving)
            .collect();
        for member in &members {
            let Ok(document) = self.load_record::<Document>(member) else {
                return Ok(project.clone());
            };
            for token in map.tokens(&document.markdown) {
                if let Some(index) = token.identity {
                    used[index] = true;
                }
            }
        }
        // A reply that restores here and is bound to one uses it too; one
        // that does not open, or cannot be read, may be such a reply.
        let (replies, unopened) = self.current_replies()?;
        if unopened {
            return Ok(project.clone());
        }
        for summary in replies {
            let restores_here = match &summary.owner {
                ReplyOwner::Project(id) => *id == project.id,
                ReplyOwner::Document(id) => members.contains(id),
            };
            if !restores_here {
                continue;
            }
            let Ok(reply) = self.load_record::<Reply>(&summary.id) else {
                return Ok(project.clone());
            };
            for binding in &reply.bindings {
                let Some(identity) = project.mapping.bound(&binding.identity) else {
                    continue;
                };
                if let Some(at) = candidates.iter().position(|(id, _)| *id == identity.id) {
                    used[at] = true;
                }
            }
        }
        let unused: Vec<&str> = (candidates.iter().zip(&used))
            .filter(|(_, used)| !**used)
            .map(|((id, _), _)| id.as_str())
            .collect();
        if unused.is_empty() {
            return Ok(project.clone());
        }
        self.update_locked::<Project>(lock, &project.id, |project| {
            (project.mapping.identities).retain(|i| !unused.contains(&i.id.as_str()));
        })
    }
}

/// A Document's text and the identities it brings, moving from mapping
/// `from` into `to`: opened in `from` as a saved document reopens, its
/// Not a name wordings `from` keeps for others left off, then
/// joined to `to` (`plan_join`), so names `to` knows take its aliases,
/// clashing aliases its next free numbers, and the text is rewritten with
/// them. The identities carry no earlier aliases: those name `from`'s
/// history, which can mean other people in `to`. Beside each, the ids of the identity it came from, in `from` or in `to`: its own
/// first, then those folded into it.
#[allow(clippy::type_complexity)]
fn realias(
    document: &Document,
    from: &Mapping,
    to: &Mapping,
) -> Result<(String, Vec<ExportedIdentity>, Vec<Vec<String>>), String> {
    let live = |mapping: &Mapping| -> (Vec<Vec<String>>, Vec<ExportedIdentity>) {
        (mapping.identities.iter())
            .filter_map(|i: &Identity| Some((i.ids().cloned().collect(), i.exported()?)))
            .unzip()
    };
    let (from_ids, from_exported) = live(from);
    let (to_ids, to_exported) = live(to);
    let text = document.markdown.as_str();
    let reserved = to.reserved_aliases();
    let mut review = Review::default();
    review.refresh(text);
    review.reserve_aliases(&from.reserved_aliases());
    let seeded = review.seed(&from_exported, text);
    review.relink(text, &from_exported, &seeded, 0);
    review.leave_off(&from.not_names_of(&document.id));
    let document = review.export_identities_with_ids();
    let plan = plan_join(&to_exported, &document, &reserved);
    review.reserve_aliases(&reserved);
    review.reassign_aliases(&plan.changes)?;
    let mut corrections = review.alias_corrections(text)?;
    // The names `to` knows that this document does not are proposed.
    let unmatched: Vec<_> = (plan.unmatched.into_iter())
        .filter(|c| review.identity_by_alias(&c.alias).is_none())
        .collect();
    let proposed = review.seed(&unmatched, text);
    let (ids, exported) = (review.export_identities_with_ids().into_iter())
        .map(|(id, mut identity)| {
            identity.previous.clear();
            let source = match seeded.iter().position(|s| *s == id) {
                Some(at) => from_ids[at].clone(),
                None => (proposed.iter().position(|p| *p == id))
                    .and_then(|at| {
                        to_exported
                            .iter()
                            .position(|e| e.alias == unmatched[at].alias)
                    })
                    .map(|at| to_ids[at].clone())
                    .unwrap_or_default(),
            };
            (source, identity)
        })
        .unzip();
    corrections.sort_by_key(|plan| plan.range.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for plan in &corrections {
        out.push_str(&text[at..plan.range.start]);
        out.push_str(&plan.after);
        at = plan.range.end;
    }
    out.push_str(&text[at..]);
    Ok((out, exported, ids))
}

#[cfg(test)]
mod tests {
    use crate::{
        DocumentOrigin, NewDocument, NewProject, OriginalInfo, ProjectDocuments, SourceKind, Vault,
    };

    /// A Document imported before `unconverted` existed (revision 1, no
    /// text, the flag absent) reads as waiting for its conversion, and
    /// stays so through a write of something else and the move a Project
    /// delete makes.
    #[test]
    fn an_import_from_before_the_flag_stays_unconverted_through_a_move() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let (project, id) = {
            let vault = Vault::open(&root, &[7; 32]).unwrap();
            let project = (vault.create_project(NewProject {
                name: "Lease".into(),
                ..NewProject::default()
            }))
            .unwrap();
            let new = NewDocument {
                project: Some(project.id.clone()),
                title: "Brief".into(),
                source_kind: SourceKind::Pdf,
                origin: DocumentOrigin::Ordinary,
                markdown: String::new(),
            };
            let info = OriginalInfo {
                file_name: "a.pdf".into(),
                ..OriginalInfo::default()
            };
            let mut legacy = vault.import_document(new, &b"source"[..], info).unwrap();
            // As an import wrote it before the flag.
            legacy.unconverted = false;
            let lock = vault.write_lock().unwrap();
            vault.write_record(&lock, &legacy).unwrap();
            (project.id, legacy.id)
        };
        let vault = Vault::open(&root, &[7; 32]).unwrap();
        let stored = vault.document(&id).unwrap();
        assert!(stored.unconverted && stored.revision == 1, "filled in");
        let renamed = (vault.update_document(&id, |d| d.title = "Lease brief".into())).unwrap();
        assert!(renamed.revision > 1);
        assert!(renamed.unconverted && renamed.pending_conversion());
        vault
            .delete_project(&project, ProjectDocuments::MoveToUnfiled)
            .unwrap();
        let moved = vault.document(&id).unwrap();
        assert!(moved.revision > 1 && moved.project.is_none());
        assert!(moved.unconverted && moved.pending_conversion());
        // A save of text clears it, whoever saves.
        let saved = (vault.update_document(&id, |d| d.markdown = "# Brief".into())).unwrap();
        assert!(!saved.unconverted && !saved.pending_conversion());
    }
}
