//! Live-document review policy. Source text is immutable during a detection job;
//! every edit plan is checked against the current source and editor revision.
mod candidates;
pub mod decisions;
mod discovery;
pub mod handoff;
mod identities;
mod manual;
pub mod merge_hints;
mod propagation;
pub use identities::{Identity, IdentitySnapshot};
pub use propagation::nicknames;
pub mod restore;
mod syntax;
pub mod tracking;
use candidates::Candidates;
pub use discovery::{DiscoveryInput, DiscoveryResult};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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
    /// A claim or case number after its label (ADR 0092). Its token is
    /// `CASE`, so an alias never reads as a saved case.
    CaseNumber,
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
            Self::CaseNumber => "Case number",
            Self::Date => "Date",
            Self::Other => "Other",
        }
    }
    /// What scans can return, in picker order: Categories to detect lists
    /// these (ADR 0041, ADR 0076).
    pub const DETECTED: [Self; 9] = [
        Self::Person,
        Self::Organization,
        Self::Email,
        Self::Phone,
        Self::Address,
        Self::CaseNumber,
        Self::Identity,
        Self::Tax,
        Self::Bank,
    ];
    /// Picker order; Other is the catch-all and comes last.
    pub const ALL: [Self; 11] = [
        Self::Person,
        Self::Organization,
        Self::Email,
        Self::Phone,
        Self::Address,
        Self::Date,
        Self::CaseNumber,
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
            Self::CaseNumber => "CASE",
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
    /// Seeded by a scan, not by the user, a case or a restored mapping.
    detected: bool,
    /// What proposed it: the detector's recognizer for a scan's seed,
    /// `Propagation` for a form of a known full name (ADR 0092); `None` for
    /// a manual mark, a case or a restored mapping.
    recognizer: Option<Recognizer>,
    /// An ordinary word too ("Rose", "Мира"): only its capitalised
    /// mentions mid-sentence count.
    mid_sentence: bool,
    /// Propagation decides its identity: its one candidate full name's, or
    /// `own` while there is none; checked again as names come and go.
    managed: bool,
    /// The identity it falls back to without one candidate.
    own: Option<u64>,
    /// Propagation found it only after a title: a surname, never a first
    /// name, when its candidates are counted.
    surname: bool,
}
impl Variant {
    /// What proposed it first; `None` for a manual mark, a case or a
    /// restored mapping.
    pub fn recognizer(&self) -> Option<Recognizer> {
        self.recognizer
    }
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
    reserved_tokens: HashSet<String>,
    discovery_version: u64,
    counters: HashMap<Category, usize>,
    next_id: u64,
    /// Last manual-addition priority handed out; see `Variant::priority`.
    manual_priority: u64,
    revision: Revision,
    /// An edit cleared the pending mentions; discovery has not found them
    /// again yet, so `remaining` may undercount.
    awaiting_discovery: bool,
}

/// Advanced by every `&mut` method, so data derived from a review can be keyed
/// on it. Values are unique across reviews: a replaced review never repeats a
/// revision its predecessor handed out.
struct Revision(u64);
static REVISIONS: AtomicU64 = AtomicU64::new(1);
impl Revision {
    fn advance(&mut self) {
        self.0 = REVISIONS.fetch_add(1, Ordering::Relaxed);
    }
}
impl Default for Revision {
    fn default() -> Self {
        Self(REVISIONS.fetch_add(1, Ordering::Relaxed))
    }
}

