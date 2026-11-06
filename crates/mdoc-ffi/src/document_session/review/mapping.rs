//! Identity changes (ADR 0098's identity tools), ported from desktop's
//! `src/pii/ui/mapping.rs` (`MappingAction`, `prepare_mapping_change`,
//! `change_mapping`) and the popup that picks the action by scope
//! (`src/pii/ui/mapping/popup.rs`: `confirm_alias`, `correct_category`,
//! `link_to_entity`, `separate_with_new_alias`). Mobile's This one is the
//! popup's This one, its All the popup's whole entity.
use super::{At, ReviewText, Scope};
use crate::CoreError;
use mdoc_pii::{Category, tracking::ReplacementPlan};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

/// Desktop's `MappingAction`, without `Rename`: its popup renames through
/// `Scoped`, which renames in place when the scope is the whole identity.
#[derive(Clone, Debug)]
pub(super) enum Mapping {
    Category(u64, Category),
    Owner(u64, Option<u64>),
    Merge(u64, u64),
    AssignVariant(u64, u64),
    AssignCandidate(u64, Range<usize>, Option<u64>),
    AssignApplied(u64, Option<u64>, bool),
    Scoped {
        identity: u64,
        ranges: Vec<Range<usize>>,
        target: Option<u64>,
        alias: Option<String>,
        category: Option<Category>,
    },
}

/// Text corrections and occurrence assignments staged by one change.
struct MappingChange {
    plans: Vec<ReplacementPlan>,
    assignments: Vec<(Range<usize>, u64)>,
}

impl ReviewText {
    // ---- What the popup selects ---------------------------------------------

    /// The identity a mention belongs to (`selected_entity`).
    pub(crate) fn entity(&self, at: &At) -> Option<u64> {
        match at {
            At::Proposed { identity, .. } | At::Kept { identity } => *identity,
            At::Replaced { identity, .. } => Some(*identity),
        }
    }

    /// The mentions a scope covers (`scoped_ranges`): this one, or every
    /// pending and applied mention of its identity.
    fn scoped_ranges(&self, range: &Range<usize>, scope: Scope, id: u64) -> Vec<Range<usize>> {
        match scope {
            Scope::This => vec![range.clone()],
            Scope::All => {
                let review = &self.review;
                let mut ranges: Vec<_> = review
                    .candidates()
                    .iter()
                    .filter(|c| review.occurrence_identity(c.variant, &c.range) == Some(id))
                    .map(|c| c.range.clone())
                    .chain(
                        review
                            .applied()
                            .iter()
                            .filter(|a| a.step.identity == id)
                            .map(|a| a.range.clone()),
                    )
                    .collect();
                ranges.sort_by_key(|r| r.start);
                ranges
            }
        }
    }

    /// The scope covers every mention of `id` (`scope_is_whole_entity`).
    fn whole(&self, range: &Range<usize>, scope: Scope, id: u64) -> bool {
        self.scoped_ranges(range, scope, id).len() == self.review.identity_count(id)
    }

    /// The mention and its identity, or why there is none.
    fn selected(&self, range: &Range<usize>) -> Result<(At, u64), CoreError> {
        let at = self.mention(range)?;
        let id = self
            .entity(&at)
            .ok_or_else(|| CoreError::review("Identity is no longer available."))?;
        Ok((at, id))
    }

    /// Popup This one without an alias or category (`scoped_mapping_action`):
    /// the exact pending or applied mention, moved to `target` or to an
    /// identity of its own. A kept mention has no such action.
    fn assign_mention(
        &self,
        at: &At,
        range: &Range<usize>,
        target: Option<u64>,
    ) -> Option<Mapping> {
        match at {
            At::Proposed { variant, .. } => {
                Some(Mapping::AssignCandidate(*variant, range.clone(), target))
            }
            At::Replaced { id, .. } => Some(Mapping::AssignApplied(*id, target, false)),
            At::Kept { .. } => None,
        }
    }

    /// The identity, other than the one at `range`, already called `alias`:
    /// Rename offers Same as instead (`confirm_alias`).
    pub fn identity_with_alias(&self, alias: &str) -> Option<u64> {
        let review = &self.review;
        review.active_identities().into_iter().find(|id| {
            review
                .identity(*id)
                .is_some_and(|i| i.alias == alias.trim())
        })
    }

