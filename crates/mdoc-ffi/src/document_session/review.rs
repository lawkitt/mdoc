//! One document's text and its review, driven as the desktop's document view
//! drives them: the text stands in for the editor, so every change reaches
//! `mdoc_pii::Review` as the same `mdoc_history` transactions, in the same
//! order, as on desktop (`src/pii/ui.rs` `commit_plans` and
//! `checkpoint_review`, `src/pii/ui/mapping/popup.rs`, `src/pii/ui/mapping.rs`
//! and `src/pii/ui/case_sync.rs`). Mobile has no text editing and no undo,
//! so the history is a line of states with only the current one retained.
use crate::CoreError;
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use mdoc_pii::{
    Category, Detection, IdentitySnapshot, Review,
    restore::{ExportedIdentity, plan_join},
    tracking::ReplacementPlan,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

/// Where a Replace or Keep reaches (ADR 0085's review sheet).
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
            retained: vec![self.history],
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
        self.checkpoint(|review| review.add_manual(&source, range, category))?
            .map_err(CoreError::review)?;
        Ok(())
    }

    /// Replace the proposals in scope (`apply_scope`): this mention, or
    /// every proposal of its identity, whatever state the mention picked
    /// is in. A kept mention is proposed again first, with its identity's
    /// other kept mentions under All (`toggle_replace`).
    pub fn replace(&mut self, range: Range<usize>, scope: Scope) -> Result<(), CoreError> {
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

    /// Link identity `from` into `into`, which keeps its alias; `from`'s
    /// applied mentions take it (`change_mapping` with `MappingAction::Merge`).
    /// Linking is only ever this explicit action (ADR 0002).
    pub fn link(&mut self, from: u64, into: u64) -> Result<(), CoreError> {
        let review = &mut self.review;
        let old = review.identity_snapshot();
        if let Err(error) = review.merge_identity(from, into) {
            review.restore_identity_snapshot(old);
            return Err(CoreError::review(error));
        }
        let assignments: Vec<_> = review
            .assignments()
            .iter()
            .filter(|a| a.identity == from)
            .map(|a| (a.range.clone(), into))
            .collect();
        let mut plans = Vec::new();
        for a in review.applied() {
            if a.step.identity != from {
                continue;
            }
            if self.text.get(a.range.clone()) != Some(a.step.after.as_ref()) {
                review.restore_identity_snapshot(old);
                return Err(CoreError::review("Replacement changed. Review it again."));
            }
            let Some(identity) = review.identity(into) else {
                review.restore_identity_snapshot(old);
                return Err(CoreError::review("Identity is no longer available."));
            };
            plans.push(ReplacementPlan {
                range: a.range.clone(),
                before: a.step.after.clone(),
                after: identity.alias.as_str().into(),
                category: identity.category,
                identity: into,
            });
        }
        plans.sort_by_key(|p| p.range.start);
        if plans.is_empty()
            && assignments.is_empty()
            && Arc::ptr_eq(&old, &self.review.identity_snapshot())
        {
            return Ok(());
        }
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        self.commit_plans(&plans, Some(old))?;
        let mut assignments = assignments;
        assignments.sort_by_key(|(range, _)| range.start);
        assignments.dedup_by(|a, b| a.0 == b.0);
        let (mut index, mut shift) = (0, 0isize);
        for (range, identity) in assignments {
            while index < edits.len() && edits[index].0.end <= range.start {
                shift += edits[index].1.len() as isize - edits[index].0.len() as isize;
                index += 1;
            }
            let range =
                range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift);
            self.review.commit_assignment(self.history, range, identity);
        }
        self.refresh();
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
        assert!(
            document
                .review
                .copy_for_ai(&document.text)
                .starts_with("Note:")
        );
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
        let text = "Maya Bennett met Ms Bennett.";
        let mut document = document(
            text,
            &[
                ("Maya Bennett", Category::Person),
                ("Ms Bennett", Category::Person),
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
}
