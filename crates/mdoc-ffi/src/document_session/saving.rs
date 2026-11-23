//! Saving after the tap (calls stay synchronous while they are cheap): a
//! review action changes the document only, and `save_pending_into_vault`
//! saves what changed since the last save, however many actions that was,
//! from any thread. The document lock is held only to take what to write,
//! to record what was written and, when another document changed the
//! Project meanwhile, to join it again; never across the write, so a tap
//! during a slow save returns at once.
use super::{Document, DocumentSession};
use std::sync::MutexGuard;

impl DocumentSession {
    /// Whether the document changed since it was last saved.
    #[cfg(test)]
    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.lock_document()
            .as_ref()
            .is_some_and(Document::has_unsaved_changes)
    }

    /// Held across a whole save, and by every call that changes what a
    /// save writes to (the record, the Project): one save at a time, and
    /// none of them in the middle of another's. Taken before the document
    /// lock, never while holding it. Review actions never take it.
    pub(super) fn lock_saving(&self) -> MutexGuard<'_, ()> {
        self.saving.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Document {
    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.saved != Some(self.change_key())
    }
}

#[cfg(test)]
mod tests;
