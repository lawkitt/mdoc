//! A Vault document's private notes (#1091 Decision 3): freeform text the
//! user keeps about it, saved with it by the same save as its review
//! (`save_pending_into_vault`), so they share its revision check. They
//! never reach Copy for AI, Share or a snapshot.
//!
//! And the AI Reply a promoted document came from
//! (`Vault::promote_reply`), for the way back to it.
use super::{DocumentSession, no_document};
use crate::CoreError;

#[uniffi::export]
impl DocumentSession {
    /// The document's notes as typed; empty when it has none or no
    /// document is read.
    pub fn notes(&self) -> String {
        (self.lock_document().as_ref())
            .map(|document| document.notes.clone())
            .unwrap_or_default()
    }

    /// Replace the notes. Not an undo step; the next save writes them
    /// (`save_pending_into_vault`), as it does a review action.
    pub fn set_notes(&self, notes: String) -> Result<(), CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        if document.notes != notes {
            document.notes = notes;
            document.notes_edits += 1;
        }
        Ok(())
    }

    /// The id of the AI Reply this document was promoted from, as its
    /// record says; the reply may have been deleted since
    /// (`Vault::reply_info`).
    pub fn from_reply(&self) -> Option<String> {
        self.lock_document().as_ref()?.from_reply.clone()
    }
}

#[cfg(test)]
mod tests;
