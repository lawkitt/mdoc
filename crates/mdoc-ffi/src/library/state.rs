//! What the library keeps beside each document that neither its file nor
//! its case holds: whether it was scanned, so a scan that found nothing
//! reopens as scanned (ADR 0104), and for its summary the review's counts
//! at its last save or open, the file as it was then, and when it was last
//! opened, so lists need not open each document. `documents.json` at the root, keyed as
//! cases name members (`Library::member`): file paths, whose names are the
//! documents' titles and can hold originals, as the files' own names do.
//! It holds no text and nothing more of a document. Written under the
//! library's write lock; entries whose file is gone (removed outside the
//! app, or by desktop) go when the library lists its documents.
use super::{Library, WriteLock};
use crate::CoreError;
use std::{collections::BTreeMap, path::Path};

const FILE: &str = "documents.json";

/// One document's entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DocumentState {
    pub scanned: bool,
    /// The review's counts at the last save or open, while the file is as
    /// it was then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Recorded>,
    /// Seconds since the Unix epoch it was last opened or saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened: Option<u64>,
}

/// A review's counts, as the snapshot gives them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Counts {
    pub proposed: u64,
    pub replaced: u64,
    pub kept: u64,
}

/// Counts, and the file they are counts of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Recorded {
    #[serde(flatten)]
    pub counts: Counts,
    pub stamp: Stamp,
}

/// A file as it is: changed when its time or length is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Stamp {
    /// Its modification time: seconds and nanoseconds since the Unix epoch.
    pub modified: u64,
    pub nanos: u32,
    pub len: u64,
}

impl Stamp {
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        let since = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .unwrap_or_default();
        Self {
            modified: since.as_secs(),
            nanos: since.subsec_nanos(),
            len: metadata.len(),
        }
    }

    pub fn of_file(path: &Path) -> Option<Self> {
        std::fs::metadata(path).ok().map(|m| Self::of(&m))
    }
}

/// Seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct States {
    pub documents: BTreeMap<String, DocumentState>,
}

impl Library {
    /// What `save` recorded for the document at `path`; `None` for one
    /// saved before the library kept this, or never.
    pub(crate) fn document_state(&self, path: &Path) -> Option<DocumentState> {
        self.states().documents.remove(&self.key(path))
    }

    /// The document at `path` saved or opened now with a review of
    /// `counts`: its entry records them with the file as it is now.
    pub(crate) fn record_document(
        &self,
        lock: &WriteLock,
        path: &Path,
        scanned: bool,
        counts: Counts,
    ) -> Result<(), CoreError> {
        let state = DocumentState {
            scanned,
            summary: Stamp::of_file(path).map(|stamp| Recorded { counts, stamp }),
            opened: Some(now()),
        };
        self.set_document_state(lock, path, Some(state))
    }

    /// Record `state` for the document at `path`, or forget it.
    pub(crate) fn set_document_state(
        &self,
        _: &WriteLock,
        path: &Path,
        state: Option<DocumentState>,
    ) -> Result<(), CoreError> {
        let mut states = self.states();
        let key = self.key(path);
        if states.documents.get(&key) == state.as_ref() {
            return Ok(());
        }
        match state {
            Some(state) => states.documents.insert(key, state),
            None => states.documents.remove(&key),
        };
        self.write_states(&states)
    }

    /// A renamed document keeps its entry.
    pub(crate) fn move_document_state(
        &self,
        lock: &WriteLock,
        old: &Path,
        new: &Path,
    ) -> Result<(), CoreError> {
        let state = self.document_state(old);
        if state.is_some() && old != new {
            self.set_document_state(lock, old, None)?;
            self.set_document_state(lock, new, state)?;
        }
        Ok(())
    }

    /// Forget the entries whose file is gone; best effort, as a listing
    /// calls it.
    pub(crate) fn prune_document_states(&self) {
        let gone = |states: &States| {
            states
                .documents
                .keys()
                .filter(|key| !self.root.join(key).exists())
                .cloned()
                .collect::<Vec<_>>()
        };
        if gone(&self.states()).is_empty() {
            return;
        }
        // Never waits: a save holding the lock leaves it for the next list.
        let Some(_lock) = self.try_lock() else { return };
        let mut states = self.states();
        for key in gone(&states) {
            states.documents.remove(&key);
        }
        let _ = self.write_states(&states);
    }

    pub(super) fn key(&self, path: &Path) -> String {
        self.member(path).to_string_lossy().into_owned()
    }

    /// The entries; a missing or damaged file has none.
    pub(super) fn states(&self) -> States {
        std::fs::read(self.root.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) fn write_states(&self, states: &States) -> Result<(), CoreError> {
        let path = self.root.join(FILE);
        let partial = path.with_extension("json.part");
        let json = serde_json::to_vec(states).map_err(|e| CoreError::Io {
            message: e.to_string(),
        })?;
        std::fs::write(&partial, json)?;
        std::fs::rename(partial, path)?;
        Ok(())
    }
}
