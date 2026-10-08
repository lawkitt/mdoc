//! Shared occurrence selection, explicit mutation scopes and draft lifecycle.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
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
    pub(super) fn direct_targets(&self, query: &str, owner: bool) -> Vec<AliasTarget> {
        let review = &self.pseudonymization.review;
        let active = self.selected_identity();
        let original = self.active_original();
        let suggestions: std::collections::HashSet<_> = original
            .as_ref()
            .and_then(|original| review.groups.iter().find(|g| &g.original == original))
            .map(|g| review.suggestions(g.id).into_iter().collect())
            .unwrap_or_default();
        let mut representatives = std::collections::BTreeMap::<u64, (Arc<str>, bool)>::new();
        let mut add = |id, value: Arc<str>| {
            let matches = query.is_empty() || value.to_lowercase().contains(query);
            let row = representatives.entry(id).or_insert((value, false));
            row.1 |= matches;
        };
        for c in &review.candidates {
            if let (Some(id), Some(group)) = (
                review.occurrence_identity(c.group, &c.range),
                review.group(c.group),
            ) {
                add(id, group.original.clone());
            }
        }
        for a in &review.tracking.applied {
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
        let count = self.direct_targets(&query, false).len();
        if count == 0 {
            return;
        }
        let index = match self.pseudonymization.mapping.target_index {
            Some(index) if backwards => index.checked_sub(1).unwrap_or(count - 1),
            Some(index) => (index + 1) % count,
            None if backwards => count - 1,
            None => 0,
        };
        let bottom = self.pseudonymization.popup_scroll.max_offset().y;
        self.pseudonymization
            .popup_scroll
            .set_offset(gpui::point(px(0.), -bottom));
        self.pseudonymization.mapping.highlight_target(index);
        cx.notify();
    }
    pub(super) fn alias_query(&self, cx: &App) -> String {
        let draft = self.pseudonymization.mapping.alias.read(cx).value().trim();
        if self
            .selected_identity()
            .and_then(|id| self.pseudonymization.review.identity(id))
            .is_some_and(|i| i.alias == draft)
        {
            String::new()
        } else {
            draft.to_lowercase()
        }
    }
    pub(super) fn open_alias_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.direct_targets(&self.alias_query(cx), false);
        if let Some(target) = self
            .pseudonymization
            .mapping
            .target_index
            .and_then(|i| targets.get(i))
        {
            self.link_direct(target.id, cx);
        } else if self
            .pseudonymization
            .mapping
            .alias
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            self.confirm_alias_direct(false, false, cx);
        }
        window.focus(&self.pseudonymization.focus, cx);
        cx.notify();
    }
    pub(super) fn identity_occurrences(&self, id: u64) -> Vec<(u64, Range<usize>)> {
        let review = &self.pseudonymization.review;
        let mut result: Vec<_> = review
            .candidates
            .iter()
            .filter(|c| review.occurrence_identity(c.group, &c.range) == Some(id))
            .map(|c| (c.id, c.range.clone()))
            .collect();
        result.extend(
            review
                .tracking
                .applied
                .iter()
                .filter(|a| a.step.identity == id)
                .map(|a| (APPLIED_ID | a.id, a.range.clone())),
        );
        result.sort_by_key(|(_, range)| range.start);
        result
    }
    pub(super) fn active_replacement_range(&self) -> Option<Range<usize>> {
        match self.pseudonymization.mapping.selected.as_ref()? {
            Selection::Candidate { range, .. } => Some(range.clone()),
            Selection::Applied(id) => self
                .pseudonymization
                .review
                .tracking
                .get(*id)
                .map(|a| a.range.clone()),
            Selection::Entity(_) => None,
        }
    }
    pub(super) fn active_original(&self) -> Option<Arc<str>> {
        match self.pseudonymization.mapping.selected.as_ref()? {
            Selection::Candidate { group, .. } => self
                .pseudonymization
                .review
                .group(*group)
                .map(|g| g.original.clone()),
            Selection::Applied(id) => self
                .pseudonymization
                .review
                .tracking
                .get(*id)
                .map(|a| a.step.original_shared().clone()),
            Selection::Entity(_) => None,
        }
    }
    pub(super) fn active_annotation(&self) -> Option<u64> {
        match self.pseudonymization.mapping.selected.as_ref()? {
            Selection::Candidate { group, range } => {
                self.pseudonymization.review.annotation_id(*group, range)
            }
            Selection::Applied(id) => Some(APPLIED_ID | id),
            Selection::Entity(_) => None,
        }
    }
    pub(super) fn scoped_ranges(&self, scope: Scope) -> Vec<Range<usize>> {
        let Some(id) = self.selected_identity() else {
            return Vec::new();
        };
        if scope == Scope::Mention {
            return self.active_replacement_range().into_iter().collect();
        }
        let original = self.active_original();
        let review = &self.pseudonymization.review;
        self.identity_occurrences(id)
            .into_iter()
            .filter(|(annotation, _)| {
                if scope == Scope::Entity {
                    return true;
                }
                if *annotation & APPLIED_ID != 0 {
                    review
                        .tracking
                        .get(*annotation & !APPLIED_ID)
                        .is_some_and(|a| Some(a.step.original()) == original.as_deref())
                } else {
                    review
                        .candidate(*annotation)
                        .and_then(|c| review.group(c.group))
                        .is_some_and(|g| Some(g.original.as_ref()) == original.as_deref())
                }
            })
            .map(|(_, range)| range)
            .collect()
    }
    pub(in crate::pseudonymization_ui) fn remember_active_replacement(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let revision = self.editor.read(cx).revision();
        self.pseudonymization.mapping.mark_popup_revision(revision);
        if let (Some(id), Some(range)) = (self.selected_identity(), self.active_replacement_range())
        {
            self.pseudonymization
                .mapping
                .remembered
                .insert(id, range.start);
        }
        let annotation = self.active_annotation();
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(annotation, cx));
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
        let review = &self.pseudonymization.review;
        let selection = review
            .tracking
            .applied
            .iter()
            .find(|a| a.range.start == offset)
            .map(|a| Selection::Applied(a.id))
            .or_else(|| {
                review
                    .candidates
                    .iter()
                    .find(|c| c.range.start == offset)
                    .map(|c| Selection::Candidate {
                        group: c.group,
                        range: c.range.clone(),
                    })
            });
        if let Some(selection) = selection {
            self.select_identity(selection, cx);
            self.pseudonymization.keep_selection_popup();
            self.remember_active_replacement(cx);
        } else {
            self.pseudonymization.dismiss_popup();
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
        let Some(id) = self.selected_identity() else {
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
        if self.pseudonymization.mapping.scope == Scope::Mention
            && alias.is_none()
            && category.is_none()
        {
            return match self.pseudonymization.mapping.selected.as_ref()? {
                Selection::Candidate { group, range } => Some(MappingAction::AssignCandidate(
                    *group,
                    range.clone(),
                    target,
                )),
                Selection::Applied(id) => Some(MappingAction::AssignApplied(*id, target, false)),
                _ => None,
            };
        }
        Some(MappingAction::Scoped {
            identity: self.selected_identity()?,
            ranges: self.scoped_ranges(self.pseudonymization.mapping.scope),
            target,
            alias,
            category,
        })
    }
    pub(super) fn link_direct(&mut self, target: u64, cx: &mut Context<Self>) {
        self.assign_selected_identity(target, cx);
    }
    pub(super) fn new_alias_direct(&mut self, cx: &mut Context<Self>) {
        self.separate_selected_identity(cx);
    }
    pub(super) fn confirm_alias_direct(
        &mut self,
        whole: bool,
        apply: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(id) = self.selected_identity() else {
            return true;
        };
        let value = self
            .pseudonymization
            .mapping
            .alias
            .read(cx)
            .value()
            .trim()
            .to_owned();
        let review = &self.pseudonymization.review;
        if review.identity(id).is_some_and(|i| i.alias == value) {
            return true;
        }
        if review
            .active_identities()
            .into_iter()
            .any(|other| other != id && review.identity(other).is_some_and(|i| i.alias == value))
        {
            self.pseudonymization.mapping.show_alias_choices();
            self.pseudonymization.mapping.set_field_error(Some(
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
            let error = self.pseudonymization.error.take();
            let confirmed = error.is_none();
            self.pseudonymization.mapping.set_field_error(error);
            confirmed
        } else {
            false
        }
    }
    pub(super) fn category_direct(&mut self, category: Category, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_identity() {
            if self
                .scoped_ranges(self.pseudonymization.mapping.scope)
                .len()
                == self.pseudonymization.review.identity_count(id)
            {
                self.change_mapping(MappingAction::Category(id, category), cx);
            } else if let Some(action) = self.scoped_mapping_action(None, None, Some(category)) {
                self.change_mapping(action, cx);
            }
        }
    }
    pub(super) fn keep_wording_direct(&mut self, cx: &mut Context<Self>) {
        if self.pseudonymization.scanning() || !self.can_copy_markdown() {
            return;
        }
        let ranges: std::collections::HashSet<_> = self
            .scoped_ranges(Scope::Wording)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect();
        let candidates: Vec<_> = self
            .pseudonymization
            .review
            .candidates
            .iter()
            .filter(|c| ranges.contains(&(c.range.start, c.range.end)))
            .map(|c| (c.group, c.range.clone()))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let kept = self.checkpoint_review(cx, |review| {
            for (group, range) in candidates {
                review.keep(group, Some(range));
            }
        });
        if kept.is_none() {
            return;
        }
        self.pseudonymization.dismiss_popup();
        self.sync_annotations(cx);
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        cx.notify();
    }
    pub(in crate::pseudonymization_ui) fn apply_replacements_direct(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.pseudonymization.scanning() || !self.can_copy_markdown() {
            return;
        }
        let draft = self
            .selected_identity()
            .and_then(|id| self.pseudonymization.review.identity(id))
            .is_some_and(|i| {
                i.alias != self.pseudonymization.mapping.alias.read(cx).value().trim()
            });
        if draft {
            self.confirm_alias_direct(false, true, cx);
            return;
        }
        let active = self.active_replacement_range();
        let review = &self.pseudonymization.review;
        let mut edits: Vec<_> = review
            .candidates
            .iter()
            .filter_map(|c| {
                let id = review.occurrence_identity(c.group, &c.range)?;
                Some((c.range.clone(), review.identity(id)?.alias.clone()))
            })
            .collect();
        edits.extend(review.tracking.applied.iter().filter_map(|a| {
            let alias = &review.identity(a.step.identity)?.alias;
            (a.step.after.as_ref() != alias).then(|| (a.range.clone(), alias.clone()))
        }));
        edits.sort_by_key(|(r, _)| r.start);
        self.apply_identity_aliases(cx);
        self.restore_active_replacement(active, &edits, cx);
    }
    pub(super) fn close_direct_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pseudonymization.mapping.close_pickers() {
            window.focus(&self.pseudonymization.focus, cx);
        } else if self
            .selected_identity()
            .and_then(|id| self.pseudonymization.review.identity(id))
            .is_some_and(|i| i.alias != self.pseudonymization.mapping.alias.read(cx).value())
        {
            let alias = self
                .pseudonymization
                .review
                .identity(self.selected_identity().unwrap())
                .unwrap()
                .alias
                .clone();
            self.pseudonymization
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias, cx));
            self.pseudonymization.mapping.set_field_error(None);
            window.focus(&self.pseudonymization.focus, cx);
        } else {
            self.close_pseudonym_popup(&ClosePseudonymPopup, window, cx);
        }
        cx.notify();
    }
}
