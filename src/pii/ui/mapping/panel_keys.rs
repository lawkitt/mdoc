//! Panel keyboard, panel selection, shared decision keys and triage focus
//! (ADR 0033).
use super::panel::PanelItem;
use super::*;
use gpui::prelude::*;

impl DocumentView {
    /// The mention whose inline controls show: selected from the panel, under
    /// the keyboard row, with no popup open.
    pub(super) fn inline_controls_for(&self) -> Option<u64> {
        let mapping = &self.pii.mapping;
        if !mapping.panel_selected || self.pii.popup.is_some() {
            return None;
        }
        match mapping.cursor {
            Some(PanelCursor::Mention(annotation))
                if self.active_annotation() == Some(annotation) =>
            {
                Some(annotation)
            }
            _ => None,
        }
    }

    /// Select a mention from the panel: the editor scrolls to and outlines it,
    /// inline controls replace the popup.
    pub(super) fn panel_select_mention(&mut self, annotation: u64, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        let start = if annotation & APPLIED_ID != 0 {
            review
                .applied_occurrence(annotation & !APPLIED_ID)
                .map(|a| a.range.start)
        } else {
            review.candidate(annotation).map(|c| c.range.start)
        };
        let Some(start) = start else {
            return;
        };
        self.pii.dismiss_popup();
        self.sync_replacement_annotation(annotation, cx);
        let mapping = &mut self.pii.mapping;
        mapping.panel_selected = true;
        mapping.cursor = Some(PanelCursor::Mention(annotation));
        mapping.set_scope(Scope::Mention);
        self.editor
            .update(cx, |e, cx| e.set_active_annotation(Some(annotation), cx));
        self.reveal_replacement_offset(start, cx);
        self.remember_active_replacement(cx);
        self.sync_scope_outlines(cx);
        self.reveal_cursor_row(cx);
        cx.notify();
    }

    fn reveal_cursor_row(&self, cx: &App) {
        let Some(cursor) = self.pii.mapping.cursor else {
            return;
        };
        if let Some(index) = self
            .replacement_entries(cx)
            .iter()
            .position(|entry| Self::cursor_of(entry) == Some(cursor))
        {
            self.pii
                .mapping
                .list_scroll
                .scroll_to_item(index, gpui::ScrollStrategy::Nearest);
        }
    }

    fn cursor_of((row, item): &super::panel::PanelEntry) -> Option<PanelCursor> {
        match item {
            PanelItem::Header => Some(PanelCursor::Header(row.id)),
            PanelItem::Mention(annotation, _) => Some(PanelCursor::Mention(*annotation)),
            PanelItem::Controls(_) => None,
        }
    }

    /// Move the keyboard row; landing on a mention selects it (D15).
    fn move_cursor(
        &mut self,
        target: impl FnOnce(usize, usize) -> Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows: Vec<PanelCursor> = self
            .replacement_entries(cx)
            .iter()
            .filter_map(Self::cursor_of)
            .collect();
        if rows.is_empty() {
            return;
        }
        let current = self
            .pii
            .mapping
            .cursor
            .and_then(|c| rows.iter().position(|r| *r == c));
        let Some(next) = current.map_or(Some(0), |i| target(i, rows.len())) else {
            // Up from the first row returns to search.
            let search = self.pii.mapping.search.read(cx).focus_handle(cx);
            window.focus(&search, cx);
            cx.notify();
            return;
        };
        self.set_cursor(rows[next], cx);
    }

    fn set_cursor(&mut self, cursor: PanelCursor, cx: &mut Context<Self>) {
        match cursor {
            PanelCursor::Mention(annotation) => self.panel_select_mention(annotation, cx),
            PanelCursor::Header(_) => {
                self.pii.mapping.cursor = Some(cursor);
                self.reveal_cursor_row(cx);
                cx.notify();
            }
        }
    }

