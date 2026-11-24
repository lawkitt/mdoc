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
    /// [`Self::readiness`] finds it Ready. The Document and the names it is
    /// checked against are read under the write lock, so they are one
    /// moment's: no write lands between the two reads (#1198).
    pub fn text_for_ai(&self, id: &str) -> Result<String, VaultError> {
        let _lock = self.held_lock()?;
        let document = self.document(id)?;
        if document.exclude_from_ai {
            return Err(VaultError::ExcludedFromAi);
        }
        #[cfg(test)]
        tests::between_reads();
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

    /// The mapping `document`'s names live in: its Project's as stored now,
    /// or its own while Unfiled.
    fn mapping_of(&self, document: &Document) -> Result<Mapping, VaultError> {
        match &document.project {
            Some(project) => Ok(self.project(project)?.mapping),
            None => Ok(document.mapping.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        NewDocument, NewProject, Readiness, Vault, VaultError,
        records::{DocumentOrigin, SourceKind},
    };
    use mdoc_pii::{Category, restore::ExportedIdentity};
    use std::{cell::RefCell, sync::Arc, sync::mpsc, time::Duration};

    type Meanwhile = Box<dyn FnOnce()>;

    thread_local! {
        /// Run once between `text_for_ai`'s read of the Document and its
        /// read of the Project's names.
        static BETWEEN: RefCell<Option<Meanwhile>> = RefCell::new(None);
    }

    pub(super) fn between_reads() {
        if let Some(meanwhile) = BETWEEN.with(|slot| slot.borrow_mut().take()) {
            meanwhile();
        }
    }

    fn person(alias: &str, original: &str) -> ExportedIdentity {
        ExportedIdentity {
            alias: alias.into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec![Arc::from(original)],
        }
    }

    /// #1198: another handle takes a name out of the text and then forgets
    /// its identity while `text_for_ai` is between its two reads. Read at
    /// one moment, the document is never Ready: it still holds the name
    /// against the old names, and it changed since its review against the
    /// new ones. Without the lock, the old text met the new names and left
    /// with the name as written.
    #[test]
    fn a_write_between_the_two_reads_waits_for_the_check() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let vault = Vault::open(&root, &[7; 32]).unwrap();
        let project = (vault.create_project(NewProject {
            name: "Case".into(),
            ..NewProject::default()
        }))
        .unwrap()
        .id;
        (vault.update_project(&project, |p| {
            p.mapping.merge(&[person("PERSON_1", "Anna Petrova")])
        }))
        .unwrap();
        let letter = (vault.create_document(NewDocument {
            project: Some(project.clone()),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "PERSON_1 met Boris Ivanov.".into(),
        }))
        .unwrap()
        .id;
        (vault.update_document(&letter, |d| d.set_ready(true))).unwrap();
        (vault.update_project(&project, |p| {
            p.mapping.merge(&[person("PERSON_2", "Boris Ivanov")])
        }))
        .unwrap();
        assert_eq!(
            vault.readiness(&letter).unwrap(),
            Readiness::NamesNotReplaced(1)
        );

        // The other handle opens now: opening takes the lock too.
        let (go, start) = mpsc::channel::<()>();
        let (finished, done) = mpsc::channel::<()>();
        let (opened, ready) = mpsc::channel::<()>();
        let writer = {
            let (root, letter, project) = (root.clone(), letter.clone(), project.clone());
            std::thread::spawn(move || {
                let other = Vault::open(&root, &[7; 32]).unwrap();
                opened.send(()).unwrap();
                start.recv().unwrap();
                (other.update_document(&letter, |d| d.markdown = "PERSON_1 met PERSON_2.".into()))
                    .unwrap();
                (other.update_project(&project, |p| assert!(p.forget_identity("PERSON_2"))))
                    .unwrap();
                // The check may have stopped waiting for it.
                let _ = finished.send(());
            })
        };
        ready.recv().unwrap();
        BETWEEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                go.send(()).unwrap();
                // Without the lock the two writes land within this wait;
                // with it they wait for the check, and this times out.
                let _ = done.recv_timeout(Duration::from_millis(500));
            }))
        });
        let sent = vault.text_for_ai(&letter);
        writer.join().unwrap();
        assert!(
            matches!(sent, Err(VaultError::NotReady(_))),
            "handed out across two moments: {sent:?}"
        );
        assert_eq!(
            vault.readiness(&letter).unwrap(),
            Readiness::ChangedSinceReview
        );
    }
}
