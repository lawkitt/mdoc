//! One document's text and its review, driven as the desktop's document view
//! drives them: the text stands in for the editor, so every change reaches
//! `mdoc_pii::Review` as the same `mdoc_history` transactions, in the same
//! order, as on desktop (`src/pii/ui.rs` `commit_plans` and
//! `checkpoint_review`, `src/pii/ui/mapping/popup.rs`, `src/pii/ui/mapping.rs`
//! and `src/pii/ui/case_sync.rs`). Mobile has no text editing, so the
//! history is a line of states, each one user action: Undo and Redo travel
//! it as desktop's editor does (`history`, ADR 0098), and the identity tools
//! are desktop's mapping changes (`mapping`).
mod history;
mod mapping;

use crate::CoreError;
pub use history::StepKind;
pub(crate) use history::{Focus, Step};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use mdoc_pii::{
    Category, Detection, IdentitySnapshot, Review,
    merge_hints::MergeHint,
    restore::{ExportedIdentity, plan_join},
    tracking::ReplacementPlan,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

/// Where a decision or an identity change reaches (ADR 0085's review sheet,
/// ADR 0098's scope chip).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Scope {
    /// Just this mention.
    This,
    /// Every mention of its identity.
    All,
}

/// What sits at a mention's range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum At {
    Proposed { variant: u64, identity: Option<u64> },
    Replaced { id: u64, identity: u64 },
    Kept { identity: Option<u64> },
}

#[derive(Default)]
pub(crate) struct ReviewText {
    pub text: String,
    pub review: Review,
    /// The current history state; every change makes the next one.
    history: u64,
    /// Bumped by every transaction, as the editor's revision is.
    transaction: u64,
    /// The step that made the current state; none at the start.
    step: Option<Step>,
    /// States Undo reaches, oldest first, at most `history::UNDO_LIMIT`.
    undo: Vec<history::Saved>,
    /// States Redo reaches, nearest last; a new step clears them.
    redo: Vec<history::Saved>,
    /// The state the step in progress started from.
    base: Option<history::Saved>,
    /// The step in progress's edits, one sorted batch per transaction.
    edits: Vec<Vec<SourceEdit>>,
    /// Merge hints dismissed here or in the case, by the wordings that
    /// matched (`MergeHint::pair`, ADR 0080).
    dismissed: HashSet<(Arc<str>, Arc<str>)>,
}

impl ReviewText {
    pub fn new(text: String) -> Self {
        let mut document = Self {
            text,
            ..Default::default()
        };
        let text = document.text.clone();
        document.review.refresh(&text);
        document
    }

    // ---- The editor's part --------------------------------------------------

    /// Replace sorted, non-overlapping ranges as one history step, as
    /// `EditorState::replace_ranges` does; an empty list is a metadata
    /// checkpoint (`checkpoint_metadata`). Refuses invalid geometry.
    fn commit_edits(&mut self, edits: &[(Range<usize>, String)]) -> Result<(), CoreError> {
        let mut end = 0;
        for (range, _) in edits {
            if range.start < end
                || range.start >= range.end
                || range.end > self.text.len()
                || !self.text.is_char_boundary(range.start)
                || !self.text.is_char_boundary(range.end)
            {
                return Err(CoreError::review(
                    "Document changed. Review the replacement again.",
                ));
            }
            end = range.end;
        }
        let source_edits = if edits.is_empty() {
            vec![SourceEdit {
                range: 0..0,
                new_len: 0,
            }]
        } else {
            for (range, replacement) in edits.iter().rev() {
                self.text.replace_range(range.clone(), replacement);
            }
            edits
                .iter()
                .map(|(range, replacement)| SourceEdit {
                    range: range.clone(),
                    new_len: replacement.len(),
                })
                .collect()
        };
        debug_assert!(self.base.is_some(), "every change is part of a step");
        self.edits.push(source_edits.clone());
        let before = self.history;
        self.history += 1;
        self.transaction += 1;
        let transaction = EditorTransaction {
            revision: self.transaction,
            changes: vec![HistoryChange {
                before,
                after: self.history,
                edits: source_edits,
            }],
            retained: self.retained(),
        };
        self.review.on_transaction(&transaction, &self.text);
        Ok(())
    }

    /// A metadata-only review change as its own step (`checkpoint_review`).
    fn checkpoint<R>(&mut self, change: impl FnOnce(&mut Review) -> R) -> Result<R, CoreError> {
        let old = self.review.identity_snapshot();
        let before = self.history;
        self.commit_edits(&[])?;
        let result = change(&mut self.review);
        self.review
            .commit_identity_snapshot(before, self.history, old);
        Ok(result)
    }

    /// Commit planned replacements as one step, or a checkpoint when there
    /// are none (`commit_plans`). `policy_before` is the identity policy
    /// before a staged mapping change.
    fn commit_plans(
        &mut self,
        plans: &[ReplacementPlan],
        policy_before: Option<IdentitySnapshot>,
    ) -> Result<(), CoreError> {
        let before = self.history;
        let review = &mut self.review;
        let staged = policy_before.as_ref().map(|old| {
            let next = review.identity_snapshot();
            review.restore_identity_snapshot(old.clone());
            next
        });
        let added = review.prepare_replacements(plans);
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        if let Err(error) = self.commit_edits(&edits) {
            if let Some(old) = policy_before {
                self.review.restore_identity_snapshot(old);
            }
            return Err(error);
        }
        let review = &mut self.review;
        if let (Some(old), Some(next)) = (policy_before, staged) {
            review.restore_identity_snapshot(next);
            review.commit_identity_snapshot(before, self.history, old);
        }
        review.commit_replacements(self.history, added);
        Ok(())
    }

