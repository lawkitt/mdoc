//! Shared occurrence selection, explicit mutation scopes and draft lifecycle.
use super::{Applying, MappingAction, Selection};
use crate::{
    document_view::DocumentView,
    pii::ui::{APPLIED_ID, KEPT_ID, PiiClosePopup},
};
use gpui::{App, Context, Focusable, Window, px};
use mdoc_pii::{
    Category,
    decisions::{Decision, Mentions},
};
use std::{ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pii::ui) enum Scope {
    Mention,
    Wording,
    Entity,
}
/// Where ‹ › and ← → move the popup among an entity's mentions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::pii::ui) enum Step {
    Previous,
    Next,
    First,
    Last,
}
/// An entity offered for linking or ownership, labelled with an original.
#[derive(Clone)]
pub(super) struct AliasTarget {
    pub id: u64,
    pub alias: String,
    pub original: String,
    pub category: Category,
    pub suggested: bool,
}
impl DocumentView {
    pub(super) fn alias_targets(&self, query: &str, owner: bool) -> Vec<AliasTarget> {
        let review = &self.pii.review;
        let active = self.selected_entity();
        let original = self.active_original();
        let suggestions: std::collections::HashSet<_> = original
            .as_ref()
            .and_then(|original| review.variants().iter().find(|g| &g.original == original))
            .map(|g| review.suggestions(g.id).into_iter().collect())
            .unwrap_or_default();
        let mut representatives = std::collections::BTreeMap::<u64, (Arc<str>, bool)>::new();
        let mut add = |id, value: Arc<str>| {
            let matches = query.is_empty() || value.to_lowercase().contains(query);
            let row = representatives.entry(id).or_insert((value, false));
            row.1 |= matches;
        };
        for c in review.candidates() {
            if let (Some(id), Some(group)) = (
                review.occurrence_identity(c.variant, &c.range),
                review.variant(c.variant),
            ) {
                add(id, group.original.clone());
            }
        }
        for a in review.applied() {
            add(a.step.identity, a.step.original_shared().clone());
        }
        let mut targets: Vec<_> = representatives
            .into_iter()
            .filter_map(|(id, (original, matches))| {
                let i = review.identity(id)?;
                if Some(id) == active
                    || (owner && !matches!(i.category, Category::Person | Category::Organization))
                    || (!matches && !i.alias.to_lowercase().contains(query))
                {
                    return None;
                }
                Some(AliasTarget {
                    id,
                    alias: i.alias.clone(),
                    original: original.to_string(),
                    category: i.category,
                    suggested: suggestions.contains(&id),
                })
            })
            .collect();
        let category = active
            .and_then(|id| review.identity(id))
            .map(|i| i.category);
        targets.sort_by_key(|t| (!t.suggested, Some(t.category) != category));
        targets
    }
    pub(super) fn step_alias_choice(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let query = self.alias_query(cx);
        let count = self.alias_targets(&query, false).len();
        if count == 0 {
            return;
        }
        let index = match self.pii.mapping.target_index {
            Some(index) if backwards => index.checked_sub(1).unwrap_or(count - 1),
            Some(index) => (index + 1) % count,
            None if backwards => count - 1,
            None => 0,
        };
        let bottom = self.pii.popup_scroll.max_offset().y;
        self.pii
            .popup_scroll
            .set_offset(gpui::point(px(0.), -bottom));
        self.pii.mapping.highlight_target(index);
        cx.notify();
    }
    pub(super) fn alias_query(&self, cx: &App) -> String {
        let draft = self.pii.mapping.alias.read(cx).value().trim();
        if self
            .selected_entity()
            .and_then(|id| self.pii.review.identity(id))
            .is_some_and(|i| i.alias == draft)
        {
            String::new()
        } else {
            draft.to_lowercase()
        }
    }
    pub(super) fn open_alias_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.alias_targets(&self.alias_query(cx), false);
        if let Some(target) = self.pii.mapping.target_index.and_then(|i| targets.get(i)) {
            self.link_to_entity(target.id, cx);
        } else if self
            .pii
            .mapping
            .alias
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            self.confirm_alias(false, Applying::Nothing, cx);
        }
        window.focus(&self.pii.focus, cx);
        cx.notify();
    }
    pub(super) fn identity_occurrences(&self, id: u64) -> Vec<(u64, Range<usize>)> {
        let review = &self.pii.review;
        let mut result: Vec<_> = review
            .candidates()
            .iter()
            .filter(|c| review.occurrence_identity(c.variant, &c.range) == Some(id))
            .map(|c| (c.id, c.range.clone()))
            .collect();
        result.extend(
            review
                .applied()
                .iter()
                .filter(|a| a.step.identity == id)
                .map(|a| (APPLIED_ID | a.id, a.range.clone())),
        );
        result.sort_by_key(|(_, range)| range.start);
        result
    }
    pub(super) fn active_replacement_range(&self) -> Option<Range<usize>> {
        match self.pii.mapping.selected.as_ref()? {
            Selection::Candidate { range, .. } => Some(range.clone()),
            Selection::Applied(id) => self
                .pii
                .review
                .applied_occurrence(*id)
                .map(|a| a.range.clone()),
            Selection::Kept { range, .. } => Some(range.clone()),
            Selection::Entity(_) => None,
        }
    }
    pub(super) fn active_original(&self) -> Option<Arc<str>> {
        match self.pii.mapping.selected.as_ref()? {
            Selection::Candidate { variant: group, .. } => {
                self.pii.review.variant(*group).map(|g| g.original.clone())
            }
            Selection::Applied(id) => self
                .pii
                .review
                .applied_occurrence(*id)
                .map(|a| a.step.original_shared().clone()),
            Selection::Kept { original, .. } => Some(original.clone()),
            Selection::Entity(_) => None,
        }
    }
    pub(super) fn active_annotation(&self) -> Option<u64> {
        match self.pii.mapping.selected.as_ref()? {
            Selection::Candidate {
                variant: group,
                range,
            } => self.pii.review.annotation_id(*group, range),
            Selection::Applied(id) => Some(APPLIED_ID | id),
            Selection::Kept { range, .. } => Some(KEPT_ID | range.start as u64),
            Selection::Entity(_) => None,
        }
    }
    /// Whether the popup's mention is a kept original (Replace unticked).
    pub(in crate::pii::ui) fn active_is_kept(&self) -> bool {
        matches!(self.pii.mapping.selected, Some(Selection::Kept { .. }))
    }
    /// What the popup steps through and counts: the entity's live mentions
    /// and its kept ones, in document order (ADR 0055).
    pub(super) fn popup_occurrences(&self, id: u64) -> Vec<(u64, Range<usize>)> {
        let mut result = self.identity_occurrences(id);
        result.extend(
            self.kept_of(id)
                .into_iter()
                .map(|(range, _)| (KEPT_ID | range.start as u64, range)),
        );
        result.sort_by_key(|(_, range)| range.start);
        result
    }
    /// Kept mentions of the selected entity inside `scope`.
    pub(super) fn scoped_kept(&self, scope: Scope) -> Vec<Range<usize>> {
        let Some(id) = self.selected_entity() else {
            return Vec::new();
        };
        let active = self.active_replacement_range();
        let original = self.active_original();
        self.kept_of(id)
            .into_iter()
            .filter(|(range, wording)| match scope {
                Scope::Mention => Some(range) == active.as_ref(),
                Scope::Wording => Some(wording) == original.as_ref(),
                Scope::Entity => true,
            })
            .map(|(range, _)| range)
            .collect()
    }
    pub(super) fn scoped_ranges(&self, scope: Scope) -> Vec<Range<usize>> {
        let Some(id) = self.selected_entity() else {
            return Vec::new();
        };
        if scope == Scope::Mention {
            return self.active_replacement_range().into_iter().collect();
        }
        let original = self.active_original();
        let review = &self.pii.review;
        self.identity_occurrences(id)
            .into_iter()
            .filter(|(annotation, _)| {
                if scope == Scope::Entity {
                    return true;
                }
                if *annotation & APPLIED_ID != 0 {
                    review
                        .applied_occurrence(*annotation & !APPLIED_ID)
                        .is_some_and(|a| Some(a.step.original()) == original.as_deref())
                } else {
                    review
                        .candidate(*annotation)
                        .and_then(|c| review.variant(c.variant))
                        .is_some_and(|g| Some(g.original.as_ref()) == original.as_deref())
                }
            })
            .map(|(_, range)| range)
            .collect()
    }
    pub(in crate::pii::ui) fn remember_active_replacement(&mut self, cx: &mut Context<Self>) {
        let revision = self.editor.read(cx).revision();
        self.pii.mapping.mark_popup_revision(revision);
        if let (Some(id), Some(range)) = (self.selected_entity(), self.active_replacement_range()) {
            self.pii.mapping.remembered.insert(id, range.start);
        }
        let annotation = self.active_annotation();
        self.pii.mapping.active_place = self.selected_entity().and_then(|id| {
            let index = self
                .popup_occurrences(id)
                .iter()
                .position(|(a, _)| Some(*a) == annotation)?;
            Some((id, index))
        });
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(annotation, cx));
        self.reveal_active_row(cx);
    }
    pub(super) fn restore_active_replacement(
        &mut self,
        before: Option<Range<usize>>,
        edits: &[(Range<usize>, String)],
        cx: &mut Context<Self>,
    ) {
        let Some(range) = before else {
            return;
        };
        let shift: isize = edits
            .iter()
            .filter(|(r, _)| r.end <= range.start)
            .map(|(r, text)| text.len() as isize - r.len() as isize)
            .sum();
        let offset = range.start.saturating_add_signed(shift);
        let entity = self.selected_entity();
        let review = &self.pii.review;
        // Several mentions can start here (a kept name and a shorter one
        // inside it): stay with the same entity when it is among them.
        let mut found: Vec<(Selection, Option<u64>)> = review
            .applied()
            .iter()
            .filter(|a| a.range.start == offset)
            .map(|a| (Selection::Applied(a.id), Some(a.step.identity)))
            .chain(
                review
                    .candidates()
                    .iter()
                    .filter(|c| c.range.start == offset)
                    .map(|c| {
                        let identity = review.occurrence_identity(c.variant, &c.range);
                        let selection = Selection::Candidate {
                            variant: c.variant,
                            range: c.range.clone(),
                        };
                        (selection, identity)
                    }),
            )
            .collect();
        if let Some(kept) = self.kept_selection(offset) {
            let identity = match &kept {
                Selection::Kept { identity, .. } => Some(*identity),
                _ => None,
            };
            found.push((kept, identity));
        }
        let index = found
            .iter()
            .position(|(_, identity)| entity.is_some() && *identity == entity)
            .unwrap_or(0);
        let selection = (!found.is_empty()).then(|| found.swap_remove(index).0);
        if let Some(selection) = selection {
            self.select_occurrence(selection, cx);
            self.pii.keep_selection_popup();
            self.remember_active_replacement(cx);
        } else {
            self.pii.dismiss_popup();
            self.editor
                .update(cx, |e, cx| e.set_active_annotation(None, cx));
        }
    }
    pub(in crate::pii::ui) fn step_identity_occurrence(
        &mut self,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_entity() else {
            return;
        };
        let mentions = self.popup_occurrences(id);
        if mentions.is_empty() {
            return;
        }
        let active = self.active_replacement_range();
        let index = mentions
            .iter()
            .position(|(_, r)| Some(r) == active.as_ref())
            .unwrap_or(0);
        let index = match step {
            Step::Previous => index.checked_sub(1).unwrap_or(mentions.len() - 1),
            Step::Next => (index + 1) % mentions.len(),
            Step::First => 0,
            Step::Last => mentions.len() - 1,
        };
        if Some(&mentions[index].1) == active.as_ref() {
            return;
        }
        self.navigate_identity_mention(mentions[index].0, mentions[index].1.start, window, cx);
    }
    pub(super) fn scoped_mapping_action(
        &self,
        target: Option<u64>,
        alias: Option<String>,
        category: Option<Category>,
    ) -> Option<MappingAction> {
        if self.pii.mapping.scope == Scope::Mention && alias.is_none() && category.is_none() {
            return match self.pii.mapping.selected.as_ref()? {
                Selection::Candidate {
                    variant: group,
                    range,
                } => Some(MappingAction::AssignCandidate(
                    *group,
                    range.clone(),
                    target,
                )),
                Selection::Applied(id) => Some(MappingAction::AssignApplied(*id, target, false)),
                _ => None,
            };
        }
        Some(MappingAction::Scoped {
            identity: self.selected_entity()?,
            ranges: self.scoped_ranges(self.pii.mapping.scope),
            target,
            alias,
            category,
        })
    }
    pub(in crate::pii::ui) fn link_to_entity(&mut self, target: u64, cx: &mut Context<Self>) {
        self.assign_selected_identity(target, cx);
    }
    pub(super) fn separate_with_new_alias(&mut self, cx: &mut Context<Self>) {
        self.separate_selected_identity(cx);
    }
    pub(in crate::pii::ui) fn confirm_alias(
        &mut self,
        whole: bool,
        apply: Applying,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(id) = self.selected_entity() else {
            return true;
        };
        let value = self.pii.mapping.alias.read(cx).value().trim().to_owned();
        let review = &self.pii.review;
        if review.identity(id).is_some_and(|i| i.alias == value) {
            return true;
        }
        if review
            .active_identities()
            .into_iter()
            .any(|other| other != id && review.identity(other).is_some_and(|i| i.alias == value))
        {
            self.pii.mapping.show_alias_choices();
            self.pii.mapping.set_field_error(Some(
                "Choose the existing entity below to use its alias.".into(),
            ));
            cx.notify();
            return false;
        }
        let action = if whole {
            Some(MappingAction::Rename(id, value))
        } else {
            self.scoped_mapping_action(None, Some(value), None)
        };
        if let Some(action) = action {
            self.change_mapping_with_apply(action, apply, cx);
            let error = self.pii.error.take();
            let confirmed = error.is_none();
            self.pii.mapping.set_field_error(error);
            confirmed
        } else {
            false
        }
    }
    /// The scope covers all of the selected entity's mentions, so a
    /// category choice recategorizes it rather than splitting them off.
    fn scope_is_whole_entity(&self, id: u64) -> bool {
        self.scoped_ranges(self.pii.mapping.scope).len() == self.pii.review.identity_count(id)
    }
    /// What the category menu's row for `category` shows: the alias choosing
    /// it gives, or `None` when the alias stays (#179).
    pub(super) fn category_choice_alias(&self, category: Category) -> Option<String> {
        let id = self.selected_entity()?;
        self.pii
            .review
            .category_choice_alias(id, category, self.scope_is_whole_entity(id))
    }
    pub(super) fn correct_category(&mut self, category: Category, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_entity() {
            if self.scope_is_whole_entity(id) {
                self.change_mapping(MappingAction::Category(id, category), cx);
            } else if let Some(action) = self.scoped_mapping_action(None, None, Some(category)) {
                self.change_mapping(action, cx);
            }
        }
    }
    /// Pending candidates and applied occurrence ids inside the selected scope.
    /// Outline what the popup's scope (or a hovered chip) covers, or the
    /// selected panel group's mentions (ADR 0025). Display only.
    pub(crate) fn sync_scope_outlines(&mut self, cx: &mut Context<Self>) {
        // Proposals ring in their category's colour, as the popup's alias
        // shows it; applied ones in the applied blue (ADR 0055).
        let proposed = self
            .selected_name()
            .map_or(self.theme.get().proposed(), |(_, category)| {
                self.theme.get().category(category)
            });
        let applied = self.theme.get().applied();
        let visible = self.pii.reviewing && (self.pii.mapping.open || self.pii.popup.is_some());
        let ranges = match self.selected_entity().filter(|_| visible) {
            Some(_) if self.pii.popup.is_some() || self.pii.mapping.panel_selected => {
                let scope = self
                    .pii
                    .mapping
                    .scope_hover
                    .unwrap_or(self.pii.mapping.scope);
                let mut ranges = self.scoped_ranges(scope);
                if scope != Scope::Mention {
                    ranges.extend(self.scoped_kept(scope));
                }
                ranges
            }
            Some(id) => self
                .identity_occurrences(id)
                .into_iter()
                .map(|(_, r)| r)
                .collect(),
            None => Vec::new(),
        };
        let applied_ranges: std::collections::HashSet<_> = self
            .pii
            .review
            .applied()
            .iter()
            .map(|a| (a.range.start, a.range.end))
            .collect();
        let outlines = ranges
            .into_iter()
            .map(|r| {
                let color = if applied_ranges.contains(&(r.start, r.end)) {
                    applied
                } else {
                    proposed
                };
                (r, color)
            })
            .collect();
        self.editor.update(cx, |editor, cx| {
            editor.set_outlines(editor.revision(), outlines, cx)
        });
    }
    pub(super) fn scoped_mentions(&self) -> (Vec<(u64, Range<usize>)>, Vec<u64>) {
        let ranges: std::collections::HashSet<_> = self
            .scoped_ranges(self.pii.mapping.scope)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect();
        let review = &self.pii.review;
        let candidates = review
            .candidates()
            .iter()
            .filter(|c| ranges.contains(&(c.range.start, c.range.end)))
            .map(|c| (c.variant, c.range.clone()))
            .collect();
        let applied = review
            .applied()
            .iter()
            .filter(|a| ranges.contains(&(a.range.start, a.range.end)))
            .map(|a| a.id)
            .collect();
        (candidates, applied)
    }
    fn alias_draft_changed(&self, cx: &App) -> bool {
        self.selected_entity()
            .and_then(|id| self.pii.review.identity(id))
            .is_some_and(|i| i.alias != self.pii.mapping.alias.read(cx).value().trim())
    }
    /// Apply the pending mentions in scope as one undo step, keeping the popup
    /// on the active mention. A visible alias draft is confirmed in the same step.
    pub(in crate::pii::ui) fn apply_scope(&mut self, cx: &mut Context<Self>) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let (candidates, _) = self.scoped_mentions();
        let ranges: std::collections::HashSet<_> =
            candidates.iter().map(|(_, r)| (r.start, r.end)).collect();
        if ranges.is_empty() {
            return;
        }
        if self.alias_draft_changed(cx) {
            // The alias and the apply are one step, which Cancel addition
            // undoes like a plain apply (ADR 0025).
            let before = self.editor.read(cx).history_id();
            self.confirm_alias(false, Applying::Ranges(ranges), cx);
            let after = self.editor.read(cx).history_id();
            if after != before {
                self.pii.mapping.record_apply(before, after);
            }
            return;
        }
        let active = self.active_replacement_range();
        let revision = self.editor.read(cx).revision();
        let source = self.editor.read(cx).text().to_owned();
        let plans: Vec<_> = match self.pii.review.pending_plans(&source, &Default::default()) {
            Ok(plans) => plans
                .into_iter()
                .filter(|p| ranges.contains(&(p.range.start, p.range.end)))
                .collect(),
            Err(error) => {
                self.pii.error = Some(error);
                cx.notify();
                return;
            }
        };
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        let before = self.editor.read(cx).history_id();
        let Some(after) = self.commit_plans(revision, &plans, None, cx) else {
            self.pii.error = Some("Document changed. Review the replacement again.".into());
            cx.notify();
            return;
        };
        let applied = Mentions::of(plans.iter().map(|p| p.before.clone()));
        self.record_decision(before, Decision::Apply(applied), cx);
        self.pii.mapping.record_apply(before, after);
        self.pii.review.refresh(self.editor.read(cx).text());
        self.pii.error = None;
        self.review_changed(cx);
        self.restore_active_replacement(active, &edits, cx);
        cx.notify();
    }
    /// Popup Enter (ADR 0055): apply the proposals in the shown scope, then
    /// open the next proposal (this entity's next one under This one, else the
    /// next entity's first in panel order), or close when none is left. A
    /// fresh manual addition stays put so its Cancel is still offered (ADR 0025).
    pub(in crate::pii::ui) fn apply_and_advance(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(annotation) = self.active_annotation() else {
            return;
        };
        let Some(entity) = self.selected_entity() else {
            return;
        };
        if annotation & (APPLIED_ID | KEPT_ID) != 0 {
            return;
        }
        let scope = self.pii.mapping.scope;
        let manual = self.pii.enter_applies;
        let before = self.editor.read(cx).history_id();
        self.apply_scope(cx);
        if self.editor.read(cx).history_id() == before || self.pii.popup.is_none() {
            return;
        }
        if manual {
            // Confirming an alias draft reopens the popup without the flag.
            self.pii.enter_applies = true;
            return;
        }
        let proposals = |this: &Self, id| -> Vec<(u64, Range<usize>)> {
            this.identity_occurrences(id)
                .into_iter()
                .filter(|(a, _)| a & APPLIED_ID == 0)
                .collect()
        };
        if scope == Scope::Mention {
            let here = self.active_replacement_range().map_or(0, |r| r.start);
            let mentions = proposals(self, entity);
            if let Some((a, r)) = mentions
                .iter()
                .find(|(_, r)| r.start >= here)
                .or(mentions.first())
            {
                self.navigate_identity_mention(*a, r.start, window, cx);
                // A scoped alias split this mention off; back on its entity
                // the segment stays This one.
                self.pii.mapping.set_scope(Scope::Mention);
                return;
            }
        }
        let order = self.panel_entity_order(cx);
        let after = order
            .iter()
            .position(|id| *id == entity)
            .map_or(0, |i| i + 1);
        let next = order[after..]
            .iter()
            .chain(&order[..after])
            .filter(|id| **id != entity)
            .find_map(|id| proposals(self, *id).into_iter().next());
        match next {
            Some((a, r)) => self.navigate_identity_mention(a, r.start, window, cx),
            None => self.close_pii_popup(&PiiClosePopup, window, cx),
        }
    }
    /// Undo a mistaken manual addition (and an Apply right after it) while
    /// those are the latest history steps (ADR 0025).
    pub(in crate::pii::ui) fn cancel_addition(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let history = self.editor.read(cx).history_id();
        let Some((_, steps)) = self.pii.mapping.added_at(history) else {
            return;
        };
        self.pii.mapping.clear_added();
        self.close_pii_popup(&PiiClosePopup, window, cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        for _ in 0..steps {
            window.dispatch_action(Box::new(mdoc_editor::Undo), cx);
        }
        cx.notify();
    }
    /// Return applied occurrences to their prior text as one undo step. `keep`
    /// records Keep decisions; otherwise they become proposals again.
    fn revert_applied(
        &mut self,
        ids: &std::collections::HashSet<u64>,
        keep: bool,
        cx: &mut Context<Self>,
    ) -> Option<Vec<(Range<usize>, String)>> {
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        if !self.pii.review.matches_history(editor.history_id()) {
            self.pii.error = Some("Document changed. Review the replacement again.".into());
            cx.notify();
            return None;
        }
        let edits = match self.pii.review.reversion_edits(editor.text(), ids) {
            Ok(edits) => edits,
            Err(error) => {
                self.pii.error = Some(error);
                cx.notify();
                return None;
            }
        };
        let restored = self.pii.review.prepare_restore(&edits);
        let before = editor.text().to_owned();
        if !self
            .editor
            .update(cx, |e, cx| e.replace_ranges(revision, &edits, cx))
        {
            return None;
        }
        let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&transaction, cx);
        self.roll_replaced(&edits, &before, cx);
        let history = self.editor.read(cx).history_id();
        let review = &mut self.pii.review;
        if keep {
            review.commit_restore(history, restored);
        } else {
            review.commit_unapply(history, restored);
        }
        review.refresh(self.editor.read(cx).text());
        self.pii.error = None;
        Some(edits)
    }
    /// Where `edits` (old range → new text) landed in the edited source.
    fn edited_ranges(edits: &[(Range<usize>, String)]) -> Vec<Range<usize>> {
        let mut sorted: Vec<_> = edits.iter().collect();
        sorted.sort_by_key(|(r, _)| r.start);
        let mut shift = 0isize;
        sorted
            .into_iter()
            .map(|(r, text)| {
                let start = r.start.saturating_add_signed(shift);
                shift += text.len() as isize - r.len() as isize;
                start..start + text.len()
            })
            .collect()
    }
    /// Return applied occurrences to proposals with their aliases, keeping the
    /// popup on the active mention when it was among them.
    pub(in crate::pii::ui) fn undo_replacements(
        &mut self,
        ids: std::collections::HashSet<u64>,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() || self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let active = self.active_replacement_range();
        let before = self.editor.read(cx).history_id();
        let undone = Mentions::of(self.applied_originals(&ids));
        let Some(edits) = self.revert_applied(&ids, false, cx) else {
            return;
        };
        self.record_decision(before, Decision::UndoReplacement(undone), cx);
        self.review_changed(cx);
        let restored: std::collections::HashSet<_> = Self::edited_ranges(&edits)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect();
        let proposals = self
            .pii
            .review
            .candidates()
            .iter()
            .filter(|c| restored.contains(&(c.range.start, c.range.end)))
            .map(|c| c.id)
            .collect();
        self.flash_unapplied(proposals, cx);
        if self.pii.popup.is_some() {
            self.restore_active_replacement(active, &edits, cx);
        } else {
            self.remember_active_replacement(cx);
        }
        cx.notify();
    }
    /// Keep every original in scope: pending mentions stop being proposed and
    /// applied ones return to their originals, as one undo step.
    pub(in crate::pii::ui) fn keep_originals(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (candidates, applied) = self.scoped_mentions();
        if self.keep_mentions_now(candidates, applied, cx) && self.pii.popup.is_none() {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
    }
    /// Keep exactly these mentions (pending by variant and range, applied by
    /// id) as one undo step. Returns whether anything was kept.
    pub(in crate::pii::ui) fn keep_mentions_now(
        &mut self,
        candidates: Vec<(u64, Range<usize>)>,
        applied: Vec<u64>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return false;
        }
        let count = candidates.len() + applied.len();
        if count == 0 {
            return false;
        }
        let active = self.active_replacement_range();
        let before = self.editor.read(cx).history_id();
        let kept = {
            let review = &self.pii.review;
            let proposed = candidates
                .iter()
                .filter_map(|(group, _)| review.variant(*group).map(|v| v.original.clone()));
            let applied = self.applied_originals(&applied.iter().copied().collect());
            Mentions::of(proposed.chain(applied).collect::<Vec<_>>())
        };
        let mut moved = Vec::new();
        let mut faded: Vec<(Range<usize>, bool)> = Vec::new();
        if applied.is_empty() {
            faded.extend(candidates.iter().map(|(_, r)| (r.clone(), false)));
            if self
                .checkpoint_review(cx, |review| review.keep_mentions(candidates))
                .is_none()
            {
                return false;
            }
        } else {
            let Some(edits) = self.revert_applied(&applied.into_iter().collect(), true, cx) else {
                return false;
            };
            faded.extend(Self::edited_ranges(&edits).into_iter().map(|r| (r, true)));
            moved = edits.clone();
            let shifted: Vec<_> = candidates
                .into_iter()
                .map(|(group, range)| {
                    let shift: isize = edits
                        .iter()
                        .filter(|(r, _)| r.end <= range.start)
                        .map(|(r, text)| text.len() as isize - r.len() as isize)
                        .sum();
                    (
                        group,
                        range.start.saturating_add_signed(shift)
                            ..range.end.saturating_add_signed(shift),
                    )
                })
                .collect();
            faded.extend(shifted.iter().map(|(_, r)| (r.clone(), false)));
            self.pii.review.keep_mentions(shifted);
        }
        self.record_decision(before, Decision::Keep(kept), cx);
        let history = self.editor.read(cx).history_id();
        self.pii.mapping.record_kept(count, history);
        self.flash_notice(cx);
        self.review_changed(cx);
        self.fade_out_kept(faded, cx);
        // The popup stays on its mention, now kept (ADR 0055).
        if self.pii.popup.is_some() {
            self.restore_active_replacement(active, &moved, cx);
        } else {
            self.editor
                .update(cx, |e, cx| e.set_active_annotation(None, cx));
        }
        cx.notify();
        true
    }
    /// The popup's Replace checkbox and Space: untick keeps the originals in
    /// scope; ticking a kept mention proposes the kept ones in scope again.
    /// Either way one undo step, and the popup stays (ADR 0055).
    pub(in crate::pii::ui) fn toggle_replace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active_is_kept() {
            self.keep_originals(window, cx);
            return;
        }
        let ranges = self.scoped_kept(self.pii.mapping.scope);
        let active = self.active_replacement_range();
        if self.propose_kept(ranges, cx) && self.pii.popup.is_some() {
            self.restore_active_replacement(active, &[], cx);
        }
    }
    pub(in crate::pii::ui) fn apply_replacements(&mut self, cx: &mut Context<Self>) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        if self.alias_draft_changed(cx) {
            self.confirm_alias(false, Applying::All, cx);
            return;
        }
        let active = self.active_replacement_range();
        let review = &self.pii.review;
        // Each edit with the original it replaces: proposals, and applied
        // mentions whose alias is corrected. Both count as applied.
        let mut edits = Vec::new();
        let mut applied = Vec::new();
        for c in review.candidates() {
            let Some(id) = review.occurrence_identity(c.variant, &c.range) else {
                continue;
            };
            if let Some(identity) = review.identity(id) {
                edits.push((c.range.clone(), identity.alias.clone()));
            }
            applied.extend(review.variant(c.variant).map(|v| v.original.clone()));
        }
        for a in review.applied() {
            if let Some(identity) = review.identity(a.step.identity)
                && a.step.after.as_ref() != identity.alias
            {
                edits.push((a.range.clone(), identity.alias.clone()));
                applied.push(a.step.original_shared().clone());
            }
        }
        edits.sort_by_key(|(r, _)| r.start);
        let before = self.editor.read(cx).history_id();
        self.apply_aliases(cx);
        self.record_decision(before, Decision::Apply(Mentions::of(applied)), cx);
        let after = self.editor.read(cx).history_id();
        if after != before {
            self.pii.mapping.record_applied(edits.len(), after);
            self.flash_notice(cx);
        }
        self.restore_active_replacement(active, &edits, cx);
    }
    pub(super) fn escape_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A panel menu opened over the popup is the top layer (#92).
        if self.close_panel_menu(window, cx) {
            return;
        }
        if self.pii.mapping.close_pickers() {
            window.focus(&self.pii.focus, cx);
        } else if self
            .selected_entity()
            .and_then(|id| self.pii.review.identity(id))
            .is_some_and(|i| i.alias != self.pii.mapping.alias.read(cx).value())
        {
            let alias = self
                .pii
                .review
                .identity(self.selected_entity().unwrap())
                .unwrap()
                .alias
                .clone();
            self.pii
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias, cx));
            self.pii.mapping.set_field_error(None);
            window.focus(&self.pii.focus, cx);
        } else {
            self.close_pii_popup(&PiiClosePopup, window, cx);
        }
        cx.notify();
    }
}
