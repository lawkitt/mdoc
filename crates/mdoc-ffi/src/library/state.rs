//! What the library keeps beside each document that neither its file nor
//! its case holds: whether it was scanned, so a scan that found nothing
//! reopens as scanned (ADR 0104). `documents.json` at the root, keyed as
//! cases name members (`Library::member`); it holds no names and no text.
//! Written under the library's write lock.
use super::{Library, WriteLock};
use crate::CoreError;
use std::{collections::BTreeMap, path::Path};

const FILE: &str = "documents.json";

/// One document's entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DocumentState {
    pub scanned: bool,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct States {
    documents: BTreeMap<String, DocumentState>,
}

impl Library {
    /// What `save` recorded for the document at `path`; `None` for one
    /// saved before the library kept this, or never.
    pub(crate) fn document_state(&self, path: &Path) -> Option<DocumentState> {
        self.states().documents.remove(&self.key(path))
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

    fn key(&self, path: &Path) -> String {
        self.member(path).to_string_lossy().into_owned()
    }

    /// The entries; a missing or damaged file has none.
    fn states(&self) -> States {
        std::fs::read(self.root.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_states(&self, states: &States) -> Result<(), CoreError> {
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
