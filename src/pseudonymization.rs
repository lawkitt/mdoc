//! Live-document review policy. Source text is immutable during a detection job;
//! every edit plan is checked against the current source and editor revision.
mod discovery;
mod identities;
pub use identities::Identity;
mod syntax;
pub mod tracking;
pub use discovery::{DiscoveryInput, DiscoveryResult};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
use syntax::protected_syntax;
use tracking::Tracking;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Person,
    Organization,
    Email,
    Phone,
    Address,
    Identity,
    Tax,
    Bank,
}
impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "Person",
            Self::Organization => "Organization",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Address => "Address",
            Self::Identity => "Identity",
            Self::Tax => "Tax identifier",
            Self::Bank => "Bank details",
        }
    }
    pub const ALL: [Self; 8] = [
        Self::Person,
        Self::Organization,
        Self::Email,
        Self::Phone,
        Self::Address,
        Self::Identity,
        Self::Tax,
        Self::Bank,
    ];
    pub fn token(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Organization => "ORG",
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Address => "ADDRESS",
            Self::Identity => "IDENTITY",
            Self::Tax => "TAX",
            Self::Bank => "BANK",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Detection {
    pub range: Range<usize>,
    pub category: Category,
    pub score: f32,
    pub recognizer: Recognizer,
}
#[derive(Clone, Debug)]
pub struct Group {
    pub id: u64,
    pub original: Arc<str>,
    pub identity: u64,
    pub category: Category,
    pub replacement: String,
    pub mentions: Vec<Range<usize>>,
    kept: bool,
    keep_default: bool,
}

#[derive(Clone)]
pub struct CandidateOccurrence {
    pub id: u64,
    pub group: u64,
    pub range: Range<usize>,
}
#[derive(Default)]
pub struct Review {
    pub open: bool,
    pub skipped_syntax_spans: usize,
    source: Arc<str>,
    pub groups: Vec<Group>,
    pub candidates: Vec<CandidateOccurrence>,
    candidate_lookup: HashMap<u64, usize>,
    candidate_counters: HashMap<u64, u64>,
    pub tracking: Tracking,
    identities: identities::IdentityStore,
    group_lookup: HashMap<u64, usize>,
    original_lookup: HashMap<Arc<str>, u64>,
    occupied_tokens: HashSet<String>,
    matcher: Option<Arc<aho_corasick::AhoCorasick>>,
    discovery_version: u64,
    counters: HashMap<Category, usize>,
    next_id: u64,
}

/// Tokens work unchanged in prose, table cells, destinations, code and HTML.
/// Custom replacements must not introduce Markdown/URL/HTML delimiters.
pub fn valid_replacement(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}
fn safe_span(value: &str) -> bool {
    value.len() <= 1024
        && value.chars().any(char::is_alphanumeric)
        && !value.starts_with('_')
        && !value.ends_with('_')
        && !value.chars().any(|ch| "\n\r[]<>`*|\\#~".contains(ch))
}
fn exact_boundary(source: &str, range: &Range<usize>, original: &str) -> bool {
    let left = original.chars().next().is_some_and(char::is_alphanumeric)
        && source[..range.start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
    let right = original
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric)
        && source[range.end..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric);
    !left && !right
}
fn interval_conflict(occupied: &BTreeMap<usize, usize>, range: &Range<usize>) -> bool {
    occupied
        .range(..=range.start)
        .next_back()
        .is_some_and(|(_, end)| *end > range.start)
        || occupied
            .range(range.start..)
            .next()
            .is_some_and(|(start, _)| *start < range.end)
}
fn intersects(protected: &[Range<usize>], range: &Range<usize>) -> bool {
    let i = protected.partition_point(|r| r.end <= range.start);
    protected.get(i).is_some_and(|r| r.start < range.end)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognizer {
    Model,
    Email,
    Inn,
    Snils,
    Ogrn,
}
impl Recognizer {
    fn rule(self) -> bool {
        self != Self::Model
    }
}
fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

impl Review {
    pub fn on_transaction(&mut self, transaction: &mdoc_editor::EditorTransaction, source: &str) {
        let unchanged_source = self.source.as_ref() == source;
        if self.tracking.on_transaction(transaction) {
            self.identities.on_transaction(transaction);
            self.sync_identity_groups();
            self.source = source.into();
            self.discovery_version += 1;
            for change in &transaction.changes {
                if change.edits.is_empty() {
                    self.candidates.clear();
                } else {
                    tracking::rebase(&mut self.candidates, &change.edits, |o| &mut o.range);
                }
            }
            self.candidate_lookup.clear();
            self.candidate_lookup
                .extend(self.candidates.iter().enumerate().map(|(i, o)| (o.id, i)));
            for group in &mut self.groups {
                group.mentions.clear();
            }
            if unchanged_source {
                self.refresh(source);
            }
        }
    }

    pub fn group(&self, id: u64) -> Option<&Group> {
        self.group_lookup.get(&id).map(|&index| &self.groups[index])
    }
    pub fn candidate(&self, id: u64) -> Option<&CandidateOccurrence> {
        self.candidate_lookup.get(&id).map(|&i| &self.candidates[i])
    }
    pub fn annotation_id(&self, group: u64, range: &Range<usize>) -> Option<u64> {
        let index = self
            .candidates
            .partition_point(|o| o.range.start < range.start);
        self.candidates
            .get(index)
            .filter(|o| o.group == group && o.range == *range)
            .map(|o| o.id)
    }
    pub fn remaining(&self) -> usize {
        self.groups.iter().map(|group| group.mentions.len()).sum()
    }
    pub fn refresh(&mut self, source: &str) {
        // A conservative source diff revalidates single-occurrence exclusions.
        // Anything crossing an edited region is invalidated rather than shifted
        // speculatively. Group seeds/mappings survive edits and undo.
        if self.source.as_ref() != source {
            let mut prefix = self
                .source
                .bytes()
                .zip(source.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            while !self.source.is_char_boundary(prefix) || !source.is_char_boundary(prefix) {
                prefix -= 1;
            }
            let mut suffix = self.source[prefix..]
                .bytes()
                .rev()
                .zip(source[prefix..].bytes().rev())
                .take_while(|(a, b)| a == b)
                .count();
            while !self.source.is_char_boundary(self.source.len() - suffix)
                || !source.is_char_boundary(source.len() - suffix)
            {
                suffix -= 1;
            }
            let old_end = self.source.len() - suffix;
            let delta = source.len() as isize - self.source.len() as isize;
            self.tracking.exclusions.retain_mut(|excluded| {
                if excluded.range.end <= prefix {
                    return source.get(excluded.range.clone()) == Some(excluded.original.as_ref());
                }
                if excluded.range.start < old_end {
                    return false;
                }
                excluded.range = excluded.range.start.saturating_add_signed(delta)
                    ..excluded.range.end.saturating_add_signed(delta);
                source.get(excluded.range.clone()) == Some(excluded.original.as_ref())
            });
            self.source = source.into();
        }
        self.discovery_version += 1;
        if let Some(result) = self
            .discovery_input()
            .run(&std::sync::atomic::AtomicBool::new(false))
        {
            self.apply_discovery(result);
        }
    }
    pub fn discovery_input(&self) -> DiscoveryInput {
        DiscoveryInput {
            source: self.source.clone(),
            version: self.discovery_version,
            originals: self.groups.iter().map(|g| g.original.clone()).collect(),
            enabled: self.groups.iter().map(|g| !g.kept).collect(),
            excluded: self
                .tracking
                .exclusions
                .iter()
                .map(|o| (o.range.start, o.range.end))
                .collect(),
            matcher: self.matcher.clone(),
        }
    }
    pub fn apply_discovery(&mut self, result: DiscoveryResult) -> bool {
        if result.version != self.discovery_version {
            return false;
        }
        self.matcher = result.matcher;
        self.occupied_tokens = result.tokens;
        self.occupied_tokens
            .extend(self.groups.iter().map(|g| g.replacement.clone()));
        let mut old: HashMap<_, _> = self
            .candidates
            .drain(..)
            .map(|o| ((o.group, o.range.start, o.range.end), o.id))
            .collect();
        for (group, mentions) in self.groups.iter_mut().zip(result.mentions) {
            for range in &mentions {
                let id = old
                    .remove(&(group.id, range.start, range.end))
                    .unwrap_or_else(|| {
                        let next = self.candidate_counters.entry(group.id).or_default();
                        let id = (group.id << 32) | *next;
                        *next += 1;
                        id
                    });
                self.candidates.push(CandidateOccurrence {
                    id,
                    group: group.id,
                    range: range.clone(),
                });
            }
            group.mentions = mentions;
        }
        self.candidates.sort_by_key(|o| o.range.start);
        self.candidate_lookup.clear();
        self.candidate_lookup
            .extend(self.candidates.iter().enumerate().map(|(i, o)| (o.id, i)));
        true
    }

    /// Rebase Keep decisions using the exact batch just committed by the editor.
    /// A broad source diff would discard unchanged exclusions between edits.
    #[cfg(test)]
    pub fn refresh_after_edits(&mut self, source: &str, edits: &[(Range<usize>, String)]) {
        self.tracking.exclusions.retain_mut(|excluded| {
            if edits
                .iter()
                .any(|(range, _)| overlaps(range, &excluded.range))
            {
                return false;
            }
            let delta: isize = edits
                .iter()
                .filter(|(range, _)| range.end <= excluded.range.start)
                .map(|(range, replacement)| replacement.len() as isize - range.len() as isize)
                .sum();
            excluded.range = excluded.range.start.saturating_add_signed(delta)
                ..excluded.range.end.saturating_add_signed(delta);
            source.get(excluded.range.clone()) == Some(excluded.original.as_ref())
        });
        self.source = source.into();
        self.refresh(source);
    }
    pub fn ingest(&mut self, source: &str, mut detections: Vec<Detection>) -> Result<(), String> {
        if detections.iter().any(|detected| {
            detected.range.start >= detected.range.end
                || source.get(detected.range.clone()).is_none()
                || !detected.score.is_finite()
        }) {
            return Err("Invalid detector offsets; scan discarded.".into());
        }
        // Validated structured spans win conflicts; otherwise prefer model confidence.
        detections.sort_by(|a, b| {
            b.recognizer
                .rule()
                .cmp(&a.recognizer.rule())
                .then_with(|| b.score.total_cmp(&a.score))
                .then(b.range.len().cmp(&a.range.len()))
                .then(a.range.start.cmp(&b.range.start))
        });
        let protected = protected_syntax(source);
        self.skipped_syntax_spans = 0;
        let mut accepted = BTreeMap::new();
        let mut selected = Vec::new();
        for detection in detections {
            // Emitted shared markers are already prepared, regardless of a
            // detector's category guess. Do not turn them back into identities.
            if Category::ALL
                .iter()
                .any(|c| c.token() == &source[detection.range.clone()])
            {
                continue;
            }
            if !safe_span(&source[detection.range.clone()])
                || intersects(&protected, &detection.range)
            {
                self.skipped_syntax_spans += 1;
                continue;
            }
            if interval_conflict(&accepted, &detection.range) {
                continue;
            }
            accepted.insert(detection.range.start, detection.range.end);
            selected.push(detection);
        }
        self.refresh(source);
        selected.sort_by_key(|detection| detection.range.start);
        for detection in selected {
            self.add_seed(&source[detection.range], detection.category);
        }
        self.discover_initial_variants(source);
        self.refresh(source);
        Ok(())
    }
    fn add_seed(&mut self, original: &str, category: Category) -> u64 {
        if let Some(&id) = self.original_lookup.get(original) {
            return id;
        }
        let normalized = self.identities.normalized_identity(original, category);
        let replacement = normalized
            .and_then(|id| self.identity(id))
            .map(|i| i.alias.clone())
            .unwrap_or_else(|| self.allocate_alias(category));
        self.next_id += 1;
        let id = self.next_id;
        self.group_lookup.insert(id, self.groups.len());
        self.original_lookup.insert(original.into(), id);
        self.occupied_tokens.insert(replacement.clone());
        self.matcher = None;
        let identity = normalized.unwrap_or(id);
        if normalized.is_none() {
            self.identities.add(Identity {
                id,
                alias: replacement.clone(),
                custom_alias: false,
                category,
                owner: None,
            });
        }
        self.identities
            .remember_normalized(original, category, identity);
        self.groups.push(Group {
            id,
            original: original.into(),
            identity,
            category,
            replacement,
            mentions: Vec::new(),
            kept: false,
            keep_default: false,
        });
        id
    }
    pub fn validate_manual(source: &str, range: Range<usize>) -> Result<&str, String> {
        let protected = protected_syntax(source);
        source.get(range.clone()).filter(|value|safe_span(value) && !protected.iter().any(|syntax|overlaps(&range,syntax)))
            .ok_or_else(||"Select identifying text without Markdown delimiters. Narrow selections that cross syntax.".into())
    }
    pub fn add_manual(
        &mut self,
        source: &str,
        range: Range<usize>,
        category: Category,
    ) -> Result<u64, String> {
        let original = Self::validate_manual(source, range.clone())?;
        self.refresh(source);
        let existed = self.original_lookup.contains_key(original);
        let id = self.add_seed(original, category);
        if !existed {
            self.groups[self.group_lookup[&id]].keep_default = true;
        }
        self.set_kept(id, false);
        self.tracking.remove_keeps_for(original);
        self.refresh(source);
        Ok(id)
    }
    #[cfg(test)]
    pub fn set_replacement(&mut self, id: u64, replacement: &str) {
        let Some(identity) = self.group_identity(id) else {
            return;
        };
        let target = self
            .active_identities()
            .into_iter()
            .find(|i| self.identity(*i).is_some_and(|i| i.alias == replacement));
        if let Some(target) = target {
            let _ = self.assign_variant(id, target);
        } else {
            let _ = self.rename_identity(identity, replacement);
        }
    }
    /// Plan all pending mentions against one source snapshot. Kept candidates
    /// have no mentions; each mention takes its assigned identity's alias.
    #[cfg(test)] // Step 2 moves Apply planning here; see ADR 0021.
    pub fn plan_all(&self, source: &str) -> Result<Vec<(Range<usize>, String)>, String> {
        if source != self.source.as_ref() {
            return Err("The document changed. Review the candidates again.".into());
        }
        let mut edits = Vec::new();
        for group in &self.groups {
            for range in &group.mentions {
                if source.get(range.clone()) != Some(group.original.as_ref()) {
                    return Err("Candidate offsets changed.".into());
                }
                let value = self
                    .occurrence_identity(group.id, range)
                    .and_then(|id| self.identity(id))
                    .map_or_else(|| group.replacement.clone(), |i| i.alias.clone());
                if !valid_replacement(&value) {
                    return Err("Invalid replacement token.".into());
                }
                if group.original.as_ref() != value {
                    edits.push((range.clone(), value));
                }
            }
        }
        edits.sort_by_key(|(range, _)| range.start);
        if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
            return Err("Candidate offsets overlap. Review the candidates again.".into());
        }
        Ok(edits)
    }
    pub fn keep(&mut self, id: u64, single: Option<Range<usize>>) {
        if let Some(&index) = self.group_lookup.get(&id) {
            let group = &mut self.groups[index];
            if let Some(range) = single {
                if group.mentions.contains(&range) {
                    let original = group.original.clone();
                    self.tracking.keep(range, original);
                }
            } else {
                self.set_kept(id, true);
            }
        }
        let source = self.source.clone();
        self.refresh(&source);
    }
}

#[cfg(test)]
mod tests;
