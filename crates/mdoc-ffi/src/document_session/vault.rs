//! A document kept in the Vault (`crate::vault::Vault`, #1091): opened by
//! id, saved with its review, and its names kept in its Project's mapping
//! or, Unfiled, in its own (Decisions 4, 16 and 17).
//!
//! A save runs under the Vault handle's write lock: the
//! document first joins its Project as stored now when another document
//! changed it since, then its names go into the Project and its record is
//! written with its text, review and counts. The Project is written
//! before the record, so a crash between them leaves at most unused names
//! there. A record another write changed since this session read it (a
//! move, a rename elsewhere) is never overwritten: open it again.
use super::{Document, DocumentOrigin, DocumentSession, ReviewSnapshot, no_document};
use crate::{
    CoreError, ReviewError,
    vault::{DocumentSourceKind, ProjectInfo, Vault},
};
use mdoc_pii::{
    restore::{ExportedIdentity, RestoreMap},
    saved::SavedReview,
};
use mdoc_vault::{Identity, Mapping, NewDocument, Project};
use std::{
    collections::HashSet,
    sync::{Arc, atomic::Ordering},
};

/// Where a document is in the Vault.
#[derive(Clone, Debug, Default)]
pub(super) struct Place {
    /// Its id, once its first save made it.
    pub id: Option<String>,
    /// The Project it joined or is in; `None`: Unfiled.
    pub project: Option<String>,
    /// Its record's revision as this session last read or wrote it.
    pub revision: u64,
    /// The Project's identities as this document last joined or saved
    /// them: while the Project still holds exactly these, no other
    /// document changed it, and a save needs no join.
    pub synced: Option<Vec<Identity>>,
    /// Its record holds a Ready fingerprint, as this session last read or
    /// wrote it: a save is owed when the document would now save Ready
    /// otherwise, even with no change of its own (the phone fell below the
    /// floor since, review 3).
    pub ready: bool,
}

/// A Vault document and its Project as `open_from_vault` read them.
pub(super) struct FromVault {
    pub document: mdoc_vault::Document,
    pub project: Option<Project>,
}

/// Where `save_into_vault` put the document.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SavedVaultDocument {
    /// Its id in the Vault.
    pub id: String,
    /// The Project it is in; `None`: Unfiled.
    pub project: Option<ProjectInfo>,
    /// The review after saving: joining its Project as stored now may have
    /// renumbered aliases another document took meanwhile.
    pub review: ReviewSnapshot,
}

/// What `save_pending_into_vault` saved.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SavedVaultChange {
    pub id: String,
    /// `None`: Unfiled.
    pub project_id: Option<String>,
    /// Another document changed the Project since this one last saved, and
    /// joining it again may have renumbered aliases: the review the app
    /// shows is out of date, `snapshot` has it as saved.
    pub rejoined: bool,
}

fn changed_elsewhere() -> CoreError {
    CoreError::project("The document changed elsewhere. Open it again.")
}

#[uniffi::export]
impl DocumentSession {
    /// Open the Vault document `id`: its review as it was saved, with no
    /// rescan, its names lined up with its Project's as they are now (a
    /// name another document renamed takes its new alias, one it added is
    /// proposed), and Undo starting afresh. A document with no saved
    /// review (a move clears it) opens with its Project's names proposed,
    /// and its aliases replaced.
    pub fn open_from_vault(
        &self,
        vault: Arc<Vault>,
        id: String,
    ) -> Result<ReviewSnapshot, CoreError> {
        let cancel = self.start_job();
        let document = vault.store().document(&id)?;
        let project = match &document.project {
            Some(project) => Some(vault.store().project(project)?),
            None => None,
        };
        let imported = super::intake::Imported {
            title: document.title.clone(),
            markdown: document.markdown.clone(),
            source_kind: document.source_kind,
            source: None,
            note: super::intake::ImportNote::default(),
            kept: None,
            #[cfg(not(target_os = "android"))]
            unlocked_docx: None,
        };
        let opened = super::Opened {
            origin: document.origin.into(),
            vault: Some(FromVault { document, project }),
        };
        self.open(imported, &cancel, Some(opened))
    }

