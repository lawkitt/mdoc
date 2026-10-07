//! Live occurrence provenance and a reversible metadata journal. Never serialized.
use super::Category;
use mdoc_editor::{EditorTransaction, SourceEdit, inverse_edits};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct Step {
    pub before: Arc<str>,
    pub after: Arc<str>,
    pub category: Category,
    pub predecessor: Option<Arc<Step>>,
}
/// Immutable values shared across a source-sorted replacement batch.
pub type ReplacementPlan = (Range<usize>, Arc<str>, Arc<str>, Category);
type StepKey = (Arc<str>, Arc<str>, Category, usize);
#[derive(Clone, Debug)]
pub struct Applied {
    pub id: u64,
    pub range: Range<usize>,
    pub step: Arc<Step>,
}
#[derive(Clone, Debug)]
pub(super) struct Exclusion {
    pub range: Range<usize>,
    pub original: Arc<str>,
    pub id: u64,
}
#[derive(Default)]
struct Delta {
    before: u64,
    edits: Vec<SourceEdit>,
    removed: Vec<Applied>,
    added: Vec<Applied>,
    removed_keeps: Vec<Exclusion>,
    added_keeps: Vec<Exclusion>,
}
#[derive(Default)]
pub struct Tracking {
    pub applied: Vec<Applied>,
    pub(super) exclusions: Vec<Exclusion>,
    lookup: HashMap<u64, usize>,
    journal: HashMap<u64, Delta>,
    current: Option<u64>,
    next_id: u64,
    revision: u64,
    head: u64,
    oldest: u64,
}

