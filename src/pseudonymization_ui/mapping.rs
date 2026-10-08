//! Identity review and direct correction. All mutation uses editor history.
use super::*;
mod direct;
mod render;
use direct::Scope;
mod direct_render;

#[derive(Clone)]
pub(super) enum Selection {
    Identity(u64),
    Candidate { group: u64, range: Range<usize> },
    Applied(u64),
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
pub(super) struct MappingUi {
    pub open: bool,
    scope: Scope,
    pub(super) alias_choices: bool,
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
    choosing_owner: bool,
    choosing_category: bool,
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
        self.alias_choices = false;
        self.field_error = None;
        self.alias
            .update(cx, |input, cx| input.set_value(String::new(), cx));
    }

    pub(super) fn begin_review(&mut self) {
        self.open = true;
        self.selected = None;
        self.actions_open = false;
        self.choosing_owner = false;
        self.choosing_category = false;
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
                        this.pseudonymization.mapping.target_index = None;
                        this.pseudonymization.mapping.field_error = None;
                        this.pseudonymization
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
            alias_choices: false,
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
            choosing_owner: false,
            choosing_category: false,
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
type MappingPlans = (
    Vec<pseudonymization::tracking::IdentifiedPlan>,
    Vec<(Range<usize>, u64)>,
);
impl Workspace {
    pub(super) fn toggle_identity_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pseudonymization.mapping.open {
            self.close_replacements(window, cx);
        } else {
            self.pseudonymization.mapping.previous_focus = window.focused(cx);
            self.pseudonymization.mapping.open = true;
            self.pseudonymization.mapping.selected = None;
            self.pseudonymization.review.open = true;
            self.sync_annotations(cx);
            self.back_to_replacements(window, cx);
        }
        cx.notify();
    }
    fn close_replacements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pseudonymization.mapping.open = false;
        self.pseudonymization.popup = None;
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(None, cx));
        self.pseudonymization.accept_all_bounds.set(None);
        if let Some(focus) = self.pseudonymization.mapping.previous_focus.take() {
            if !self
                .pseudonymization
                .mapping
                .focus
                .contains_focused(window, cx)
            {
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            } else {
                window.focus(&focus, cx);
            }
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn back_to_replacements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mapping = &mut self.pseudonymization.mapping;
        mapping.selected = None;
        mapping.actions_open = false;
        mapping.choosing_owner = false;
        mapping.choosing_category = false;
        window.focus(&mapping.search.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    fn selected_identity(&self) -> Option<u64> {
        match self.pseudonymization.mapping.selected.as_ref()? {
            Selection::Identity(id) => Some(*id),
            Selection::Candidate { group, range } => self
                .pseudonymization
                .review
                .occurrence_identity(*group, range),
            Selection::Applied(id) => self
                .pseudonymization
                .review
                .tracking
                .get(*id)
                .map(|a| a.step.identity),
        }
    }
    fn keep_replacement(&mut self, single: bool, cx: &mut Context<Self>) {
        if self.pseudonymization.scanning() || !self.can_copy_markdown() {
            return;
        }
        let Some(id) = self.selected_identity() else {
            return;
        };
        let selection = self.pseudonymization.mapping.selected.clone();
        let mention = match selection {
            Some(Selection::Candidate { group, range }) if single => Some((group, range)),
            _ if single => return,
            _ => None,
        };
        let review = &self.pseudonymization.review;
        if !review
            .candidates
            .iter()
            .any(|c| review.occurrence_identity(c.group, &c.range) == Some(id))
        {
            return;
        }
        let old = review.identity_snapshot();
        let before = self.editor.read(cx).history_id();
        let revision = self.editor.read(cx).revision();
        if !self
            .editor
            .update(cx, |e, cx| e.checkpoint_metadata(revision, cx))
        {
            return;
        }
        let tx = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&tx, cx);
        if let Some((group, range)) = mention {
            self.pseudonymization.review.keep(group, Some(range));
        } else {
            self.pseudonymization.review.keep_identity(id);
        }
        self.pseudonymization.review.commit_identity_snapshot(
            before,
            self.editor.read(cx).history_id(),
            old,
        );
        self.pseudonymization.popup = None;
        self.sync_annotations(cx);
        cx.notify();
    }
    fn reveal_replacement(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let mentions = self.identity_occurrences(id);
        let preferred = self.pseudonymization.mapping.remembered.get(&id).copied();
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
    pub(super) fn select_identity(&mut self, selected: Selection, cx: &mut Context<Self>) {
        self.pseudonymization.mapping.selected = Some(selected);
        self.pseudonymization.mapping.choosing_owner = false;
        self.pseudonymization.mapping.choosing_category = false;
        self.pseudonymization.mapping.scope = Scope::Wording;
        self.pseudonymization.mapping.alias_choices = false;
        self.pseudonymization.mapping.target_index = None;
        self.pseudonymization.mapping.field_error = None;
        self.pseudonymization.mapping.actions_open = false;
        self.pseudonymization
            .mapping
            .scroll
            .set_offset(gpui::point(px(0.), px(0.)));
        if let Some(identity) = self
            .selected_identity()
            .and_then(|id| self.pseudonymization.review.identity(id))
        {
            let alias = identity.alias.clone();
            self.pseudonymization
                .mapping
                .alias
                .update(cx, |input, cx| input.set_value(alias, cx));
        }
        cx.notify();
    }
    pub(super) fn sync_replacement_annotation(&mut self, annotation: u64, cx: &mut Context<Self>) {
        if !self.pseudonymization.mapping.open
            && self.pseudonymization.review.mode != Mode::Pseudonymize
        {
            return;
        }
        self.pseudonymization.mapping.open = true;
        let review = &self.pseudonymization.review;
        let selection = if annotation & APPLIED_ID != 0 {
            review
                .tracking
                .get(annotation & !APPLIED_ID)
                .map(|_| Selection::Applied(annotation & !APPLIED_ID))
        } else {
            review.candidate(annotation).map(|c| Selection::Candidate {
                group: c.group,
                range: c.range.clone(),
            })
        };
        if let Some(selection) = selection {
            self.select_identity(selection, cx);
        }
    }
    pub(super) fn sync_identity_selection_after_history(&mut self, cx: &mut Context<Self>) {
        if let Some(selected) = self.pseudonymization.mapping.selected.clone() {
            if self.selected_identity().is_some() {
                self.select_identity(selected, cx);
            } else {
                self.pseudonymization.mapping.selected = None;
            }
        }
    }
    pub(super) fn edit_annotation_identity(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = &self.pseudonymization.popup else {
            return;
        };
        let selection = match &popup.target {
            PopupTarget::Candidate { group, mention } => Selection::Candidate {
                group: *group,
                range: mention.clone(),
            },
            PopupTarget::Applied(id) => Selection::Applied(*id),
            PopupTarget::Choose { .. } => return,
        };
        self.pseudonymization.mapping.open = true;
        self.select_identity(selection, cx);
        self.remember_active_replacement(cx);
        cx.notify();
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
        if !self.can_copy_markdown() || self.pseudonymization.scanning() {
            return;
        }
        let active = self.active_replacement_range();
        let before = self.editor.read(cx).history_id();
        let revision = self.editor.read(cx).revision();
        let source = self.editor.read(cx).text().to_owned();
        let old = self.pseudonymization.review.identity_snapshot();
        let result = self.prepare_mapping_change(&action, &source);
        let (mut plans, assignments) = match result {
            Ok(result) => result,
            Err(error) => {
                self.pseudonymization.review.restore_identity_snapshot(old);
                self.pseudonymization.error = Some(error);
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
                .or_else(|| plans.first().map(|p| p.1)),
            MappingAction::AssignApplied(id, _, _) => self
                .pseudonymization
                .review
                .tracking
                .get(*id)
                .and_then(|a| plans.iter().find(|p| p.0.0 == a.range))
                .map(|p| p.1),
        };
        if apply {
            let assigned: std::collections::HashMap<_, _> = assignments
                .iter()
                .map(|(r, id)| ((r.start, r.end), *id))
                .collect();
            let review = &self.pseudonymization.review;
            for candidate in &review.candidates {
                let Some(group) = review.group(candidate.group) else {
                    return;
                };
                if source.get(candidate.range.clone()) != Some(group.original.as_ref()) {
                    self.pseudonymization.review.restore_identity_snapshot(old);
                    self.pseudonymization.error = Some("Document changed. Review it again.".into());
                    return;
                }
                let id = assigned
                    .get(&(candidate.range.start, candidate.range.end))
                    .copied()
                    .or_else(|| review.occurrence_identity(candidate.group, &candidate.range))
                    .unwrap();
                let identity = review.identity(id).unwrap();
                plans.push((
                    (
                        candidate.range.clone(),
                        group.original.clone(),
                        identity.alias.as_str().into(),
                        identity.category,
                    ),
                    id,
                ));
            }
            plans.sort_by_key(|p| p.0.0.start);
        }
        let next = self.pseudonymization.review.identity_snapshot();
        self.pseudonymization
            .review
            .restore_identity_snapshot(old.clone());
        let edits: Vec<_> = plans
            .iter()
            .map(|((range, _, after, _), _)| (range.clone(), after.to_string()))
            .collect();
        let added = self
            .pseudonymization
            .review
            .tracking
            .prepare_corrections(&plans);
        let committed = self.editor.update(cx, |e, cx| {
            if edits.is_empty() {
                e.checkpoint_metadata(revision, cx)
            } else {
                e.replace_ranges(revision, &edits, cx)
            }
        });
        if !committed {
            self.pseudonymization.error =
                Some("Document changed. Review the mapping again.".into());
            cx.notify();
            return;
        }
        let transaction = self.editor.read(cx).last_transaction().cloned().unwrap();
        self.pii_transaction(&transaction, cx);
        let after = self.editor.read(cx).history_id();
        self.pseudonymization.review.restore_identity_snapshot(next);
        self.pseudonymization
            .review
            .commit_identity_snapshot(before, after, old);
        self.pseudonymization.review.tracking.commit(after, added);
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
            self.pseudonymization
                .review
                .tracking
                .commit_assignment(after, range, identity);
        }
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.pseudonymization.error = None;
        if apply {
            self.pseudonymization.completion = Some((edits.len(), self.editor.read(cx).revision()));
        }
        self.pseudonymization.popup = None;
        if let Some(id) = selected_after {
            self.select_identity(Selection::Identity(id), cx);
        }
        self.sync_annotations(cx);
        self.restore_active_replacement(active, &edits, cx);
        cx.notify();
    }
    fn prepare_mapping_change(
        &mut self,
        action: &MappingAction,
        source: &str,
    ) -> Result<MappingPlans, String> {
        let review = &mut self.pseudonymization.review;
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
                for c in &review.candidates {
                    if ranges.contains(&(c.range.start, c.range.end))
                        && review.occurrence_identity(c.group, &c.range) == Some(identity)
                    {
                        if source.get(c.range.clone())
                            != review.group(c.group).map(|g| g.original.as_ref())
                        {
                            return Err("Candidate changed. Review it again.".into());
                        }
                        assignments.push((c.range.clone(), target));
                    }
                }
                for a in &review.tracking.applied {
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
                        .tracking
                        .assignments
                        .iter()
                        .filter(|a| a.identity == from)
                        .map(|a| (a.range.clone(), target)),
                );
            }
            MappingAction::AssignVariant(group, target) => {
                let original = review
                    .group(group)
                    .ok_or("Variant is no longer available.")?
                    .original
                    .clone();
                review.assign_variant(group, target)?;
                for a in &review.tracking.applied {
                    if a.step.original() == original.as_ref() {
                        changed.insert(a.id | APPLIED_ID, target);
                    }
                }
                let ranges = review.group(group).unwrap().mentions.clone();
                assignments.extend(ranges.into_iter().map(|range| (range, target)));
            }
            MappingAction::AssignCandidate(group, ref range, target) => {
                if !review.group(group).is_some_and(|g| {
                    g.mentions.contains(range)
                        && source.get(range.clone()) == Some(g.original.as_ref())
                }) {
                    return Err("Candidate changed. Review it again.".into());
                }
                let target = target
                    .unwrap_or_else(|| review.new_identity(review.group(group).unwrap().category));
                review
                    .identity(target)
                    .ok_or("Identity is no longer available.")?;
                assignments.push((range.clone(), target));
            }
            MappingAction::AssignApplied(id, target, all) => {
                let selected = review
                    .tracking
                    .get(id)
                    .ok_or("Replacement is no longer available.")?
                    .clone();
                let target = target.unwrap_or_else(|| review.new_identity(selected.step.category));
                review
                    .identity(target)
                    .ok_or("Identity is no longer available.")?;
                if all {
                    let group = review
                        .groups
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
        let mut plans: Vec<pseudonymization::tracking::IdentifiedPlan> = Vec::new();
        for a in &review.tracking.applied {
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
            let alias = if review.mode == Mode::Anonymize {
                identity.category.token()
            } else {
                &identity.alias
            };
            // An equal visible marker can still need a different identity/category.
            plans.push((
                (
                    a.range.clone(),
                    a.step.after.clone(),
                    alias.into(),
                    identity.category,
                ),
                target,
            ));
        }
        plans.sort_by_key(|p| p.0.0.start);
        Ok((plans, assignments))
    }
    fn apply_identity_aliases(&mut self, cx: &mut Context<Self>) {
        if self.pseudonymization.scanning() || !self.can_copy_markdown() {
            return;
        }
        let source = self.editor.read(cx).text().to_owned();
        let revision = self.editor.read(cx).revision();
        let mut plans: Vec<pseudonymization::tracking::IdentifiedPlan> = Vec::new();
        let review = &self.pseudonymization.review;
        for a in &review.tracking.applied {
            if source.get(a.range.clone()) != Some(a.step.after.as_ref()) {
                self.pseudonymization.error = Some("Replacement changed. Review it again.".into());
                cx.notify();
                return;
            }
            if let Some(identity) = review.identity(a.step.identity)
                && a.step.after.as_ref() != identity.alias
            {
                plans.push((
                    (
                        a.range.clone(),
                        a.step.after.clone(),
                        identity.alias.as_str().into(),
                        identity.category,
                    ),
                    identity.id,
                ));
            }
        }
        for candidate in &review.candidates {
            let Some(group) = review.group(candidate.group) else {
                return;
            };
            if source.get(candidate.range.clone()) != Some(group.original.as_ref()) {
                return;
            }
            let Some(identity) = review
                .occurrence_identity(group.id, &candidate.range)
                .and_then(|id| review.identity(id))
            else {
                return;
            };
            plans.push((
                (
                    candidate.range.clone(),
                    group.original.clone(),
                    identity.alias.as_str().into(),
                    identity.category,
                ),
                identity.id,
            ));
        }
        plans.sort_by_key(|p| p.0.0.start);
        if plans.windows(2).any(|p| p[0].0.0.end > p[1].0.0.start) {
            self.pseudonymization.error = Some("Candidates overlap. Review them again.".into());
            cx.notify();
            return;
        }
        let edits: Vec<_> = plans
            .iter()
            .map(|((range, _, after, _), _)| (range.clone(), after.to_string()))
            .collect();
        if !edits.is_empty() {
            let added = self
                .pseudonymization
                .review
                .tracking
                .prepare_corrections(&plans);
            if !self
                .editor
                .update(cx, |e, cx| e.replace_ranges(revision, &edits, cx))
            {
                return;
            }
            let tx = self.editor.read(cx).last_transaction().cloned().unwrap();
            self.pii_transaction(&tx, cx);
            self.pseudonymization
                .review
                .tracking
                .commit(self.editor.read(cx).history_id(), added);
        }
        if self.pseudonymization.review.mode != Mode::Pseudonymize {
            self.select_pii_mode(Mode::Pseudonymize, cx);
        }
        self.pseudonymization.review.open = true;
        self.pseudonymization.manual_review = true;
        self.pseudonymization.completion = Some((edits.len(), self.editor.read(cx).revision()));
        self.pseudonymization
            .review
            .refresh(self.editor.read(cx).text());
        self.pseudonymization.popup = None;
        self.sync_annotations(cx);
        cx.notify();
    }
    fn assign_selected_identity(&mut self, target: u64, cx: &mut Context<Self>) {
        if self.pseudonymization.mapping.scope == Scope::Entity {
            if let Some(from) = self.selected_identity() {
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
        window.focus(&self.pseudonymization.focus, cx);
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