    /// Start this document, not yet saved, in Project `project_id`, as
    /// intake from a Project page does: its names take the Project's
    /// aliases and the Project's names are proposed. A document saved
    /// already moves instead (`Vault::move_document`).
    pub fn join_project(
        &self,
        vault: Arc<Vault>,
        project_id: String,
    ) -> Result<ReviewSnapshot, CoreError> {
        let project = vault.store().project(&project_id)?;
        let _saving = self.lock_saving();
        self.with_document(|document| match &document.vault {
            Some(place) if place.project.as_deref() == Some(project_id.as_str()) => Ok(()),
            Some(place) if place.id.is_some() => Err(CoreError::project(
                "This document is saved in the Vault: move it instead.",
            )),
            Some(place) if place.project.is_some() => {
                Err(CoreError::project("This document is in another Project."))
            }
            _ => {
                document.review.join(
                    &project.mapping.exported(),
                    &project.mapping.reserved_aliases(),
                )?;
                document.review.dismiss_pairs(&project.dismissed_hints);
                document.skip = project.skipped();
                document.vault = Some(Place {
                    id: None,
                    project: Some(project_id.clone()),
                    revision: 0,
                    synced: Some(project.mapping.identities.clone()),
                    ready: false,
                });
                Ok(())
            }
        })
    }

    /// Save into the Vault: Unfiled intake on a first save, unless it
    /// joined a Project (`join_project`), and its text, review and counts
    /// every time, with its names in its Project or, Unfiled, in its own
    /// mapping. The document first joins its Project as stored now, so
    /// names another document added meanwhile keep their aliases and this
    /// one's move to free numbers.
    pub fn save_into_vault(&self, vault: Arc<Vault>) -> Result<SavedVaultDocument, CoreError> {
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.vault.get_or_insert_with(Place::default);
        loop {
            let mut pending = document.vault_pending();
            pending.by_hand = self.by_hand.load(Ordering::Relaxed);
            let written = pending.write(&vault);
            document.take_vault_written(&pending);
            match written? {
                Some(written) => {
                    let project = written.project.as_ref().map(|p| vault.project_info(p));
                    let id = written.id.clone();
                    document.vault_saved_as(&pending, written);
                    document.rejoined = false;
                    return Ok(SavedVaultDocument {
                        id,
                        project,
                        review: document.snapshot(),
                    });
                }
                None => document.rejoin_vault(&vault)?,
            }
        }
    }

    /// Save what changed since the last save, as `save_into_vault` does;
    /// none when nothing did, no document is read, or the document was
    /// never saved into the Vault (its first save is `save_into_vault`).
    /// Several review actions before it make one save. Safe to call from
    /// any thread while review actions go on: an action during the save is
    /// not in it and stays unsaved. A save that fails keeps the change
    /// unsaved, for the next call. A document replaced by an import or open
    /// during the save is not recorded as saved: save before replacing it.
    pub fn save_pending_into_vault(
        &self,
        vault: Arc<Vault>,
    ) -> Result<Option<SavedVaultChange>, ReviewError> {
        let _saving = self.lock_saving();
        let by_hand = self.by_hand.load(Ordering::Relaxed);
        loop {
            let mut pending = {
                let guard = self.lock_document();
                match guard.as_ref() {
                    Some(document)
                        if document.saved.is_some()
                            && (document.has_unsaved_changes() || document.ready_owed(by_hand))
                            && (document.vault.as_ref()).is_some_and(|p| p.id.is_some()) =>
                    {
                        document.vault_pending()
                    }
                    _ => return Ok(None),
                }
            };
            pending.by_hand = self.by_hand.load(Ordering::Relaxed);
            let written = pending.write(&vault);
            let mut guard = self.lock_document();
            let Some(document) = guard
                .as_mut()
                .filter(|document| document.generation == pending.generation)
            else {
                return written.map(|_| None).map_err(Into::into);
            };
            document.take_vault_written(&pending);
            match written? {
                Some(written) => {
                    let change = SavedVaultChange {
                        id: written.id.clone(),
                        project_id: written.project.as_ref().map(|p| p.id.clone()),
                        rejoined: std::mem::take(&mut document.rejoined),
                    };
                    document.vault_saved_as(&pending, written);
                    return Ok(Some(change));
                }
                None => {
                    document.rejoin_vault(&vault)?;
                    document.rejoined = true;
                }
            }
        }
    }

