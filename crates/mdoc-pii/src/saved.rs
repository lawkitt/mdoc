//! A review as its document keeps it between sessions (#1091, Decision 16):
//! pending proposals, applied replacements with their provenance, Keep and
//! Not a name decisions, occurrence overrides and the identities they name,
//! so a reopened document needs no rescan. Undo history, derived indexes and
//! caches are left out: undo lasts for the session, and the rest is built
//! again. The value serializes with serde; the Vault stores it as JSON.
use super::{
    Candidate, Category, Identity, Recognizer, Review, Variant, candidates::Candidates,
    discovery::Cue, identities::IdentityStore, tracking,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, ops::Range, sync::Arc};

/// The format this build writes. A newer one is refused, never guessed at.
const FORMAT: u32 = 1;

/// One document's review, saved. Equal reviews save equal values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedReview {
    format: u32,
    /// Wordings with their pending mentions.
    variants: Vec<SavedVariant>,
    /// Pending mentions with their annotation ids, in source order.
    pending: Vec<SavedCandidate>,
    /// Variant id → its next annotation number.
    annotation_counters: Vec<(u64, u64)>,
    /// Replacement steps; a predecessor comes before the steps that name it.
    steps: Vec<SavedStep>,
    /// Applied replacements in source order.
    applied: Vec<SavedApplied>,
    /// Occurrence overrides: one mention pinned to an identity.
    assignments: Vec<SavedAssignment>,
    /// Keep and Not a name decisions in source order.
    exclusions: Vec<SavedExclusion>,
    /// The last provenance id handed out.
    provenance_id: u64,
    /// As created; `policy.edited` holds later edits.
    identities: Vec<Identity>,
    policy: SavedPolicy,
    /// Normalized wording → identity.
    normalized: Vec<(Category, String, u64)>,
    occupied_tokens: Vec<String>,
    reserved_tokens: Vec<String>,
    /// The highest alias number per category.
    alias_counters: Vec<(Category, usize)>,
    next_id: u64,
    manual_priority: u64,
    /// Saved after an edit, before discovery found its mentions again.
    awaiting_discovery: bool,
}

