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
pub use parts::{Heading, LONG, MAX_OUTLINE_HEADINGS, PART_MAX, PartInfo};

use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{DocumentSummary, Vault, VaultError};
use std::sync::{Arc, Mutex, MutexGuard};

/// The service: the Vault and the AI project.
pub struct Connector {
    vault: Arc<Vault>,
    ai_project: Mutex<Option<String>>,
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
    /// part's text.
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
    #[error(transparent)]
    Vault(#[from] VaultError),
}

/// A listed document as read now: its summary, its checked text or why
/// not, and its label.
struct Entry {
    summary: DocumentSummary,
    read: Result<String, VaultError>,
    label: String,
}

impl Connector {
    /// The service, while "AI connector (preview)" is on; `None` while it
    /// is off, so nothing of the connector exists. Nothing is shared yet.
    pub fn start(vault: Arc<Vault>, setting: &AiConnectorConfig) -> Option<Self> {
        setting.preview.then(|| Self {
            vault,
            ai_project: Mutex::new(None),
        })
    }

    /// The AI project's id; `None`: nothing is shared.
    pub fn ai_project(&self) -> Option<String> {
        self.project_slot().clone()
    }

    /// Share Project `project` with AI apps, in place of any other. It must
    /// open in the Vault.
    pub fn share(&self, project: &str) -> Result<(), ConnectorError> {
        let project = self.vault.project(project)?;
        *self.project_slot() = Some(project.id);
        Ok(())
    }

    /// Share nothing.
    pub fn stop_sharing(&self) {
        *self.project_slot() = None;
    }

    /// `list_documents`: the AI project's documents not excluded from AI,
    /// oldest first, each checked now.
    pub fn list_documents(&self) -> Result<Vec<Listed>, ConnectorError> {
        let project = self.shared()?;
        let mut listed = Vec::new();
        for entry in self.entries(&project, None)? {
            let state = match &entry.read {
                Ok(_) => State::Ready,
                Err(VaultError::NotReady(readiness)) => State::NeedsReview(*readiness),
                // Moved out, excluded or damaged since the manifest said.
                Err(_) => continue,
            };
            let size = match &entry.read {
                Ok(text) => Size::of(mdoc_pii::handoff::split_note(text).1),
                Err(_) => self.stored_size(&entry.summary.id),
            };
            listed.push(Listed {
                doc_id: entry.summary.id,
                label: entry.label,
                size,
                state,
            });
        }
        Ok(listed)
    }

    /// `get_document`: document `doc_id` of the AI project, checked now.
    /// `part`: `None` for its text, or its outline above [`LONG`]
    /// characters; `Some(n)` for part n, from 1. A short document has one
    /// part, its text.
    pub fn get_document(&self, doc_id: &str, part: Option<usize>) -> Result<Read, ConnectorError> {
        let project = self.shared()?;
        let entry = (self.entries(&project, Some(doc_id))?.pop())
            .filter(|entry| entry.summary.id == doc_id)
            .ok_or(ConnectorError::NotShared)?;
        let doc_id = entry.summary.id;
        let label = entry.label;
        let text = match entry.read {
            Ok(text) => text,
            Err(VaultError::NotReady(readiness)) => {
                return Ok(Read::NeedsReview {
                    doc_id,
                    label,
                    readiness,
                });
            }
            Err(VaultError::NotFound { .. } | VaultError::ExcludedFromAi) => {
                return Err(ConnectorError::NotShared);
            }
            Err(error) => return Err(error.into()),
        };
        let (note, body) = mdoc_pii::handoff::split_note(&text);
        if body.chars().count() <= LONG {
            return match part {
                None | Some(1) => Ok(Read::Text {
                    doc_id,
                    label,
                    text,
                }),
                Some(_) => Err(ConnectorError::NoSuchPart { of: 1 }),
            };
        }
        let parts = parts::parts(body);
        let of = parts.len();
        match part {
            None => Ok(Read::Outline {
                doc_id,
                label,
                note: note.to_owned(),
                size: Size::of(body),
                parts: parts::outline(&parts),
            }),
            Some(n) if (1..=of).contains(&n) => {
                let chosen = &parts[n - 1];
                Ok(Read::Part {
                    doc_id,
                    label,
                    part: n,
                    of,
                    headings: chosen.headings.clone(),
                    text: format!("{note}{}", chosen.text),
                })
            }
            Some(_) => Err(ConnectorError::NoSuchPart { of }),
        }
    }

    fn project_slot(&self) -> MutexGuard<'_, Option<String>> {
        self.ai_project.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The AI project, if it still opens; a deleted one is shared no more.
    fn shared(&self) -> Result<String, ConnectorError> {
        let Some(project) = self.ai_project() else {
            return Err(ConnectorError::NothingShared);
        };
        match self.vault.project(&project) {
            Ok(_) => Ok(project),
            Err(VaultError::NotFound { .. }) => {
                self.stop_sharing();
                Err(ConnectorError::NothingShared)
            }
            Err(error) => Err(error.into()),
        }
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
                    .and_then(|text| label::heading(mdoc_pii::handoff::split_note(text).1));
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
    fn checked(&self, project: &str, id: &str) -> Result<String, VaultError> {
        #[cfg(any(test, feature = "exits-suite"))]
        if exits::taken_out() {
            // The exits suite took the check out: the stored text, unchecked.
            let document = self.vault.document(id)?;
            return Ok(mdoc_pii::handoff::for_ai(
                &document.markdown,
                std::iter::empty(),
            ));
        }
        self.vault.text_for_ai_in(project, id)
    }

    /// The size of a document that is not Ready, from its stored Markdown;
    /// nothing of it leaves but the counts.
    fn stored_size(&self, id: &str) -> Size {
        (self.vault.document(id))
            .map(|document| Size::of(&document.markdown))
            .unwrap_or_default()
    }
}