    /// The wording an identity's toast names: its primary original, else
    /// its alias.
    pub(super) fn wording(&self, id: u64) -> String {
        self.review
            .identity_wording(id)
            .map(|w| w.to_string())
            .or_else(|| self.review.identity(id).map(|i| i.alias.clone()))
            .unwrap_or_default()
    }

    // ---- The identity tools -------------------------------------------------

    /// Rename… (`confirm_alias`): the alias of every mention in scope. All
    /// renames the identity; This one splits the mention off under it. An
    /// alias another identity uses is refused: Same as is the way to it.
    pub fn rename(
        &mut self,
        range: Range<usize>,
        scope: Scope,
        alias: &str,
    ) -> Result<(), CoreError> {
        let (_, id) = self.selected(&range)?;
        let value = alias.trim().to_owned();
        if self.review.identity(id).is_some_and(|i| i.alias == value) {
            return Ok(());
        }
        if self
            .identity_with_alias(&value)
            .is_some_and(|other| other != id)
        {
            return Err(CoreError::review(
                "Choose the existing entity below to use its alias.",
            ));
        }
        let action = Mapping::Scoped {
            identity: id,
            ranges: self.scoped_ranges(&range, scope, id),
            target: None,
            alias: Some(value.clone()),
            category: None,
        };
        let step = self.mention_step(super::StepKind::Rename, value, &range, id);
        self.step(step, |document, _| document.change_mapping(action))
    }

    /// The category menu's rows (`category_choice_alias`): each category
    /// with the alias choosing it gives, `None` when the alias stays.
    pub fn category_choices(
        &self,
        range: Range<usize>,
        scope: Scope,
    ) -> Result<Vec<(Category, Option<String>)>, CoreError> {
        let (_, id) = self.selected(&range)?;
        let whole = self.whole(&range, scope, id);
        Ok(Category::ALL
            .iter()
            .map(|c| (*c, self.review.category_choice_alias(id, *c, whole)))
            .collect())
    }

    /// The category of every mention in scope (`correct_category`): the
    /// whole identity's, or the mentions split off in it.
    pub fn set_category(
        &mut self,
        range: Range<usize>,
        scope: Scope,
        category: Category,
    ) -> Result<(), CoreError> {
        let (_, id) = self.selected(&range)?;
        let action = if self.whole(&range, scope, id) {
            Mapping::Category(id, category)
        } else {
            Mapping::Scoped {
                identity: id,
                ranges: self.scoped_ranges(&range, scope, id),
                target: None,
                alias: None,
                category: Some(category),
            }
        };
        let step = self.mention_step(super::StepKind::Category, category.label(), &range, id);
        self.step(step, |document, _| document.change_mapping(action))
    }

    /// Same as… (`link_to_entity`): All links the whole identity into
    /// `target`, which keeps its alias and restore wording; This one moves
    /// only this mention to it. Nothing is replaced by it.
    pub fn same_as(
        &mut self,
        range: Range<usize>,
        scope: Scope,
        target: u64,
    ) -> Result<(), CoreError> {
        let (at, id) = self.selected(&range)?;
        let action = match scope {
            Scope::All => Mapping::Merge(id, target),
            Scope::This => match self.assign_mention(&at, &range, Some(target)) {
                Some(action) => action,
                None => return Ok(()),
            },
        };
        let mut step = self.mention_step(super::StepKind::SameAs, self.wording(target), &range, id);
        step.after.identity = Some(target);
        self.step(step, |document, step| {
            document.change_mapping(action)?;
            step.after.range = document.carried(&range);
            Ok(())
        })
    }

    /// New alias (`separate_with_new_alias`, This one): this mention split
    /// off as an identity of its own.
    pub fn new_alias(&mut self, range: Range<usize>) -> Result<(), CoreError> {
        let (at, id) = self.selected(&range)?;
        let Some(action) = self.assign_mention(&at, &range, None) else {
            return Ok(());
        };
        let step = self.mention_step(super::StepKind::NewAlias, String::new(), &range, id);
        self.step(step, |document, step| {
            document.change_mapping(action)?;
            let range = document.carried(&range);
            let identity = range
                .as_ref()
                .and_then(|r| document.at(r))
                .and_then(|at| document.entity(&at));
            step.subject = identity
                .and_then(|id| document.review.identity(id))
                .map(|i| i.alias.clone())
                .unwrap_or_default();
            step.after = super::Focus { range, identity };
            Ok(())
        })
    }