/// `PERSON_3`-shaped text: a category token, `_`, then digits.
pub(crate) fn generated_alias(value: &str) -> bool {
    value.split_once('_').is_some_and(|(prefix, suffix)| {
        !suffix.is_empty()
            && suffix.bytes().all(|b| b.is_ascii_digit())
            && Category::ALL.iter().any(|c| c.token() == prefix)
    })
}
/// Tokens work unchanged in prose, table cells, destinations, code and HTML.
/// Custom replacements must not introduce Markdown/URL/HTML delimiters.
pub(crate) fn valid_replacement(value: &str) -> bool {
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
/// A detected span the review can take: like `safe_span`, but a literal
/// `*`, `#`, `~` or `\` inside a value ("Fennel*Orbit Ltd", "LC#713624")
/// is text when replacing the span leaves its block's structure as it was.
fn detected_span(source: &str, range: &Range<usize>) -> bool {
    let value = &source[range.clone()];
    value.len() <= 1024
        && value.chars().any(char::is_alphanumeric)
        && !value.starts_with('_')
        && !value.ends_with('_')
        && !value.chars().any(|ch| "\n\r[]<>`|".contains(ch))
        && plain_text_span(source, range.clone())
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
    /// A value beside its label: a `context` rule of the rule pack
    /// ("Claim No 6001842/2026", ADR 0092).
    Context,
    /// A form of a known full name, found by the review after a scan or a
    /// mark (ADR 0092); never a detector's.
    Propagation,
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
        self.revision.advance();
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
            self.awaiting_discovery = true;
            if unchanged_source {
                self.refresh(source);
            }
        }
    }

    /// Changes whenever the review changes; see `Revision`.
    pub fn revision(&self) -> u64 {
        self.revision.0
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
    /// Whether an edit's pending mentions wait for discovery: until then
    /// `remaining` may miss some, so nothing counts as fully reviewed.
    pub fn awaiting_discovery(&self) -> bool {
        self.awaiting_discovery
    }
    /// Applied replacements in source order.
    pub fn applied(&self) -> &[Applied] {
        &self.tracking.applied
    }
    /// Mentions kept as their originals (Keep decisions): range, wording, and
    /// the occurrence's identity when it was kept (ADR 0048).
    pub fn kept_mentions(&self) -> Vec<(Range<usize>, Arc<str>, Option<u64>)> {
        self.tracking
            .exclusions
            .iter()
            .map(|e| (e.range.clone(), e.original.clone(), e.identity))
            .collect()
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
        self.revision.advance();
        self.tracking.commit_restore(history, restored, true);
    }
    /// Record reversions committed as `history` that become proposals again.
    /// A mention separated from its variant's identity stays pinned to it.
    pub fn commit_unapply(&mut self, history: u64, restored: Vec<(Applied, Range<usize>)>) {
        self.revision.advance();
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
        self.revision.advance();
        self.tracking.prepare_corrections(plans)
    }
    /// Record prepared replacements once the editor committed history state `history`.
    pub fn commit_replacements(&mut self, history: u64, added: Vec<Applied>) {
        self.revision.advance();
        self.tracking.commit(history, added);
    }
    /// Pin one occurrence to an identity, separating homonyms within a variant.
    pub fn commit_assignment(&mut self, history: u64, range: Range<usize>, identity: u64) {
        self.revision.advance();
        self.tracking.commit_assignment(history, range, identity);
    }
    pub fn refresh(&mut self, source: &str) {
        self.revision.advance();
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
            mid_sentence: self.variants().iter().map(|g| g.mid_sentence).collect(),
            unglued: self
                .variants()
                .iter()
                .map(|g| g.recognizer == Some(Recognizer::Propagation))
                .collect(),
            excluded: self
                .tracking
                .exclusions
                .iter()
                .map(|o| (o.range.start, o.range.end))
                .collect(),
            matcher: self.candidates.matcher.clone(),
        }
    }
    pub fn apply_discovery(&mut self, mut result: DiscoveryResult) -> bool {
        if result.version != self.discovery_version {
            return false;
        }
        self.revision.advance();
        for (variant, mentions) in self.variants().iter().zip(&mut result.mentions) {
            mentions.retain(|range| self.occurrence_identity(variant.id, range).is_some());
        }
        self.candidates.matcher = result.matcher;
        self.awaiting_discovery = false;
        self.occupied_tokens = result.tokens;
        self.occupied_tokens
            .extend(self.reserved_tokens.iter().cloned());
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
    pub(crate) fn refresh_after_edits(&mut self, source: &str, edits: &[(Range<usize>, String)]) {
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
        self.revision.advance();
        if detections.iter().any(|detected| {
            detected.range.start >= detected.range.end
                || source.get(detected.range.clone()).is_none()
                || !detected.score.is_finite()
        }) {
            return Err("Invalid detector offsets; scan discarded.".into());
        }
        // A span that starts with a list item's marker ("+ 7 (900) …" on a
        // line of its own) keeps its value: the marker stays syntax.
        let markers = syntax::list_markers(source);
        for detection in &mut detections {
            let range = &mut detection.range;
            let next = markers.partition_point(|m| m.end <= range.start);
            if let Some(marker) = markers.get(next).filter(|m| m.start <= range.start) {
                let from = marker.end.min(range.end);
                let value = source[from..range.end].trim_start_matches([' ', '\t']);
                range.start = range.end - value.len();
            }
        }
        detections.retain(|detection| !detection.range.is_empty());
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
        // Applied replacements and alias text (generated or custom) are this
        // review's own output; a rescan must not alias them again.
        let mut applied: Vec<_> = self
            .tracking
            .applied
            .iter()
            .map(|a| a.range.clone())
            .collect();
        applied.sort_by_key(|r| r.start);
        let aliases: HashSet<&str> = self
            .identities
            .aliases()
            .chain(self.tracking.applied.iter().map(|a| a.step.after.as_ref()))
            .collect();
        let mut accepted = BTreeMap::new();
        let mut selected = Vec::new();
        for detection in detections {
            // A bare category marker is not identifying, whatever category the
            // detector guesses. Do not turn it into an identity.
            let text = &source[detection.range.clone()];
            if Category::ALL.iter().any(|c| c.token() == text)
                || generated_alias(text)
                || aliases.contains(text)
                || intersects(&applied, &detection.range)
            {
                continue;
            }
            if !detected_span(source, &detection.range)
                || intersects(&protected, &detection.range)
                || interval_conflict(&accepted, &detection.range)
            {
                continue;
            }
            accepted.insert(detection.range.start, detection.range.end);
            selected.push(detection);
        }
        self.refresh(source);
        let first = self.next_id;
        selected.sort_by_key(|detection| detection.range.start);
        // A new wording found only inside longer ones ("Harrow & Finch" of
        // "Harrow & Finch Ltd") would have no mention to show; it must not
        // take an alias either, or the numbering skips one (issue #302).
        let mut listed = HashSet::new();
        let fresh: Vec<Arc<str>> = selected
            .iter()
            .map(|detection| &source[detection.range.clone()])
            .filter(|text| self.candidates.by_original(text).is_none() && listed.insert(*text))
            .map(Arc::from)
            .collect();
        let mut input = self.discovery_input();
        let known = input.originals.len();
        input.enabled.extend(fresh.iter().map(|_| true));
        input.priority.extend(fresh.iter().map(|_| 0));
        input.mid_sentence.extend(fresh.iter().map(|_| false));
        input.unglued.extend(fresh.iter().map(|_| false));
        input.originals.extend(fresh.iter().cloned());
        input.matcher = None;
        let unseen: HashSet<Arc<str>> = input
            .run(&std::sync::atomic::AtomicBool::new(false))
            .map(|result| {
                fresh
                    .into_iter()
                    .zip(&result.mentions[known..])
                    .filter(|(_, mentions)| mentions.is_empty())
                    .map(|(original, _)| original)
                    .collect()
            })
            .unwrap_or_default();
        selected.retain(|detection| !unseen.contains(&source[detection.range.clone()]));
        // A form of a full name joins it when that is its one candidate
        // (ADR 0092), so it waits for the full names and takes no alias of
        // its own. Others keep their place in alias order.
        let mut people: Vec<(u64, &str)> = Vec::new();
        let mut provisional = HashMap::new();
        for detection in selected.iter().filter(|d| d.category == Category::Person) {
            let text = &source[detection.range.clone()];
            let owner = self
                .candidates
                .by_original(text)
                .and_then(|id| self.variant_identity(id))
                .or_else(|| self.identities.normalized_identity(text, Category::Person))
                .unwrap_or_else(|| {
                    let next = u64::MAX - provisional.len() as u64;
                    *provisional
                        .entry(identities::normalization_key(text, Category::Person))
                        .or_insert(next)
                });
            people.push((owner, text));
        }
        let known = self.known_people();
        people.extend(known.iter().map(|(id, w)| (*id, w.as_ref())));
        // A surname with initials that is a form of one full name stands for
        // that person ("Павловой М. С."), not for a second one.
        let full = propagation::People::new(
            people
                .iter()
                .copied()
                .filter(|(_, w)| identities::split_full_name(w).is_some()),
        );
        for (owner, wording) in &mut people {
            if let Some(target) =
                propagation::parse_form(wording).and_then(|form| full.candidates(&form).target)
            {
                *owner = target;
            }
        }
        let people = propagation::People::new(people);
        let mut forms = Vec::new();
        for detection in selected {
            let text = &source[detection.range.clone()];
            let joins = detection.category == Category::Person
                && propagation::parse_form(text)
                    .is_some_and(|form| people.candidates(&form).target.is_some());
            if joins {
                forms.push(detection);
            } else {
                let id = self.add_seed(text, detection.category);
                self.recognized(id, first, detection.recognizer);
            }
        }
        if !forms.is_empty() {
            self.refresh(source);
        }
        for detection in forms {
            let text = &source[detection.range];
            let target = self.propagation_target(text);
            let id = self.seed_form(text, target);
            self.recognized(id, first, detection.recognizer);
        }
        // Variants this scan seeded, including the name forms derived from
        // them; marked first, so propagation can link this scan's own lone
        // proposals too.
        let mark_detected = |review: &mut Self| {
            for variant in &mut review.candidates.variants {
                variant.detected |= variant.id > first;
            }
        };
        mark_detected(self);
        self.propagate(source, false);
        mark_detected(self);
        self.refresh(source);
        Ok(())
    }
    /// The recognizer of variant `id` when this scan created it (its id
    /// is past `first`); a marked, case or restored variant keeps `None`.
    fn recognized(&mut self, id: u64, first: u64, recognizer: Recognizer) {
        if let Some(variant) = self.candidates.variant_mut(id).filter(|v| v.id > first) {
            variant.recognizer.get_or_insert(recognizer);
        }
    }
    fn add_seed(&mut self, original: &str, category: Category) -> u64 {
        self.add_seed_linked(original, category, None)
    }
    /// Seed `original`, joining identity `link` unless its wording already
    /// normalizes to one.
    fn add_seed_linked(&mut self, original: &str, category: Category, link: Option<u64>) -> u64 {
        if let Some(id) = self.candidates.by_original(original) {
            return id;
        }
        let normalized = self
            .identities
            .normalized_identity(original, category)
            .or(link.filter(|id| self.identity(*id).is_some_and(|i| i.category == category)));
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
            detected: false,
            recognizer: None,
            mid_sentence: false,
            managed: false,
            own: None,
            surname: false,
        });
        id
    }
    /// Whether `range` of `source` can be a manual addition: plain text,
    /// crossing no Markdown syntax. Returns the wording.
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
        self.revision.advance();
        let original = Self::validate_manual(source, range.clone())?;
        self.refresh(source);
        let existed = self.candidates.by_original(original).is_some();
        let id = self.add_seed(original, category);
        self.manual_priority += 1;
        let priority = self.manual_priority;
        if let Some(group) = self.candidates.variant_mut(id) {
            group.keep_default |= !existed;
            group.priority = priority;
            // Marked by hand, every mention counts, even an ordinary word's
            // at the start of a sentence.
            group.mid_sentence = false;
        }
        self.set_kept(id, false);
        self.tracking.remove_keeps_for(original);
        self.refresh(source);
        // A marked full name proposes its forms elsewhere (ADR 0092).
        if category == Category::Person && identities::split_full_name(original).is_some() {
            self.propagate(source, true);
        }
        Ok(id)
    }
    /// The category of `original`'s variant when the wording is proposed
    /// already. `add_manual` then keeps that variant and its category, so
    /// a category is picked only for new wording (ADR 0024).
    pub fn proposed_wording_category(&self, original: &str) -> Option<Category> {
        let id = self.candidates.by_original(original)?;
        self.variant(id).map(|variant| variant.category)
    }
    /// The alias adding new wording `original` in `category` gives: the
    /// identity it normalizes to there, else the next free one.
    pub fn manual_alias(&self, original: &str, category: Category) -> String {
        self.identities
            .normalized_identity(original, category)
            .and_then(|id| self.identity(id))
            .map_or_else(|| self.next_alias(category), |i| i.alias.clone())
    }
    #[cfg(test)]
    pub(crate) fn set_replacement(&mut self, id: u64, replacement: &str) {
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
    /// Propose exactly these kept occurrences again without lifting other Keep
    /// decisions for the same wording or changing their assigned identities.
    pub fn propose_kept_mentions(&mut self, source: &str, ranges: &[Range<usize>]) -> usize {
        self.revision.advance();
        self.refresh(source);
        let ranges: HashSet<_> = ranges.iter().map(|r| (r.start, r.end)).collect();
        let mentions = self.tracking.propose_kept(&ranges);
        for mention in &mentions {
            let category = mention
                .identity
                .and_then(|id| self.identity(id))
                .map_or(Category::Other, |i| i.category);
            let variant = self.add_seed(&mention.original, category);
            self.set_kept(variant, false);
        }
        self.refresh(source);
        mentions.len()
    }

    /// Keep these pending mentions as one metadata change, refreshing once.
    pub fn keep_mentions(&mut self, mentions: impl IntoIterator<Item = (u64, Range<usize>)>) {
        self.revision.advance();
        let mentions: Vec<_> = mentions
            .into_iter()
            .filter_map(|(id, range)| {
                let group = self.variant(id)?;
                group.mentions.contains(&range).then(|| {
                    let identity = self.occurrence_identity(id, &range);
                    (range, group.original.clone(), identity)
                })
            })
            .collect();
        self.tracking.keep_many(mentions);
        let source = self.source.clone();
        self.refresh(&source);
    }
    pub fn keep(&mut self, id: u64, single: Option<Range<usize>>) {
        self.revision.advance();
        if let Some(group) = self.variant(id) {
            if let Some(range) = single {
                if group.mentions.contains(&range) {
                    let original = group.original.clone();
                    let identity = self.occurrence_identity(id, &range);
                    self.tracking.keep(range, original, identity);
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
