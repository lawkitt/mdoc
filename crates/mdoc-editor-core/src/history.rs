//! The coalescing undo history: whole-text snapshots, a run of typing or
//! deleting kept as one step, and the history-state ids the review model
//! follows (`mdoc-history`).

use std::ops::Range;

use mdoc_history::{EditorTransaction, HistoryChange, HistoryPath, SourceEdit};
use unicode_segmentation::UnicodeSegmentation;

/// Cap on undo history (full snapshots) to bound memory.
pub const UNDO_LIMIT: usize = 256;

/// A restorable editor state, for undo/redo. Stores the caret offset (not a
/// selection), so undo/redo place the caret rather than re-selecting text.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub content: String,
    pub caret: usize,
    pub history_id: u64,
}

/// The last edit's kind, for coalescing a run of edits into one undo step.
/// `Insert(end)` is a single-grapheme insert whose caret ends at `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert(usize),
    Delete,
    Other,
}

impl EditKind {
    /// The kind of replacing `range` with `new_text`.
    pub fn of(range: &Range<usize>, new_text: &str) -> Self {
        if new_text.is_empty() {
            EditKind::Delete
        } else if range.start == range.end
            && new_text != "\n"
            && new_text.graphemes(true).count() == 1
        {
            EditKind::Insert(range.start + new_text.len())
        } else {
            EditKind::Other
        }
    }
}

/// Undo and redo stacks with the history-state ids of each state.
#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    history_id: u64,
    next_history_id: u64,
    pending: Vec<HistoryChange>,
    last_edit: EditKind,
}

impl Default for History {
    fn default() -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            history_id: 0,
            next_history_id: 0,
            pending: Vec::new(),
            last_edit: EditKind::Other,
        }
    }
}

impl History {
    /// The current history state's id.
    pub fn history_id(&self) -> u64 {
        self.history_id
    }

    /// The last edit's kind (what the next edit may coalesce with).
    pub fn last_edit(&self) -> EditKind {
        self.last_edit
    }

