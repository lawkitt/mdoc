//! Undo and redo (ADR 0098): the line of states desktop's editor keeps
//! (`crates/mdoc-editor/src/input.rs`, 256 steps), each with its text and
//! the step that made it. Travelling sends `mdoc_pii::Review` the same
//! transaction an editor undo sends: a change with no edits, and every state
//! still reachable as retained.
use super::ReviewText;
use crate::CoreError;
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use std::ops::Range;

/// As desktop's editor (`UNDO_LIMIT`).
pub(crate) const UNDO_LIMIT: usize = 256;

/// What one undo step did, for the toast that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StepKind {
    /// Replace, of one mention or of an identity's.
    Replace,
    /// Keep, of one mention or of an identity's.
    Keep,
    /// Replace all: every proposal in the document.
    ReplaceAll,
    /// A selection marked as a proposal.
    Mark,
    Rename,
    Category,
    /// Same as…: an identity, or one mention, linked to another identity.
    SameAs,
    /// A mention split off under an alias of its own.
    NewAlias,
    /// Belongs to…: a contact's owner set or cleared.
    Owner,
    /// Joining a case renumbered aliases or proposed its names.
    JoinCase,
}

impl StepKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Replace => "Replace",
            Self::Keep => "Keep",
            Self::ReplaceAll => "Replace all",
            Self::Mark => "Mark",
            Self::Rename => "Rename",
            Self::Category => "Category",
            Self::SameAs => "Same as",
            Self::NewAlias => "New alias",
            Self::Owner => "Belongs to",
            Self::JoinCase => "Join case",
        }
    }
}

/// Where a step acted in one state's text: a mention, an identity or both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Focus {
    pub range: Option<Range<usize>>,
    pub identity: Option<u64>,
}

/// One undo step: what it did and where, before and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub kind: StepKind,
    /// What the toast names: a wording, an alias, a category or a count.
    pub subject: String,
    /// Where it acted, in the text before the step.
    pub before: Focus,
    /// Where it acted, in the text after it; left default, it is `before`
    /// carried through the step's edits.
    pub after: Focus,
}

impl Step {
    pub fn new(kind: StepKind, subject: impl Into<String>, before: Focus) -> Self {
        Self {
            kind,
            subject: subject.into(),
            before,
            after: Focus::default(),
        }
    }

    /// "Replace · Riverside Holdings"; the kind alone without a subject.
    pub fn label(&self) -> String {
        if self.subject.is_empty() {
            self.kind.label().to_owned()
        } else {
            format!("{} · {}", self.kind.label(), self.subject)
        }
    }
}

/// A state Undo or Redo reaches: the editor's snapshot.
pub(super) struct Saved {
    pub history: u64,
    pub text: String,
    /// The step that made this state; none for the first.
    pub step: Option<Step>,
}

impl ReviewText {
    /// Run `change` as one undo step, however many transactions it sends;
    /// a change that sends none is no step and keeps Redo. The states in
    /// between are not retained, as an editor's coalesced typing.
    pub(super) fn step(
        &mut self,
        mut step: Step,
        change: impl FnOnce(&mut Self, &mut Step) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        debug_assert!(self.base.is_none(), "steps never nest");
        self.base = Some(Saved {
            history: self.history,
            text: self.text.clone(),
            step: self.step.clone(),
        });
        let redo = std::mem::take(&mut self.redo);
        self.edits.clear();
        let result = change(self, &mut step);
        let base = self.base.take().expect("the step in progress");
        if self.history == base.history {
            self.redo = redo;
            return result;
        }
        if step.after == Focus::default() {
            step.after = Focus {
                range: step
                    .before
                    .range
                    .as_ref()
                    .map(|range| self.edits.iter().fold(range.clone(), |r, e| moved(r, e))),
                identity: step.before.identity,
            };
        }
        self.undo.push(base);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.step = Some(step);
        result
    }

    /// Every state the history still reaches, as `EditorTransaction`
    /// retains them: Undo's, the step in progress's start, Redo's, current.
    pub(super) fn retained(&self) -> Vec<u64> {
        let mut retained: Vec<_> = self
            .undo
            .iter()
            .chain(&self.base)
            .chain(&self.redo)
            .map(|s| s.history)
            .chain(std::iter::once(self.history))
            .collect();
        retained.sort_unstable();
        retained.dedup();
        retained
    }

    /// Undo the last step; false when there is none.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = Saved {
            history: self.history,
            text: std::mem::replace(&mut self.text, previous.text),
            step: std::mem::replace(&mut self.step, previous.step),
        };
        let before = current.history;
        self.redo.push(current);
        self.travel(before, previous.history);
        true
    }

    /// Redo the last undone step; false when there is none.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = Saved {
            history: self.history,
            text: std::mem::replace(&mut self.text, next.text),
            step: std::mem::replace(&mut self.step, next.step),
        };
        let before = current.history;
        self.undo.push(current);
        self.travel(before, next.history);
        true
    }

    /// The review follows the editor to state `after`, whose text is set.
    fn travel(&mut self, before: u64, after: u64) {
        self.history = after;
        self.transaction += 1;
        let transaction = EditorTransaction {
            revision: self.transaction,
            changes: vec![HistoryChange {
                before,
                after,
                edits: Vec::new(),
            }],
            retained: self.retained(),
        };
        self.review.on_transaction(&transaction, &self.text);
        // Desktop rediscovers proposals after travel (its `Changed`).
        self.refresh();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The step Undo reverts, and where it acted in the current text.
    pub fn undo_step(&self) -> Option<(&Step, &Focus)> {
        let step = self.step.as_ref().filter(|_| self.can_undo())?;
        Some((step, &step.after))
    }

    /// The step Redo makes again, and where it acts in the current text.
    pub fn redo_step(&self) -> Option<(&Step, &Focus)> {
        let step = self.redo.last()?.step.as_ref()?;
        Some((step, &step.before))
    }
}

/// `range` carried through sorted edits in pre-edit coordinates: an edit
/// of exactly the range gives its new text; others before it shift it.
pub(super) fn moved(range: Range<usize>, edits: &[SourceEdit]) -> Range<usize> {
    let mut shift = 0isize;
    for edit in edits {
        if edit.range == range {
            let start = range.start.saturating_add_signed(shift);
            return start..start + edit.new_len;
        }
        if edit.range.end <= range.start {
            shift += edit.new_len as isize - edit.range.len() as isize;
        } else if edit.range.start >= range.end {
            break;
        }
    }
    range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift)
}
