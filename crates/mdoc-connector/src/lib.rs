//! The AI connector's service over the Vault (ADR 0126; #1164 Decisions
//! 2-4, 23 and 24; Milestone 2 item 5). A plain interface, no socket and no
//! MCP: the shim and mdoc's listener (item 7) call it.
//!
//! - It serves one **AI project**, none until mdoc shares one, and kept in
//!   memory only: every start begins with nothing shared. Unfiled has no
//!   id, so it can never be shared.
//! - [`Connector::list_documents`] lists the AI project's documents that
//!   are not excluded from AI, by AI label, size and state;
//!   [`Connector::get_document`] returns a Ready document's text for AI, or
//!   its "needs review" state at once (Decision 4), and above [`LONG`]
//!   characters an outline and numbered parts (Decision 24).
//! - Every read is the Vault's outbound check, run now
//!   (`Vault::text_for_ai_in`, #1091 Decision 19): nothing is cached, so a
//!   name another document adds makes this one need review on the next
//!   call, and no read reaches another Project or Unfiled.
//! - Labels never read a stored title: titles come from file names and are
//!   never reviewed (#1091, "Decided for Milestone 2"); `label.rs` says
//!   how they are made.
//! - No response holds an original or a real name: only text that just
//!   passed the check, labels from it, counts and states.
//!
//! It exists only while Settings' "AI connector (preview)" is on
//! ([`Connector::start`]).
#![forbid(unsafe_code)]

#[cfg(any(test, feature = "exits-suite"))]
pub mod exits;
mod label;
mod parts;

pub use mdoc_vault::Readiness;
pub use parts::{Heading, LONG, MAX_OUTLINE_HEADINGS, MAX_PART_HEADINGS, PART_MAX, PartInfo};

use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{AiRead, AiText, DocumentSummary, Vault, VaultError};
use std::sync::{Arc, Mutex, MutexGuard};

/// The service: the Vault and the AI project.
pub struct Connector {
    vault: Arc<Vault>,
    share: Mutex<Share>,
}

/// A document of the AI project, as `list_documents` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub doc_id: String,
    /// Its AI label (`label.rs`).
    pub label: String,
    pub size: Size,
    pub state: State,
}

/// A document's length: its Markdown, without the note.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    /// Unicode characters, as the hosts count their limits.
    pub chars: usize,
    pub words: usize,
}

impl Size {
    fn of(text: &str) -> Self {
        Self {
            chars: text.chars().count(),
            words: text.split_whitespace().count(),
        }
    }
}

/// Whether a document may be read now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    /// Not Ready now, and why; mdoc asks the user to review it.
    NeedsReview(Readiness),
}

