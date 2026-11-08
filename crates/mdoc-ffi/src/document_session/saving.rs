//! Saving a document into the library, and saving after the tap (ADR 0104,
//! Calls stay synchronous while they are cheap): a review action with no
//! library changes the document only, and `save_pending` saves what
//! changed since the last save, however many actions that was, from any
//! thread. The document lock is held only to take what to write, to
//! record what was written and, when another document changed the case
//! meanwhile, to join it again; never across the write, so a tap during a
//! slow save returns at once.
use super::{Document, DocumentSession, reopened_counts};
use crate::{
    CoreError, ReviewError,
    library::{CaseInfo, Library, Recorded, Stamp},
};
use mdoc_pii::restore::ExportedIdentity;
use mdoc_store::cases::StoredIdentity;
use std::{
    path::PathBuf,
    sync::{Arc, MutexGuard},
};

/// What `save_pending` saved.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SavedChange {
    pub path: String,
    pub case: CaseInfo,
    /// Another document changed the case since this one last saved, and
    /// joining it again may have renumbered aliases: the review the app
    /// shows is out of date, `snapshot` has it as saved.
    pub rejoined: bool,
}

#[uniffi::export]
impl DocumentSession {
    /// Save what changed since the last save, as `save` does; none when
    /// nothing did, or no document is read. Several review actions before
    /// it make one save. Safe to call from any thread while review actions
    /// go on: an action during the save is not in it and stays unsaved.
    /// A save that fails keeps the change unsaved, for the next call. A
    /// document never saved is left to `save`: its first save is the
    /// import's or scan's, which may join a chosen case first (a phone
    /// locked when the job ended holds it back). A document replaced by an
    /// import or open during the save is not recorded as saved: save
    /// before replacing it.
    pub fn save_pending(&self, library: Arc<Library>) -> Result<Option<SavedChange>, ReviewError> {
        let _saving = self.lock_saving();
        loop {
            let mut pending = {
                let guard = self.lock_document();
                match guard.as_ref() {
                    Some(document)
                        if document.saved.is_some() && document.has_unsaved_changes() =>
                    {
                        document.pending()
                    }
                    _ => return Ok(None),
                }
            };
            let written = pending.write(&library);
            let mut guard = self.lock_document();
            let Some(document) = guard
                .as_mut()
                .filter(|document| document.generation == pending.generation)
            else {
                // Replaced meanwhile: what was written is the document
                // before, which the session no longer holds.
                return written.map(|_| None).map_err(Into::into);
            };
            document.take_written(&mut pending);
            match written? {
                Some(saved) => {
                    let case = saved.case.clone();
                    document.saved_as(&pending, saved);
                    return Ok(Some(SavedChange {
                        path: pending.path.unwrap_or_default().display().to_string(),
                        case,
                        rejoined: std::mem::take(&mut document.rejoined),
                    }));
                }
                // Rare: joining needs the review, so this holds the
                // document, but only while it joins.
                // Told with the save that lands, even if this one fails.
                None => {
                    document.rejoin(&library)?;
                    document.rejoined = true;
                }
            }
        }
    }
}

impl DocumentSession {
    /// Whether the document changed since it was last saved.
    #[cfg(test)]
    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.lock_document()
            .as_ref()
            .is_some_and(Document::has_unsaved_changes)
    }

    /// Held across a whole save, and by every call that changes what a
    /// save writes to (the file, the case): one save at a time, and none
    /// of them in the middle of another's. Taken before the document lock,
    /// never while holding it. Review actions never take it.
    pub(super) fn lock_saving(&self) -> MutexGuard<'_, ()> {
        self.saving.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What a save writes, taken from the document under its lock; then
/// written without it.
struct Pending {
    generation: u64,
    key: ((u64, u64, usize), bool),
    title: String,
    /// The file; reserved by the write on a first save.
    path: Option<PathBuf>,
    case: Option<String>,
    synced: Option<Vec<StoredIdentity>>,
    /// The file as last saved; as written, once `wrote`.
    stored: mdoc_store::document::Document,
    wrote: bool,
    text: String,
    identities: Vec<ExportedIdentity>,
    dismissed: Vec<(String, String)>,
    counts: super::Counts,
    scanned: bool,
}

/// What a write stored.
struct Saved {
    case: CaseInfo,
    /// The case's identities as stored now.
    synced: Vec<StoredIdentity>,
}

impl Document {
    fn has_unsaved_changes(&self) -> bool {
        self.saved != Some(self.change_key())
    }

    fn pending(&self) -> Pending {
        Pending {
            generation: self.generation,
            key: self.change_key(),
            title: self.title.clone(),
            path: self.path.clone(),
            case: self.case.clone(),
            synced: self.synced.clone(),
            stored: mdoc_store::document::Document {
                path: self.stored.path.clone(),
                saved: self.stored.saved.clone(),
            },
            wrote: false,
            text: self.review.text.clone(),
            identities: self.review.review.export_identities(),
            dismissed: self.review.dismissed_pairs(),
            counts: self.review.counts(),
            scanned: self.scanned,
        }
    }