    /// Belongs to…: a contact's or identifier's owner, a person or an
    /// organization, or none.
    pub fn set_owner(&mut self, id: u64, owner: Option<u64>) -> Result<(), CoreError> {
        // The owner it has already: no step, and Redo stays. (`set_owner`
        // makes a new policy either way, so `change_mapping` cannot tell.)
        if self
            .review
            .identity(id)
            .is_some_and(|identity| identity.owner == owner)
        {
            return Ok(());
        }
        let subject = owner.map_or_else(|| "none".to_owned(), |o| self.wording(o));
        let focus = super::Focus {
            range: None,
            identity: Some(id),
        };
        let step = super::Step::new(super::StepKind::Owner, subject, focus);
        self.step(step, |document, _| {
            document.change_mapping(Mapping::Owner(id, owner))
        })
    }

    /// Link identity `from` into `into`, which keeps its alias; `from`'s
    /// applied mentions take it (`change_mapping` with `MappingAction::Merge`).
    /// Linking is only ever this explicit action (ADR 0002).
    pub fn link(&mut self, from: u64, into: u64) -> Result<(), CoreError> {
        let step = super::Step {
            kind: super::StepKind::SameAs,
            subject: self.wording(into),
            before: super::Focus {
                range: None,
                identity: Some(from),
            },
            after: super::Focus {
                range: None,
                identity: Some(into),
            },
        };
        self.step(step, |document, _| {
            document.change_mapping(Mapping::Merge(from, into))
        })
    }

    /// A step acting on the mention at `range` of identity `id`.
    fn mention_step(
        &self,
        kind: super::StepKind,
        subject: impl Into<String>,
        range: &Range<usize>,
        id: u64,
    ) -> super::Step {
        super::Step::new(
            kind,
            subject,
            super::Focus {
                range: Some(range.clone()),
                identity: Some(id),
            },
        )
    }

    /// `range` carried through the step's edits so far.
    fn carried(&self, range: &Range<usize>) -> Option<Range<usize>> {
        Some(
            self.edits
                .iter()
                .fold(range.clone(), |r, edits| super::history::moved(r, edits)),
        )
    }

    // ---- Desktop's change_mapping -------------------------------------------