/// The identity policy as it stands, without its history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedPolicy {
    pub edited: Vec<Identity>,
    pub variants: Vec<(u64, u64)>,
    pub kept: Vec<(u64, bool)>,
    pub retired: Vec<(u64, Vec<String>)>,
    pub primary: Vec<(u64, String)>,
    pub automatic: Vec<u64>,
    pub every_mention: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedVariant {
    id: u64,
    original: String,
    identity: u64,
    category: Category,
    replacement: String,
    mentions: Vec<Range<usize>>,
    source_variant: Option<u64>,
    kept: bool,
    keep_default: bool,
    priority: u64,
    detected: bool,
    recognizer: Option<Recognizer>,
    mid_sentence: bool,
    cues: Option<Vec<Cue>>,
    managed: bool,
    own: Option<u64>,
    surname: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedCandidate {
    id: u64,
    variant: u64,
    range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedStep {
    identity: u64,
    before: String,
    after: String,
    category: Category,
    /// An index into `SavedReview::steps`, below this step's own.
    predecessor: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedApplied {
    id: u64,
    range: Range<usize>,
    /// An index into `SavedReview::steps`.
    step: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedAssignment {
    id: u64,
    range: Range<usize>,
    identity: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedExclusion {
    id: u64,
    range: Range<usize>,
    original: String,
    identity: Option<u64>,
    /// Not a name; a Keep otherwise.
    silent: bool,
}

/// Why a saved review cannot be reopened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreError {
    /// Written by a newer mdoc, in this format.
    Newer(u32),
    /// Its ranges or references do not fit the text it was given.
    Mismatch,
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newer(_) => f.write_str("This review was saved by a newer mdoc."),
            Self::Mismatch => f.write_str("The saved review does not match the document."),
        }
    }
}

impl std::error::Error for RestoreError {}

/// Hash-set entries sorted.
fn sorted(set: &std::collections::HashSet<String>) -> Vec<String> {
    let mut values: Vec<_> = set.iter().cloned().collect();
    values.sort_unstable();
    values
}

impl Review {
    /// This review as its document keeps it.
    pub fn save(&self) -> SavedReview {
        let mut steps = Vec::new();
        let mut step_index = HashMap::new();
        let applied = self
            .tracking
            .applied
            .iter()
            .map(|applied| SavedApplied {
                id: applied.id,
                range: applied.range.clone(),
                step: save_step(&applied.step, &mut steps, &mut step_index),
            })
            .collect();
        let (identities, policy, normalized) = self.identities.saved();
        let mut annotation_counters: Vec<_> = self
            .candidates
            .counters()
            .iter()
            .map(|(id, next)| (*id, *next))
            .collect();
        annotation_counters.sort_unstable();
        let mut alias_counters: Vec<_> = self
            .counters
            .iter()
            .map(|(category, count)| (*category, *count))
            .collect();
        alias_counters.sort_unstable_by_key(|(category, _)| *category as u8);
        SavedReview {
            format: FORMAT,
            variants: self.variants().iter().map(SavedVariant::of).collect(),
            pending: self
                .candidates()
                .iter()
                .map(|candidate| SavedCandidate {
                    id: candidate.id,
                    variant: candidate.variant,
                    range: candidate.range.clone(),
                })
                .collect(),
            annotation_counters,
            steps,
            applied,
            assignments: self
                .tracking
                .assignments
                .iter()
                .map(|assignment| SavedAssignment {
                    id: assignment.id,
                    range: assignment.range.clone(),
                    identity: assignment.identity,
                })
                .collect(),
            exclusions: self
                .tracking
                .exclusions
                .iter()
                .map(|exclusion| SavedExclusion {
                    id: exclusion.id,
                    range: exclusion.range.clone(),
                    original: exclusion.original.to_string(),
                    identity: exclusion.identity,
                    silent: exclusion.silent,
                })
                .collect(),
            provenance_id: self.tracking.last_id(),
            identities,
            policy,
            normalized,
            occupied_tokens: sorted(&self.occupied_tokens),
            reserved_tokens: sorted(&self.reserved_tokens),
            alias_counters,
            next_id: self.next_id,
            manual_priority: self.manual_priority,
            awaiting_discovery: self.awaiting_discovery,
        }
    }

    /// The review `saved` holds, over `source`, the text it was saved with,
    /// at editor history state `history` (as `relink` takes it), so its
    /// replacements can be reverted at once. Nothing is scanned again, and
    /// discovery runs only for a review saved while it waited for discovery;
    /// undo starts afresh, as for a document just opened.
    pub fn restore(saved: SavedReview, source: &str, history: u64) -> Result<Self, RestoreError> {
        if saved.format > FORMAT {
            return Err(RestoreError::Newer(saved.format));
        }
        saved.check(source)?;
        let mut steps: Vec<Arc<tracking::Step>> = Vec::with_capacity(saved.steps.len());
        for step in saved.steps {
            let predecessor = step.predecessor.map(|index| steps[index].clone());
            steps.push(Arc::new(tracking::Step {
                identity: step.identity,
                before: step.before.into(),
                after: step.after.into(),
                category: step.category,
                predecessor,
            }));
        }
        let applied = saved
            .applied
            .into_iter()
            .map(|applied| tracking::Applied {
                id: applied.id,
                range: applied.range,
                step: steps[applied.step].clone(),
            })
            .collect();
        let assignments = saved
            .assignments
            .into_iter()
            .map(|assignment| tracking::Assignment {
                id: assignment.id,
                range: assignment.range,
                identity: assignment.identity,
            })
            .collect();
        let exclusions = saved
            .exclusions
            .into_iter()
            .map(|exclusion| tracking::Exclusion {
                id: exclusion.id,
                range: exclusion.range,
                original: exclusion.original.into(),
                identity: exclusion.identity,
                silent: exclusion.silent,
            })
            .collect();
        let pending = saved
            .pending
            .into_iter()
            .map(|candidate| Candidate {
                id: candidate.id,
                variant: candidate.variant,
                range: candidate.range,
            })
            .collect();
        let mut review = Self {
            source: source.into(),
            candidates: Candidates::restored(
                saved
                    .variants
                    .into_iter()
                    .map(SavedVariant::into_variant)
                    .collect(),
                pending,
                saved.annotation_counters.into_iter().collect(),
            ),
            tracking: tracking::Tracking::restored(
                applied,
                assignments,
                exclusions,
                saved.provenance_id,
                history,
            ),
            identities: IdentityStore::restored(saved.identities, saved.policy, saved.normalized),
            occupied_tokens: saved.occupied_tokens.into_iter().collect(),
            reserved_tokens: saved.reserved_tokens.into_iter().collect(),
            discovery_version: 0,
            counters: saved.alias_counters.into_iter().collect(),
            next_id: saved.next_id,
            manual_priority: saved.manual_priority,
            revision: Default::default(),
            awaiting_discovery: saved.awaiting_discovery,
        };
        if review.awaiting_discovery {
            review.refresh(source);
        }
        Ok(review)
    }
}

/// The index of `step` in `steps`, adding it after its predecessors; shared
/// steps are saved once.
fn save_step(
    step: &Arc<tracking::Step>,
    steps: &mut Vec<SavedStep>,
    index: &mut HashMap<*const tracking::Step, usize>,
) -> usize {
    if let Some(&at) = index.get(&Arc::as_ptr(step)) {
        return at;
    }
    let predecessor = step
        .predecessor
        .as_ref()
        .map(|predecessor| save_step(predecessor, steps, index));
    steps.push(SavedStep {
        identity: step.identity,
        before: step.before.to_string(),
        after: step.after.to_string(),
        category: step.category,
        predecessor,
    });
    index.insert(Arc::as_ptr(step), steps.len() - 1);
    steps.len() - 1
}

impl SavedVariant {
    fn of(variant: &Variant) -> Self {
        Self {
            id: variant.id,
            original: variant.original.to_string(),
            identity: variant.identity,
            category: variant.category,
            replacement: variant.replacement.clone(),
            mentions: variant.mentions.clone(),
            source_variant: variant.source_variant,
            kept: variant.kept,
            keep_default: variant.keep_default,
            priority: variant.priority,
            detected: variant.detected,
            recognizer: variant.recognizer,
            mid_sentence: variant.mid_sentence,
            cues: variant.cues.clone(),
            managed: variant.managed,
            own: variant.own,
            surname: variant.surname,
        }
    }

    fn into_variant(self) -> Variant {
        Variant {
            id: self.id,
            original: self.original.into(),
            identity: self.identity,
            category: self.category,
            replacement: self.replacement,
            mentions: self.mentions,
            source_variant: self.source_variant,
            kept: self.kept,
            keep_default: self.keep_default,
            priority: self.priority,
            detected: self.detected,
            recognizer: self.recognizer,
            mid_sentence: self.mid_sentence,
            cues: self.cues,
            managed: self.managed,
            own: self.own,
            surname: self.surname,
        }
    }
}

impl SavedReview {
    /// Every range lies on `source`'s character boundaries and holds what the
    /// review says is there, lists the review searches by position are in
    /// source order, and every reference points at something saved, with no
    /// loop: what restoring needs not to misread, panic or hang.
    fn check(&self, source: &str) -> Result<(), RestoreError> {
        let holds = |range: &Range<usize>, text: &str| {
            fits(source, range) && source.get(range.clone()) == Some(text)
        };
        let ordered = |ranges: Vec<&Range<usize>>| {
            ranges.iter().all(|range| fits(source, range))
                && ranges.windows(2).all(|pair| pair[0].start <= pair[1].start)
        };
        let variants: HashMap<u64, &SavedVariant> =
            self.variants.iter().map(|v| (v.id, v)).collect();
        // A wording's spelling lineage ends within as many steps as there
        // are wordings.
        let lineage_ends = |variant: &SavedVariant| {
            let mut at = variant.source_variant;
            for _ in 0..variants.len() {
                match at.and_then(|id| variants.get(&id)) {
                    Some(seed) => at = seed.source_variant,
                    None => return true,
                }
            }
            false
        };
        let steps_point_back = self
            .steps
            .iter()
            .enumerate()
            .all(|(at, step)| step.predecessor.is_none_or(|index| index < at));
        let ok = variants.len() == self.variants.len()
            && steps_point_back
            && self.variants.iter().all(|variant| {
                lineage_ends(variant)
                    && variant
                        .mentions
                        .iter()
                        .all(|range| holds(range, &variant.original))
            })
            && self.pending.iter().all(|candidate| {
                variants
                    .get(&candidate.variant)
                    .is_some_and(|variant| holds(&candidate.range, &variant.original))
            })
            && self.applied.iter().all(|applied| {
                self.steps
                    .get(applied.step)
                    .is_some_and(|step| holds(&applied.range, &step.after))
            })
            && self
                .exclusions
                .iter()
                .all(|exclusion| holds(&exclusion.range, &exclusion.original))
            && ordered(self.pending.iter().map(|c| &c.range).collect())
            && ordered(self.applied.iter().map(|a| &a.range).collect())
            && ordered(self.assignments.iter().map(|a| &a.range).collect())
            && ordered(self.exclusions.iter().map(|e| &e.range).collect());
        ok.then_some(()).ok_or(RestoreError::Mismatch)
    }
}

/// `range` lies on `source`'s character boundaries.
fn fits(source: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && source.is_char_boundary(range.start)
        && source.is_char_boundary(range.end)
}

#[cfg(test)]
#[path = "saved_tests.rs"]
mod tests;
