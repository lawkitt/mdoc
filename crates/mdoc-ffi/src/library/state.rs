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
pub(crate) use crate::document_session::Counts;
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

/// Counts, and what they are counts of: the document's file and its case,
/// as each was. Opening the document proposes its case's names, so a change
/// to either makes them stale.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Recorded {
    #[serde(flatten)]
    pub counts: Counts,
    pub stamp: Stamp,
    /// The case it was in; `None`: in none.
    pub case: Option<CaseMark>,
}

impl Recorded {
    /// `counts` of the file stamped `stamp`, in `case` (its id and its
    /// file's stamp) if any; `None` when a stamp is missing.
    pub fn of(
        counts: Counts,
        stamp: Option<Stamp>,
        case: Option<(String, Option<Stamp>)>,
    ) -> Option<Self> {
        let case = match case {
            None => None,
            Some((id, stamp)) => Some(CaseMark { id, stamp: stamp? }),
        };
        Some(Self {
            counts,
            stamp: stamp?,
            case,
        })
    }
}

/// A case, and its file as it was.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CaseMark {
    pub id: String,
    pub stamp: Stamp,
}

/// A file as it is: any write, replacement or copy over it changes this.
/// On Unix its inode and status-change time too, which copying tools
/// cannot set and an atomic save changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Stamp {
    /// Its modification time: seconds and nanoseconds since the Unix epoch.
    pub modified: u64,
    pub nanos: u32,
    pub len: u64,
    #[serde(default)]
    pub inode: u64,
    /// Its status-change time, seconds and nanoseconds.
    #[serde(default)]
    pub changed: i64,
    #[serde(default)]
    pub changed_nanos: i64,
}

impl Stamp {
    /// `None` when the system gives no modification time: such a file is
    /// never taken as unchanged.
    pub fn of(metadata: &std::fs::Metadata) -> Option<Self> {
        let since = metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        #[cfg(unix)]
        let (inode, changed, changed_nanos) = {
            use std::os::unix::fs::MetadataExt;
            (metadata.ino(), metadata.ctime(), metadata.ctime_nsec())
        };
        #[cfg(not(unix))]
        let (inode, changed, changed_nanos) = (0, 0, 0);
        Some(Self {
            modified: since.as_secs(),
            nanos: since.subsec_nanos(),
            len: metadata.len(),
            inode,
            changed,
            changed_nanos,
        })
    }

    pub fn of_file(path: &Path) -> Option<Self> {
        Self::of(&std::fs::metadata(path).ok()?)
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

    /// The document at `path` saved now, under the lock its save holds:
    /// `summary` is its counts with the file and case they were counted
    /// from.
    pub(crate) fn record_saved(
        &self,
        lock: &WriteLock,
        path: &Path,
        scanned: bool,
        summary: Option<Recorded>,
    ) -> Result<(), CoreError> {
        let state = DocumentState {
            scanned,
            summary: summary.filter(|summary| self.still(path, summary)),
            opened: Some(now()),
        };
        self.set_document_state(lock, path, Some(state))
    }

    /// The document at `path` opened now, from the entry `read` before it
    /// was read. While the entry is still that one, it takes `scanned` and
    /// `summary` (the counts, kept only while the file and case are still
    /// as counted). An entry a save wrote since keeps what the save wrote,
    /// and only `opened` changes.
    pub(crate) fn record_opened(
        &self,
        lock: &WriteLock,
        path: &Path,
        read: Option<&DocumentState>,
        scanned: bool,
        summary: Option<Recorded>,
    ) -> Result<(), CoreError> {
        let current = self.document_state(path);
        let state = if current.as_ref() == read {
            DocumentState {
                scanned,
                summary: summary.filter(|summary| self.still(path, summary)),
                opened: Some(now()),
            }
        } else {
            match current {
                Some(current) => DocumentState {
                    opened: Some(now()),
                    ..current
                },
                None => return Ok(()),
            }
        };
        self.set_document_state(lock, path, Some(state))
    }

    /// Whether the file at `path` and its case are as `summary` counted them.
    pub(crate) fn still(&self, path: &Path, summary: &Recorded) -> bool {
        Stamp::of_file(path) == Some(summary.stamp)
            && summary
                .case
                .as_ref()
                .is_none_or(|case| self.case_stamp(&case.id) == Some(case.stamp))
    }

    /// Case `id`'s file as it is now.
    pub(crate) fn case_stamp(&self, id: &str) -> Option<Stamp> {
        Stamp::of_file(&self.store.file(id))
    }

    /// Every case file as it is now, by case id: taken before the cases are
    /// read, so counts made from them are never newer than the stamps.
    pub(crate) fn case_stamps(&self) -> std::collections::HashMap<String, Stamp> {
        let Ok(entries) = std::fs::read_dir(self.root.join(super::CASES)) else {
            return Default::default();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let id = path
                    .file_name()?
                    .to_str()?
                    .strip_suffix(".case")?
                    .to_owned();
                Some((id, Stamp::of_file(&path)?))
            })
            .collect()
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

// Unix only: elsewhere a stamp has no inode nor status-change time.
#[cfg(all(test, unix))]
mod tests {
    use super::Stamp;
    use crate::test_support::Scratch;

    /// A file replaced by one of the same length with its modification
    /// time kept (`cp -p`, a restore from backup) is a changed file.
    #[test]
    fn a_file_copied_over_with_its_time_is_a_change() {
        let scratch = Scratch::new();
        let (file, copy) = (scratch.path().join("a.md"), scratch.path().join("b.md"));
        std::fs::write(&file, "Ann Lee").unwrap();
        let before = Stamp::of_file(&file).unwrap();
        std::fs::write(&copy, "Bob Ray").unwrap();
        let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&copy)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        std::fs::rename(&copy, &file).unwrap();
        let after = Stamp::of_file(&file).unwrap();
        assert_eq!(
            (after.modified, after.nanos, after.len),
            (before.modified, before.nanos, before.len)
        );
        assert_ne!(after, before);
    }
}