    /// One mapping change as one history step (`change_mapping`, applying
    /// nothing): the identity policy, applied aliases rewritten to match,
    /// and pending mentions pinned to their new identity.
    pub(super) fn change_mapping(&mut self, action: Mapping) -> Result<(), CoreError> {
        let source = self.text.clone();
        let old = self.review.identity_snapshot();
        let MappingChange { plans, assignments } =
            match self.prepare_mapping_change(&action, &source) {
                Ok(change) => change,
                Err(error) => {
                    self.review.restore_identity_snapshot(old);
                    return Err(CoreError::review(error));
                }
            };
        // Re-choosing what is already so changes nothing: no undo step. An
        // applied mention always gets a plan, so a plan that rewrites it to
        // its own alias, category and identity counts as nothing too.
        let review = &self.review;
        let unchanged = |p: &ReplacementPlan| {
            review.applied().iter().any(|a| {
                a.range == p.range
                    && a.step.after == p.after
                    && a.step.category == p.category
                    && a.step.identity == p.identity
            })
        };
        if plans.iter().all(unchanged)
            && assignments.is_empty()
            && Arc::ptr_eq(&old, &review.identity_snapshot())
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

    /// Desktop's `prepare_mapping_change`, line for line: applied mentions
    /// are keyed apart from identities instead of by a flag bit.
    fn prepare_mapping_change(
        &mut self,
        action: &Mapping,
        source: &str,
    ) -> Result<MappingChange, String> {
        let review = &mut self.review;
        let mut assignments = Vec::new();
        let mut identities: HashMap<u64, u64> = HashMap::new();
        let mut applied: HashMap<u64, u64> = HashMap::new();
        match *action {
            Mapping::Scoped {
                identity,
                ref ranges,
                target,
                ref alias,
                category,
            } => {
                let ranges: HashSet<_> = ranges.iter().map(|r| (r.start, r.end)).collect();
                let current = review
                    .identity(identity)
                    .ok_or("Entity is no longer available.")?
                    .clone();
                let target = if let Some(target) = target {
                    review
                        .identity(target)
                        .ok_or("Entity is no longer available.")?;
                    target
                } else if ranges.len() == review.identity_count(identity) && alias.is_some() {
                    identity
                } else {
                    review.new_identity(category.unwrap_or(current.category))
                };
                if let Some(category) = category {
                    review.recategorize_identity(target, category)?;
                }
                if let Some(alias) = alias {
                    review.rename_identity(target, alias)?;
                }
                for c in review.candidates() {
                    if ranges.contains(&(c.range.start, c.range.end))
                        && review.occurrence_identity(c.variant, &c.range) == Some(identity)
                    {
                        if source.get(c.range.clone())
                            != review.variant(c.variant).map(|g| g.original.as_ref())
                        {
                            return Err("Candidate changed. Review it again.".into());
                        }
                        assignments.push((c.range.clone(), target));
                    }
                }
                for a in review.applied() {
                    if ranges.contains(&(a.range.start, a.range.end)) && a.step.identity == identity
                    {
                        applied.insert(a.id, target);
                    }
                }
            }
            Mapping::Category(id, category) => {
                review.recategorize_identity(id, category)?;
                identities.insert(id, id);
            }
            Mapping::Owner(id, owner) => {
                review.set_owner(id, owner)?;
            }
            Mapping::Merge(from, target) => {
                review.merge_identity(from, target)?;
                identities.insert(from, target);
                assignments.extend(
                    review
                        .assignments()
                        .iter()
                        .filter(|a| a.identity == from)
                        .map(|a| (a.range.clone(), target)),
                );
            }
            Mapping::AssignVariant(group, target) => {
                let original = review
                    .variant(group)
                    .ok_or("Variant is no longer available.")?
                    .original
                    .clone();
                review.assign_variant(group, target)?;
                for a in review.applied() {
                    if a.step.original() == original.as_ref() {
                        applied.insert(a.id, target);
                    }
                }
                let ranges = review
                    .variant(group)
                    .ok_or("Variant is no longer available.")?
                    .mentions
                    .clone();
                assignments.extend(ranges.into_iter().map(|range| (range, target)));
            }
            Mapping::AssignCandidate(group, ref range, target) => {
                if !review.variant(group).is_some_and(|g| {
                    g.mentions.contains(range)
                        && source.get(range.clone()) == Some(g.original.as_ref())
                }) {
                    return Err("Candidate changed. Review it again.".into());
                }
                let target = match target {
                    Some(target) => target,
                    None => {
                        let category = review
                            .variant(group)
                            .ok_or("Variant is no longer available.")?
                            .category;
                        review.new_identity(category)
                    }
                };
                review
                    .identity(target)
                    .ok_or("Identity is no longer available.")?;
                assignments.push((range.clone(), target));
            }
            Mapping::AssignApplied(id, target, all) => {
                let selected = review
                    .applied_occurrence(id)
                    .ok_or("Replacement is no longer available.")?
                    .clone();
                let target = target.unwrap_or_else(|| review.new_identity(selected.step.category));
                review
                    .identity(target)
                    .ok_or("Identity is no longer available.")?;
                if all {
                    let group = review
                        .variants()
                        .iter()
                        .find(|g| g.original.as_ref() == selected.step.original())
                        .map(|g| g.id)
                        .ok_or("Original variant is no longer available.")?;
                    return self
                        .prepare_mapping_change(&Mapping::AssignVariant(group, target), source);
                }
                applied.insert(id, target);
            }
        }
        let review = &self.review;
        let mut plans = Vec::new();
        for a in review.applied() {
            let Some(target) = applied
                .get(&a.id)
                .or_else(|| identities.get(&a.step.identity))
                .copied()
            else {
                continue;
            };
            if source.get(a.range.clone()) != Some(a.step.after.as_ref()) {
                return Err("Replacement changed. Review it again.".into());
            }
            let identity = review
                .identity(target)
                .ok_or("Identity is no longer available.")?;
            // An equal visible alias can still need a different identity/category.
            plans.push(ReplacementPlan {
                range: a.range.clone(),
                before: a.step.after.clone(),
                after: identity.alias.as_str().into(),
                category: identity.category,
                identity: target,
            });
        }
        plans.sort_by_key(|p| p.range.start);
        Ok(MappingChange { plans, assignments })
    }
}