    /// Return applied occurrences to their prior text as one step; `keep`
    /// records Keep decisions, otherwise they are proposed again
    /// (`revert_applied`). Returns the edits made.
    fn revert_applied(
        &mut self,
        ids: &HashSet<u64>,
        keep: bool,
    ) -> Result<Vec<(Range<usize>, String)>, CoreError> {
        if !self.review.matches_history(self.history) {
            return Err(CoreError::review(
                "Document changed. Review the replacement again.",
            ));
        }
        let edits = self
            .review
            .reversion_edits(&self.text, ids)
            .map_err(CoreError::review)?;
        let restored = self.review.prepare_restore(&edits);
        self.commit_edits(&edits)?;
        if keep {
            self.review.commit_restore(self.history, restored);
        } else {
            self.review.commit_unapply(self.history, restored);
        }
        self.refresh();
        Ok(edits)
    }

    fn refresh(&mut self) {
        let text = self.text.clone();
        self.review.refresh(&text);
    }

    /// Copy for AI's text: the note naming the first applied aliases,
    /// then the text, built by `mdoc_pii::handoff` as desktop builds it.
    pub fn copy_for_ai(&self) -> String {
        mdoc_pii::handoff::for_ai(
            &self.text,
            self.review.applied().iter().map(|a| a.step.after.as_ref()),
        )
    }

    // ---- Lookups ------------------------------------------------------------

    /// The pending, applied or kept mention at exactly `range`.
    pub fn at(&self, range: &Range<usize>) -> Option<At> {
        let review = &self.review;
        if let Some(c) = review.candidates().iter().find(|c| c.range == *range) {
            return Some(At::Proposed {
                variant: c.variant,
                identity: review.occurrence_identity(c.variant, &c.range),
            });
        }
        if let Some(a) = review.applied().iter().find(|a| a.range == *range) {
            return Some(At::Replaced {
                id: a.id,
                identity: a.step.identity,
            });
        }
        review
            .kept_mentions()
            .into_iter()
            .find(|(r, _, _)| r == range)
            .map(|(_, _, identity)| At::Kept { identity })
    }

    /// Pending mentions (variant, range) and applied ids of `identity`.
    fn occurrences(&self, identity: u64) -> (Vec<(u64, Range<usize>)>, Vec<u64>) {
        let review = &self.review;
        let pending = review
            .candidates()
            .iter()
            .filter(|c| review.occurrence_identity(c.variant, &c.range) == Some(identity))
            .map(|c| (c.variant, c.range.clone()))
            .collect();
        let applied = review
            .applied()
            .iter()
            .filter(|a| a.step.identity == identity)
            .map(|a| a.id)
            .collect();
        (pending, applied)
    }

    fn mention(&self, range: &Range<usize>) -> Result<At, CoreError> {
        self.at(range)
            .ok_or_else(|| CoreError::review("Document changed. Review it again."))
    }

    /// The original wording of the mention at `range`.
    fn original_at(&self, range: &Range<usize>) -> String {
        match self.at(range) {
            Some(At::Replaced { id, .. }) => self
                .review
                .applied_occurrence(id)
                .map(|a| a.step.original().to_owned())
                .unwrap_or_default(),
            Some(_) => self.text.get(range.clone()).unwrap_or_default().to_owned(),
            None => String::new(),
        }
    }

    /// A step deciding the mention at `range`.
    fn decision(&self, kind: StepKind, range: &Range<usize>) -> Step {
        let identity = self.at(range).and_then(|at| self.entity(&at));
        Step::new(
            kind,
            self.original_at(range),
            Focus {
                range: Some(range.clone()),
                identity,
            },
        )
    }

    // ---- Merge hints --------------------------------------------------------

    /// Merge hints (ADR 0080) without the dismissed ones
    /// (`MappingUi::hint_dismissed`).
    pub fn merge_hints(&self) -> Vec<MergeHint> {
        let mut hints = self.review.merge_hints();
        hints.retain(|hint| {
            let (a, b) = &hint.pair;
            !self.dismissed.contains(&(a.clone(), b.clone()))
                && !self.dismissed.contains(&(b.clone(), a.clone()))
        });
        hints
    }

    /// × on the hint from `from` into `into`: it stays hidden, and saving
    /// keeps it hidden in the case. Not an undo step, as on desktop.
    pub fn dismiss_hint(&mut self, from: u64, into: u64) -> Result<(), CoreError> {
        let hint = self
            .merge_hints()
            .into_iter()
            .find(|hint| hint.from == from && hint.into == into)
            .ok_or_else(|| CoreError::review("That hint is no longer shown."))?;
        self.dismissed.insert(hint.pair);
        Ok(())
    }

    /// Hints the case's members dismissed (`dismiss_case_hints`).
    pub fn dismiss_pairs(&mut self, pairs: &[(String, String)]) {
        for (a, b) in pairs {
            self.dismissed
                .insert((Arc::from(a.as_str()), Arc::from(b.as_str())));
        }
    }