    /// Rename…: this document's title, in its record too once saved into
    /// the Vault. A new document becomes an ordinary one: its title stops
    /// following its first line. The title as kept: trimmed.
    pub fn rename_in_vault(&self, vault: Arc<Vault>, title: String) -> Result<String, CoreError> {
        let title = crate::vault::document_title(&title)?;
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        document.retitle_in_vault(&vault, &title, DocumentOrigin::Ordinary)?;
        Ok(title)
    }

    /// `follow_first_line` for a document in the Vault, or to be saved
    /// there: a new document takes `first_line_title` of its text as its
    /// title when that changed since it last followed, and stays a new
    /// document. `None` when nothing changed.
    pub fn follow_first_line_in_vault(
        &self,
        vault: Arc<Vault>,
    ) -> Result<Option<String>, CoreError> {
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        if document.origin != DocumentOrigin::New {
            return Ok(None);
        }
        let Some(title) = super::origin::title_from(&document.review.text) else {
            return Ok(None);
        };
        let unchanged = match &document.followed {
            Some(followed) => *followed == title,
            None => document.title == title,
        };
        if !unchanged {
            document.retitle_in_vault(&vault, &title, DocumentOrigin::New)?;
        }
        document.followed = Some(title.clone());
        Ok((!unchanged).then_some(title))
    }

    /// `discard_if_empty` for a document in the Vault, or to be saved
    /// there: a new document left empty, with no notes either, goes, its
    /// record too once saved. True when it went; the session then holds no
    /// document.
    pub fn discard_if_empty_in_vault(&self, vault: Arc<Vault>) -> Result<bool, CoreError> {
        let _saving = self.lock_saving();
        let mut guard = self.lock_document();
        let Some(document) = guard.as_ref() else {
            return Ok(false);
        };
        if document.origin != DocumentOrigin::New
            || !document.review.text.trim().is_empty()
            || !document.notes.trim().is_empty()
        {
            return Ok(false);
        }
        if let Some(id) = document.vault.as_ref().and_then(|p| p.id.as_ref()) {
            vault.delete_document(id.clone())?;
        }
        *guard = None;
        Ok(true)
    }

    /// The file family a first save into the Vault records, when the app
    /// knows better than the import: a camera page is `Scan`, where
    /// `import_images_resuming` takes every image as `Image`. A document saved
    /// already keeps the kind it was saved with.
    pub fn set_source_kind(&self, kind: DocumentSourceKind) -> Result<(), CoreError> {
        let mut guard = self.lock_document();
        guard.as_mut().ok_or_else(no_document)?.source_kind = kind.into();
        Ok(())
    }

    /// This phone is below the name finder's floor: no name finder fits
    /// it (#1091 Q4: the maintainer's (b)). Then a Vault save with no
    /// proposals left records the document Ready, its names decided by
    /// hand, so Copy for AI and Share (`Vault::text_for_ai`) take it. It
    /// stays unscanned, so lists keep their labels and a scan is still
    /// offered where one becomes possible.
    pub fn set_no_name_finder(&self, no_name_finder: bool) {
        self.by_hand.store(no_name_finder, Ordering::Relaxed);
    }
}

/// What a Vault save writes, taken from the document under its lock; then
/// written without it.
struct Pending {
    generation: u64,
    key: super::ChangeKey,
    notes: String,
    title: String,
    origin: DocumentOrigin,
    source_kind: mdoc_vault::SourceKind,
    place: Place,
    text: String,
    identities: Vec<ExportedIdentity>,
    /// Not a name (alias, wording) pairs the mapping drops on this save,
    /// and the kept ones it may take back from an undone one.
    not_names: Vec<(String, String)>,
    kept: Vec<(String, String)>,
    dismissed: Vec<(String, String)>,
    counts: super::Counts,
    /// A look for names is owed (`owed`): the counts may miss a name, so it
    /// is not Ready.
    owed: bool,
    scanned: bool,
    /// The phone is below the name finder's floor (`set_no_name_finder`).
    by_hand: bool,
    review: SavedReview,
    /// What a first save keeps as the Source Original, with what the
    /// import said it left out; `None` once the record is made.
    source: Option<(super::intake::Source, super::intake::ImportNote)>,
}

