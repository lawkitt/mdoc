//! Generic source transactions and text-history identity. No host policy.
//! Kept free of dependencies so history-following models need no GUI toolkit.
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub range: Range<usize>,
    pub new_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryChange {
    pub before: u64,
    pub after: u64,
    /// Sorted edits in pre-operation coordinates; empty for undo/redo travel.
    pub edits: Vec<SourceEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorTransaction {
    pub revision: u64,
    pub changes: Vec<HistoryChange>,
    /// Current state and every state reachable through the editor's history.
    pub retained: Vec<u64>,
}

/// Derive exact destination ranges in the same ordered pass as a batch build.
pub fn inverse_edits(edits: &[SourceEdit]) -> Vec<SourceEdit> {
    let mut delta = 0isize;
    edits
        .iter()
        .map(|edit| {
            let start = edit.range.start.saturating_add_signed(delta);
            delta += edit.new_len as isize - edit.range.len() as isize;
            SourceEdit {
                range: start..start + edit.new_len,
                new_len: edit.range.len(),
            }
        })
        .collect()
}
