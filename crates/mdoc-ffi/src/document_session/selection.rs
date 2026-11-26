//! A selection the user copies by hand (#1209, ADR 0109 as amended): the
//! user's own copy, never blocked, so not an exit (`crate::exits`). The
//! core says what it holds, for the copy menu and the clipboard's marking,
//! and "Replace N and copy" replaces its proposals as one undo step.
use super::{DocumentSession, ReviewSnapshot, no_document, span};
use crate::{CoreError, text::TextRange, vault::Vault};
use mdoc_pii::{
    restore::{ExportedIdentity, RestoreMap},
    selection,
};
use std::sync::{Arc, atomic::Ordering};

/// What a selection holds: its copy menu, and whether its copy is marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SelectionCheck {
    /// Scanned and unchanged since, no proposal in it, no name its Project
    /// knows written out: Copy, unmarked.
    Clean,
    /// Scanned, not clean; its copy is marked. With `proposals`, "Replace
    /// N and copy" (N = `proposals`), then "Copy with names"; without,
    /// "Copy with names" alone.
    NotClean {
        /// Pending proposals in it or across its edges.
        proposals: u64,
        /// A paragraph it touches changed since it was scanned.
        changed_since_scan: bool,
        /// Names its Project knows still written out once its proposals
        /// are replaced (a Keep, a name another document added).
        known_names: u64,
    },
    /// The document was never scanned: "Copy with names" alone, marked.
    NeverScanned,
}

impl From<selection::SelectionCheck> for SelectionCheck {
    fn from(check: selection::SelectionCheck) -> Self {
        match check {
            selection::SelectionCheck::Clean => Self::Clean,
            selection::SelectionCheck::NeverScanned => Self::NeverScanned,
            selection::SelectionCheck::NotClean(not) => Self::NotClean {
                proposals: not.proposals as u64,
                changed_since_scan: not.changed_since_scan,
                known_names: not.known_names as u64,
            },
        }
    }
}

/// What `replace_selection` did.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SelectionReplaced {
    pub snapshot: ReviewSnapshot,
    /// The selection in the snapshot's text, widened over a proposal
    /// across its edge so its alias is whole: what to copy, after asking
    /// `check_selection` of it again for the marking.
    pub range: TextRange,
    /// Proposals replaced; 0 made no undo step.
    pub replaced: u64,
}

#[uniffi::export]
impl DocumentSession {
    /// What `range` (UTF-8 offsets in the snapshot's text) holds. Clean
    /// when the document was scanned (below the floor, names decided by
    /// hand and checked by the user count, as for Ready) and the
    /// paragraphs the range touches have
    /// not changed since, no proposal is in it or across its edges, and no
    /// name its Project knows is written out in it: the names
    /// `Vault::readiness` checks, read now, with this review's own. A range
    /// cutting into a word is checked as the whole word. Excluded from AI
    /// does not matter here. Writes nothing. The Vault is read without the
    /// document's lock; a document opened or moved meanwhile is read again.
    pub fn check_selection(
        &self,
        vault: Arc<Vault>,
        range: TextRange,
    ) -> Result<SelectionCheck, CoreError> {
        let range = span(range);
        let by_hand = self.by_hand.load(Ordering::Relaxed);
        // Where the document is, and which: the names are read for that one.
        let at = |document: &super::Document| {
            let place = (document.vault.as_ref()).map(|p| (p.id.clone(), p.project.clone()));
            (document.generation, place)
        };
        for _ in 0..3 {
            let read = {
                let guard = self.lock_document();
                let document = guard.as_ref().ok_or_else(no_document)?;
                document.review.in_text(&range)?;
                if !(document.scanned || (by_hand && document.names_checked())) {
                    return Ok(SelectionCheck::NeverScanned);
                }
                at(document)
            };
            let mut identities: Vec<ExportedIdentity> = match &read.1 {
                Some((Some(id), _)) => vault.store().known_identities(id)?,
                Some((None, Some(project))) => vault.store().project(project)?.mapping.exported(),
                _ => Vec::new(),
            };
            let guard = self.lock_document();
            let document = guard.as_ref().ok_or_else(no_document)?;
            if at(document) != read {
                continue;
            }
            identities.extend(document.review.review.export_identities());
            let known = RestoreMap::new(identities);
            return (document.review)
                .check_selection(range, true, &known)
                .map(Into::into);
        }
        Err(CoreError::review(
            "The document changed. Select the text again.",
        ))
    }

    /// "Replace N and copy": the proposals in `range` or across its edges
    /// replaced as Replace all replaces them, with the aliases there whose
    /// identity was renamed since, as one "Replace in selection" undo step
    /// (its subject the count). The other proposals stay.
    pub fn replace_selection(&self, range: TextRange) -> Result<SelectionReplaced, CoreError> {
        let mut guard = self.lock_document();
        let document = guard.as_mut().ok_or_else(no_document)?;
        let (after, replaced) = document.review.replace_selection(span(range))?;
        Ok(SelectionReplaced {
            snapshot: document.snapshot(),
            range: TextRange::of(&after),
            replaced: replaced as u64,
        })
    }
}

#[cfg(test)]
mod tests;