/// What a Vault save stored.
struct Written {
    id: String,
    /// The Project as stored now; `None`: Unfiled.
    project: Option<Project>,
    revision: u64,
}

impl Document {
    fn vault_pending(&self) -> Pending {
        Pending {
            generation: self.generation,
            key: self.change_key(),
            notes: self.notes.clone(),
            title: self.title.clone(),
            origin: self.origin,
            source_kind: self.source_kind,
            place: self.vault.clone().unwrap_or_default(),
            text: self.review.text.clone(),
            identities: self.review.review.export_identities(),
            not_names: self.review.review.not_a_name_pairs(),
            kept: self.review.review.kept_pairs(),
            dismissed: self.review.dismissed_pairs(),
            counts: self.review.counts(),
            owed: owed(&self.review.review),
            scanned: self.scanned,
            by_hand: false,
            review: self.review.saved_review(),
            source: (self.vault.as_ref())
                .is_none_or(|place| place.id.is_none())
                .then(|| {
                    self.source
                        .clone()
                        .map(|source| (source, self.note.clone()))
                })
                .flatten(),
        }
    }

    /// What a write made stays, whether or not the rest of it failed: the
    /// record a first save created, so the next save writes it instead of
    /// making another.
    fn take_vault_written(&mut self, pending: &Pending) {
        if let Some(place) = &mut self.vault
            && place.id.is_none()
            && pending.place.id.is_some()
        {
            place.id.clone_from(&pending.place.id);
            place.revision = pending.place.revision;
            // Kept with the record it made.
            self.source = None;
        }
    }

    /// The document is saved as it was when `pending` was taken.
    fn vault_saved_as(&mut self, pending: &Pending, written: Written) {
        self.vault = Some(Place {
            id: Some(written.id),
            synced: written
                .project
                .as_ref()
                .map(|p| p.mapping.identities.clone()),
            project: written.project.map(|p| p.id),
            revision: written.revision,
            ready: pending.ready(),
        });
        self.saved = Some(pending.key);
    }

    /// Saved, it would be Ready now (`by_hand`: below the floor) where its
    /// record is not, or the other way round.
    fn ready_owed(&self, by_hand: bool) -> bool {
        let counts = self.review.counts();
        let ready = is_ready(
            self.scanned,
            by_hand,
            counts.proposed,
            owed(&self.review.review),
        );
        self.vault
            .as_ref()
            .is_some_and(|place| place.ready != ready)
    }

    /// Join its Project as stored now: only when another document changed
    /// it since, as joining against this document's own last save would
    /// take back what Undo just reverted.
    fn rejoin_vault(&mut self, vault: &Vault) -> Result<(), CoreError> {
        let Some(place) = &mut self.vault else {
            return Ok(());
        };
        let Some(id) = place.project.clone() else {
            return Ok(());
        };
        let project = vault.store().project(&id)?;
        // This live document's own Not a name reserved an alias its undo
        // may still give back; its own decisions stand.
        let reserved = match &place.id {
            Some(document) => project.mapping.reserved_aliases_for(document),
            None => project.mapping.reserved_aliases(),
        };
        self.review.join(&project.mapping.exported(), &reserved)?;
        self.review.dismiss_pairs(&project.dismissed_hints);
        // Only once joined: a join that failed must be tried again.
        place.synced = Some(project.mapping.identities);
        Ok(())
    }

