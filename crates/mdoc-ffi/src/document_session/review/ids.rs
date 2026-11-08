//! Mention ids that stay the same across a change (ADR 0104): the apps keep
//! the review bar on a mention by its id while Replace, Keep or an edit
//! elsewhere moves its range. `mdoc_pii` numbers proposals, replacements and
//! kept mentions apart, so a mention changing state would change id; here a
//! mention keeps its id while the text's edits carry its range to the
//! mention there now, and Undo and Redo bring back the ids of the state they
//! reach.
use mdoc_history::SourceEdit;
use std::{collections::HashMap, ops::Range};

#[derive(Default)]
pub(crate) struct MentionIds {
    /// Each mention's range and id at the last sync, in source order.
    table: Vec<(Range<usize>, u64)>,
    /// The text's edits since, one sorted batch per transaction.
    edits: Vec<Vec<SourceEdit>>,
    /// The last id handed out; ids are never reused.
    last: u64,
}

impl MentionIds {
    /// The text changed by one transaction's edits.
    pub fn edited(&mut self, batch: &[SourceEdit]) {
        self.edits.push(batch.to_vec());
    }

    /// Ids for the mentions at `ranges`, sorted: a mention the edits since
    /// the last sync carried from one there keeps its id, others get new
    /// ones.
    pub fn sync(&mut self, ranges: Vec<Range<usize>>) {
        let mut old = HashMap::new();
        for (range, id) in self.table.drain(..) {
            let carried = self
                .edits
                .iter()
                .try_fold(range, |range, batch| carried(range, batch));
            if let Some(range) = carried {
                old.entry((range.start, range.end)).or_insert(id);
            }
        }
        self.edits.clear();
        let last = &mut self.last;
        self.table = ranges
            .into_iter()
            .map(|range| {
                let id = old.remove(&(range.start, range.end)).unwrap_or_else(|| {
                    *last += 1;
                    *last
                });
                (range, id)
            })
            .collect();
    }

    /// The id of the mention at `range`, as of the last sync.
    pub fn id(&self, range: &Range<usize>) -> Option<u64> {
        self.table
            .binary_search_by_key(&(range.start, range.end), |(r, _)| (r.start, r.end))
            .ok()
            .map(|i| self.table[i].1)
    }

    /// The range of mention `id`, as of the last sync.
    pub fn range(&self, id: u64) -> Option<Range<usize>> {
        self.table
            .iter()
            .find(|(_, other)| *other == id)
            .map(|(range, _)| range.clone())
    }

    /// The ids as of the last sync, for a state Undo or Redo reaches.
    pub fn saved(&self) -> Vec<(Range<usize>, u64)> {
        self.table.clone()
    }

    /// Undo or Redo reached a state with these ids.
    pub fn restore(&mut self, table: Vec<(Range<usize>, u64)>) {
        self.table = table;
        self.edits.clear();
    }
}

/// `range` carried through one transaction's sorted edits, in pre-edit
/// coordinates: an edit of exactly the range gives its new text, edits
/// before it shift it, and one that cuts into it ends it.
fn carried(range: Range<usize>, edits: &[SourceEdit]) -> Option<Range<usize>> {
    let mut shift = 0isize;
    for edit in edits {
        if edit.range == range {
            let start = range.start.saturating_add_signed(shift);
            return Some(start..start + edit.new_len);
        }
        if edit.range.end <= range.start {
            shift += edit.new_len as isize - edit.range.len() as isize;
        } else if edit.range.start >= range.end {
            break;
        } else {
            return None;
        }
    }
    Some(range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(range: Range<usize>, new_len: usize) -> SourceEdit {
        SourceEdit { range, new_len }
    }

    #[test]
    fn edits_carry_ids_and_cuts_end_them() {
        let mut ids = MentionIds::default();
        ids.sync(vec![0..3, 10..13, 20..23]);
        let (a, b, c) = (ids.id(&(0..3)), ids.id(&(10..13)), ids.id(&(20..23)));
        // The first replaced by 8 bytes, the third cut into.
        ids.edited(&[edit(0..3, 8), edit(21..22, 0)]);
        ids.sync(vec![0..8, 15..18, 25..27]);
        assert_eq!(ids.id(&(0..8)), a);
        assert_eq!(ids.id(&(15..18)), b);
        assert!(ids.id(&(25..27)).is_some_and(|id| Some(id) != c));
        assert_eq!(ids.range(b.unwrap()), Some(15..18));
        // No edits: ids follow ranges, and new mentions get new ids.
        let saved = ids.saved();
        ids.sync(vec![0..8, 15..18, 30..31]);
        assert_eq!(ids.id(&(15..18)), b);
        ids.restore(saved);
        assert_eq!(ids.id(&(0..8)), a);
    }
}