    /// Set the last edit's kind; `EditKind::Other` ends a typing run, so the
    /// next edit starts its own undo step.
    pub fn set_last_edit(&mut self, kind: EditKind) {
        self.last_edit = kind;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The states Undo and Redo reach from here, for hosts that travel the
    /// history by counting presses.
    pub fn path(&self) -> HistoryPath {
        HistoryPath {
            undo: self.undo.iter().map(|s| s.history_id).collect(),
            current: self.history_id,
            redo: self.redo.iter().rev().map(|s| s.history_id).collect(),
        }
    }

    /// Record replacing `range` with `new_text` in `content` (the text before
    /// the edit; `caret` is its forward caret), coalescing a run of
    /// single-grapheme inserts (or a run of deletes) into one undo step so
    /// typing isn't undone one character at a time.
    pub fn record(&mut self, content: &str, caret: usize, range: &Range<usize>, new_text: &str) {
        let kind = EditKind::of(range, new_text);
        let coalesce = match (self.last_edit, kind) {
            (EditKind::Insert(end), EditKind::Insert(_)) => end == range.start,
            (EditKind::Delete, EditKind::Delete) => true,
            _ => false,
        };
        self.begin(
            coalesce,
            vec![SourceEdit {
                range: range.clone(),
                new_len: new_text.len(),
            }],
            content,
            caret,
        );
        self.last_edit = kind;
    }

    /// Start a new history state for `edits` to `content` (the text before
    /// them; `caret` is its forward caret). Without `coalesce` the current
    /// state becomes an undo point; redo is cleared either way.
    pub fn begin(&mut self, coalesce: bool, edits: Vec<SourceEdit>, content: &str, caret: usize) {
        if !coalesce {
            self.push_undo_point(content, caret);
        }
        self.redo.clear();
        self.advance(edits);
    }

    /// Make `content` (with its forward `caret`) an undo point and start a
    /// new branch: redo is cleared. The history state does not change.
    pub fn push_undo_point(&mut self, content: &str, caret: usize) {
        self.undo.push(Snapshot {
            content: content.to_owned(),
            caret,
            history_id: self.history_id,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Move to a new history state reached by `edits`, without an undo point
    /// or touching redo (an IME composition's intermediate updates).
    pub fn advance(&mut self, edits: Vec<SourceEdit>) {
        let before = self.history_id;
        self.next_history_id += 1;
        self.history_id = self.next_history_id;
        self.pending.push(HistoryChange {
            before,
            after: self.history_id,
            edits,
        });
    }

    /// Step back from `content` (the current text; `caret` its forward caret):
    /// the state to restore, `None` with nothing to undo.
    pub fn undo(&mut self, content: &str, caret: usize) -> Option<Snapshot> {
        let prev = self.undo.pop()?;
        self.travel(&prev);
        self.redo.push(Snapshot {
            content: content.to_owned(),
            caret,
            history_id: std::mem::replace(&mut self.history_id, prev.history_id),
        });
        Some(prev)
    }

    /// Step forward from `content`: the state to restore, `None` with nothing
    /// to redo.
    pub fn redo(&mut self, content: &str, caret: usize) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.travel(&next);
        self.undo.push(Snapshot {
            content: content.to_owned(),
            caret,
            history_id: std::mem::replace(&mut self.history_id, next.history_id),
        });
        Some(next)
    }

    fn travel(&mut self, to: &Snapshot) {
        self.pending.push(HistoryChange {
            before: self.history_id,
            after: to.history_id,
            edits: Vec::new(),
        });
        self.last_edit = EditKind::Other;
    }

    /// Forget every state: a programmatic load isn't undoable to the prior
    /// document.
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.next_history_id += 1;
        self.history_id = self.next_history_id;
        self.pending.clear();
        self.last_edit = EditKind::Other;
    }

    /// The changes since the last call, as one transaction at `revision`;
    /// `None` when nothing changed.
    pub fn take_transaction(&mut self, revision: u64) -> Option<EditorTransaction> {
        if self.pending.is_empty() {
            return None;
        }
        let mut retained: Vec<_> = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(|s| s.history_id)
            .chain(std::iter::once(self.history_id))
            .collect();
        retained.sort_unstable();
        retained.dedup();
        Some(EditorTransaction {
            revision,
            changes: std::mem::take(&mut self.pending),
            retained,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_coalesces_and_undo_redo_travel_the_ids() {
        let mut h = History::default();
        h.record("", 0, &(0..0), "a");
        h.record("a", 1, &(1..1), "b");
        assert_eq!(h.path().undo, vec![0]);
        let typed = h.history_id();
        // A newline is its own step.
        h.record("ab", 2, &(2..2), "\n");
        assert_eq!(h.path().undo, vec![0, typed]);
        let tx = h.take_transaction(7).unwrap();
        assert_eq!((tx.revision, tx.changes.len()), (7, 3));
        assert!(h.take_transaction(8).is_none());

        let back = h.undo("ab\n", 3).unwrap();
        assert_eq!((back.content.as_str(), back.history_id), ("ab", typed));
        assert_eq!(h.history_id(), typed);
        assert_eq!(h.last_edit(), EditKind::Other);
        let fwd = h.redo("ab", 2).unwrap();
        assert_eq!(fwd.content, "ab\n");
        assert!(h.can_undo() && !h.can_redo());
    }

    #[test]
    fn deletes_coalesce_and_a_gap_breaks_typing() {
        let mut h = History::default();
        h.record("abc", 3, &(2..3), "");
        h.record("ab", 2, &(1..2), "");
        assert_eq!(h.path().undo.len(), 1);
        h.record("a", 1, &(1..1), "x");
        h.record("ax", 2, &(0..0), "y");
        assert_eq!(h.path().undo.len(), 3);
    }

    #[test]
    fn the_stack_keeps_the_newest_steps() {
        let mut h = History::default();
        for _ in 0..UNDO_LIMIT + 10 {
            h.record("", 0, &(0..0), "\n");
        }
        assert_eq!(h.path().undo.len(), UNDO_LIMIT);
        h.reset();
        assert!(!h.can_undo());
    }
}
