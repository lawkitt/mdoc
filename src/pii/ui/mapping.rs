//! Identity review and direct correction. All mutation uses editor history.
use super::*;
mod panel;
mod popup;
use popup::Scope;
mod popup_render;

/// What the workspace popup edits: a whole entity after a mapping change, or
/// one exact pending or applied occurrence.
#[derive(Clone)]
pub(super) enum Selection {
    Entity(u64),
    Candidate { variant: u64, range: Range<usize> },
    Applied(u64),
}

/// Inline popup pickers. Several may be open at once.
#[derive(Default)]
pub(super) struct Pickers {
    pub alias: bool,
    pub owner: bool,
    pub category: bool,
}
impl Pickers {
    fn any(&self) -> bool {
        self.alias || self.owner || self.category
    }
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
/// Replacements panel and word-popup state. Every transition is a method here.
pub(super) struct MappingUi {
    pub open: bool,
    scope: Scope,
    pub(super) pickers: Pickers,
    pub(super) target_index: Option<usize>,
    pub(super) target_scroll: gpui::UniformListScrollHandle,
    field_error: Option<String>,
    remembered: std::collections::HashMap<u64, usize>,
    bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    pub(super) popup_revision: Option<u64>,
    selected: Option<Selection>,
    search: Entity<markdown_search::SearchInput>,
    pub(super) alias: Entity<markdown_search::SearchInput>,
    target: Entity<markdown_search::SearchInput>,
    actions_open: bool,
    previous_focus: Option<FocusHandle>,
    controls: std::cell::RefCell<std::collections::HashMap<gpui::ElementId, FocusHandle>>,
    scroll: gpui::ScrollHandle,
    list_scroll: gpui::UniformListScrollHandle,
    focus: FocusHandle,
}
impl MappingUi {
    pub(super) fn invalidate_source_edit(&mut self, cx: &mut Context<Workspace>) {
        self.selected = None;
        self.remembered.clear();
        self.pickers.alias = false;
        self.field_error = None;
        self.alias
            .update(cx, |input, cx| input.set_value(String::new(), cx));
    }
    pub(super) fn begin_review(&mut self) {
        self.open = true;
        self.clear_selection();
    }
    /// Show the overview without changing its selection.
    pub(super) fn show(&mut self) {
        self.open = true;
    }
    pub(super) fn open_panel(&mut self, previous_focus: Option<FocusHandle>) {
        self.previous_focus = previous_focus;
        self.begin_review();
    }
    /// Hide the panel, returning the focus it should restore.
    pub(super) fn close_panel(&mut self) -> Option<FocusHandle> {
        self.open = false;
        self.previous_focus.take()
    }
    pub(super) fn clear_selection(&mut self) {
        self.selected = None;
        self.actions_open = false;
        self.pickers.owner = false;
        self.pickers.category = false;
    }
    /// Forget a selection whose occurrence no longer exists.
    pub(super) fn deselect(&mut self) {
        self.selected = None;
    }
    pub(super) fn select(&mut self, selected: Selection) {
        self.selected = Some(selected);
        self.pickers = Pickers::default();
        self.scope = Scope::Wording;
        self.target_index = None;
        self.field_error = None;
        self.actions_open = false;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
    }
    fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }
    pub(super) fn toggle_actions(&mut self) {
        self.actions_open = !self.actions_open;
    }
    /// Close the secondary actions menu; false when it was already closed.
    pub(super) fn close_actions(&mut self) -> bool {
        std::mem::take(&mut self.actions_open)
    }
    pub(super) fn toggle_category_picker(&mut self) {
        self.pickers.category = !self.pickers.category;
    }
    pub(super) fn toggle_owner_picker(&mut self) {
        self.pickers.owner = !self.pickers.owner;
    }
    pub(super) fn show_alias_choices(&mut self) {
        self.pickers.alias = true;
    }
    /// Close every inline picker; false when none was open.
    pub(super) fn close_pickers(&mut self) -> bool {
        std::mem::take(&mut self.pickers).any()
    }
    pub(super) fn highlight_target(&mut self, index: usize) {
        self.target_index = Some(index);
        self.pickers.alias = true;
        self.target_scroll
            .scroll_to_item(index, gpui::ScrollStrategy::Nearest);
    }
    /// A changed draft invalidates the highlighted target and its explanation.
    fn draft_changed(&mut self) {
        self.target_index = None;
        self.field_error = None;
    }
    pub(super) fn set_field_error(&mut self, error: Option<String>) {
        self.field_error = error;
    }
    pub(super) fn mark_popup_revision(&mut self, revision: u64) {
        self.popup_revision = Some(revision);
    }
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let mut input = |placeholder| {
            let input = cx.new(|cx| {
                markdown_search::SearchInput::new(cx)
                    .with_key_context(if placeholder == "Alias" {
                        "PseudonymReplacement"
                    } else {
                        "SettingsInput"
                    })
                    .with_placeholder(placeholder)
            });
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            if placeholder == "Alias" {
                cx.subscribe(
                    &input,
                    |this, _, _: &markdown_search::SearchInputEvent, cx| {
                        this.pii.mapping.draft_changed();
                        this.pii
                            .popup_scroll
                            .set_offset(gpui::point(px(0.), px(0.)));
                        cx.notify();
                    },
                )
                .detach();
            }
            input
        };
        Self {
            open: false,
            scope: Scope::Wording,
            pickers: Pickers::default(),
            target_index: None,
            target_scroll: gpui::UniformListScrollHandle::new(),
            field_error: None,
            remembered: Default::default(),
            bounds: Rc::new(Cell::new(None)),
            popup_revision: None,
            selected: None,
            search: input("Find an alias or original"),
            alias: input("Alias"),
            target: input("Find an identity"),
            actions_open: false,
            previous_focus: None,
            controls: Default::default(),
            scroll: gpui::ScrollHandle::new(),
            list_scroll: gpui::UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }
}
#[derive(Clone)]
pub(super) enum MappingAction {
    Rename(u64, String),
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
/// Text corrections and occurrence assignments staged by one mapping change.
struct MappingChange {
    plans: Vec<ReplacementPlan>,
    assignments: Vec<(Range<usize>, u64)>,
}
impl Workspace {
    pub(super) fn toggle_replacements_panel(
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
    fn close_replacements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn selected_applied(&self) -> Option<u64> {
        match self.pii.mapping.selected {
            Some(Selection::Applied(id)) => Some(id),
            _ => None,
        }
    }
    fn selected_entity(&self) -> Option<u64> {
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
        }
    }
    fn keep_replacement(&mut self, single: bool, cx: &mut Context<Self>) {
        if self.pii.scanning() || !self.can_copy_markdown() {
            return;
        }
        let Some(id) = self.selected_entity() else {
            return;
        };
        let selection = self.pii.mapping.selected.clone();
        let mention = match selection {
            Some(Selection::Candidate {
                variant: group,
                range,
            }) if single => Some((group, range)),
            _ if single => return,
            _ => None,
        };
        let review = &self.pii.review;
        if !review
            .candidates()
            .iter()
            .any(|c| review.occurrence_identity(c.variant, &c.range) == Some(id))
        {
            return;
        }
        let kept = self.checkpoint_review(cx, |review| {
            if let Some((group, range)) = mention {
                review.keep(group, Some(range));
            } else {
                review.keep_identity(id);
            }
        });
        if kept.is_none() {
            return;
        }
        self.pii.dismiss_popup();
        self.sync_annotations(cx);
        cx.notify();
    }
    fn reveal_replacement(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
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
    fn reveal_replacement_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.set_cursor(offset, cx));
        if let Some(top) = self.editor.read(cx).offset_screen_top(offset) {
            let delta = top - self.scroll.bounds().top() - px(24.);
            let mut scroll = self.scroll.offset();
            scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
            self.scroll.set_offset(scroll);
        }
    }
    pub(super) fn select_occurrence(&mut self, selected: Selection, cx: &mut Context<Self>) {
        self.pii.mapping.select(selected);
        if let Some(identity) = self
            .selected_entity()
            .and_then(|id| self.pii.review.identity(id))
        {
            let alias = identity.alias.clone();
            self.pii
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias, cx));
        }
        cx.notify();
    }
    pub(super) fn sync_replacement_annotation(&mut self, annotation: u64, cx: &mut Context<Self>) {
        self.pii.mapping.show();
        let review = &self.pii.review;
        let selection = if annotation & APPLIED_ID != 0 {
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
    pub(super) fn sync_identity_selection_after_history(&mut self, cx: &mut Context<Self>) {
        if let Some(selected) = self.pii.mapping.selected.clone() {
            if self.selected_entity().is_some() {
                self.select_occurrence(selected, cx);
            } else {
                self.pii.mapping.deselect();
            }
        }
    }
    fn change_mapping(&mut self, action: MappingAction, cx: &mut Context<Self>) {
        self.change_mapping_with_apply(action, false, cx);
    }
    fn change_mapping_with_apply(
        &mut self,
        action: MappingAction,
        apply: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy_markdown() || self.pii.scanning() {
            return;
        }
        let active = self.active_replacement_range();
        let revision = self.editor.read(cx).revision();
        let source = self.editor.read(cx).text().to_owned();
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
        if apply {
            let assigned: std::collections::HashMap<_, _> = assignments
                .iter()
                .map(|(r, id)| ((r.start, r.end), *id))
                .collect();
            match self.pii.review.pending_plans(&source, &assigned) {
                Ok(pending) => plans.extend(pending),
                Err(error) => {
                    self.pii.review.restore_identity_snapshot(old);
                    self.pii.error = Some(error);
                    return;
                }
            }
            plans.sort_by_key(|p| p.range.start);
        }
        let edits: Vec<_> = plans
            .iter()
            .map(|p| (p.range.clone(), p.after.to_string()))
            .collect();
        let Some(after) = self.commit_plans(revision, &plans, Some(old), cx) else {
            self.pii.error = Some("Document changed. Review the mapping again.".into());
            cx.notify();
            return;
        };
        let edited: std::collections::HashSet<_> =
            edits.iter().map(|(r, _)| (r.start, r.end)).collect();
        let mut assignments = assignments;
        assignments.sort_by_key(|(range, _)| range.start);
        assignments.dedup_by(|a, b| a.0 == b.0);
        let mut index = 0;
        let mut shift = 0isize;
        for (range, identity) in assignments {
            if apply && edited.contains(&(range.start, range.end)) {
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
        self.sync_annotations(cx);
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
                    .ok_or("Variant is no longer available.")?
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
                    return Err("Candidate changed. Review it again.".into());
                }
                let target = target.unwrap_or_else(|| {
                    review.new_identity(review.variant(group).unwrap().category)
                });
                review
                    .identity(target)
                    .ok_or("Identity is no longer available.")?;
                assignments.push((range.clone(), target));
            }
            MappingAction::AssignApplied(id, target, all) => {
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
    fn apply_aliases(&mut self, cx: &mut Context<Self>) {
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
            self.pii.error = Some("Candidates overlap. Review them again.".into());
            cx.notify();
            return;
        }
        if !plans.is_empty() && self.commit_plans(revision, &plans, None, cx).is_none() {
            return;
        }
        self.pii.reviewing = true;
        self.pii.review.refresh(self.editor.read(cx).text());
        self.pii.dismiss_popup();
        self.sync_annotations(cx);
        cx.notify();
    }
    fn assign_selected_identity(&mut self, target: u64, cx: &mut Context<Self>) {
        if self.pii.mapping.scope == Scope::Entity {
            if let Some(from) = self.selected_entity() {
                self.change_mapping(MappingAction::Merge(from, target), cx);
            }
        } else if let Some(action) = self.scoped_mapping_action(Some(target), None, None) {
            self.change_mapping(action, cx);
        }
    }
    fn separate_selected_identity(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = self.scoped_mapping_action(None, None, None) {
            self.change_mapping(action, cx);
        }
    }
    fn navigate_identity_mention(
        &mut self,
        annotation: u64,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.original_selected = false;
        self.editor.update(cx, |e, cx| e.set_cursor(offset, cx));
        if let Some(top) = self.editor.read(cx).offset_screen_top(offset) {
            let delta = top - self.scroll.bounds().top() - px(24.);
            let mut scroll = self.scroll.offset();
            scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
            self.scroll.set_offset(scroll);
        }
        self.activate_annotation(annotation, window, cx);
        self.remember_active_replacement(cx);
        window.focus(&self.pii.focus, cx);
        let revision = self.editor.read(cx).revision();
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.editor.read(cx).revision() != revision
                    || this.active_annotation() != Some(annotation)
                {
                    return;
                }
                this.reveal_replacement_offset(offset, cx);
                let weak = cx.entity().downgrade();
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |_, cx| cx.notify());
                });
                cx.notify();
            });
        });
    }
}