    /// Every dismissed pair, sorted, for the case (`dismissed_hint_pairs`).
    pub fn dismissed_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<_> = self
            .dismissed
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        pairs.sort_unstable();
        pairs
    }

    // ---- Transitions --------------------------------------------------------

    /// Scan results, or injected candidates, as proposals (`complete_pii_scan`).
    pub fn ingest(&mut self, detections: Vec<Detection>) -> Result<(), CoreError> {
        let text = self.text.clone();
        self.review
            .ingest(&text, detections)
            .map_err(CoreError::review)
    }

    /// A selection added as proposals of every exact repeat
    /// (`add_manual_candidate`): `picked` is taken for new wording, else the
    /// guess; proposed wording keeps its own category (ADR 0024).
    pub fn add_selection(
        &mut self,
        range: Range<usize>,
        picked: Option<Category>,
    ) -> Result<(), CoreError> {
        let source = self.text.clone();
        let wording = source
            .get(range.clone())
            .ok_or_else(|| CoreError::invalid("The selection is not in the text."))?;
        let category = match self.review.proposed_wording_category(wording) {
            Some(category) => category,
            None => picked.unwrap_or_else(|| mdoc_detect::guess_category(&source, range.clone())),
        };
        // Checked first, so a refused selection makes no step.
        Review::validate_manual(&source, range.clone()).map_err(CoreError::review)?;
        let step = Step::new(
            StepKind::Mark,
            wording,
            Focus {
                range: Some(range.clone()),
                identity: None,
            },
        );
        self.step(step, |document, step| {
            document
                .checkpoint(|review| review.add_manual(&source, range.clone(), category))?
                .map_err(CoreError::review)?;
            step.after = Focus {
                identity: document.at(&range).and_then(|at| document.entity(&at)),
                range: Some(range),
            };
            Ok(())
        })
    }

    /// Replace the proposals in scope (`apply_scope`): this mention, or
    /// every proposal of its identity, whatever state the mention picked
    /// is in. A kept mention is proposed again first, with its identity's
    /// other kept mentions under All (`toggle_replace`).
    pub fn replace(&mut self, range: Range<usize>, scope: Scope) -> Result<(), CoreError> {
        let step = self.decision(StepKind::Replace, &range);
        self.step(step, |document, _| document.replace_now(range, scope))
    }

    fn replace_now(&mut self, range: Range<usize>, scope: Scope) -> Result<(), CoreError> {
        let at = self.mention(&range)?;
        let mut identity = match at {
            At::Proposed { identity, .. } | At::Kept { identity } => identity,
            At::Replaced { identity, .. } => Some(identity),
        };
        if let At::Kept { .. } = at {
            let ranges: Vec<_> = match (scope, identity) {
                (Scope::All, Some(identity)) => self
                    .review
                    .kept_mentions()
                    .into_iter()
                    .filter(|(_, _, id)| *id == Some(identity))
                    .map(|(r, _, _)| r)
                    .collect(),
                _ => vec![range.clone()],
            };
            let source = self.text.clone();
            self.checkpoint(|review| review.propose_kept_mentions(&source, &ranges))?;
            // Proposed again, a mention without an identity has one now.
            if let Some(At::Proposed { identity: now, .. }) = self.at(&range) {
                identity = identity.or(now);
            }
        }
        let ranges: HashSet<_> = match (scope, identity) {
            (Scope::All, Some(identity)) => self
                .occurrences(identity)
                .0
                .into_iter()
                .map(|(_, r)| (r.start, r.end))
                .collect(),
            (Scope::This, _) if matches!(at, At::Replaced { .. }) => return Ok(()),
            _ => HashSet::from([(range.start, range.end)]),
        };
        let plans: Vec<_> = self
            .review
            .pending_plans(&self.text, &HashMap::new())
            .map_err(CoreError::review)?
            .into_iter()
            .filter(|p| ranges.contains(&(p.range.start, p.range.end)))
            .collect();
        if plans.is_empty() {
            return Ok(());
        }
        self.commit_plans(&plans, None)?;
        self.refresh();
        Ok(())
    }

    /// Replace every proposal and correct applied aliases (`apply_aliases`).
    pub fn replace_all(&mut self) -> Result<(), CoreError> {
        let step = Step::new(StepKind::ReplaceAll, String::new(), Focus::default());
        self.step(step, |document, step| {
            let before = document.review.remaining();
            document.replace_all_now()?;
            let replaced = before.saturating_sub(document.review.remaining());
            if replaced > 0 {
                step.subject = replaced.to_string();
            }
            Ok(())
        })
    }

    fn replace_all_now(&mut self) -> Result<(), CoreError> {
        let mut plans = self
            .review
            .alias_corrections(&self.text)
            .map_err(CoreError::review)?;
        plans.extend(
            self.review
                .pending_plans(&self.text, &HashMap::new())
                .map_err(CoreError::review)?,
        );
        plans.sort_by_key(|p| p.range.start);
        if plans.windows(2).any(|p| p[0].range.end > p[1].range.start) {
            return Err(CoreError::review("Candidates overlap. Review them again."));
        }
        if !plans.is_empty() {
            self.commit_plans(&plans, None)?;
        }
        self.refresh();
        Ok(())
    }

    /// Keep the originals in scope: proposals stop being proposed and
    /// applied mentions return to their originals, as one step
    /// (`keep_originals`, `keep_mentions_now`).
    pub fn keep(&mut self, range: Range<usize>, scope: Scope) -> Result<(), CoreError> {
        let step = self.decision(StepKind::Keep, &range);
        self.step(step, |document, _| document.keep_now(range, scope))
    }

    fn keep_now(&mut self, range: Range<usize>, scope: Scope) -> Result<(), CoreError> {
        let (pending, applied) = match (self.mention(&range)?, scope) {
            (
                At::Kept {
                    identity: Some(identity),
                },
                Scope::All,
            )
            | (
                At::Proposed {
                    identity: Some(identity),
                    ..
                },
                Scope::All,
            )
            | (At::Replaced { identity, .. }, Scope::All) => self.occurrences(identity),
            (At::Kept { .. }, _) => return Ok(()),
            (At::Proposed { variant, .. }, _) => (vec![(variant, range)], Vec::new()),
            (At::Replaced { id, .. }, Scope::This) => (Vec::new(), vec![id]),
        };
        if pending.is_empty() && applied.is_empty() {
            return Ok(());
        }
        if applied.is_empty() {
            self.checkpoint(|review| review.keep_mentions(pending))?;
            return Ok(());
        }
        let edits = self.revert_applied(&applied.into_iter().collect(), true)?;
        let shifted: Vec<_> = pending
            .into_iter()
            .map(|(variant, range)| {
                let shift: isize = edits
                    .iter()
                    .filter(|(r, _)| r.end <= range.start)
                    .map(|(r, text)| text.len() as isize - r.len() as isize)
                    .sum();
                (
                    variant,
                    range.start.saturating_add_signed(shift)
                        ..range.end.saturating_add_signed(shift),
                )
            })
            .collect();
        self.review.keep_mentions(shifted);
        Ok(())
    }

    /// Line this document up with a case (`join_case_identities`): names the
    /// case knows take its aliases, colliding ones the next free number, and
    /// the case's other names are proposed wherever they occur.
    pub fn join(
        &mut self,
        case: &[ExportedIdentity],
        reserved: &[String],
    ) -> Result<(), CoreError> {
        let step = Step::new(StepKind::JoinCase, String::new(), Focus::default());
        self.step(step, |document, _| document.join_now(case, reserved))
    }

    fn join_now(
        &mut self,
        case: &[ExportedIdentity],
        reserved: &[String],
    ) -> Result<(), CoreError> {
        let document = self.review.export_identities_with_ids();
        let plan = plan_join(case, &document, reserved);
        self.review.reserve_aliases(reserved);
        let history = self.history;
        let old = self.review.identity_snapshot();
        self.retag(&plan.changes)?;
        // Joining again (each save does, against the case as stored now)
        // seeds only names this review does not know yet, as a case-wide
        // sync does (`sync_case_identities`).
        let unmatched: Vec<_> = plan
            .unmatched
            .into_iter()
            .filter(|c| self.review.identity_by_alias(&c.alias).is_none())
            .collect();
        if !unmatched.is_empty() {
            if plan.changes.is_empty() {
                self.checkpoint(|_| ())?;
            }
            let text = self.text.clone();
            if self.history == history {
                self.review.seed(&unmatched, &text);
            } else {
                self.review.seed_on_step(&unmatched, &text);
                self.review
                    .commit_identity_snapshot(history, self.history, old);
            }
        }
        Ok(())
    }

    /// A saved member reopened (`open_case_mapping`): the case's names are
    /// proposed, and alias tokens already in the text become their applied
    /// replacements, so Copy for AI and restore know them.
    pub fn open_case(&mut self, case: &[ExportedIdentity], reserved: &[String]) {
        let text = self.text.clone();
        self.review.reserve_aliases(reserved);
        let ids = self.review.seed(case, &text);
        self.review.relink(&text, case, &ids, self.history);
    }

    /// Retag identities and rewrite their applied aliases as one step
    /// (`retag_identities`).
    fn retag(&mut self, changes: &[(u64, String, Category, bool)]) -> Result<(), CoreError> {
        if changes.is_empty() {
            return Ok(());
        }
        let old = self.review.identity_snapshot();
        if let Err(error) = self.review.reassign_aliases(changes) {
            return Err(CoreError::review(error));
        }
        let plans = match self.review.alias_corrections(&self.text) {
            Ok(plans) => plans,
            Err(error) => {
                self.review.restore_identity_snapshot(old);
                return Err(CoreError::review(error));
            }
        };
        self.commit_plans(&plans, Some(old))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_pii::Recognizer;

    fn detect(text: &str, wording: &str, category: Category) -> Vec<Detection> {
        text.match_indices(wording)
            .map(|(start, _)| Detection {
                range: start..start + wording.len(),
                category,
                score: 0.9,
                recognizer: Recognizer::Model,
            })
            .collect()
    }

    fn document(text: &str, names: &[(&str, Category)]) -> ReviewText {
        let mut document = ReviewText::new(text.into());
        let detections = names
            .iter()
            .flat_map(|(wording, category)| detect(text, wording, *category))
            .collect();
        document.ingest(detections).unwrap();
        document
    }

    fn range(text: &str, wording: &str, nth: usize) -> Range<usize> {
        let start = text.match_indices(wording).nth(nth).unwrap().0;
        start..start + wording.len()
    }

    #[test]
    fn replace_this_one_or_all_of_an_identity() {
        let text = "Ann Lee called Bob Hart. Ann Lee wrote to Bob Hart.";
        let mut document = document(
            text,
            &[
                ("Ann Lee", Category::Person),
                ("Bob Hart", Category::Person),
            ],
        );
        assert_eq!(document.review.remaining(), 4);
        document
            .replace(range(text, "Ann Lee", 1), Scope::This)
            .unwrap();
        assert_eq!(
            document.text,
            "Ann Lee called Bob Hart. PERSON_1 wrote to Bob Hart."
        );
        let bob = range(&document.text.clone(), "Bob Hart", 0);
        document.replace(bob, Scope::All).unwrap();
        assert_eq!(
            document.text,
            "Ann Lee called PERSON_2. PERSON_1 wrote to PERSON_2."
        );
        assert_eq!(document.review.remaining(), 1);
        // Replacing applied text again changes nothing.
        let applied = range(&document.text.clone(), "PERSON_2", 0);
        document.replace(applied, Scope::All).unwrap();
        assert_eq!(document.review.applied().len(), 3);
    }

    #[test]
    fn keep_declines_proposals_and_returns_applied_text() {
        let text = "Ann Lee met Ann Lee and Ann Lee.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        document
            .keep(range(text, "Ann Lee", 0), Scope::This)
            .unwrap();
        assert_eq!(document.review.remaining(), 2);
        assert_eq!(document.review.kept_mentions().len(), 1);
        document.replace_all().unwrap();
        assert_eq!(document.text, "Ann Lee met PERSON_1 and PERSON_1.");
        // Keep on an applied mention, for all of its identity: both return.
        let applied = range(&document.text.clone(), "PERSON_1", 1);
        document.keep(applied, Scope::All).unwrap();
        assert_eq!(document.text, text);
        assert!(document.review.applied().is_empty());
        assert_eq!(document.review.kept_mentions().len(), 3);
        // Replace on a kept mention proposes it again and applies it.
        document
            .replace(range(text, "Ann Lee", 2), Scope::This)
            .unwrap();
        assert_eq!(document.text, "Ann Lee met Ann Lee and PERSON_1.");
    }

    /// All reaches the identity's other mentions from a mention in any
    /// state: replaced, kept or proposed.
    #[test]
    fn all_works_from_a_mention_in_any_state() {
        let text = "Ann Lee met Ann Lee and Ann Lee.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        document
            .replace(range(text, "Ann Lee", 0), Scope::This)
            .unwrap();
        let replaced = range(&document.text.clone(), "PERSON_1", 0);
        document.replace(replaced.clone(), Scope::This).unwrap();
        assert_eq!(
            document.review.remaining(),
            2,
            "This on a replaced one: no change"
        );
        document.replace(replaced, Scope::All).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and PERSON_1.");
        // Keep one, then Keep all from that kept mention.
        let last = range(&document.text.clone(), "PERSON_1", 2);
        document.keep(last, Scope::This).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and Ann Lee.");
        let kept = range(&document.text.clone(), "Ann Lee", 0);
        document.keep(kept, Scope::All).unwrap();
        assert_eq!(document.text, text);
        assert_eq!(document.review.kept_mentions().len(), 3);
        // Replace all from a kept mention proposes and replaces them all.
        document
            .replace(range(text, "Ann Lee", 1), Scope::All)
            .unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and PERSON_1.");
    }

    #[test]
    fn a_reopened_member_relinks_its_aliases() {
        let case = [ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec![Arc::from("Ann Lee")],
        }];
        let mut document = ReviewText::new("PERSON_1 wrote to Ann Lee.".into());
        document.open_case(&case, &[]);
        assert_eq!(document.review.applied().len(), 1);
        assert_eq!(document.review.remaining(), 1);
        document.replace_all().unwrap();
        assert_eq!(document.text, "PERSON_1 wrote to PERSON_1.");
        assert!(document.copy_for_ai().starts_with("Note:"));
    }

    #[test]
    fn a_selection_proposes_every_repeat_and_keeps_proposed_categories() {
        let text = "Call Mia. Mia's number is 12345.";
        let mut document = ReviewText::new(text.into());
        document
            .add_selection(range(text, "Mia", 0), Some(Category::Person))
            .unwrap();
        assert_eq!(document.review.remaining(), 2);
        let variant = document.review.candidates()[0].variant;
        assert_eq!(
            document.review.variant(variant).unwrap().category,
            Category::Person
        );
        // A selection crossing Markdown is refused without a change.
        let mut markdown = ReviewText::new("**Ann** Lee".into());
        assert!(markdown.add_selection(0..9, None).is_err());
        assert_eq!(markdown.history, 0);
        // Outside the text, or inside a character, is refused.
        assert!(matches!(
            document.add_selection(0..999, None),
            Err(CoreError::Invalid { .. })
        ));
    }

    #[test]
    fn linking_is_explicit_and_moves_applied_aliases() {
        // Two full names, one a slip: a short form ("Ms Bennett") would join
        // the name by propagation (ADR 0092).
        let text = "Maya Bennett met Maya Benett.";
        let mut document = document(
            text,
            &[
                ("Maya Bennett", Category::Person),
                ("Maya Benett", Category::Person),
            ],
        );
        let identities = document.review.active_identities();
        assert_eq!(identities.len(), 2, "variants stay apart until linked");
        document.replace_all().unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_2.");
        document.link(identities[1], identities[0]).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1.");
        assert_eq!(document.review.active_identities(), [identities[0]]);
        // The retired alias still restores.
        let exported = document.review.export_identities();
        assert_eq!(exported[0].previous, ["PERSON_2"]);
    }

    #[test]
    fn joining_a_case_takes_its_aliases_and_proposes_its_names() {
        let text = "Ivan Petrov and Anna Ivanova signed.";
        let mut document = document(text, &[("Anna Ivanova", Category::Person)]);
        let case = [ExportedIdentity {
            alias: "PERSON_4".into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec![Arc::from("Ivan Petrov")],
        }];
        document.join(&case, &["PERSON_1".into()]).unwrap();
        document.replace_all().unwrap();
        assert_eq!(document.text, "PERSON_4 and PERSON_5 signed.");
    }

    // ---- Undo, redo and the identity tools (ADR 0098) -------------------------

    /// What the review screen shows, without the history's ends.
    fn shown(document: &ReviewText) -> String {
        let snapshot = super::super::snapshot::of("", None, document, None);
        let mut shown = format!("{}\n", snapshot.text);
        for m in &snapshot.mentions {
            shown += &format!(
                "{}..{} {:?} {} {} {:?}\n",
                m.range.start,
                m.range.end,
                m.state,
                m.alias.as_deref().unwrap_or("-"),
                m.original,
                m.category
            );
        }
        for i in &snapshot.identities {
            shown += &format!(
                "{} {:?} {:?} {:?}\n",
                i.alias, i.category, i.originals, i.owner
            );
        }
        for h in &snapshot.link_hints {
            shown += &format!("hint {} → {}\n", h.from, h.into);
        }
        shown
    }

    /// Undo reaches `before` exactly, and Redo the state after, twice over.
    fn round_trip(document: &mut ReviewText, before: &str) {
        let after = shown(document);
        assert_ne!(before, after, "the step changed nothing");
        for _ in 0..2 {
            assert!(document.undo());
            assert_eq!(shown(document), before, "undo");
            assert!(document.redo());
            assert_eq!(shown(document), after, "redo");
        }
    }

    fn undo_label(document: &ReviewText) -> String {
        document.undo_step().unwrap().0.label()
    }

    fn id_of(document: &ReviewText, alias: &str) -> u64 {
        document.review.identity_by_alias(alias).unwrap()
    }

    fn aliases(document: &ReviewText) -> Vec<String> {
        super::super::snapshot::of("", None, document, None)
            .mentions
            .into_iter()
            .map(|m| m.alias.unwrap_or_default())
            .collect()
    }

    #[test]
    fn replace_and_keep_undo_and_redo_one_step_each() {
        let text = "Ann Lee called Bob Hart. Ann Lee wrote to Bob Hart.";
        let mut document = document(
            text,
            &[
                ("Ann Lee", Category::Person),
                ("Bob Hart", Category::Person),
            ],
        );
        assert!(!document.can_undo() && !document.can_redo());
        assert!(!document.undo(), "nothing to undo");
        let start = shown(&document);
        let ann = range(text, "Ann Lee", 1);
        document.replace(ann.clone(), Scope::This).unwrap();
        assert_eq!(undo_label(&document), "Replace · Ann Lee");
        round_trip(&mut document, &start);
        // After Undo, Redo names the step and where it acts now.
        document.undo();
        let (step, focus) = document.redo_step().unwrap();
        assert_eq!(step.kind, StepKind::Replace);
        assert_eq!(focus.range, Some(ann));
        document.redo();
        // Undo names where it acted after: the alias's range.
        let alias = range(&document.text.clone(), "PERSON_1", 0);
        assert_eq!(document.undo_step().unwrap().1.range, Some(alias));

        let replaced = shown(&document);
        let bob = range(&document.text.clone(), "Bob Hart", 0);
        document.keep(bob, Scope::All).unwrap();
        assert_eq!(undo_label(&document), "Keep · Bob Hart");
        round_trip(&mut document, &replaced);

        // Keep of a replaced mention returns its original, one step.
        let kept = shown(&document);
        let alias = range(&document.text.clone(), "PERSON_1", 0);
        document.keep(alias, Scope::This).unwrap();
        assert_eq!(document.text, text);
        round_trip(&mut document, &kept);

        // Replace on a kept mention proposes it again and replaces it: one step.
        let before = shown(&document);
        document
            .replace(range(text, "Bob Hart", 1), Scope::All)
            .unwrap();
        assert_eq!(
            document.text,
            "Ann Lee called PERSON_2. Ann Lee wrote to PERSON_2."
        );
        round_trip(&mut document, &before);
    }

    #[test]
    fn replace_all_and_a_mark_are_one_step_each() {
        let text = "Ann Lee met Mia. Ann Lee paid Mia.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        let start = shown(&document);
        let mia = range(text, "Mia", 0);
        document
            .add_selection(mia.clone(), Some(Category::Person))
            .unwrap();
        assert_eq!(document.review.remaining(), 4, "a mark proposes");
        assert_eq!(document.text, text);
        assert_eq!(undo_label(&document), "Mark · Mia");
        assert_eq!(document.undo_step().unwrap().1.range, Some(mia));
        round_trip(&mut document, &start);

        let marked = shown(&document);
        document.replace_all().unwrap();
        assert_eq!(
            document.text,
            "PERSON_1 met PERSON_2. PERSON_1 paid PERSON_2."
        );
        assert_eq!(undo_label(&document), "Replace all · 4");
        round_trip(&mut document, &marked);
        // A new step clears Redo.
        document.undo();
        assert!(document.can_redo());
        document.keep(range(text, "Mia", 1), Scope::This).unwrap();
        assert!(!document.can_redo());
    }

    #[test]
    fn a_change_that_changes_nothing_keeps_redo() {
        let text = "Ann Lee met Ann Lee.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        document.replace_all().unwrap();
        let applied = range(&document.text.clone(), "PERSON_1", 0);
        document.keep(applied, Scope::This).unwrap();
        document.undo();
        assert!(document.can_redo());
        // Replacing a replaced mention again changes nothing.
        let applied = range(&document.text.clone(), "PERSON_1", 0);
        document.replace(applied.clone(), Scope::This).unwrap();
        document.rename(applied, Scope::All, "PERSON_1").unwrap();
        assert!(document.can_redo());
    }

    #[test]
    fn the_history_keeps_256_steps() {
        let text = "Ann Lee. ".repeat(300);
        let mut document = document(&text, &[("Ann Lee", Category::Person)]);
        for _ in 0..300 {
            let next = document.review.candidates()[0].range.clone();
            document.replace(next, Scope::This).unwrap();
        }
        let mut undone = 0;
        while document.undo() {
            undone += 1;
        }
        assert_eq!(undone, history::UNDO_LIMIT);
        assert_eq!(document.review.applied().len(), 300 - history::UNDO_LIMIT);
        while document.redo() {}
        assert_eq!(document.review.applied().len(), 300);
    }

    #[test]
    fn rename_all_renames_the_identity_and_this_one_splits_the_mention() {
        let text = "Ann Lee met Ann Lee and Ann Lee.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        document
            .replace(range(text, "Ann Lee", 0), Scope::This)
            .unwrap();
        let before = shown(&document);
        let applied = range(&document.text.clone(), "PERSON_1", 0);
        document.rename(applied, Scope::All, " CLIENT ").unwrap();
        assert_eq!(document.text, "CLIENT met Ann Lee and Ann Lee.");
        assert_eq!(aliases(&document), ["CLIENT", "CLIENT", "CLIENT"]);
        assert_eq!(undo_label(&document), "Rename · CLIENT");
        let client = range(&document.text.clone(), "CLIENT", 0);
        assert_eq!(document.undo_step().unwrap().1.range, Some(client.clone()));
        round_trip(&mut document, &before);

        // This one: a proposed mention splits off under the new alias.
        let renamed = shown(&document);
        let last = range(&document.text.clone(), "Ann Lee", 1);
        document.rename(last, Scope::This, "WITNESS").unwrap();
        assert_eq!(aliases(&document), ["CLIENT", "CLIENT", "WITNESS"]);
        round_trip(&mut document, &renamed);

        // An alias another identity has is refused and named; no step.
        let witness = id_of(&document, "WITNESS");
        assert_eq!(document.identity_with_alias("WITNESS"), Some(witness));
        assert_eq!(document.identity_with_alias("NOBODY"), None);
        let steps = document.undo.len();
        assert!(
            document
                .rename(client.clone(), Scope::All, "WITNESS")
                .is_err()
        );
        assert!(document.rename(client, Scope::All, "not an alias").is_err());
        assert_eq!(document.undo.len(), steps);
        assert_eq!(undo_label(&document), "Rename · WITNESS");
    }

    #[test]
    fn category_all_recategorizes_and_this_one_splits_off() {
        let text = "Acme Ltd sued Acme Ltd.";
        let mut document = document(text, &[("Acme Ltd", Category::Person)]);
        let first = range(text, "Acme Ltd", 0);
        let choices = document
            .category_choices(first.clone(), Scope::All)
            .unwrap();
        let alias = |choices: &[(Category, Option<String>)], category| {
            choices
                .iter()
                .find(|(c, _)| *c == category)
                .unwrap()
                .1
                .clone()
        };
        assert_eq!(
            alias(&choices, Category::Organization).as_deref(),
            Some("ORG_1")
        );
        assert_eq!(alias(&choices, Category::Person), None, "the alias stays");
        let before = shown(&document);
        document
            .set_category(first.clone(), Scope::All, Category::Organization)
            .unwrap();
        assert_eq!(aliases(&document), ["ORG_1", "ORG_1"]);
        assert_eq!(undo_label(&document), "Category · Organization");
        round_trip(&mut document, &before);

        // This one: the mention splits off in its category; the rest stays.
        let recategorized = shown(&document);
        let this = document
            .category_choices(first.clone(), Scope::This)
            .unwrap();
        assert_eq!(alias(&this, Category::Other).as_deref(), Some("REDACTED_1"));
        document
            .set_category(first, Scope::This, Category::Other)
            .unwrap();
        assert_eq!(aliases(&document), ["REDACTED_1", "ORG_1"]);
        round_trip(&mut document, &recategorized);
    }

    #[test]
    fn same_as_links_the_identity_or_moves_one_mention() {
        let text = "Maya Bennett met Maya Benett and Maya Benett.";
        let mut document = document(
            text,
            &[
                ("Maya Bennett", Category::Person),
                ("Maya Benett", Category::Person),
            ],
        );
        document.replace_all().unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_2 and PERSON_2.");
        let (maya, slip) = (id_of(&document, "PERSON_1"), id_of(&document, "PERSON_2"));
        let before = shown(&document);
        // This one moves one mention; the other stays PERSON_2.
        let second = range(&document.text.clone(), "PERSON_2", 0);
        document.same_as(second, Scope::This, maya).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and PERSON_2.");
        assert_eq!(undo_label(&document), "Same as · Maya Bennett");
        assert_eq!(document.undo_step().unwrap().1.identity, Some(maya));
        round_trip(&mut document, &before);
        document.undo();

        // All links the identity: the target keeps its alias.
        let second = range(&document.text.clone(), "PERSON_2", 0);
        document.same_as(second, Scope::All, maya).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and PERSON_1.");
        assert_eq!(document.review.active_identities(), [maya]);
        round_trip(&mut document, &before);
        document.undo();

        // `link` is the same step, from a merge hint.
        document.link(slip, maya).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_1 and PERSON_1.");
        assert_eq!(undo_label(&document), "Same as · Maya Bennett");
        round_trip(&mut document, &before);
    }

    #[test]
    fn same_as_replaces_nothing_by_itself() {
        let text = "Maya Bennett met Maya Benett.";
        let mut document = document(
            text,
            &[
                ("Maya Bennett", Category::Person),
                ("Maya Benett", Category::Person),
            ],
        );
        let maya = id_of(&document, "PERSON_1");
        let before = shown(&document);
        document
            .same_as(range(text, "Maya Benett", 0), Scope::All, maya)
            .unwrap();
        assert_eq!(document.text, text);
        assert_eq!(document.review.remaining(), 2);
        assert_eq!(aliases(&document), ["PERSON_1", "PERSON_1"]);
        round_trip(&mut document, &before);
    }

    #[test]
    fn new_alias_splits_a_mention_off() {
        let text = "Ann Lee met Ann Lee.";
        let mut document = document(text, &[("Ann Lee", Category::Person)]);
        document.replace_all().unwrap();
        let before = shown(&document);
        let second = range(&document.text.clone(), "PERSON_1", 1);
        document.new_alias(second).unwrap();
        assert_eq!(document.text, "PERSON_1 met PERSON_2.");
        assert_eq!(undo_label(&document), "New alias · PERSON_2");
        let moved = range(&document.text.clone(), "PERSON_2", 0);
        let (_, focus) = document.undo_step().unwrap();
        assert_eq!(focus.range, Some(moved));
        assert_eq!(focus.identity, Some(id_of(&document, "PERSON_2")));
        round_trip(&mut document, &before);
        // A proposed mention splits off too, still proposed.
        let mut proposed = super::tests::document(text, &[("Ann Lee", Category::Person)]);
        proposed.new_alias(range(text, "Ann Lee", 1)).unwrap();
        assert_eq!(proposed.text, text);
        assert_eq!(aliases(&proposed), ["PERSON_1", "PERSON_2"]);
    }

    #[test]
    fn an_owner_is_set_and_cleared_as_steps() {
        let text = "Ann Lee, ann@example.com.";
        let mut document = document(
            text,
            &[
                ("Ann Lee", Category::Person),
                ("ann@example.com", Category::Email),
            ],
        );
        let (ann, email) = (id_of(&document, "PERSON_1"), id_of(&document, "EMAIL_1"));
        let before = shown(&document);
        document.set_owner(email, Some(ann)).unwrap();
        assert_eq!(document.review.identity(email).unwrap().owner, Some(ann));
        assert_eq!(undo_label(&document), "Belongs to · Ann Lee");
        assert_eq!(document.undo_step().unwrap().1.identity, Some(email));
        round_trip(&mut document, &before);
        let owned = shown(&document);
        document.set_owner(email, None).unwrap();
        assert_eq!(undo_label(&document), "Belongs to · none");
        round_trip(&mut document, &owned);
        // A person belongs to nobody; the refusal is no step.
        assert!(document.set_owner(ann, Some(ann)).is_err());
        assert_eq!(undo_label(&document), "Belongs to · none");
        // The owner it has already is no step either, and Redo stays.
        document.undo();
        document.set_owner(email, Some(ann)).unwrap();
        assert!(document.can_redo());
    }

    #[test]
    fn joining_a_case_is_one_step() {
        let text = "Ivan Petrov and Anna Ivanova signed.";
        let mut document = document(text, &[("Anna Ivanova", Category::Person)]);
        document.replace_all().unwrap();
        let before = shown(&document);
        let case = [ExportedIdentity {
            alias: "PERSON_1".into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec![Arc::from("Ivan Petrov")],
        }];
        document.join(&case, &[]).unwrap();
        // Anna renumbered and Ivan proposed, in one step.
        assert_eq!(document.text, "Ivan Petrov and PERSON_2 signed.");
        assert_eq!(document.review.remaining(), 1);
        assert_eq!(undo_label(&document), "Join case");
        round_trip(&mut document, &before);
    }

    #[test]
    fn a_dismissed_hint_stays_hidden_and_is_no_step() {
        let text = "Maya Bennett met Ms Bennett.";
        let names = [
            ("Maya Bennett", Category::Person),
            ("Ms Bennett", Category::Person),
        ];
        let hinted = || {
            let mut document = document(text, &names);
            // A short form joins its full name by propagation (ADR 0092):
            // split it off to see the hint.
            if document.review.active_identities().len() == 1 {
                document.new_alias(range(text, "Ms Bennett", 0)).unwrap();
            }
            document
        };
        let mut document = hinted();
        let hints = document.merge_hints();
        assert_eq!(hints.len(), 1, "{hints:?}");
        let (from, into) = (hints[0].from, hints[0].into);
        let steps = document.undo.len();
        document.dismiss_hint(from, into).unwrap();
        assert!(document.merge_hints().is_empty());
        assert_eq!(document.review.merge_hints().len(), 1);
        assert_eq!(document.undo.len(), steps, "no step");
        assert!(document.dismiss_hint(from, into).is_err(), "hidden already");
        // The case keeps it hidden in another document.
        let mut other = hinted();
        other.dismiss_pairs(&document.dismissed_pairs());
        assert!(other.merge_hints().is_empty());
    }
}