    /// The title, in its record too once saved, and the origin it takes
    /// with it (a rename makes a new document ordinary).
    fn retitle_in_vault(
        &mut self,
        vault: &Vault,
        title: &str,
        origin: DocumentOrigin,
    ) -> Result<(), CoreError> {
        let origin = if self.origin == DocumentOrigin::New {
            origin
        } else {
            self.origin
        };
        if let Some(place) = &mut self.vault
            && let Some(id) = &place.id
        {
            let _writes = vault.lock_writes();
            let stored = vault.store().document(id)?;
            if stored.revision != place.revision {
                return Err(changed_elsewhere());
            }
            let mut moved = false;
            let saved = vault.store().update_document(id, |record| {
                // Checked again under the Vault's own lock.
                if record.revision != place.revision {
                    moved = true;
                    return;
                }
                record.title = title.to_owned();
                record.origin = origin.into();
            })?;
            if moved {
                return Err(changed_elsewhere());
            }
            place.revision = saved.revision;
        }
        self.title = title.to_owned();
        self.origin = origin;
        Ok(())
    }

    /// Open as `open_from_vault` does.
    pub(super) fn open_in_vault(&mut self, from: FromVault) {
        let FromVault {
            document: stored,
            project,
        } = from;
        open_review(&mut self.review, &stored, project.as_ref());
        self.scanned = stored.scanned;
        self.notes.clone_from(&stored.notes);
        self.from_reply.clone_from(&stored.from_reply);
        if let Some(project) = &project {
            self.review.dismiss_pairs(&project.dismissed_hints);
            self.skip = project.skipped();
        }
        // As saved, unless lining its names up with its Project's changed
        // the text, or it has no saved review (a move cleared it, with its
        // Ready fingerprint): then the next save stores them.
        let as_saved = self.review.text == stored.markdown && stored.review.is_some();
        self.saved = Some(if as_saved {
            self.change_key()
        } else {
            ((0, 0, 0), false, 0)
        });
        self.vault = Some(Place {
            id: Some(stored.id),
            synced: project.as_ref().map(|p| p.mapping.identities.clone()),
            project: project.map(|p| p.id),
            revision: stored.revision,
            ready: stored.ready_fingerprint.is_some(),
        });
    }
}

/// `review`, just made from `stored`'s text, as `open_from_vault` opens
/// it: its saved review, joined with its Project's names as they are now
/// with Undo starting after that, or, without a review that fits the text,
/// as a reopened member, with the names of its Project or its own mapping.
fn open_review(
    review: &mut super::review::ReviewText,
    stored: &mdoc_vault::Document,
    project: Option<&Project>,
) {
    let mapping: &Mapping = project.map_or(&stored.mapping, |p| &p.mapping);
    let restored = (stored.review.clone()).is_some_and(|saved| review.reopen(saved).is_ok());
    // Every retired alias stays retired, this document's own Not a name's
    // too: Undo starts afresh, so nothing here can give it back.
    let reserved = mapping.reserved_aliases();
    if !restored {
        review.open_mapping(&mapping.exported(), &reserved);
    } else {
        review.review.reserve_aliases(&reserved);
        if project.is_some() {
            // A join that cannot apply leaves the review as it was saved.
            let _ = review.join(&mapping.exported(), &reserved);
            review.forget_undo();
        }
    }
    review.review.leave_off(&mapping.not_names_of(&stored.id));
}

/// Whether a save records the document Ready: scanned, no proposals left
/// and no discovery or scan of edited text owed (`owed`); below the floor
/// (`by_hand`), names decided by hand count as scanned (#1091 Q4: the
/// maintainer's (b)). The one rule.
fn is_ready(scanned: bool, by_hand: bool, proposed: u64, owed: bool) -> bool {
    (scanned || by_hand) && proposed == 0 && !owed
}

/// Whether `review` owes a look for names before it can be Ready: typing
/// took a proposal away and discovery has not run since, or an edit
/// changed a paragraph not scanned since (`scan_edits`).
fn owed(review: &mdoc_pii::Review) -> bool {
    review.awaiting_discovery() || review.edits_unscanned()
}

