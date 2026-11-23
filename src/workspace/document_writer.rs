//! Documents' autosaves: one write at a time, with the latest text of each
//! document waiting behind it, so a document is never written older than it
//! was last written. Generic over the write, so the queue rules run without
//! a window or a Vault; the workspace view snapshots documents, runs the
//! writes in the background and reports when each ends. A quit waits until
//! it is idle.
use mdoc_store::document_id::DocumentId;
use std::collections::VecDeque;

/// What a write saves into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    /// The document tab `u64` is becoming: created by its first write.
    Tab(u64),
    Document(DocumentId),
}

pub(super) struct DocumentWriter<W> {
    writing: Option<Target>,
    /// At most one write per target, in the order they first queued.
    pending: VecDeque<(Target, W)>,
}

impl<W> Default for DocumentWriter<W> {
    fn default() -> Self {
        Self {
            writing: None,
            pending: VecDeque::new(),
        }
    }
}

impl<W> DocumentWriter<W> {
    /// Save `write` into `target`, replacing a write of it still waiting.
    /// Returns the write to start now when none runs.
    pub(super) fn queue(&mut self, target: Target, write: W) -> Option<(Target, W)> {
        match self
            .pending
            .iter_mut()
            .find(|(queued, _)| *queued == target)
        {
            Some(waiting) => waiting.1 = write,
            None => self.pending.push_back((target, write)),
        }
        self.next()
    }

    fn next(&mut self) -> Option<(Target, W)> {
        if self.writing.is_some() {
            return None;
        }
        let (target, write) = self.pending.pop_front()?;
        self.writing = Some(target.clone());
        Some((target, write))
    }

    /// The running write ended, however it went; returns the next one.
    pub(super) fn finished(&mut self) -> Option<(Target, W)> {
        self.writing = None;
        self.next()
    }

    /// A write runs or waits.
    pub(super) fn busy(&self) -> bool {
        self.writing.is_some() || !self.pending.is_empty()
    }

    /// A write into `target` runs or waits.
    pub(super) fn has(&self, target: &Target) -> bool {
        self.writing.as_ref() == Some(target) || self.pending.iter().any(|(t, _)| t == target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_store::document_id::VaultId;

    fn document(n: u8) -> Target {
        let id = VaultId::parse(&format!("{n:02x}").repeat(16)).unwrap();
        Target::Document(DocumentId::Vault(id))
    }

    #[test]
    fn one_write_runs_and_only_the_latest_of_each_document_waits() {
        let mut writer = DocumentWriter::default();
        assert!(!writer.busy());
        assert_eq!(writer.queue(document(1), "a1"), Some((document(1), "a1")));
        assert_eq!(writer.queue(document(2), "b1"), None);
        assert_eq!(writer.queue(document(1), "a2"), None);
        assert_eq!(writer.queue(document(1), "a3"), None);
        assert!(writer.has(&document(1)) && writer.has(&document(2)));
        assert_eq!(writer.finished(), Some((document(2), "b1")));
        assert_eq!(writer.finished(), Some((document(1), "a3")));
        assert!(writer.busy());
        assert_eq!(writer.finished(), None);
        assert!(!writer.busy() && !writer.has(&document(1)));
    }

    #[test]
    fn a_tab_being_created_is_known_until_its_write_ends() {
        let mut writer = DocumentWriter::default();
        assert!(writer.queue(Target::Tab(3), "new").is_some());
        assert!(writer.has(&Target::Tab(3)));
        assert_eq!(writer.finished(), None);
        assert!(!writer.has(&Target::Tab(3)));
    }
}
