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
//! - [`Connector::deliver`] takes an app's answer back (Decision 8, item
//!   6): an AI Reply of the AI project, or a new Document promoted from
//!   one, naming documents read under the current share; it restores only
//!   inside mdoc and answers with ids and counts.
//! - Every read that hands text out, and every delivery, is a line of the
//!   Project's AI activity in the Vault, with its app and time (Decision
//!   25); a read whose line can't be written hands nothing out.
//!
//! It exists only while Settings' "AI connector (preview)" is on
//! ([`Connector::start`]).
#![forbid(unsafe_code)]

#[cfg(any(test, feature = "exits-suite"))]
pub mod exits;
mod label;
mod parts;

pub use mdoc_vault::{AiActivity, AiEvent, Delivered, Readiness};
pub use parts::{Heading, LONG, MAX_OUTLINE_HEADINGS, MAX_PART_HEADINGS, PART_MAX, PartInfo};

use mdoc_store::settings::AiConnectorConfig;
use mdoc_vault::{
    AiRead, AiText, DocumentSummary, NewDelivery, ObjectKind, ReadAt, Vault, VaultError,
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
};

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

impl Read {
    /// The document this read handed text of out, if it did: its text, its
    /// outline or a part; a "needs review" answer hands nothing out.
    fn handed_out(&self) -> Option<(&str, Option<usize>)> {
        match self {
            Self::Text { doc_id, .. } | Self::Outline { doc_id, .. } => Some((doc_id, None)),
            Self::Part { doc_id, part, .. } => Some((doc_id, Some(*part))),
            Self::NeedsReview { .. } => None,
        }
    }
}

/// An AI app's answer, delivered (Decision 8): pseudonymized Markdown and
/// the documents it read for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub kind: DeliveryKind,
    /// The reply's title, aliases and all; optional for a reply ("Reply
    /// to" the first document read), needed for a Document.
    pub title: Option<String>,
    /// The documents it read, by id: at least one, each read under the
    /// current share.
    pub read: Vec<String>,
    /// As the app wrote it: aliases, never real names.
    pub markdown: String,
}

/// Where a delivery lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryKind {
    /// An AI Reply of the AI project, for the Reply page.
    Reply,
    /// A new Document in the AI project, restored and not yet reviewed,
    /// promoted from the AI Reply it is saved as too (`from_reply`).
    Document,
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
    /// A delivery that names no document, or one not read since the
    /// Project was shared: a reply after a switch (ADR 0126).
    #[error("A delivery names the documents read for it, from the shared Project.")]
    NotRead,
    /// A delivery as a Document without a title.
    #[error("A document needs a title.")]
    TitleNeeded,
    /// A delivery with no text.
    #[error("The delivery has no text.")]
    Empty,
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

/// The AI project, a count of every change to it, and what was read
/// from it since.
#[derive(Default)]
struct Share {
    project: Option<String>,
    /// Bumped by every share and stop: a read whose share changed while it
    /// ran is read again, so nothing read after a stop is handed out.
    generation: u64,
    /// The documents whose text was handed out under this generation, with
    /// the app each was handed to: an app's delivery names only its own.
    served: HashSet<(String, String)>,
}