/// What a Vault document counts as `open_from_vault` opens it in
/// `project`, for a list when its record holds no counts (a move cleared
/// them).
pub(crate) fn vault_counts(
    stored: &mdoc_vault::Document,
    project: Option<&Project>,
) -> super::Counts {
    let mut review = super::review::ReviewText::new(stored.markdown.clone());
    open_review(&mut review, stored, project);
    review.counts()
}

impl Pending {
    /// Whether the record is Ready: scanned, no proposals left and no look
    /// for names owed. On a phone below the name finder's floor
    /// (`set_no_name_finder`), names decided by hand with none left count
    /// too (#1091 Q4: the maintainer's (b)); the record stays unscanned.
    /// This is the one place that decides.
    fn ready(&self) -> bool {
        is_ready(self.scanned, self.by_hand, self.counts.proposed, self.owed)
    }

    /// Store the names and then the record, under the Vault handle's write
    /// lock; none when another document changed the Project since this
    /// one last joined or saved it, which must be joined first.
    fn write(&mut self, vault: &Vault) -> Result<Option<Written>, CoreError> {
        let _writes = vault.lock_writes();
        #[cfg(test)]
        if let Some(hook) = (vault.during_save.lock())
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            hook();
        }
        let store = vault.store();
        let id = match &self.place.id {
            Some(id) => {
                let stored = store.document(id)?;
                if stored.revision != self.place.revision || stored.project != self.place.project {
                    return Err(changed_elsewhere());
                }
                id.clone()
            }
            None => {
                // Empty first, so a crash before the rest leaves no text
                // without the names it needs. An import keeps its source
                // with it, streamed in as item 3 does.
                let new = NewDocument {
                    project: self.place.project.clone(),
                    title: self.title.clone(),
                    source_kind: self.source_kind,
                    origin: self.origin.into(),
                    markdown: String::new(),
                };
                let created = match &self.source {
                    Some((source, note)) => {
                        let reader = source.reader().map_err(|error| CoreError::Io {
                            message: format!(
                                "The imported file could not be kept as the original: {error}"
                            ),
                        })?;
                        let info = mdoc_vault::OriginalInfo {
                            file_name: source.file_name.clone(),
                            password_protected: source.password_protected,
                            // The notice alone: the left-out line is beside it.
                            import_warning: note.notice.clone(),
                            left_out: note.left_out.clone(),
                        };
                        store.import_document(new, reader, info)?
                    }
                    None => store.create_document(new)?,
                };
                self.place.id = Some(created.id.clone());
                self.place.revision = created.revision;
                created.id
            }
        };
        let project = match self.place.project.clone() {
            Some(project) => match self.write_names(vault, &project, &id)? {
                Some(project) => Some(project),
                None => return Ok(None),
            },
            None => None,
        };
        let ready = self.ready();
        // Checked again under the Vault's own lock: another handle's write
        // since is never overwritten (it is rewritten unchanged instead).
        let (revision, place) = (self.place.revision, self.place.project.clone());
        let mut moved = false;
        let saved = store.update_document(&id, |record| {
            if record.revision != revision || record.project != place {
                moved = true;
                return;
            }
            record.title.clone_from(&self.title);
            record.origin = self.origin.into();
            record.markdown.clone_from(&self.text);
            record.notes.clone_from(&self.notes);
            // An import's first save holds its converted text, even when
            // that is empty. A record opened before its import finished
            // stays waiting for its conversion.
            if self.source.is_some() {
                record.unconverted = false;
            }
            record.scanned = self.scanned;
            record.review = Some(self.review.clone());
            record.counts = Some(self.counts.into());
            if record.project.is_none() {
                // Unfiled: no other document shares its names.
                record.mapping.merge_from(&self.identities, Some(&id));
                record.mapping.refill(&self.kept, &id);
                (record.mapping).drop_wordings(&self.not_names, &id, |_| false);
            }
            record.set_ready(ready);
        })?;
        if moved {
            return Err(changed_elsewhere());
        }
        self.place.revision = saved.revision;
        Ok(Some(Written {
            id,
            project,
            revision: saved.revision,
        }))
    }

    /// Merge the names into Project `id` (refilled, dropped as Not a name
    /// where no other document uses them, merge hints dismissed), unless
    /// another document changed it since this one last joined or saved it:
    /// then `None`.
    fn write_names(
        &self,
        vault: &Vault,
        id: &str,
        document: &str,
    ) -> Result<Option<Project>, CoreError> {
        let store = vault.store();
        let project = store.project(id)?;
        if self.place.synced.as_ref() != Some(&project.mapping.identities) {
            return Ok(None);
        }
        let mut changed = project.clone();
        changed.mapping.merge_from(&self.identities, Some(document));
        changed.mapping.refill(&self.kept, document);
        // Under the aliases the merge gave.
        let used = self.used_elsewhere(vault, &changed.mapping, id, document);
        (changed.mapping).drop_wordings(&self.not_names, document, |identity| {
            used.contains(&identity.alias)
        });
        for (a, b) in &self.dismissed {
            let pair = (a.clone(), b.clone());
            let flipped = (b.clone(), a.clone());
            if !changed.dismissed_hints.contains(&pair)
                && !changed.dismissed_hints.contains(&flipped)
            {
                changed.dismissed_hints.push(pair);
            }
        }
        if changed.mapping == project.mapping && changed.dismissed_hints == project.dismissed_hints
        {
            return Ok(Some(project));
        }
        let mut raced = false;
        let stored = store.update_project(id, |stored| {
            // Projects are written under the handle's write lock, held
            // here; another handle's write since is joined first.
            if stored.mapping != project.mapping {
                raced = true;
                return;
            }
            stored.mapping = changed.mapping.clone();
            stored.dismissed_hints = changed.dismissed_hints.clone();
        })?;
        Ok((!raced).then_some(stored))
    }

    /// The aliases of `mapping` restored in Project `project`'s other
    /// documents' saved text, so this one's Not a name leaves those
    /// wordings alone; every alias while a document cannot be read, as it
    /// may be one of them.
    fn used_elsewhere(
        &self,
        vault: &Vault,
        mapping: &Mapping,
        project: &str,
        document: &str,
    ) -> HashSet<String> {
        if self.not_names.is_empty() {
            return HashSet::new();
        }
        let store = vault.store();
        let map = RestoreMap::new(mapping.exported());
        let all = || -> Vec<String> { map.identities().iter().map(|i| i.alias.clone()).collect() };
        let unreadable = store.refresh().is_err()
            || (store.damaged().iter()).any(|(kind, _)| *kind == mdoc_vault::ObjectKind::Document);
        if unreadable {
            return all().into_iter().collect();
        }
        (store.documents_in(Some(project)).iter())
            .filter(|other| other.id != document)
            .flat_map(|other| match store.document(&other.id) {
                Ok(other) => map.restored_aliases(&other.markdown),
                Err(_) => all(),
            })
            .collect()
    }
}

