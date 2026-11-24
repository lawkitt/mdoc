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
/// shelf, later MCP.
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
        let document = self.document(id)?;
        if document.exclude_from_ai {
            return Err(VaultError::ExcludedFromAi);
        }
        let mapping = self.mapping_of(&document)?;
        match document.readiness(&mapping) {
            Readiness::Ready => {}
            not => return Err(VaultError::NotReady(not)),
        }
        let text = document.markdown.as_str();
        // The aliases a reopened document relinks as applied, as the
        // editor's Copy for AI names them.
        let names = RestoreMap::new(mapping.exported());
        Ok(mdoc_pii::handoff::for_ai(text, names.exact_aliases(text)))
    }

    /// The Document `id`'s title as it leaves with [`Self::text_for_ai`]'s
    /// text (a shared file's name, a share sheet's subject): every original
    /// its Project knows now, or its own mapping while Unfiled, replaced by
    /// its alias, as the desktop shelf shows titles. A name the Project
    /// knows only from another Document is masked too (#1192). Whether the
    /// Document may leave is `text_for_ai`'s to decide.
    pub fn title_for_ai(&self, id: &str) -> Result<String, VaultError> {
        let document = self.document(id)?;
        let mapping = self.mapping_of(&document)?;
        Ok(RestoreMap::new(mapping.exported())
            .pseudonymize(&document.title)
            .0)
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
