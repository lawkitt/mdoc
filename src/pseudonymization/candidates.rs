//! Detected originals (groups) and their pending mentions, indexed for lookup.
use super::{CandidateOccurrence, Group, tracking};
use aho_corasick::AhoCorasick;
use mdoc_editor::SourceEdit;
use std::{collections::HashMap, ops::Range, sync::Arc};

#[derive(Default)]
pub(super) struct Candidates {
    pub(super) groups: Vec<Group>,
    group_lookup: HashMap<u64, usize>,
    original_lookup: HashMap<Arc<str>, u64>,
    occurrences: Vec<CandidateOccurrence>,
    occurrence_lookup: HashMap<u64, usize>,
    counters: HashMap<u64, u64>,
    /// Cached discovery matcher over every group original, in group order.
    pub(super) matcher: Option<Arc<AhoCorasick>>,
}
impl Candidates {
    pub(super) fn group(&self, id: u64) -> Option<&Group> {
        self.group_lookup.get(&id).map(|&index| &self.groups[index])
    }
    pub(super) fn group_mut(&mut self, id: u64) -> Option<&mut Group> {
        self.group_lookup
            .get(&id)
            .map(|&index| &mut self.groups[index])
    }
    pub(super) fn by_original(&self, original: &str) -> Option<u64> {
        self.original_lookup.get(original).copied()
    }
    /// Pending mentions in source order.
    pub(super) fn occurrences(&self) -> &[CandidateOccurrence] {
        &self.occurrences
    }
    pub(super) fn occurrence(&self, id: u64) -> Option<&CandidateOccurrence> {
        self.occurrence_lookup
            .get(&id)
            .map(|&i| &self.occurrences[i])
    }
    pub(super) fn annotation_id(&self, group: u64, range: &Range<usize>) -> Option<u64> {
        let index = self
            .occurrences
            .partition_point(|o| o.range.start < range.start);
        self.occurrences
            .get(index)
            .filter(|o| o.group == group && o.range == *range)
            .map(|o| o.id)
    }
    pub(super) fn remaining(&self) -> usize {
        self.groups.iter().map(|group| group.mentions.len()).sum()
    }
    /// Register a new original; the cached matcher no longer covers it.
    pub(super) fn push(&mut self, group: Group) {
        self.group_lookup.insert(group.id, self.groups.len());
        self.original_lookup
            .insert(group.original.clone(), group.id);
        self.groups.push(group);
        self.matcher = None;
    }
    /// Follow one history change: edits rebase pending mentions, while undo and
    /// redo discard them until rediscovery.
    pub(super) fn follow(&mut self, edits: &[SourceEdit]) {
        if edits.is_empty() {
            self.occurrences.clear();
        } else {
            tracking::rebase(&mut self.occurrences, edits, |o| &mut o.range);
        }
    }
    /// After following a transaction, group mentions await rediscovery.
    pub(super) fn await_discovery(&mut self) {
        self.reindex();
        for group in &mut self.groups {
            group.mentions.clear();
        }
    }
    /// Replace every group's mentions, keeping occurrence ids stable for
    /// unchanged spans so active annotations survive rediscovery.
    pub(super) fn set_mentions(&mut self, mentions: Vec<Vec<Range<usize>>>) {
        let mut old: HashMap<_, _> = self
            .occurrences
            .drain(..)
            .map(|o| ((o.group, o.range.start, o.range.end), o.id))
            .collect();
        for (group, mentions) in self.groups.iter_mut().zip(mentions) {
            for range in &mentions {
                let id = old
                    .remove(&(group.id, range.start, range.end))
                    .unwrap_or_else(|| {
                        let next = self.counters.entry(group.id).or_default();
                        let id = (group.id << 32) | *next;
                        *next += 1;
                        id
                    });
                self.occurrences.push(CandidateOccurrence {
                    id,
                    group: group.id,
                    range: range.clone(),
                });
            }
            group.mentions = mentions;
        }
        self.occurrences.sort_by_key(|o| o.range.start);
        self.reindex();
    }
    fn reindex(&mut self) {
        self.occurrence_lookup.clear();
        self.occurrence_lookup
            .extend(self.occurrences.iter().enumerate().map(|(i, o)| (o.id, i)));
    }
}