    /// Apply, keep or undo the active mention in `scope`, as one undo step.
    /// Applied mentions only take Keep (as Undo, back to proposed). Shared by
    /// the panel and the popup (D12, D13).
    pub(in crate::pii::ui) fn decide(
        &mut self,
        scope: Scope,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(annotation) = self.active_annotation() else {
            return false;
        };
        if annotation & APPLIED_ID != 0 && !keep {
            return false;
        }
        self.pii.mapping.set_scope(scope);
        if annotation & APPLIED_ID != 0 {
            let (_, applied) = self.scoped_mentions();
            self.undo_replacements(applied.into_iter().collect(), cx);
        } else if keep {
            self.keep_originals(window, cx);
        } else {
            self.apply_scope(cx);
        }
        true
    }

    /// A decision on the panel's keyboard row, then triage focus (D14).
    pub(super) fn panel_decide(
        &mut self,
        scope: Scope,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let after = match self.pii.mapping.cursor {
            Some(PanelCursor::Mention(annotation)) => {
                if self.active_annotation() != Some(annotation) {
                    self.panel_select_mention(annotation, cx);
                }
                self.active_replacement_range().map(|r| r.start)
            }
            // A header takes only the whole-entity keys.
            Some(PanelCursor::Header(id)) if scope == Scope::Entity => {
                let Some((annotation, range)) = self.identity_occurrences(id).into_iter().next()
                else {
                    return;
                };
                self.panel_select_mention(annotation, cx);
                self.pii.mapping.cursor = Some(PanelCursor::Header(id));
                Some(range.start)
            }
            _ => return,
        };
        if self.decide(scope, keep, window, cx) {
            // Keep hands focus to the editor for the popup; the panel keeps it.
            window.focus(&self.pii.mapping.focus, cx);
            self.triage_next(after.unwrap_or(0), cx);
        }
    }

    /// Focus the next undecided mention after `offset` in document order,
    /// opening its group; groups opened this way close once fully decided.
    pub(super) fn triage_next(&mut self, offset: usize, cx: &mut Context<Self>) {
        let review = &self.pii.review;
        let pending: Vec<_> = review
            .candidates()
            .iter()
            .filter_map(|c| {
                Some((
                    c.id,
                    c.range.start,
                    review.occurrence_identity(c.variant, &c.range)?,
                ))
            })
            .collect();
        let mapping = &mut self.pii.mapping;
        let open: std::collections::HashSet<_> = pending.iter().map(|(_, _, id)| *id).collect();
        let done: Vec<_> = mapping
            .triage_expanded
            .iter()
            .copied()
            .filter(|id| !open.contains(id))
            .collect();
        for id in done {
            mapping.triage_expanded.remove(&id);
            mapping.expanded.remove(&id);
        }
        let next = pending
            .iter()
            .find(|(_, start, _)| *start > offset)
            .or_else(|| pending.first())
            .copied();
        match next {
            Some((annotation, _, identity)) => {
                if mapping.expanded.insert(identity) {
                    mapping.triage_expanded.insert(identity);
                }
                self.panel_select_mention(annotation, cx);
            }
            None => {
                mapping.cursor = None;
                mapping.panel_selected = false;
                cx.notify();
            }
        }
    }