// Input spans and edits are sorted. Insertions at start shift, at end preserve;
// every overlap and insertion strictly inside invalidates the span.
pub(super) fn rebase<T>(
    items: &mut Vec<T>,
    edits: &[SourceEdit],
    range: impl Fn(&mut T) -> &mut Range<usize>,
) -> Vec<T> {
    let mut removed = Vec::new();
    let mut kept = Vec::with_capacity(items.len());
    let mut index = 0;
    let mut delta = 0isize;
    for mut item in std::mem::take(items) {
        let span = range(&mut item);
        while let Some(edit) = edits.get(index).filter(|e| e.range.end <= span.start) {
            delta += edit.new_len as isize - edit.range.len() as isize;
            index += 1;
        }
        if edits.get(index).is_some_and(|e| e.range.start < span.end) {
            removed.push(item);
        } else {
            *span = span.start.saturating_add_signed(delta)..span.end.saturating_add_signed(delta);
            kept.push(item);
        }
    }
    *items = kept;
    removed
}
fn merge<T>(items: &mut Vec<T>, added: Vec<T>, key: impl Fn(&T) -> usize) {
    if added.is_empty() {
        return;
    }
    let mut left = std::mem::take(items).into_iter().peekable();
    let mut right = added.into_iter().peekable();
    let mut result = Vec::with_capacity(left.len() + right.len());
    while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
        if key(a) <= key(b) {
            result.push(left.next().unwrap());
        } else {
            result.push(right.next().unwrap());
        }
    }
    result.extend(left);
    result.extend(right);
    *items = result;
}
// Squash adjacent edits in a coalesced undo unit. Disjoint/structural batches
// retain their exact paths; no intermediate identity still reachable by Undo
// may be removed. This covers ordinary typing/deletion and IME replacement.
fn compose_adjacent(first: &SourceEdit, next: &SourceEdit) -> Option<SourceEdit> {
    let destination_end = first.range.start + first.new_len;
    if next.range.start > destination_end || next.range.end < first.range.start {
        return None;
    }
    let start = first.range.start.min(next.range.start);
    let destination_len = destination_end.max(next.range.end) - start;
    Some(SourceEdit {
        range: start..first.range.end + next.range.end.saturating_sub(destination_end),
        new_len: destination_len - next.range.len() + next.new_len,
    })
}
impl Tracking {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
    fn index(&mut self) {
        self.lookup.clear();
        self.lookup
            .extend(self.applied.iter().enumerate().map(|(i, o)| (o.id, i)));
    }
    pub fn matches_history(&self, history: u64) -> bool {
        self.current == Some(history)
    }
    pub fn get(&self, id: u64) -> Option<&Applied> {
        self.lookup.get(&id).map(|&i| &self.applied[i])
    }
    pub fn at(&self, range: &Range<usize>) -> Option<&Applied> {
        let i = self
            .applied
            .partition_point(|o| o.range.start < range.start);
        self.applied.get(i).filter(|o| o.range == *range)
    }
    pub fn keep(&mut self, range: Range<usize>, original: Arc<str>) {
        let id = self.id();
        merge(
            &mut self.exclusions,
            vec![Exclusion {
                id,
                range,
                original,
            }],
            |o| o.range.start,
        );
        self.index();
    }
    pub fn on_transaction(&mut self, transaction: &EditorTransaction) -> bool {
        if transaction.revision <= self.revision {
            return false;
        }
        self.revision = transaction.revision;
        let mut branch = false;
        for change in &transaction.changes {
            if self.current == Some(change.after) {
                continue;
            }
            if self.current.is_none() {
                self.current = Some(change.before);
            }
            if !change.edits.is_empty() {
                branch |= self.head != change.before;
                self.head = change.after;
                let removed = rebase(&mut self.applied, &change.edits, |o| &mut o.range);
                let removed_keeps = rebase(&mut self.exclusions, &change.edits, |o| &mut o.range);
                let mut delta = Delta {
                    before: change.before,
                    edits: change.edits.clone(),
                    removed,
                    removed_keeps,
                    ..Default::default()
                };
                if !transaction.retained.contains(&change.before)
                    && let Some(previous) = self.journal.get(&change.before)
                    && previous.edits.len() == 1
                    && delta.edits.len() == 1
                    && previous.added.is_empty()
                    && previous.added_keeps.is_empty()
                    && let Some(composed) = compose_adjacent(&previous.edits[0], &delta.edits[0])
                {
                    let previous = self.journal.remove(&change.before).unwrap();
                    // Newly invalidated fields are in intermediate coordinates.
                    // Return them to the original undo-unit source before merging.
                    let inverse = inverse_edits(&previous.edits);
                    rebase(&mut delta.removed, &inverse, |o| &mut o.range);
                    rebase(&mut delta.removed_keeps, &inverse, |o| &mut o.range);
                    merge(&mut delta.removed, previous.removed, |o| o.range.start);
                    merge(&mut delta.removed_keeps, previous.removed_keeps, |o| {
                        o.range.start
                    });
                    delta.before = previous.before;
                    delta.edits = vec![composed];
                }
                self.journal.insert(change.after, delta);
                self.current = Some(change.after);
            } else {
                self.travel(change.after);
            }
        }
        let oldest = transaction.retained.first().copied().unwrap_or(0);
        if branch || oldest != self.oldest {
            self.prune(&transaction.retained);
            self.oldest = oldest;
        }
        self.index();
        true
    }
    fn travel(&mut self, target: u64) {
        let current = self.current.unwrap_or(target);
        let mut path = Vec::new();
        let mut at = current;
        while at != target {
            let Some(delta) = self.journal.get(&at) else {
                break;
            };
            path.push(at);
            at = delta.before;
        }
        if at == target {
            for id in path {
                let delta = &self.journal[&id];
                let added: HashSet<_> = delta.added.iter().map(|o| o.id).collect();
                let keeps: HashSet<_> = delta.added_keeps.iter().map(|o| o.id).collect();
                self.applied.retain(|o| !added.contains(&o.id));
                self.exclusions.retain(|o| !keeps.contains(&o.id));
                rebase(&mut self.applied, &inverse_edits(&delta.edits), |o| {
                    &mut o.range
                });
                rebase(&mut self.exclusions, &inverse_edits(&delta.edits), |o| {
                    &mut o.range
                });
                merge(&mut self.applied, delta.removed.clone(), |o| o.range.start);
                merge(&mut self.exclusions, delta.removed_keeps.clone(), |o| {
                    o.range.start
                });
                self.index();
            }
        } else {
            path.clear();
            at = target;
            while at != current {
                let Some(delta) = self.journal.get(&at) else {
                    break;
                };
                path.push(at);
                at = delta.before;
            }
            if at != current {
                // A host load/reset has no reversible metadata. Never invent provenance.
                self.applied.clear();
                self.exclusions.clear();
            } else {
                for id in path.into_iter().rev() {
                    let delta = &self.journal[&id];
                    rebase(&mut self.applied, &delta.edits, |o| &mut o.range);
                    rebase(&mut self.exclusions, &delta.edits, |o| &mut o.range);
                    merge(&mut self.applied, delta.added.clone(), |o| o.range.start);
                    merge(&mut self.exclusions, delta.added_keeps.clone(), |o| {
                        o.range.start
                    });
                    self.index();
                }
            }
        }
        self.current = Some(target);
    }
    fn prune(&mut self, retained: &[u64]) {
        let oldest = retained.iter().copied().min().unwrap_or(0);
        let mut live = HashSet::new();
        for &id in retained {
            let mut at = id;
            while at > oldest && live.insert(at) {
                let Some(delta) = self.journal.get(&at) else {
                    break;
                };
                at = delta.before;
            }
        }
        self.journal.retain(|id, _| live.contains(id));
    }
    /// Prepare before the text commit; captured predecessor is occurrence-specific.
    pub fn prepare(&mut self, plans: &[ReplacementPlan]) -> Vec<Applied> {
        let mut delta = 0isize;
        let mut steps: HashMap<StepKey, Arc<Step>> = HashMap::new();
        plans
            .iter()
            .map(|(range, before, after, category)| {
                let predecessor = self.at(range).map(|o| o.step.clone());
                let key = (
                    before.clone(),
                    after.clone(),
                    *category,
                    predecessor.as_ref().map_or(0, |s| Arc::as_ptr(s) as usize),
                );
                let step = steps
                    .entry(key)
                    .or_insert_with(|| {
                        Arc::new(Step {
                            before: before.clone(),
                            after: after.clone(),
                            category: *category,
                            predecessor,
                        })
                    })
                    .clone();
                let id = self.at(range).map(|o| o.id).unwrap_or_else(|| self.id());
                let start = range.start.saturating_add_signed(delta);
                delta += after.len() as isize - range.len() as isize;
                Applied {
                    id,
                    range: start..start + after.len(),
                    step,
                }
            })
            .collect()
    }
    pub fn commit(&mut self, history: u64, added: Vec<Applied>) {
        if let Some(delta) = self.journal.get_mut(&history) {
            delta.added.extend(added.clone());
        }
        merge(&mut self.applied, added, |o| o.range.start);
        self.index();
    }
    pub fn restore_plan(
        &self,
        source: &str,
        id: u64,
        all: bool,
    ) -> Result<Vec<(Range<usize>, String)>, String> {
        let selected = self.get(id).ok_or("Replacement is no longer available.")?;
        let targets: Vec<_> = self
            .applied
            .iter()
            .filter(|o| o.id == id || (all && o.step.before == selected.step.before))
            .collect();
        if targets
            .iter()
            .any(|o| source.get(o.range.clone()) != Some(o.step.after.as_ref()))
        {
            return Err("Replacement changed. Review it again.".into());
        }
        Ok(targets
            .into_iter()
            .map(|o| (o.range.clone(), o.step.before.to_string()))
            .collect())
    }
    pub fn prepare_restore(
        &self,
        edits: &[(Range<usize>, String)],
    ) -> Vec<(Applied, Range<usize>)> {
        let mut delta = 0isize;
        edits
            .iter()
            .filter_map(|(range, value)| {
                let original = self.at(range)?.clone();
                let start = range.start.saturating_add_signed(delta);
                delta += value.len() as isize - range.len() as isize;
                Some((original, start..start + value.len()))
            })
            .collect()
    }
    pub fn commit_restore(&mut self, history: u64, restored: Vec<(Applied, Range<usize>)>) {
        let mut added = Vec::new();
        let mut keeps = Vec::new();
        for (old, range) in restored {
            let id = self.id();
            keeps.push(Exclusion {
                id,
                range: range.clone(),
                original: old.step.before.clone(),
            });
            if let Some(step) = old.step.predecessor.clone() {
                added.push(Applied {
                    id: old.id,
                    range,
                    step,
                });
            }
        }
        if let Some(delta) = self.journal.get_mut(&history) {
            delta.added_keeps.extend(keeps.clone());
        }
        merge(&mut self.exclusions, keeps, |o| o.range.start);
        self.commit(history, added);
    }
}

#[cfg(test)]
#[path = "tracking_tests.rs"]
mod tests;
