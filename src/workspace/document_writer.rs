//! The window's writes into the Vault, one job at a time (ADR 0130): every
//! save, move, delete, export, import and reply, and the quit last. At most
//! one job per target waits, so a later request of a target replaces the
//! one waiting. Generic over the job, so the queue rules run without a
//! window or a Vault; the workspace view runs each job when it reaches the
//! head, reading what it writes then, and reports when it ends.
use mdoc_store::document_id::DocumentId;
use std::collections::VecDeque;

/// What a write saves into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    /// The document tab `u64` is becoming: created by its first write.
    Tab(u64),
    Document(DocumentId),
    /// A Project, as the window holds it when its write starts.
    Project(String),
    /// A move, a delete, an export, an import, a reply's save or delete:
    /// each is its own.
    Op(u64),
    /// The AI reply in tab `u64`'s reply box: only its latest text waits.
    Reply(u64),
    /// A Vault document's private notes: only their latest text waits.
    Notes(mdoc_store::document_id::VaultId),
    /// The user-wide lists: only their latest change waits.
    UserLists,
    /// The quit: the last job.
    Quit,
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
    /// A waiting quit stays the last job: what is queued while it waits
    /// goes before it. Returns the write to start now when none runs.
    pub(super) fn queue(&mut self, target: Target, write: W) -> Option<(Target, W)> {
        match self
            .pending
            .iter_mut()
            .find(|(queued, _)| *queued == target)
        {
            Some(waiting) => waiting.1 = write,
            None => match self.pending.iter().position(|(t, _)| *t == Target::Quit) {
                Some(quit) => self.pending.insert(quit, (target, write)),
                None => self.pending.push_back((target, write)),
            },
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

    /// ADR 0130: the quit is the last job, whatever is queued while it
    /// waits.
    #[test]
    fn what_is_queued_while_a_quit_waits_goes_before_it() {
        let mut writer = DocumentWriter::default();
        assert!(writer.queue(document(1), "a1").is_some());
        assert_eq!(writer.queue(Target::Quit, "quit"), None);
        assert_eq!(writer.queue(Target::UserLists, "lists"), None);
        assert_eq!(writer.queue(document(2), "b1"), None);
        assert_eq!(writer.finished(), Some((Target::UserLists, "lists")));
        assert_eq!(writer.finished(), Some((document(2), "b1")));
        assert_eq!(writer.finished(), Some((Target::Quit, "quit")));
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
