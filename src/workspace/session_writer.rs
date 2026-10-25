//! The tab list's saves to `session.json` (ADR 0003): one write at a time,
//! the latest snapshot waiting behind it, and none when nothing changed
//! since the last save. Generic over the snapshot, so the queue rules run
//! without a window or a disk; the workspace view takes snapshots, runs the
//! writes in the background and reports how they ended.

use std::path::PathBuf;

/// What the workspace view carries out for a session-writer transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SessionEffect<S> {
    /// Save `snapshot` to `path` in the background, then call `saved` or
    /// `failed`.
    Write { path: PathBuf, snapshot: S },
}

pub(super) struct SessionWriter<S> {
    /// Where the session lives; none in tests, which never save.
    path: Option<PathBuf>,
    /// What the last successful write saved.
    persisted: Option<S>,
    writing: bool,
    /// The newest snapshot not yet written.
    pending: Option<S>,
}

impl<S: Clone + PartialEq> SessionWriter<S> {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            persisted: None,
            writing: false,
            pending: None,
        }
    }

    pub(super) fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Save to `path` from now on.
    pub(super) fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    /// Whether checkpoints save at all.
    pub(super) fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// A write is running.
    pub(super) fn writing(&self) -> bool {
        self.writing
    }

    /// The tab list now looks like `snapshot`. Unchanged since the last
    /// save, with no write running, it is not written again.
    pub(super) fn checkpoint(&mut self, snapshot: S) -> Option<SessionEffect<S>> {
        if self.persisted.as_ref() == Some(&snapshot) && !self.writing {
            return None;
        }
        self.pending = Some(snapshot);
        self.next()
    }

    /// Start writing the waiting snapshot unless a write is running.
    fn next(&mut self) -> Option<SessionEffect<S>> {
        if self.writing {
            return None;
        }
        let snapshot = self.pending.take()?;
        let path = self.path.clone()?;
        self.writing = true;
        Some(SessionEffect::Write { path, snapshot })
    }

    /// The write of `snapshot` finished; a newer snapshot that differs from
    /// it is written next.
    pub(super) fn saved(&mut self, snapshot: S) -> Option<SessionEffect<S>> {
        self.writing = false;
        self.persisted = Some(snapshot);
        if self.pending == self.persisted {
            self.pending = None;
        }
        self.next()
    }

    /// The write failed. The waiting snapshot is dropped; the next
    /// checkpoint tries again.
    pub(super) fn failed(&mut self) {
        self.writing = false;
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(snapshot: u8) -> Option<SessionEffect<u8>> {
        Some(SessionEffect::Write {
            path: "/profile/session.json".into(),
            snapshot,
        })
    }

    #[test]
    fn one_write_runs_at_a_time_and_only_the_newest_waits() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert!(writer.enabled());
        assert_eq!(writer.checkpoint(1), write(1));
        assert!(writer.writing());
        assert_eq!(writer.checkpoint(2), None);
        assert_eq!(writer.checkpoint(3), None);
        assert_eq!(writer.saved(1), write(3));
        assert_eq!(writer.saved(3), None);
        assert!(!writer.writing());
        // Unchanged since the last save: nothing to write.
        assert_eq!(writer.checkpoint(3), None);
        assert_eq!(writer.checkpoint(4), write(4));
    }

    #[test]
    fn a_snapshot_equal_to_the_one_just_saved_is_not_written_again() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
        // While writing, even an unchanged snapshot waits: the write may fail.
        assert_eq!(writer.checkpoint(1), None);
        assert_eq!(writer.saved(1), None);
        assert!(!writer.writing());
    }

    #[test]
    fn a_failed_write_drops_the_waiting_snapshot() {
        let mut writer = SessionWriter::new(Some("/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
        assert_eq!(writer.checkpoint(2), None);
        writer.failed();
        assert!(!writer.writing());
        assert_eq!(writer.checkpoint(2), write(2));
    }

    #[test]
    fn without_a_path_nothing_is_written() {
        let mut writer = SessionWriter::<u8>::new(None);
        assert!(!writer.enabled());
        assert_eq!(writer.checkpoint(1), None);
        assert!(!writer.writing());
        writer.set_path(Some("/profile/session.json".into()));
        assert_eq!(writer.path(), Some(&"/profile/session.json".into()));
        assert_eq!(writer.checkpoint(1), write(1));
    }
}