    /// The panel list's keyboard and the shared decision keys.
    pub(super) fn panel_actions(
        &self,
        panel: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        panel
            .on_action(cx.listener(|this, _: &crate::PanelDown, window, cx| {
                this.move_cursor(|i, n| Some((i + 1).min(n - 1)), window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PanelUp, window, cx| {
                this.move_cursor(|i, _| i.checked_sub(1), window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PanelFirst, window, cx| {
                this.move_cursor(|_, _| Some(0), window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PanelLast, window, cx| {
                this.move_cursor(|_, n| Some(n - 1), window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PanelEnterList, window, cx| {
                window.focus(&this.pii.mapping.focus, cx);
                if this.pii.mapping.cursor.is_none() {
                    this.move_cursor(|_, _| Some(0), window, cx);
                }
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &crate::PanelMoveUp, _, cx| this.step_key_move(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &crate::PanelMoveDown, _, cx| this.step_key_move(true, cx)),
            )
            .on_action(cx.listener(|this, _: &crate::PanelEscape, window, cx| {
                cx.stop_propagation();
                // Esc while ⌥ is held cancels a keyboard move.
                if this.pii.mapping.key_moving {
                    let mapping = &mut this.pii.mapping;
                    mapping.key_moving = false;
                    mapping.dragging = None;
                    mapping.drop_target = None;
                    cx.notify();
                    return;
                }
                // Esc in the list returns to search; from search it closes.
                let search = this.pii.mapping.search.read(cx).focus_handle(cx);
                if search.is_focused(window) {
                    this.close_replacements(window, cx);
                } else {
                    window.focus(&search, cx);
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::ApplyThis, window, cx| {
                if let Some(PanelCursor::Header(id)) = this.pii.mapping.cursor {
                    this.pii.mapping.triage_expanded.remove(&id);
                    this.toggle_group(id, cx);
                } else {
                    this.panel_decide(Scope::Mention, false, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::ApplySame, window, cx| {
                this.panel_decide(Scope::Wording, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::ApplyAll, window, cx| {
                this.panel_decide(Scope::Entity, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::KeepThis, window, cx| {
                this.panel_decide(Scope::Mention, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::KeepSame, window, cx| {
                this.panel_decide(Scope::Wording, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::KeepAll, window, cx| {
                this.panel_decide(Scope::Entity, true, window, cx)
            }))
    }

    /// The same decision keys in the popup, on its mention.
    pub(super) fn popup_decision_actions(
        &self,
        popup: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        popup
            .on_action(cx.listener(|this, _: &crate::ApplyThis, window, cx| {
                this.decide(Scope::Mention, false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::ApplySame, window, cx| {
                this.decide(Scope::Wording, false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::ApplyAll, window, cx| {
                this.decide(Scope::Entity, false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::KeepThis, window, cx| {
                this.decide(Scope::Mention, true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::KeepSame, window, cx| {
                this.decide(Scope::Wording, true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::KeepAll, window, cx| {
                this.decide(Scope::Entity, true, window, cx);
                cx.stop_propagation();
            }))
    }

    /// What a keyboard move of the current row carries: a mention, or the
    /// whole entity for a header.
    fn cursor_drag(&self, cx: &App) -> Option<PanelDrag> {
        match self.pii.mapping.cursor? {
            PanelCursor::Header(id) => {
                let original = self
                    .replacement_entries(cx)
                    .iter()
                    .find(|(row, _)| row.id == id)?
                    .0
                    .original
                    .clone();
                Some(PanelDrag::Entity {
                    identity: id,
                    original,
                })
            }
            PanelCursor::Mention(annotation) => {
                let review = &self.pii.review;
                let (identity, range, original) = if annotation & APPLIED_ID != 0 {
                    let a = review.applied_occurrence(annotation & !APPLIED_ID)?;
                    (
                        a.step.identity,
                        a.range.clone(),
                        a.step.original_shared().clone(),
                    )
                } else {
                    let c = review.candidate(annotation)?;
                    (
                        review.occurrence_identity(c.variant, &c.range)?,
                        c.range.clone(),
                        review.variant(c.variant)?.original.clone(),
                    )
                };
                Some(PanelDrag::Mention {
                    identity,
                    range,
                    original,
                })
            }
        }
    }

    /// ⌥↑/↓: start a keyboard move on the current row, or step its target
    /// through the groups, then "New alias" for a mention of a multi-mention
    /// entity. The ends stop rather than wrap (ADR 0033).
    fn step_key_move(&mut self, down: bool, cx: &mut Context<Self>) {
        if !self.pii.mapping.key_moving {
            let Some(drag) = self.cursor_drag(cx) else {
                return;
            };
            let mapping = &mut self.pii.mapping;
            mapping.drop_target = Some(DropTarget::Entity(drag.identity()));
            mapping.dragging = Some(drag);
            mapping.key_moving = true;
        }
        let Some(drag) = self.pii.mapping.dragging.clone() else {
            return;
        };
        let entries = self.replacement_entries(cx);
        let mut targets: Vec<(DropTarget, Option<usize>)> = entries
            .iter()
            .enumerate()
            .filter(|(_, (_, item))| matches!(item, PanelItem::Header))
            .map(|(index, (row, _))| (DropTarget::Entity(row.id), Some(index)))
            .collect();
        if matches!(drag, PanelDrag::Mention { .. })
            && self.pii.review.identity_count(drag.identity()) > 1
        {
            targets.push((DropTarget::NewEntity, None));
        }
        let current = targets
            .iter()
            .position(|(t, _)| Some(*t) == self.pii.mapping.drop_target)
            .unwrap_or(0);
        let next = if down {
            (current + 1).min(targets.len() - 1)
        } else {
            current.saturating_sub(1)
        };
        let (target, index) = targets[next];
        self.pii.mapping.drop_target = Some(target);
        if let Some(index) = index {
            self.pii
                .mapping
                .list_scroll
                .scroll_to_item(index, gpui::ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Releasing ⌥ drops the keyboard move like a mouse drop, as one step.
    pub(super) fn finish_key_move(&mut self, cx: &mut Context<Self>) {
        let mapping = &mut self.pii.mapping;
        mapping.key_moving = false;
        let (Some(drag), Some(target)) = (mapping.dragging.take(), mapping.drop_target.take())
        else {
            cx.notify();
            return;
        };
        match target {
            DropTarget::Entity(id) => self.drop_on_entity(&drag, id, cx),
            DropTarget::NewEntity => self.drop_as_new_entity(&drag, cx),
        }
    }

    /// Middle click: keep the original of `annotation` (⌘ same text, ⇧⌘ all;
    /// Undo when applied). From the panel it is a triage decision.
    pub(crate) fn middle_click_mention(
        &mut self,
        annotation: u64,
        modifiers: gpui::Modifiers,
        panel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = match (modifiers.secondary(), modifiers.shift) {
            (true, true) => Scope::Entity,
            (true, false) => Scope::Wording,
            _ => Scope::Mention,
        };
        if panel {
            self.panel_select_mention(annotation, cx);
            self.panel_decide(scope, true, window, cx);
        } else {
            self.pii.dismiss_popup();
            self.sync_replacement_annotation(annotation, cx);
            self.decide(scope, true, window, cx);
        }
    }

    /// Over the list, a mouse wheel (line deltas) steps the keyboard row one
    /// notch at a time; trackpads (pixel deltas) scroll as usual (ADR 0033).
    pub(super) fn wheel_steps(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let weak = weak.clone();
                window.on_mouse_event(move |e: &gpui::ScrollWheelEvent, phase, window, cx| {
                    let gpui::ScrollDelta::Lines(delta) = e.delta else {
                        return;
                    };
                    if phase != gpui::DispatchPhase::Capture
                        || delta.y == 0.
                        || !bounds.contains(&e.position)
                    {
                        return;
                    }
                    cx.stop_propagation();
                    let _ = weak.update(cx, |this, cx| {
                        window.focus(&this.pii.mapping.focus, cx);
                        // Positive deltas scroll toward the top.
                        if delta.y > 0. {
                            this.move_cursor(|i, _| Some(i.saturating_sub(1)), window, cx);
                        } else {
                            this.move_cursor(|i, n| Some((i + 1).min(n - 1)), window, cx);
                        }
                    });
                });
            },
        )
        .absolute()
        .inset_0()
    }
}