/// What `get_document` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// A Ready document of at most [`LONG`] characters: its text for AI.
    Text {
        doc_id: String,
        label: String,
        text: String,
    },
    /// A Ready document above [`LONG`] characters: the note before its
    /// text, its size, and its parts; the model reads the parts it needs.
    Outline {
        doc_id: String,
        label: String,
        note: String,
        size: Size,
        parts: Vec<PartInfo>,
    },
    /// Part `part` of `of` of a long Ready document: the note, then the
    /// part's text, with at most [`MAX_PART_HEADINGS`] of its headings.
    Part {
        doc_id: String,
        label: String,
        part: usize,
        of: usize,
        headings: Vec<Heading>,
        text: String,
    },
    /// Not Ready now: no text (Decision 4).
    NeedsReview {
        doc_id: String,
        label: String,
        readiness: Readiness,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    /// No AI project: mdoc asks to share one (Decision 2).
    #[error("No Project is shared with AI apps.")]
    NothingShared,
    /// No such document in the AI project: another Project's, Unfiled,
    /// deleted or excluded from AI, all alike, so a read learns nothing of
    /// documents outside the share.
    #[error("No such document in the shared Project.")]
    NotShared,
    /// There is no such part: the document has `of`.
    #[error("The document has {of} parts.")]
    NoSuchPart { of: usize },
    /// The shared Project kept changing while the read ran; nothing read
    /// is handed out.
    #[error("The shared Project changed; ask again.")]
    ShareChanged,
    #[error(transparent)]
    Vault(#[from] VaultError),
}

/// A listed document as read now: its summary, what the check found, and
/// its label.
struct Entry {
    summary: DocumentSummary,
    read: Result<AiRead, VaultError>,
    label: String,
}

/// The AI project, and a count of every change to it.
#[derive(Default)]
struct Share {
    project: Option<String>,
    /// Bumped by every share and stop: a read whose share changed while it
    /// ran is read again, so nothing read after a stop is handed out.
    generation: u64,
}

/// How many times a read starts when the share keeps changing under it.
const ATTEMPTS: usize = 3;

impl Connector {
    /// The service, while "AI connector (preview)" is on; `None` while it
    /// is off, so nothing of the connector exists. Nothing is shared yet.
    pub fn start(vault: Arc<Vault>, setting: &AiConnectorConfig) -> Option<Self> {
        setting.preview.then(|| Self {
            vault,
            share: Mutex::default(),
        })
    }

    /// The AI project's id; `None`: nothing is shared.
    pub fn ai_project(&self) -> Option<String> {
        self.share_slot().project.clone()
    }

    /// Share Project `project` with AI apps, in place of any other. It must
    /// open in the Vault.
    pub fn share(&self, project: &str) -> Result<(), ConnectorError> {
        let project = self.vault.project(project)?;
        let mut share = self.share_slot();
        share.project = Some(project.id);
        share.generation += 1;
        Ok(())
    }

    /// Share nothing.
    pub fn stop_sharing(&self) {
        let mut share = self.share_slot();
        share.project = None;
        share.generation += 1;
    }

    /// `list_documents`: the AI project's documents not excluded from AI,
    /// oldest first, each checked now.
    pub fn list_documents(&self) -> Result<Vec<Listed>, ConnectorError> {
        self.while_shared(|project| {
            let mut listed = Vec::new();
            for entry in self.entries(project, None)? {
                // Moved out, excluded or damaged since the manifest said.
                let Ok(read) = entry.read else { continue };
                listed.push(Listed {
                    doc_id: entry.summary.id,
                    label: entry.label,
                    size: Size {
                        chars: read.chars,
                        words: read.words,
                    },
                    state: match read.checked {
                        Ok(_) => State::Ready,
                        Err(readiness) => State::NeedsReview(readiness),
                    },
                });
            }
            Ok(listed)
        })
    }

    /// `get_document`: document `doc_id` of the AI project, checked now.
    /// `part`: `None` for its text, or its outline above [`LONG`]
    /// characters; `Some(n)` for part n, from 1. A short document has one
    /// part, its text.
    pub fn get_document(&self, doc_id: &str, part: Option<usize>) -> Result<Read, ConnectorError> {
        self.while_shared(|project| self.read(project, doc_id, part))
    }

    fn read(
        &self,
        project: &str,
        doc_id: &str,
        part: Option<usize>,
    ) -> Result<Read, ConnectorError> {
        let entry = (self.entries(project, Some(doc_id))?.pop())
            .filter(|entry| entry.summary.id == doc_id)
            .ok_or(ConnectorError::NotShared)?;
        let doc_id = entry.summary.id;
        let label = entry.label;
        let checked = match entry.read {
            Ok(read) => read.checked,
            Err(VaultError::NotFound { .. } | VaultError::ExcludedFromAi) => {
                return Err(ConnectorError::NotShared);
            }
            Err(error) => return Err(error.into()),
        };
        let AiText { note, markdown } = match checked {
            Ok(text) => text,
            Err(readiness) => {
                return Ok(Read::NeedsReview {
                    doc_id,
                    label,
                    readiness,
                });
            }
        };
        if markdown.chars().count() <= LONG {
            return match part {
                None | Some(1) => Ok(Read::Text {
                    doc_id,
                    label,
                    text: note + &markdown,
                }),
                Some(_) => Err(ConnectorError::NoSuchPart { of: 1 }),
            };
        }
        let parts = parts::parts(&markdown);
        let of = parts.len();
        match part {
            None => Ok(Read::Outline {
                doc_id,
                label,
                size: Size::of(&markdown),
                parts: parts::outline(&parts),
                note,
            }),
            Some(n) if (1..=of).contains(&n) => {
                let chosen = &parts[n - 1];
                let headings = chosen.headings.iter().take(MAX_PART_HEADINGS);
                Ok(Read::Part {
                    doc_id,
                    label,
                    part: n,
                    of,
                    headings: headings.cloned().collect(),
                    text: note + chosen.text,
                })
            }
            Some(_) => Err(ConnectorError::NoSuchPart { of }),
        }
    }

    fn share_slot(&self) -> MutexGuard<'_, Share> {
        self.share.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `read` of the AI project, if it still opens, handed back only if the
    /// share did not change while it ran, and read again if it did. A
    /// deleted Project is shared no more, unless the share changed since.
    fn while_shared<T>(
        &self,
        read: impl Fn(&str) -> Result<T, ConnectorError>,
    ) -> Result<T, ConnectorError> {
        for _ in 0..ATTEMPTS {
            let (project, generation) = {
                let share = self.share_slot();
                (share.project.clone(), share.generation)
            };
            let Some(project) = project else {
                return Err(ConnectorError::NothingShared);
            };
            let result = match self.vault.project(&project) {
                Ok(_) => read(&project),
                Err(VaultError::NotFound { .. }) => {
                    let mut share = self.share_slot();
                    if share.generation == generation {
                        share.project = None;
                        share.generation += 1;
                        return Err(ConnectorError::NothingShared);
                    }
                    continue;
                }
                Err(error) => Err(error.into()),
            };
            #[cfg(test)]
            tests::after_read(self);
            if self.share_slot().generation == generation {
                return result;
            }
        }
        Err(ConnectorError::ShareChanged)
    }

    /// The AI project's listed documents in label order, each read through
    /// the check now, up to and including `until` when given (its label
    /// needs only the ones before it).
    fn entries(&self, project: &str, until: Option<&str>) -> Result<Vec<Entry>, ConnectorError> {
        let mut documents = self.vault.documents_in(Some(project));
        documents.retain(|document| !document.exclude_from_ai);
        documents.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
        if let Some(until) = until {
            let at = (documents.iter().position(|document| document.id == until))
                .ok_or(ConnectorError::NotShared)?;
            documents.truncate(at + 1);
        }
        let mut labels = label::Labels::default();
        Ok(documents
            .into_iter()
            .enumerate()
            .map(|(index, summary)| {
                let read = self.checked(project, &summary.id);
                let heading = (read.as_ref().ok())
                    .and_then(|read| read.checked.as_ref().ok())
                    .and_then(|text| label::heading(&text.markdown));
                let label = labels.next(index + 1, heading);
                Entry {
                    summary,
                    read,
                    label,
                }
            })
            .collect())
    }

    /// The one check every read goes through: the Vault's text for AI of
    /// a document of `project`, as it is now.
    fn checked(&self, project: &str, id: &str) -> Result<AiRead, VaultError> {
        #[cfg(any(test, feature = "exits-suite"))]
        if exits::taken_out() {
            // The exits suite took the check out: the stored text, unchecked.
            let document = self.vault.document(id)?;
            let size = Size::of(&document.markdown);
            return Ok(AiRead {
                checked: Ok(AiText {
                    note: String::new(),
                    markdown: document.markdown,
                }),
                chars: size.chars,
                words: size.words,
            });
        }
        self.vault.text_for_ai_in(project, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_vault::{DocumentOrigin, NewDocument, NewProject, SourceKind};
    use std::cell::RefCell;

    /// What the user does while a read is under way.
    type Meanwhile = Box<dyn FnOnce(&Connector)>;

    thread_local! {
        /// Run once after a read, before the share is checked again: what
        /// the user does while a read is under way.
        static MEANWHILE: RefCell<Option<Meanwhile>> = RefCell::new(None);
    }

    pub(super) fn after_read(connector: &Connector) {
        if let Some(meanwhile) = MEANWHILE.with(|slot| slot.borrow_mut().take()) {
            meanwhile(connector);
        }
    }

    fn meanwhile(then: impl FnOnce(&Connector) + 'static) {
        MEANWHILE.with(|slot| *slot.borrow_mut() = Some(Box::new(then)));
    }

    /// A connector sharing a Project with one Ready document, another
    /// Project's id, and the document's id.
    fn shared() -> (tempfile::TempDir, Connector, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::open(temp.path().join("vault"), &[7; 32]).unwrap());
        let project = |name: &str| {
            let new = NewProject {
                name: name.into(),
                ..NewProject::default()
            };
            vault.create_project(new).unwrap().id
        };
        let (case, other) = (project("Case"), project("Other"));
        let new = NewDocument {
            project: Some(case.clone()),
            title: "Letter".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "Signed.".into(),
        };
        let letter = vault.create_document(new).unwrap().id;
        (vault.update_document(&letter, |d| d.set_ready(true))).unwrap();
        let connector = Connector::start(vault, &AiConnectorConfig { preview: true }).unwrap();
        connector.share(&case).unwrap();
        (temp, connector, other, letter)
    }

    /// Stop sharing while a read is under way: what it read is not handed
    /// out.
    #[test]
    fn a_read_under_way_when_sharing_stops_hands_out_nothing() {
        let (_temp, connector, _, letter) = shared();
        assert!(connector.get_document(&letter, None).is_ok());
        meanwhile(Connector::stop_sharing);
        assert!(matches!(
            connector.get_document(&letter, None),
            Err(ConnectorError::NothingShared)
        ));
    }

    /// Share another Project while a read is under way: the read starts
    /// again against the new share.
    #[test]
    fn a_read_under_way_when_the_share_changes_reads_the_new_share() {
        let (_temp, connector, other, letter) = shared();
        meanwhile(move |connector| connector.share(&other).unwrap());
        assert!(matches!(
            connector.get_document(&letter, None),
            Err(ConnectorError::NotShared)
        ));
        meanwhile(Connector::stop_sharing);
        assert!(matches!(
            connector.list_documents(),
            Err(ConnectorError::NothingShared)
        ));
    }
}
