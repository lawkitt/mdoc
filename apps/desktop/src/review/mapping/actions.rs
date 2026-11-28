//! What the identity review does on a tab: opening the Inspector,
//! selecting mentions, and every mapping change, applied as one undo step.
use super::{Applying, MappingAction, MappingChange, Popup, Scope, Selection};
use crate::document::DocumentView;
use crate::review::{APPLIED_ID, KEPT_ID, text};
use gpui::{App, Context, Focusable, Window, px};
use mdoc_pii::{Category, decisions::Decision};
use mdoc_pii::{decisions::Mentions, tracking::ReplacementPlan};
use std::sync::Arc;

impl DocumentView {
    /// Whether the Replacements panel has anything to show: a review is
    /// under way or names were found or applied (the toolbar's panel button).
    pub(crate) fn can_show_replacements(&self) -> bool {
        !self.lifecycle.source_only()
            && (self.pii.reviewing()
                || self.replacements_open()
                || !self.pii.review().variants().is_empty()
                || !self.pii.review().applied().is_empty())
    }
    /// View › Show Replacements: open the panel when it can show; an open
    /// panel stays as it is.
    pub(crate) fn show_replacements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.replacements_open() && self.can_show_replacements() {
            self.toggle_replacements_panel(window, cx);
        }
    }
    pub(crate) fn toggle_replacements_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pii.mapping.open {
            self.close_replacements(window, cx);
        } else {
            self.pii.mapping.open_panel(window.focused(cx));
            self.pii.reviewing = true;
            self.sync_annotations(cx);
            let search = &self.pii.mapping.search;
            window.focus(&search.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    /// Open the Inspector without starting a review: its Notes tab needs
    /// none, and Pseudonymize still scans on its first click.
    pub(crate) fn open_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pii.mapping.open {
            self.pii.mapping.open_panel(window.focused(cx));
        }
        cx.notify();
    }
    /// Stop the running scan. With nothing found or applied yet, also leave
    /// review, so Pseudonymize starts afresh next time.
    pub(in crate::review) fn stop_pii_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pii.cancel();
        let review = &self.pii.review;
        if review.variants().is_empty() && review.applied().is_empty() {
            self.pii.reviewing = false;
            self.close_replacements(window, cx);
        }
        cx.notify();
    }
    pub(crate) fn close_replacements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pii.setup = false;
        let previous = self.pii.mapping.close_panel();
        self.pii.dismiss_popup();
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        if let Some(focus) = previous {
            if !self.pii.mapping.focus.contains_focused(window, cx) {
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            } else {
                window.focus(&focus, cx);
            }
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    pub(in crate::review) fn selected_entity(&self) -> Option<u64> {
        match self.pii.mapping.selected.as_ref()? {
            Selection::Entity(id) => Some(*id),
            Selection::Candidate {
                variant: group,
                range,
            } => self.pii.review.occurrence_identity(*group, range),
            Selection::Applied(id) => self
                .pii
                .review
                .applied_occurrence(*id)
                .map(|a| a.step.identity),
            Selection::Kept { identity, .. } => Some(*identity),
        }
    }
    #[cfg(test)]
    pub(super) fn reveal_replacement(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mentions = self.identity_occurrences(id);
        let preferred = self.pii.mapping.remembered.get(&id).copied();
        if let Some((annotation, range)) = mentions
            .iter()
            .find(|(_, r)| Some(r.start) == preferred)
            .or_else(|| mentions.first())
        {
            self.navigate_identity_mention(*annotation, range.start, window, cx);
        }
    }
    pub(super) fn reveal_replacement_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.set_cursor(offset, cx));
        if self.compare.is_some() {
            self.reveal_in_compare(offset, cx);
            return;
        }
        if let Some(top) = self.editor.read(cx).offset_screen_top(offset) {
            let delta = top - self.scroll.bounds().top() - px(24.);
            let mut scroll = self.scroll.offset();
            scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
            self.scroll.set_offset(scroll);
        }
    }
    pub(in crate::review) fn select_occurrence(
        &mut self,
        selected: Selection,
        cx: &mut Context<Self>,
    ) {
        let previous = self.selected_entity();
        self.pii.mapping.select(selected);
        // A newly selected entity's group opens and the scope returns to the
        // whole entity; within an entity the chosen scope stays.
        if let Some(id) = self.selected_entity().filter(|id| previous != Some(*id)) {
            self.pii.mapping.expanded.insert(id);
            self.pii.mapping.set_scope(Scope::Entity);
        }
        if let Some((alias, _)) = self.selected_name() {
            self.pii
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias, cx));
        }
        cx.notify();
    }
    /// The selected entity's alias and category; a fully kept entity no
    /// longer in the review answers from its untick-time record.
    pub(in crate::review) fn selected_name(&self) -> Option<(String, Category)> {
        let id = self.selected_entity()?;
        if let Some(identity) = self.pii.review.identity(id) {
            return Some((identity.alias.clone(), identity.category));
        }
        let original = self.active_original()?;
        let name = self.pii.mapping.kept_names.get(&original)?;
        Some((name.alias.clone(), name.category))
    }
    /// The kept mention starting at `start`, as a selection.
    pub(in crate::review) fn kept_selection(&self, start: usize) -> Option<Selection> {
        self.kept_entries()
            .into_iter()
            .find(|(range, _, _)| range.start == start)
            .and_then(|(range, original, identity)| {
                Some(Selection::Kept {
                    identity: identity?,
                    range,
                    original,
                })
            })
    }
    pub(in crate::review) fn sync_replacement_annotation(
        &mut self,
        annotation: u64,
        cx: &mut Context<Self>,
    ) {
        self.pii.mapping.show();
        let review = &self.pii.review;
        let selection = if annotation & KEPT_ID != 0 {
            self.kept_selection((annotation & !KEPT_ID) as usize)
        } else if annotation & APPLIED_ID != 0 {
            review
                .applied_occurrence(annotation & !APPLIED_ID)
                .map(|_| Selection::Applied(annotation & !APPLIED_ID))
        } else {
            review.candidate(annotation).map(|c| Selection::Candidate {
                variant: c.variant,
                range: c.range.clone(),
            })
        };
        if let Some(selection) = selection {
            self.select_occurrence(selection, cx);
        }
    }
    pub(in crate::review) fn sync_identity_selection_after_history(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        // An undo or redo can turn the popup's mention kept or live again:
        // follow it by its place among the entity's mentions.
        if let (Some((id, index)), Some(_)) = (self.pii.mapping.active_place, &self.pii.popup) {
            let mentions = self.popup_occurrences(id);
            let current = self
                .active_annotation()
                .is_some_and(|a| mentions.iter().any(|(m, _)| *m == a));
            if !current && let Some(&(annotation, _)) = mentions.get(index) {
                self.sync_replacement_annotation(annotation, cx);
                self.pii.keep_selection_popup();
                self.remember_active_replacement(cx);
                return;
            }
        }
        if let Some(selected) = self.pii.mapping.selected.clone() {
            if self.selected_entity().is_some() {
                self.select_occurrence(selected, cx);
            } else {
                self.pii.mapping.deselect();
            }
        }
    }
    pub(super) fn change_mapping(&mut self, action: MappingAction, cx: &mut Context<Self>) {
        self.change_mapping_with_apply(action, Applying::Nothing, cx);
    }
    pub(super) fn change_mapping_with_apply(
        &mut self,
        action: MappingAction,
        apply: Applying,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() || self.pii.scanning() {
            return;
        }
        let active = self.active_replacement_range();
        let revision = self.editor.read(cx).revision();
        let history = self.editor.read(cx).history_id();
        let source = self.editor.read(cx).text().to_owned();
        // Named before the change: a merge or split renames what it names.
        let decision = self.mapping_decision(&action);
        let mut applying = Vec::new();
        let old = self.pii.review.identity_snapshot();
        let result = self.prepare_mapping_change(&action, &source);
        let MappingChange {
            mut plans,
            assignments,
        } = match result {
            Ok(result) => result,
            Err(error) => {
                self.pii.review.restore_identity_snapshot(old);
                self.pii.error = Some(error);
                cx.notify();
                return;
            }
        };
        let selected_after = match &action {
            MappingAction::Rename(id, _)
            | MappingAction::Category(id, _)
            | MappingAction::Owner(id, _) => Some(*id),
            MappingAction::Merge(_, target) | MappingAction::AssignVariant(_, target) => {
                Some(*target)
            }
            MappingAction::AssignCandidate(_, range, _) => assignments
                .iter()
                .find(|(r, _)| r == range)
                .map(|(_, id)| *id),
            MappingAction::Scoped { target, ranges, .. } => target
                .or_else(|| {
                    assignments
                        .iter()
                        .find(|(r, _)| ranges.contains(r))
                        .map(|(_, id)| *id)
                })
                .or_else(|| plans.first().map(|p| p.identity)),
            MappingAction::AssignApplied(id, _, _) => self
                .pii
                .review
                .applied_occurrence(*id)
                .and_then(|a| plans.iter().find(|p| p.range == a.range))
                .map(|p| p.identity),
        };
        if !matches!(apply, Applying::Nothing) {
            let assigned: std::collections::HashMap<_, _> = assignments
                .iter()
                .map(|(r, id)| ((r.start, r.end), *id))
                .collect();
            match self.pii.review.pending_plans(&source, &assigned) {
                Ok(pending) => {
                    let pending: Vec<_> = pending
                        .into_iter()
                        .filter(|p| match &apply {
                            Applying::Ranges(ranges) => {
                                ranges.contains(&(p.range.start, p.range.end))
                            }
                            _ => true,
                        })
                        .collect();
                    applying.extend(pending.iter().map(|p| p.before.clone()));
                    plans.extend(pending);
                }
                Err(error) => {
                    self.pii.review.restore_identity_snapshot(old);
                    self.pii.error = Some(error);
                    return;
                }
            }
            plans.sort_by_key(|p| p.range.start);
        }
        // Re-choosing what is already so (the current category) changes
        // nothing: no undo step and no entry in What you did. An
        // applied mention always gets a plan, so a plan that rewrites it to
        // its own alias, category and identity counts as nothing too.
        let unchanged = |p: &ReplacementPlan| {
            self.pii.review.applied().iter().any(|a| {
                a.range == p.range
                    && a.step.after == p.after
                    && a.step.category == p.category
                    && a.step.identity == p.identity
            })
        };
        if plans.iter().all(unchanged)
            && assignments.is_empty()
            && Arc::ptr_eq(&old, &self.pii.review.identity_snapshot())
        {
            self.pii.mapping.close_pickers();
            self.pii.error = None;
            cx.notify();
            return;
        }
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        let Some(after) = self.commit_plans(revision, &plans, Some(old), cx) else {
            self.pii.error = Some(text::REVIEW_THE_MAPPING_AGAIN.into());
            cx.notify();
            return;
        };
        // One undo point, one entry: an alias confirmed with Apply lists
        // both ("Renamed … · applied 4 replacements").
        if let Some(decision) = decision {
            self.record_decision(history, decision, cx);
        }
        if !applying.is_empty() {
            self.record_decision(history, Decision::Apply(Mentions::of(applying)), cx);
        }
        let edited: std::collections::HashSet<_> =
            edits.iter().map(|(r, _)| (r.start, r.end)).collect();
        let mut assignments = assignments;
        assignments.sort_by_key(|(range, _)| range.start);
        assignments.dedup_by(|a, b| a.0 == b.0);
        let mut index = 0;
        let mut shift = 0isize;
        for (range, identity) in assignments {
            if !matches!(apply, Applying::Nothing) && edited.contains(&(range.start, range.end)) {
                continue;
            }
            while index < edits.len() && edits[index].0.end <= range.start {
                shift += edits[index].1.len() as isize - edits[index].0.len() as isize;
                index += 1;
            }
            let range =
                range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift);
            self.pii.review.commit_assignment(after, range, identity);
        }
        self.pii.review.refresh(self.editor.read(cx).text());
        self.pii.error = None;
        self.pii.dismiss_popup();
        if let Some(id) = selected_after {
            self.select_occurrence(Selection::Entity(id), cx);
        }
        self.review_changed(cx);
        self.restore_active_replacement(active, &edits, cx);
        cx.notify();
    }
    fn prepare_mapping_change(
        &mut self,
        action: &MappingAction,
        source: &str,
    ) -> Result<MappingChange, String> {
        let review = &mut self.pii.review;
        let mut assignments = Vec::new();
        let mut changed: std::collections::HashMap<u64, u64> = Default::default();
        match *action {
            MappingAction::Scoped {
                identity,
                ref ranges,
                target,
                ref alias,
                category,
            } => {
                let ranges: std::collections::HashSet<_> =
                    ranges.iter().map(|r| (r.start, r.end)).collect();
                let current = review
                    .identity(identity)
                    .ok_or(text::ENTITY_NO_LONGER_AVAILABLE)?
                    .clone();
                let target = if let Some(target) = target {
                    review
                        .identity(target)
                        .ok_or(text::ENTITY_NO_LONGER_AVAILABLE)?;
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
                            return Err(text::CANDIDATE_CHANGED.into());
                        }
                        assignments.push((c.range.clone(), target));
                    }
                }
                for a in review.applied() {
                    if ranges.contains(&(a.range.start, a.range.end)) && a.step.identity == identity
                    {
                        changed.insert(a.id | APPLIED_ID, target);
                    }
                }
            }
            MappingAction::Rename(id, ref alias) => {
                review.rename_identity(id, alias)?;
                changed.insert(id, id);
            }
            MappingAction::Category(id, category) => {
                review.recategorize_identity(id, category)?;
                changed.insert(id, id);
            }
            MappingAction::Owner(id, owner) => {
                review.set_owner(id, owner)?;
            }
            MappingAction::Merge(from, target) => {
                review.merge_identity(from, target)?;
                changed.insert(from, target);
                assignments.extend(
                    review
                        .assignments()
                        .iter()
                        .filter(|a| a.identity == from)
                        .map(|a| (a.range.clone(), target)),
                );
            }
            MappingAction::AssignVariant(group, target) => {
                let original = review
                    .variant(group)
                    .ok_or(text::VARIANT_NO_LONGER_AVAILABLE)?
                    .original
                    .clone();
                review.assign_variant(group, target)?;
                for a in review.applied() {
                    if a.step.original() == original.as_ref() {
                        changed.insert(a.id | APPLIED_ID, target);
                    }
                }
                let ranges = review.variant(group).unwrap().mentions.clone();
                assignments.extend(ranges.into_iter().map(|range| (range, target)));
            }
            MappingAction::AssignCandidate(group, ref range, target) => {
                if !review.variant(group).is_some_and(|g| {
                    g.mentions.contains(range)
                        && source.get(range.clone()) == Some(g.original.as_ref())
                }) {
                    return Err(text::CANDIDATE_CHANGED.into());
                }
                let target = target.unwrap_or_else(|| {
                    review.new_identity(review.variant(group).unwrap().category)
                });
                review
                    .identity(target)
                    .ok_or(text::IDENTITY_NO_LONGER_AVAILABLE)?;
                assignments.push((range.clone(), target));
            }
            MappingAction::AssignApplied(id, target, all) => {
                let selected = review
                    .applied_occurrence(id)
                    .ok_or(text::REPLACEMENT_NO_LONGER_AVAILABLE)?
                    .clone();
                let target = target.unwrap_or_else(|| review.new_identity(selected.step.category));
                review
                    .identity(target)
                    .ok_or(text::IDENTITY_NO_LONGER_AVAILABLE)?;
                if all {
                    let group = review
                        .variants()
                        .iter()
                        .find(|g| g.original.as_ref() == selected.step.original())
                        .map(|g| g.id)
                        .ok_or(text::ORIGINAL_VARIANT_NO_LONGER_AVAILABLE)?;
                    return self.prepare_mapping_change(
                        &MappingAction::AssignVariant(group, target),
                        source,
                    );
                }
                changed.insert(id | APPLIED_ID, target);
            }
        }
        let mut plans = Vec::new();
        for a in review.applied() {
            let Some(target) = changed
                .get(&(a.id | APPLIED_ID))
                .or_else(|| changed.get(&a.step.identity))
                .copied()
            else {
                continue;
            };
            if source.get(a.range.clone()) != Some(a.step.after.as_ref()) {
                return Err(text::REPLACEMENT_CHANGED.into());
            }
            let identity = review
                .identity(target)
                .ok_or(text::IDENTITY_NO_LONGER_AVAILABLE)?;
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
    pub(super) fn apply_aliases(&mut self, cx: &mut Context<Self>) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let source = self.editor.read(cx).text().to_owned();
        let revision = self.editor.read(cx).revision();
        let review = &self.pii.review;
        let mut plans = match review.alias_corrections(&source) {
            Ok(plans) => plans,
            Err(error) => {
                self.pii.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Ok(pending) = review.pending_plans(&source, &Default::default()) else {
            return;
        };
        plans.extend(pending);
        plans.sort_by_key(|p| p.range.start);
        if plans.windows(2).any(|p| p[0].range.end > p[1].range.start) {
            self.pii.error = Some(text::CANDIDATES_OVERLAP.into());
            cx.notify();
            return;
        }
        if !plans.is_empty() && self.commit_plans(revision, &plans, None, cx).is_none() {
            return;
        }
        self.pii.reviewing = true;
        self.pii.review.refresh(self.editor.read(cx).text());
        self.pii.dismiss_popup();
        self.review_changed(cx);
        cx.notify();
    }
    pub(super) fn assign_selected_identity(&mut self, target: u64, cx: &mut Context<Self>) {
        if self.pii.mapping.scope == Scope::Entity {
            if let Some(from) = self.selected_entity() {
                self.change_mapping(MappingAction::Merge(from, target), cx);
            }
        } else if let Some(action) = self.scoped_mapping_action(Some(target), None, None) {
            self.change_mapping(action, cx);
        }
    }
    pub(super) fn separate_selected_identity(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = self.scoped_mapping_action(None, None, None) {
            self.change_mapping(action, cx);
        }
    }
    pub(super) fn navigate_identity_mention(
        &mut self,
        annotation: u64,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.original_selected = false;
        self.editor.update(cx, |e, cx| e.set_cursor(offset, cx));
        self.activate_annotation(annotation, window, cx);
        self.remember_active_replacement(cx);
        window.focus(&self.pii.focus, cx);
        let revision = self.editor.read(cx).revision();
        self.reveal_popup_mention(annotation, offset, revision, cx);
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.editor.read(cx).revision() != revision
                    || this.active_annotation() != Some(annotation)
                {
                    return;
                }
                if this.compare.is_some() {
                    this.reveal_in_compare(offset, cx);
                } else {
                    this.reveal_popup_mention(annotation, offset, revision, cx);
                }
                let weak = cx.entity().downgrade();
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        // Enter changed the text: the first frame paints its
                        // new layout before we can fit the next proposal.
                        this.reveal_popup_mention(annotation, offset, revision, cx);
                        cx.notify();
                    });
                });
                cx.notify();
            });
        });
    }

    /// Fit the mention and its bar on whichever side needs the least scroll.
    fn reveal_popup_mention(&self, annotation: u64, offset: usize, revision: u64, cx: &App) {
        let editor = self.editor.read(cx);
        if editor.revision() != revision
            || self.active_annotation() != Some(annotation)
            || !matches!(self.pii.popup, Some(Popup::Selection))
            || self.compare.is_some()
        {
            return;
        }
        let Some(anchor) = editor.local_annotation_bounds(annotation, offset) else {
            // Unlike offset_screen_top, this rejects a layout from before an
            // edit. Enter's rebased byte offset must use the new layout.
            return;
        };
        let Some((layer, painted_scroll)) = self.pii.popup_layer.get() else {
            return;
        };
        // Window-space coordinates from the last paint become stale as soon
        // as we set the scroll offset. Use content coordinates, just as the
        // popup does, so a second reveal cannot apply the same delta again.
        let current = self.scroll.offset();
        let origin = layer + (current - painted_scroll);
        let top = origin.y + anchor.top();
        let bottom = origin.y + anchor.bottom();
        let view = self.scroll.bounds();
        let gap = editor.base_line_height().max(px(12.)) * 1.25;
        let bar = self.pii.popup_size.get().height;
        let low = view.top();
        let high = view.bottom();
        // Valid shifts for placing the bar below / above, including the
        // clear neighbouring line. Zero wins when already visible.
        let shift = [
            (low - top, high - px(4.) - gap - bar - bottom),
            (low + px(4.) + gap + bar - top, high - bottom),
        ]
        .into_iter()
        .filter(|(min, max)| min <= max)
        .map(|(min, max)| px(0.).clamp(min, max))
        .min_by(|a, b| f32::from(*a).abs().total_cmp(&f32::from(*b).abs()))
        // An unusually tall mention cannot fit with its bar: show its start.
        .unwrap_or(low - top);
        if shift != px(0.) {
            self.scroll.set_offset(gpui::point(
                current.x,
                (current.y + shift).clamp(-self.scroll.max_offset().y, px(0.)),
            ));
        }
    }

    /// Show the footer's "… · Undo" notice for a few seconds.
    pub(in crate::review) fn flash_notice(&mut self, cx: &mut Context<Self>) {
        self.pii.mapping.notice = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(6))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pii.mapping.notice = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}
