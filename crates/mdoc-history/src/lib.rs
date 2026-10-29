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

/// Where an editor stands in its linear history: Undo reaches `undo` (oldest
/// first, nearest last), Redo reaches `redo` (nearest first).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryPath {
    pub undo: Vec<u64>,
    pub current: u64,
    pub redo: Vec<u64>,
}

impl HistoryPath {
    /// The presses that reach `state`: negative for Undo, positive for Redo,
    /// zero when it is current, `None` when neither reaches it any more.
    pub fn steps_to(&self, state: u64) -> Option<isize> {
        if state == self.current {
            return Some(0);
        }
        if let Some(i) = self.undo.iter().rposition(|&s| s == state) {
            return Some(-((self.undo.len() - i) as isize));
        }
        self.redo
            .iter()
            .position(|&s| s == state)
            .map(|i| i as isize + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::HistoryPath;

    #[test]
    fn steps_count_presses_along_the_one_history() {
        let path = HistoryPath {
            undo: vec![1, 2, 3],
            current: 4,
            redo: vec![5, 6],
        };
        assert_eq!(path.steps_to(4), Some(0));
        assert_eq!(path.steps_to(3), Some(-1));
        assert_eq!(path.steps_to(1), Some(-3));
        assert_eq!(path.steps_to(5), Some(1));
        assert_eq!(path.steps_to(6), Some(2));
        assert_eq!(path.steps_to(9), None);
    }
}
