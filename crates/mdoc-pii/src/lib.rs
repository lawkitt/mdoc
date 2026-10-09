//! Live-document review policy. Source text is immutable during a detection job;
//! every edit plan is checked against the current source and editor revision.
mod candidates;
mod discovery;
mod identities;
mod manual;
pub use identities::{Identity, IdentitySnapshot};
mod syntax;
pub mod tracking;
use candidates::Candidates;
pub use discovery::{DiscoveryInput, DiscoveryResult};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
use syntax::{plain_text_span, protected_syntax};
use tracking::{Applied, Assignment, ReplacementPlan, Tracking};

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
    /// Manual additions only; the detector never emits Date or Other.
    Date,
    Other,
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
            Self::Date => "Date",
            Self::Other => "Other",
        }
    }
    /// Picker order; Other is the catch-all and comes last.
    pub const ALL: [Self; 10] = [
        Self::Person,
        Self::Organization,
        Self::Email,
        Self::Phone,
        Self::Address,
        Self::Date,
        Self::Identity,
        Self::Tax,
        Self::Bank,
        Self::Other,
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
            Self::Date => "DATE",
            Self::Other => "REDACTED",
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
pub struct Variant {
    pub id: u64,
    pub original: Arc<str>,
    pub identity: u64,
    pub category: Category,
    pub replacement: String,
    pub mentions: Vec<Range<usize>>,
    kept: bool,
    keep_default: bool,
    /// Manual additions win discovery overlaps, the latest first; 0 when detected.
    priority: u64,
}

#[derive(Clone)]
pub struct Candidate {
    pub id: u64,
    pub variant: u64,
    pub range: Range<usize>,
}
/// One live document's review: pending candidates, the identity policy and
/// applied-replacement provenance. Every change goes through a named method.
#[derive(Default)]
pub struct Review {
    source: Arc<str>,
    candidates: Candidates,
    tracking: Tracking,
    identities: identities::IdentityStore,
    occupied_tokens: HashSet<String>,
    discovery_version: u64,
    counters: HashMap<Category, usize>,
    next_id: u64,
    /// Last manual-addition priority handed out; see `Variant::priority`.
    manual_priority: u64,
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
/// Like `safe_span`, but escapes and emphasis characters are allowed; the
/// caller checks them structurally with `plain_text_span`.
fn manual_span(value: &str) -> bool {
    value.len() <= 1024
        && value.chars().any(char::is_alphanumeric)
        && !value.chars().any(|ch| "\n\r[]<>`|#$".contains(ch))
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
    pub fn on_transaction(&mut self, transaction: &mdoc_history::EditorTransaction, source: &str) {
        let unchanged_source = self.source.as_ref() == source;
        if self.tracking.on_transaction(transaction) {
            self.identities.on_transaction(transaction);
            self.sync_identity_variants();
            self.source = source.into();
            self.discovery_version += 1;
            for change in &transaction.changes {
                self.candidates.follow(&change.edits);
            }
            self.candidates.await_discovery();
            if unchanged_source {
                self.refresh(source);
            }
        }
    }

    pub fn variants(&self) -> &[Variant] {
        &self.candidates.variants
    }
    pub fn variant(&self, id: u64) -> Option<&Variant> {
        self.candidates.variant(id)
    }
    /// Pending mentions in source order.
    pub fn candidates(&self) -> &[Candidate] {
        self.candidates.occurrences()
    }
    pub fn candidate(&self, id: u64) -> Option<&Candidate> {
        self.candidates.occurrence(id)
    }
    pub fn annotation_id(&self, group: u64, range: &Range<usize>) -> Option<u64> {
        self.candidates.annotation_id(group, range)
    }
    pub fn remaining(&self) -> usize {
        self.candidates.remaining()
    }
    /// Applied replacements in source order.
    pub fn applied(&self) -> &[Applied] {
        &self.tracking.applied
    }
    pub fn applied_occurrence(&self, id: u64) -> Option<&Applied> {
        self.tracking.get(id)
    }
    pub fn assignments(&self) -> &[Assignment] {
        &self.tracking.assignments
    }
    /// Whether provenance was recorded against this editor history state.
    pub fn matches_history(&self, history: u64) -> bool {
        self.tracking.matches_history(history)
    }
    /// Text edits restoring one applied occurrence, or every occurrence of the
    /// same immediate prior value.
    pub fn restoration_edits(
        &self,
        source: &str,
        id: u64,
        all: bool,
    ) -> Result<Vec<(Range<usize>, String)>, String> {
        self.tracking.restore_plan(source, id, all)
    }
    pub fn prepare_restore(
        &self,
        edits: &[(Range<usize>, String)],
    ) -> Vec<(Applied, Range<usize>)> {
        self.tracking.prepare_restore(edits)
    }
    /// Text edits returning these applied occurrences to their prior values.
    pub fn reversion_edits(
        &self,
        source: &str,
        ids: &HashSet<u64>,
    ) -> Result<Vec<(Range<usize>, String)>, String> {
        self.tracking.reversion_plan(source, ids)
    }
    /// Record restorations committed as history state `history`; each becomes Keep.
    pub fn commit_restore(&mut self, history: u64, restored: Vec<(Applied, Range<usize>)>) {
        self.tracking.commit_restore(history, restored, true);
    }
    /// Record reversions committed as `history` that become proposals again.
    /// A mention separated from its variant's identity stays pinned to it.
    pub fn commit_unapply(&mut self, history: u64, restored: Vec<(Applied, Range<usize>)>) {
        for (old, range) in &restored {
            let separated = self
                .candidates
                .by_original(&old.step.before)
                .and_then(|group| self.variant_identity(group))
                .is_some_and(|id| id != old.step.identity);
            if separated {
                self.tracking
                    .commit_assignment(history, range.clone(), old.step.identity);
            }
        }
        self.tracking.commit_restore(history, restored, false);
    }
    /// Prepare provenance for plans before their text is committed.
    pub fn prepare_replacements(&mut self, plans: &[ReplacementPlan]) -> Vec<Applied> {
        self.tracking.prepare_corrections(plans)
    }
    /// Record prepared replacements once the editor committed history state `history`.
    pub fn commit_replacements(&mut self, history: u64, added: Vec<Applied>) {
        self.tracking.commit(history, added);
    }
    /// Pin one occurrence to an identity, separating homonyms within a variant.
    pub fn commit_assignment(&mut self, history: u64, range: Range<usize>, identity: u64) {
        self.tracking.commit_assignment(history, range, identity);
    }
    pub fn refresh(&mut self, source: &str) {
        // A conservative source diff revalidates single-occurrence exclusions.
        // Anything crossing an edited region is invalidated rather than shifted
        // speculatively. Variant seeds/mappings survive edits and undo.
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
            originals: self.variants().iter().map(|g| g.original.clone()).collect(),
            enabled: self.variants().iter().map(|g| !g.kept).collect(),
            priority: self.variants().iter().map(|g| g.priority).collect(),
            excluded: self
                .tracking
                .exclusions
                .iter()
                .map(|o| (o.range.start, o.range.end))
                .collect(),
            matcher: self.candidates.matcher.clone(),
        }
    }
    pub fn apply_discovery(&mut self, result: DiscoveryResult) -> bool {
        if result.version != self.discovery_version {
            return false;
        }
        self.candidates.matcher = result.matcher;
        self.occupied_tokens = result.tokens;
        self.occupied_tokens.extend(
            self.candidates
                .variants
                .iter()
                .map(|g| g.replacement.clone()),
        );
        self.candidates.set_mentions(result.mentions);
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
        let mut accepted = BTreeMap::new();
        let mut selected = Vec::new();
        for detection in detections {
            // A bare category marker is not identifying, whatever category the
            // detector guesses. Do not turn it into an identity.
            if Category::ALL
                .iter()
                .any(|c| c.token() == &source[detection.range.clone()])
            {
                continue;
            }
            if !safe_span(&source[detection.range.clone()])
                || intersects(&protected, &detection.range)
                || interval_conflict(&accepted, &detection.range)
            {
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
        if let Some(id) = self.candidates.by_original(original) {
            return id;
        }
        let normalized = self.identities.normalized_identity(original, category);
        let replacement = normalized
            .and_then(|id| self.identity(id))
            .map(|i| i.alias.clone())
            .unwrap_or_else(|| self.allocate_alias(category));
        self.next_id += 1;
        let id = self.next_id;
        self.occupied_tokens.insert(replacement.clone());
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
        self.candidates.push(Variant {
            id,
            original: original.into(),
            identity,
            category,
            replacement,
            mentions: Vec::new(),
            kept: false,
            keep_default: false,
            priority: 0,
        });
        id
    }
    pub fn validate_manual(source: &str, range: Range<usize>) -> Result<&str, String> {
        let protected = protected_syntax(source);
        source
            .get(range.clone())
            .filter(|value| {
                manual_span(value)
                    && !protected.iter().any(|syntax| overlaps(&range, syntax))
                    && plain_text_span(source, range.clone())
            })
            .ok_or_else(|| "Selection crosses Markdown formatting.".into())
    }
    pub fn add_manual(
        &mut self,
        source: &str,
        range: Range<usize>,
        category: Category,
    ) -> Result<u64, String> {
        let original = Self::validate_manual(source, range.clone())?;
        self.refresh(source);
        let existed = self.candidates.by_original(original).is_some();
        let id = self.add_seed(original, category);
        self.manual_priority += 1;
        let priority = self.manual_priority;
        if let Some(group) = self.candidates.variant_mut(id) {
            group.keep_default |= !existed;
            group.priority = priority;
        }
        self.set_kept(id, false);
        self.tracking.remove_keeps_for(original);
        self.refresh(source);
        Ok(id)
    }
    #[cfg(test)]
    pub fn set_replacement(&mut self, id: u64, replacement: &str) {
        let Some(identity) = self.variant_identity(id) else {
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
    /// Plan every pending mention with its assigned identity's alias.
    /// `assigned` overrides occurrence identities staged by a mapping change.
    pub fn pending_plans(
        &self,
        source: &str,
        assigned: &HashMap<(usize, usize), u64>,
    ) -> Result<Vec<ReplacementPlan>, String> {
        self.candidates()
            .iter()
            .map(|candidate| {
                let range = &candidate.range;
                let group = self
                    .variant(candidate.variant)
                    .filter(|g| source.get(range.clone()) == Some(g.original.as_ref()))
                    .ok_or("Document changed. Review it again.")?;
                let identity = assigned
                    .get(&(range.start, range.end))
                    .copied()
                    .or_else(|| self.occurrence_identity(group.id, range))
                    .and_then(|id| self.identity(id))
                    .ok_or("Identity is no longer available.")?;
                Ok(ReplacementPlan {
                    range: range.clone(),
                    before: group.original.clone(),
                    after: identity.alias.as_str().into(),
                    category: identity.category,
                    identity: identity.id,
                })
            })
            .collect()
    }
    /// Re-alias applied mentions whose identity alias changed since application.
    pub fn alias_corrections(&self, source: &str) -> Result<Vec<ReplacementPlan>, String> {
        let mut plans = Vec::new();
        for applied in &self.tracking.applied {
            if source.get(applied.range.clone()) != Some(applied.step.after.as_ref()) {
                return Err("Replacement changed. Review it again.".into());
            }
            if let Some(identity) = self.identity(applied.step.identity)
                && applied.step.after.as_ref() != identity.alias
            {
                plans.push(ReplacementPlan {
                    range: applied.range.clone(),
                    before: applied.step.after.clone(),
                    after: identity.alias.as_str().into(),
                    category: identity.category,
                    identity: identity.id,
                });
            }
        }
        Ok(plans)
    }
    /// Keep these pending mentions as one metadata change, refreshing once.
    pub fn keep_mentions(&mut self, mentions: impl IntoIterator<Item = (u64, Range<usize>)>) {
        let mentions: Vec<_> = mentions
            .into_iter()
            .filter_map(|(id, range)| {
                let group = self.variant(id)?;
                group
                    .mentions
                    .contains(&range)
                    .then(|| (range, group.original.clone()))
            })
            .collect();
        self.tracking.keep_many(mentions);
        let source = self.source.clone();
        self.refresh(&source);
    }
    pub fn keep(&mut self, id: u64, single: Option<Range<usize>>) {
        if let Some(group) = self.variant(id) {
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
