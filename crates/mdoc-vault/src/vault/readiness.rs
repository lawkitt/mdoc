//! Readiness checked when text leaves (#1091 Decision 19). Ready stays
//! "scanned, and no proposals left", decided by the caller at each save
//! and kept as the fingerprint of the text last saved while Ready: on a
//! phone below the name finder's floor, "names decided by hand, none left"
//! (#1091 Q4), with `scanned` left false; the AI text is computed from the Document's
//! Markdown on every outbound read and checked against the names its
//! Project knows then. The Vault stores no AI text, so a name another
//! Document adds to the Project makes this one not Ready without touching
//! its file.
use super::Vault;
use crate::{VaultError, mapping::Mapping, records::Document};
use mdoc_pii::restore::RestoreMap;
use sha2::Digest;

/// Whether a Document may leave for an AI now: Copy for AI, Share, the
/// shelf, the AI connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// Never saved while Ready: not scanned (nor decided by hand below the
    /// floor), or proposals were left.
    NotReviewed,
    /// Its text is not the text it saved while Ready.
    ChangedSinceReview,
    /// Its text still holds this many names its Project knows (its own
    /// mapping while Unfiled) by an original wording: a name another
    /// Document added since, a Keep, or an edit.
    NamesNotReplaced(usize),
}

impl Readiness {
    pub fn is_ready(self) -> bool {
        self == Self::Ready
    }
}

/// What [`Vault::text_for_ai_in`] read of a Document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiRead {
    /// Its text for AI when Ready now, else why not.
    pub checked: Result<AiText, Readiness>,
    /// Its Markdown's length in characters, and its words.
    pub chars: usize,
    pub words: usize,
}

/// A Ready Document's text for AI, the note apart:
/// [`Vault::text_for_ai`] is `note` then `markdown`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiText {
    /// The note naming its first aliases; empty when it names none.
    pub note: String,
    pub markdown: String,
}

/// The fingerprint of a text saved while Ready: SHA-256, hex.
fn fingerprint(text: &str) -> String {
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Document {
    /// Before a save: whether its Markdown is Ready as the review counts it
    /// (scanned, and no proposals left; below the floor, no proposals left
    /// of names decided by hand). Keeps the fingerprint of the text as it
    /// is now, or forgets it. The fingerprint alone decides: `scanned` is
    /// not asked again.
    pub fn set_ready(&mut self, ready: bool) {
        self.ready_fingerprint = ready.then(|| fingerprint(&self.markdown));
    }

    /// Its readiness against `mapping`, the names its Project knows now.
    fn readiness(&self, mapping: &Mapping) -> Readiness {
        let Some(saved) = self.ready_fingerprint.as_deref() else {
            return Readiness::NotReviewed;
        };
        match known_names(self, mapping).known_names(&self.markdown) {
            0 if saved == fingerprint(&self.markdown) => Readiness::Ready,
            0 => Readiness::ChangedSinceReview,
            missed => Readiness::NamesNotReplaced(missed),
        }
    }
}

/// The names `mapping` knows as `document` reads them: without the wordings
/// it took off as Not a name that the mapping keeps for another Document's
/// sake, as it reopens without them.
fn known_names(document: &Document, mapping: &Mapping) -> RestoreMap {
    let left_off = mapping.not_names_of(&document.id);
    let identities = (mapping.exported().into_iter())
        .filter_map(|mut identity| {
            let alias = identity.alias.clone();
            (identity.originals)
                .retain(|o| !left_off.iter().any(|(a, w)| *a == alias && w == o.as_ref()));
            (!identity.originals.is_empty()).then_some(identity)
        })
        .collect();
    RestoreMap::new(identities)
}

impl Vault {
    /// Whether the Document `id`, as stored, may leave for an AI now:
    /// checked against the names its Project, or its own mapping while
    /// Unfiled, holds now. Writes nothing. A Project that does not open
    /// fails the check with its error.
    pub fn readiness(&self, id: &str) -> Result<Readiness, VaultError> {
        let document = self.document(id)?;
        let mapping = self.mapping_of(&document)?;
        Ok(document.readiness(&mapping))
    }

    /// What Copy for AI, Share, the shelf and MCP send for the Document
    /// `id`: its Markdown after the note naming its first aliases
    /// (`mdoc_pii::handoff`), computed now and never stored. Refused with
    /// [`VaultError::ExcludedFromAi`], or [`VaultError::NotReady`] unless
    /// [`Self::readiness`] finds it Ready.
    pub fn text_for_ai(&self, id: &str) -> Result<String, VaultError> {
        let (document, mapping) = self.checkable(self.document(id)?)?;
        match checked_for_ai(document, &mapping) {
            Ok(checked) => Ok(checked.note + &checked.markdown),
            Err(not) => Err(VaultError::NotReady(not)),
        }
    }

    /// The AI connector's read (ADR 0126): [`Self::text_for_ai`] of a
    /// Document of `project` only, with the note and the text apart. A
    /// Document elsewhere, in another Project or Unfiled, is
    /// [`VaultError::NotFound`] before anything of it is told; one excluded
    /// from AI is [`VaultError::ExcludedFromAi`]. The Document and the
    /// names it is checked against are read under the write lock, so they
    /// are one moment's (#1198); the check runs once it is released. Its
    /// counts come from the record checked.
    pub fn text_for_ai_in(&self, project: &str, id: &str) -> Result<AiRead, VaultError> {
        let (document, mapping) = {
            let _lock = self.write_lock()?;
            let document = self.document(id)?;
            if document.project.as_deref() != Some(project) {
                return Err(VaultError::NotFound {
                    kind: crate::ObjectKind::Document,
                    id: id.to_owned(),
                });
            }
            #[cfg(test)]
            connector_read::between_reads();
            self.checkable(document)?
        };
        let chars = document.markdown.chars().count();
        let words = document.markdown.split_whitespace().count();
        Ok(AiRead {
            checked: checked_for_ai(document, &mapping),
            chars,
            words,
        })
    }

    /// `document` and the names its Project holds now, unless it is
    /// excluded from AI.
    fn checkable(&self, document: Document) -> Result<(Document, Mapping), VaultError> {
        if document.exclude_from_ai {
            return Err(VaultError::ExcludedFromAi);
        }
        let mapping = self.mapping_of(&document)?;
        Ok((document, mapping))
    }

    /// The mapping `document`'s names live in: its Project's as stored now,
    /// or its own while Unfiled.
    fn mapping_of(&self, document: &Document) -> Result<Mapping, VaultError> {
        match &document.project {
            Some(project) => Ok(self.project(project)?.mapping),
            None => Ok(document.mapping.clone()),
        }
    }
}

/// The one outbound check: `document`'s readiness against `mapping`, the
/// names its Project held when it was read.
fn checked_for_ai(document: Document, mapping: &Mapping) -> Result<AiText, Readiness> {
    match document.readiness(mapping) {
        Readiness::Ready => {}
        not => return Err(not),
    }
    // The aliases a reopened document relinks as applied, as the editor's
    // Copy for AI names them.
    let names = RestoreMap::new(mapping.exported());
    let note = mdoc_pii::handoff::note_for(names.exact_aliases(&document.markdown));
    Ok(AiText {
        note,
        markdown: document.markdown,
    })
}

#[cfg(test)]
mod connector_read;