impl Share {
    /// Share `project`, or nothing: a new generation, nothing served yet.
    fn set(&mut self, project: Option<String>) {
        self.project = project;
        self.generation += 1;
        self.served.clear();
    }
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
        self.share_slot().set(Some(project.id));
        Ok(())
    }

    /// Share nothing.
    pub fn stop_sharing(&self) {
        self.share_slot().set(None);
    }

    /// `list_documents`: the AI project's documents not excluded from AI,
    /// oldest first, each checked now.
    pub fn list_documents(&self) -> Result<Vec<Listed>, ConnectorError> {
        let keep = |_: &mut Share, _: &str, _: &Vec<Listed>| Ok(true);
        self.while_shared(keep, |project| {
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

    /// `get_document` for AI app `app`: document `doc_id` of the AI
    /// project, checked now. `part`: `None` for its text, or its outline
    /// above [`LONG`] characters; `Some(n)` for part n, from 1. A short
    /// document has one part, its text. A read that hands text out is a
    /// line of the Project's AI activity first, written only while the
    /// Document and its Project are as they were checked (else it is read
    /// again): when that can't be written, nothing is handed out.
    pub fn get_document(
        &self,
        app: &str,
        doc_id: &str,
        part: Option<usize>,
    ) -> Result<Read, ConnectorError> {
        let keep = |share: &mut Share, project: &str, (read, at): &(Read, ReadAt)| {
            let Some((document, part)) = read.handed_out() else {
                return Ok(true);
            };
            let vault = &self.vault;
            if vault
                .record_ai_read(project, app, document, part, *at)?
                .is_none()
            {
                return Ok(false);
            }
            share.served.insert((app.to_string(), document.to_string()));
            Ok(true)
        };
        let read = self.while_shared(keep, |project| self.read(project, doc_id, part))?;
        Ok(read.0)
    }

    /// `deliver_reply` for AI app `app` (Decision 8): `delivery` saved as an
    /// AI Reply of the AI project, bound to the identities its aliases mean
    /// there now, and as a Document promoted from it for
    /// [`DeliveryKind::Document`] (`Vault::deliver_in`). Refused unless each
    /// document it names was read by `app` since the Project was shared
    /// ([`ConnectorError::NotRead`]) and is still in it
    /// ([`ConnectorError::NotShared`]). The answer holds ids and counts,
    /// and the aliases no identity answers: nothing restored.
    pub fn deliver(&self, app: &str, delivery: Delivery) -> Result<Delivered, ConnectorError> {
        let Delivery {
            kind,
            title,
            read,
            markdown,
        } = delivery;
        if markdown.trim().is_empty() {
            return Err(ConnectorError::Empty);
        }
        let blank = (title.as_deref()).is_none_or(|title| title.trim().is_empty());
        if kind == DeliveryKind::Document && blank {
            return Err(ConnectorError::TitleNeeded);
        }
        let mut named: Vec<String> = Vec::with_capacity(read.len());
        for id in read {
            if !named.contains(&id) {
                named.push(id);
            }
        }
        // Held to the end: the share can't change while the reply is saved.
        let mut share = self.share_slot();
        let Some(project) = share.project.clone() else {
            return Err(ConnectorError::NothingShared);
        };
        if let Err(VaultError::NotFound { .. }) = self.vault.project(&project) {
            share.set(None);
            return Err(ConnectorError::NothingShared);
        }
        let read_by_app = |id: &String| share.served.contains(&(app.to_string(), id.clone()));
        if named.is_empty() || !named.iter().all(read_by_app) {
            return Err(ConnectorError::NotRead);
        }
        let new = NewDelivery {
            read: named,
            title,
            app: app.to_string(),
            markdown,
            as_document: kind == DeliveryKind::Document,
        };
        match self.vault.deliver_in(&project, new) {
            Ok(delivered) => Ok(delivered),
            Err(VaultError::NotFound {
                kind: ObjectKind::Project,
                ..
            }) => {
                share.set(None);
                Err(ConnectorError::NothingShared)
            }
            Err(VaultError::NotFound { .. } | VaultError::ExcludedFromAi) => {
                Err(ConnectorError::NotShared)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn read(
        &self,
        project: &str,
        doc_id: &str,
        part: Option<usize>,
    ) -> Result<(Read, ReadAt), ConnectorError> {
        let entry = (self.entries(project, Some(doc_id))?.pop())
            .filter(|entry| entry.summary.id == doc_id)
            .ok_or(ConnectorError::NotShared)?;
        let doc_id = entry.summary.id;
        let label = entry.label;
        let (checked, at) = match entry.read {
            Ok(read) => (read.checked, read.at),
            Err(VaultError::NotFound { .. } | VaultError::ExcludedFromAi) => {
                return Err(ConnectorError::NotShared);
            }
            Err(error) => return Err(error.into()),
        };
        let AiText { note, markdown } = match checked {
            Ok(text) => text,
            Err(readiness) => {
                let read = Read::NeedsReview {
                    doc_id,
                    label,
                    readiness,
                };
                return Ok((read, at));
            }
        };
        let read = Self::text_or_part(doc_id, label, note, &markdown, part)?;
        Ok((read, at))
    }

    /// A Ready document's text, or above [`LONG`] characters its outline or
    /// part `part`.
    fn text_or_part(
        doc_id: String,
        label: String,
        note: String,
        markdown: &str,
        part: Option<usize>,
    ) -> Result<Read, ConnectorError> {
        if markdown.chars().count() <= LONG {
            return match part {
                None | Some(1) => Ok(Read::Text {
                    doc_id,
                    label,
                    text: note + markdown,
                }),
                Some(_) => Err(ConnectorError::NoSuchPart { of: 1 }),
            };
        }
        let parts = parts::parts(markdown);
        let of = parts.len();
        match part {
            None => Ok(Read::Outline {
                doc_id,
                label,
                size: Size::of(markdown),
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
    /// share did not change while it ran, and read again if it did; `keep`
    /// runs on it first, under the share's lock, and may still refuse it,
    /// or have it read again (`false`: what it read changed since).
    /// A deleted Project is shared no more, unless the share changed since.
    fn while_shared<T>(
        &self,
        keep: impl Fn(&mut Share, &str, &T) -> Result<bool, ConnectorError>,
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
                        share.set(None);
                        return Err(ConnectorError::NothingShared);
                    }
                    continue;
                }
                Err(error) => Err(error.into()),
            };
            #[cfg(test)]
            tests::after_read(self);
            let mut share = self.share_slot();
            if share.generation == generation {
                let value = result?;
                if keep(&mut share, &project, &value)? {
                    return Ok(value);
                }
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
            let at = ReadAt {
                document: document.revision,
                project: self.vault.project(project)?.revision,
            };
            return Ok(AiRead {
                at,
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

    const CLAUDE: &str = "Claude";

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
        assert!(connector.get_document(CLAUDE, &letter, None).is_ok());
        meanwhile(Connector::stop_sharing);
        assert!(matches!(
            connector.get_document(CLAUDE, &letter, None),
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
            connector.get_document(CLAUDE, &letter, None),
            Err(ConnectorError::NotShared)
        ));
        meanwhile(Connector::stop_sharing);
        assert!(matches!(
            connector.list_documents(),
            Err(ConnectorError::NothingShared)
        ));
    }

    /// Review 2: a write between the check and the activity line (an
    /// exclusion, a name the Project learns) makes the read start again,
    /// so the text checked before it is not handed out, nor recorded.
    #[test]
    fn a_write_between_the_check_and_its_line_reads_again() {
        let (_temp, connector, _, letter) = shared();
        let case = connector.ai_project().unwrap();
        let exclude = letter.clone();
        meanwhile(move |connector| {
            (connector.vault)
                .update_document(&exclude, |d| d.exclude_from_ai = true)
                .unwrap();
        });
        assert!(matches!(
            connector.get_document(CLAUDE, &letter, None),
            Err(ConnectorError::NotShared)
        ));
        (connector.vault)
            .update_document(&letter, |d| {
                d.exclude_from_ai = false;
                d.markdown = "Anna Petrova signed.".into();
                d.set_ready(true);
            })
            .unwrap();
        let project = case.clone();
        meanwhile(move |connector| {
            let anna = mdoc_pii::restore::ExportedIdentity {
                alias: "PERSON_1".into(),
                previous: Vec::new(),
                category: mdoc_pii::Category::Person,
                custom: false,
                originals: vec![Arc::from("Anna Petrova")],
            };
            (connector.vault)
                .update_project(&project, |p| p.mapping.merge(&[anna]))
                .unwrap();
        });
        assert!(matches!(
            connector.get_document(CLAUDE, &letter, None).unwrap(),
            Read::NeedsReview { .. }
        ));
        assert!(connector.vault.ai_activity(&case).unwrap().is_empty());
        assert!(connector.share_slot().served.is_empty());
    }
}
