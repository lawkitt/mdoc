//! Shared occurrence selection, explicit mutation scopes and draft lifecycle.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::pii::ui) enum Scope {
    Mention,
    Wording,
    Entity,
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
impl Workspace {
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
            Selection::Entity(_) => None,
        }
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
        let review = &self.pii.review;
        let selection = review
            .applied()
            .iter()
            .find(|a| a.range.start == offset)
            .map(|a| Selection::Applied(a.id))
            .or_else(|| {
                review
                    .candidates()
                    .iter()
                    .find(|c| c.range.start == offset)
                    .map(|c| Selection::Candidate {
                        variant: c.variant,
                        range: c.range.clone(),
                    })
            });
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
    pub(super) fn step_identity_occurrence(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_entity() else {
            return;
        };
        let mentions = self.identity_occurrences(id);
        if mentions.is_empty() {
            return;
        }
        let active = self.active_replacement_range();
        let index = mentions
            .iter()
            .position(|(_, r)| Some(r) == active.as_ref())
            .unwrap_or(0);
        let index = if backwards {
            index.checked_sub(1).unwrap_or(mentions.len() - 1)
        } else {
            (index + 1) % mentions.len()
        };
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
    pub(super) fn link_to_entity(&mut self, target: u64, cx: &mut Context<Self>) {
        self.assign_selected_identity(target, cx);
    }
    pub(super) fn separate_with_new_alias(&mut self, cx: &mut Context<Self>) {
        self.separate_selected_identity(cx);
    }
    pub(super) fn confirm_alias(
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
    pub(super) fn correct_category(&mut self, category: Category, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_entity() {
            if self.scoped_ranges(self.pii.mapping.scope).len()
                == self.pii.review.identity_count(id)
            {
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
        let proposed = style::markdown_style(self.theme.get()).alert_warning;
        let applied = self.theme.get().search_accent();
        let visible = self.pii.reviewing && (self.pii.mapping.open || self.pii.popup.is_some());
        let ranges = match self.selected_entity().filter(|_| visible) {
            Some(_) if self.pii.popup.is_some() => self.scoped_ranges(
                self.pii
                    .mapping
                    .scope_hover
                    .unwrap_or(self.pii.mapping.scope),
            ),
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
                (r, Hsla { a: 0.85, ..color })
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
            self.confirm_alias(false, Applying::Ranges(ranges), cx);
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
        self.pii.mapping.record_apply(before, after);
        self.pii.review.refresh(self.editor.read(cx).text());
        self.pii.error = None;
        self.sync_annotations(cx);
        self.restore_active_replacement(active, &edits, cx);
        cx.notify();
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
        if !self
            .editor
            .update(cx, |e, cx| e.replace_ranges(revision, &edits, cx))
        {
            return None;
        }
        let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&transaction, cx);
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
        let Some(edits) = self.revert_applied(&ids, false, cx) else {
            return;
        };
        self.sync_annotations(cx);
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
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let (candidates, applied) = self.scoped_mentions();
        let count = candidates.len() + applied.len();
        if count == 0 {
            return;
        }
        if applied.is_empty() {
            if self
                .checkpoint_review(cx, |review| review.keep_mentions(candidates))
                .is_none()
            {
                return;
            }
        } else {
            let Some(edits) = self.revert_applied(&applied.into_iter().collect(), true, cx) else {
                return;
            };
            let shifted = candidates.into_iter().map(|(group, range)| {
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
            });
            self.pii.review.keep_mentions(shifted);
        }
        let history = self.editor.read(cx).history_id();
        self.pii.mapping.record_kept(count, history);
        self.pii.dismiss_popup();
        self.sync_annotations(cx);
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
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
        let mut edits: Vec<_> = review
            .candidates()
            .iter()
            .filter_map(|c| {
                let id = review.occurrence_identity(c.variant, &c.range)?;
                Some((c.range.clone(), review.identity(id)?.alias.clone()))
            })
            .collect();
        edits.extend(review.applied().iter().filter_map(|a| {
            let alias = &review.identity(a.step.identity)?.alias;
            (a.step.after.as_ref() != alias).then(|| (a.range.clone(), alias.clone()))
        }));
        edits.sort_by_key(|(r, _)| r.start);
        self.apply_aliases(cx);
        self.restore_active_replacement(active, &edits, cx);
    }
    pub(super) fn escape_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
