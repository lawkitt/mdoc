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
    /// The revisions of the Document and of its Project's record read.
    pub at: ReadAt,
}

/// The revisions a read of a Document checked: its own and its Project's.
/// A write to either since then (an edit, a move, an exclusion, a name)
/// bumps one of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadAt {
    pub document: u64,
    pub project: u64,
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
    /// [`Self::readiness`] finds it Ready. The Document and the names it is
    /// checked against are read under the write lock, so they are one
    /// moment's: no write lands between the two reads (#1198). The lock is
    /// held for the two reads only, not for the check.
    pub fn text_for_ai(&self, id: &str) -> Result<String, VaultError> {
        let (document, mapping) = {
            let _lock = self.held_lock()?;
            let document = self.document(id)?;
            #[cfg(test)]
            tests::between_reads();
            self.checkable(document)?
        };
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
        let (document, mapping, at) = {
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
            if document.exclude_from_ai {
                return Err(VaultError::ExcludedFromAi);
            }
            let project = self.project(project)?;
            let at = ReadAt {
                document: document.revision,
                project: project.revision,
            };
            (document, project.mapping, at)
        };
        let chars = document.markdown.chars().count();
        let words = document.markdown.split_whitespace().count();
        Ok(AiRead {
            checked: checked_for_ai(document, &mapping),
            chars,
            words,
            at,
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

#[cfg(test)]
mod tests {
    use crate::{
        NewDocument, NewProject, Readiness, Vault, VaultError,
        records::{DocumentOrigin, SourceKind},
    };
    use mdoc_pii::{Category, restore::ExportedIdentity};
    use std::{cell::RefCell, fs::TryLockError, sync::Arc, sync::mpsc};

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

        // The other handle opens now: opening takes the lock too. Between
        // the reads it tries the lock without waiting: free, it writes at
        // once, as any writer would without the lock; held, it writes once
        // the check lets go. Either way the hook returns only once it knows.
        let (go, start) = mpsc::channel::<()>();
        let (tried, outcome) = mpsc::channel::<bool>();
        let (opened, ready) = mpsc::channel::<()>();
        let writer = {
            let (root, letter, project) = (root.clone(), letter.clone(), project.clone());
            std::thread::spawn(move || {
                let other = Vault::open(&root, &[7; 32]).unwrap();
                opened.send(()).unwrap();
                start.recv().unwrap();
                let free = {
                    let file = other.lock_file.lock().unwrap();
                    match file.try_lock() {
                        Ok(()) => {
                            file.unlock().unwrap();
                            true
                        }
                        Err(TryLockError::WouldBlock) => false,
                        Err(TryLockError::Error(error)) => panic!("{error}"),
                    }
                };
                if !free {
                    tried.send(false).unwrap();
                }
                (other.update_document(&letter, |d| d.markdown = "PERSON_1 met PERSON_2.".into()))
                    .unwrap();
                (other.update_project(&project, |p| assert!(p.forget_identity("PERSON_2"))))
                    .unwrap();
                if free {
                    tried.send(true).unwrap();
                }
            })
        };
        ready.recv().unwrap();
        BETWEEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                go.send(()).unwrap();
                outcome.recv().unwrap();
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
