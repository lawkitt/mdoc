//! Detected originals (groups) and their pending mentions, indexed for lookup.
use super::{Candidate, Variant, tracking};
use aho_corasick::AhoCorasick;
use mdoc_history::SourceEdit;
use std::{collections::HashMap, ops::Range, sync::Arc};

#[derive(Default)]
pub(super) struct Candidates {
    pub(super) variants: Vec<Variant>,
    group_lookup: HashMap<u64, usize>,
    original_lookup: HashMap<Arc<str>, u64>,
    occurrences: Vec<Candidate>,
    occurrence_lookup: HashMap<u64, usize>,
    /// Exact short-number detections follow editor undo/redo, like policy.
    location_history: HashMap<u64, HashMap<u64, Vec<Range<usize>>>>,
    counters: HashMap<u64, u64>,
    /// Cached discovery matcher: raw and read wording per group, in order.
    pub(super) matcher: Option<Arc<AhoCorasick>>,
}
impl Candidates {
    pub(super) fn forget(&mut self, ids: &std::collections::HashSet<u64>) {
        self.variants.retain(|variant| !ids.contains(&variant.id));
        self.occurrences
            .retain(|occurrence| !ids.contains(&occurrence.variant));
        self.group_lookup.clear();
        self.original_lookup.clear();
        for (index, variant) in self.variants.iter().enumerate() {
            self.group_lookup.insert(variant.id, index);
            self.original_lookup
                .insert(variant.original.clone(), variant.id);
        }
        self.counters.retain(|id, _| !ids.contains(id));
        self.matcher = None;
        self.reindex();
    }
    pub(super) fn variant(&self, id: u64) -> Option<&Variant> {
        self.group_lookup
            .get(&id)
            .map(|&index| &self.variants[index])
    }
    pub(super) fn variant_mut(&mut self, id: u64) -> Option<&mut Variant> {
        self.group_lookup
            .get(&id)
            .map(|&index| &mut self.variants[index])
    }
    pub(super) fn by_original(&self, original: &str) -> Option<u64> {
        self.original_lookup.get(original).copied()
    }
    /// Pending mentions in source order.
    pub(super) fn occurrences(&self) -> &[Candidate] {
        &self.occurrences
    }
    pub(super) fn occurrence(&self, id: u64) -> Option<&Candidate> {
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
            .filter(|o| o.variant == group && o.range == *range)
            .map(|o| o.id)
    }
    pub(super) fn remaining(&self) -> usize {
        self.variants.iter().map(|group| group.mentions.len()).sum()
    }
    /// Register a new original; the cached matcher no longer covers it.
    pub(super) fn push(&mut self, group: Variant) {
        self.group_lookup.insert(group.id, self.variants.len());
        self.original_lookup
            .insert(group.original.clone(), group.id);
        self.variants.push(group);
        self.matcher = None;
    }
    /// Follow one history change: edits rebase pending mentions, while undo and
    /// redo discard them until rediscovery.
    pub(super) fn rebase_locations(&mut self, edits: &[SourceEdit]) {
        for variant in &mut self.variants {
            if let Some(locations) = variant.locations.as_mut() {
                tracking::rebase(locations, edits, |range| range);
            }
        }
    }
    pub(super) fn follow(&mut self, change: &mdoc_history::HistoryChange) {
        self.location_history.insert(
            change.before,
            self.variants
                .iter()
                .filter_map(|v| v.locations.clone().map(|ranges| (v.id, ranges)))
                .collect(),
        );
        if change.edits.is_empty() {
            let saved = self.location_history.get(&change.after);
            for variant in &mut self.variants {
                if let Some(locations) = variant.locations.as_mut() {
                    *locations = saved
                        .and_then(|state| state.get(&variant.id))
                        .cloned()
                        .unwrap_or_default();
                }
            }
            self.occurrences.clear();
        } else {
            self.rebase_locations(&change.edits);
            tracking::rebase(&mut self.occurrences, &change.edits, |o| &mut o.range);
        }
    }
    pub(super) fn retain_location_history(&mut self, retained: &[u64]) {
        self.location_history
            .retain(|version, _| retained.contains(version));
    }
    /// After following a transaction, group mentions await rediscovery.
    pub(super) fn await_discovery(&mut self) {
        self.reindex();
        for group in &mut self.variants {
            group.mentions.clear();
        }
    }
    /// Replace every group's mentions, keeping occurrence ids stable for
    /// unchanged spans so active annotations survive rediscovery.
    pub(super) fn set_mentions(&mut self, mentions: Vec<Vec<Range<usize>>>) {
        let mut old: HashMap<_, _> = self
            .occurrences
            .drain(..)
            .map(|o| ((o.variant, o.range.start, o.range.end), o.id))
            .collect();
        for (group, mentions) in self.variants.iter_mut().zip(mentions) {
            for range in &mentions {
                let id = old
                    .remove(&(group.id, range.start, range.end))
                    .unwrap_or_else(|| {
                        let next = self.counters.entry(group.id).or_default();
                        let id = (group.id << 32) | *next;
                        *next += 1;
                        id
                    });
                self.occurrences.push(Candidate {
                    id,
                    variant: group.id,
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
