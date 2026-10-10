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
            .on_action(cx.listener(|this, _: &crate::PanelEscape, window, cx| {
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
}