impl From<super::Counts> for mdoc_vault::ReviewCounts {
    fn from(counts: super::Counts) -> Self {
        Self {
            proposed: counts.proposed,
            replaced: counts.replaced,
            kept: counts.kept,
        }
    }
}

impl From<mdoc_vault::ReviewCounts> for super::Counts {
    fn from(counts: mdoc_vault::ReviewCounts) -> Self {
        Self {
            proposed: counts.proposed,
            replaced: counts.replaced,
            kept: counts.kept,
        }
    }
}

impl From<mdoc_vault::DocumentOrigin> for DocumentOrigin {
    fn from(origin: mdoc_vault::DocumentOrigin) -> Self {
        match origin {
            mdoc_vault::DocumentOrigin::Ordinary => Self::Ordinary,
            mdoc_vault::DocumentOrigin::New => Self::New,
            mdoc_vault::DocumentOrigin::Sample => Self::Sample,
        }
    }
}

impl From<DocumentOrigin> for mdoc_vault::DocumentOrigin {
    fn from(origin: DocumentOrigin) -> Self {
        match origin {
            DocumentOrigin::Ordinary => Self::Ordinary,
            DocumentOrigin::New => Self::New,
            DocumentOrigin::Sample => Self::Sample,
        }
    }
}

#[cfg(test)]
mod tests;