    /// What a write did to the file stays, whether or not the rest of it
    /// failed: the path it reserved, and the file as written, so the next
    /// save does not take its own write for a change made elsewhere.
    fn take_written(&mut self, pending: &mut Pending) {
        if pending.path.is_some() {
            self.path = pending.path.clone();
        }
        if pending.wrote {
            self.stored = std::mem::take(&mut pending.stored);
        }
    }

    /// The document is saved as it was when `pending` was taken.
    fn saved_as(&mut self, pending: &Pending, saved: Saved) {
        self.case = Some(saved.case.id);
        self.synced = Some(saved.synced);
        self.saved = Some(pending.key);
    }

    /// Only a case another document changed since is joined again: joining
    /// against this document's own last save would take back what Undo
    /// just reverted.
    fn rejoin(&mut self, library: &Library) -> Result<(), CoreError> {
        let Some(id) = &self.case else {
            return Ok(());
        };
        let case = library.load(id)?;
        self.review
            .join(&case.exported(), &case.reserved_aliases())?;
        self.review.dismiss_pairs(&case.dismissed_hints);
        self.synced = Some(case.identities);
        Ok(())
    }

    /// `DocumentSession::save`.
    pub(super) fn save(&mut self, library: &Library) -> Result<super::SavedDocument, CoreError> {
        loop {
            let mut pending = self.pending();
            let written = pending.write(library);
            self.take_written(&mut pending);
            match written? {
                Some(saved) => {
                    let case = saved.case.clone();
                    self.saved_as(&pending, saved);
                    // The review returned shows any join.
                    self.rejoined = false;
                    return Ok(super::SavedDocument {
                        path: pending.path.unwrap_or_default().display().to_string(),
                        case,
                        review: self.snapshot(),
                    });
                }
                None => self.rejoin(library)?,
            }
        }
    }
}

impl Pending {
    /// Save the Markdown and merge the identities into the case, under the
    /// library's write lock; none when another document changed the case
    /// since this one last joined or saved it, which must be joined first.
    fn write(&mut self, library: &Library) -> Result<Option<Saved>, CoreError> {
        let lock = library.lock()?;
        #[cfg(test)]
        if let Some(hook) = library
            .during_save
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            hook();
        }
        let path = match &self.path {
            Some(path) => path.clone(),
            None => {
                let path = library.reserve_document_path(&self.title)?;
                self.path = Some(path.clone());
                path
            }
        };
        let member = library.member(&path);
        library.refuse_other_case(&member, self.case.as_deref().unwrap_or_default())?;
        // The case as this save stores it.
        let mut stored = None;
        let case = match self.case.clone() {
            Some(id) => library.edit_locked(&lock, &id, |case| {
                if self.synced.as_ref() != Some(&case.identities) {
                    return Ok(false);
                }
                self.stored.save(path.clone(), &self.text)?;
                self.wrote = true;
                let before = case.clone();
                case.merge(&self.identities);
                if !case.members.contains(&member) {
                    case.members.push(member.clone());
                }
                for (a, b) in &self.dismissed {
                    let pair = (a.clone(), b.clone());
                    let flipped = (b.clone(), a.clone());
                    if !case.dismissed_hints.contains(&pair)
                        && !case.dismissed_hints.contains(&flipped)
                    {
                        case.dismissed_hints.push(pair);
                    }
                }
                stored = Some(case.clone());
                Ok(*case != before)
            })?,
            None => {
                self.stored.save(path.clone(), &self.text)?;
                self.wrote = true;
                let mut case = mdoc_store::cases::Case::new(
                    self.title.clone(),
                    &self.identities,
                    Some(member),
                );
                // Hints dismissed before the case existed stay dismissed.
                case.dismissed_hints = self.dismissed.clone();
                stored = Some(case.clone());
                library.create_locked(&lock, case)?
            }
        };
        let Some(stored) = stored else {
            return Ok(None);
        };
        // The document and its case are saved: an entry that cannot be
        // written only makes it reopen, and list, as one saved before
        // entries were.
        // What opening it gives, as lists show: Keep decisions are not
        // saved, so with any the saved text and case are counted as a
        // reopen counts them (a kept name the case knows is proposed again).
        let mut counts = self.counts;
        if counts.kept > 0 {
            counts = reopened_counts(self.text.clone(), Some(&stored)).0;
        }
        let summary = Recorded::of(
            counts,
            Stamp::of_file(&path),
            Some((case.id.clone(), library.case_stamp(&case.id))),
        );
        let _ = library.record_saved(&lock, &path, self.scanned, summary);
        Ok(Some(Saved {
            case,
            synced: stored.identities,
        }))
    }
}

#[cfg(test)]
mod tests;
