//! What the Replacements panel does: opening Settings for its model,
//! drops on an identity or a new one, undo and redo, expanding and
//! collapsing identities, and renaming one from its row.
use super::{DropTarget, MappingAction, PanelDrag, PanelItem, PanelMenu};
use crate::document::DocumentView;
use crate::review::{KEPT_ID, mapping::PanelCursor};
use gpui::{Context, Focusable, Window};
use mdoc_pii::Category;
use mdoc_store::settings;

impl DocumentView {
    pub(super) fn show_pii_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = self
            .preferences
            .borrow()
            .snapshot()
            .map(|p| p.pseudonymization.model)
            .unwrap_or_default();
        self.model_panel.update(cx, |panel, cx| {
            panel.show_section(settings::Model::Pii(model), window, cx)
        });
    }

    /// A panel drop leaves the popup as it was: a mapping change otherwise
    /// reopens it on the previously active mention.
    fn after_drop(&mut self, had_popup: bool, cx: &mut Context<Self>) {
        if !had_popup && self.pii.popup.is_some() {
            self.pii.dismiss_popup();
            self.editor
                .update(cx, |e, cx| e.set_active_annotation(None, cx));
        }
    }
    /// Drop a dragged row on a group: link one mention, or merge an entity.
    pub(in crate::review::mapping) fn drop_on_entity(
        &mut self,
        drag: &PanelDrag,
        target: u64,
        cx: &mut Context<Self>,
    ) {
        self.pii.mapping.dragging = None;
        self.pii.mapping.drop_target = None;
        self.pii.mapping.drag_in_panel = false;
        let had_popup = self.pii.popup.is_some();
        match drag.clone() {
            PanelDrag::Mention {
                identity, range, ..
            } if identity != target => self.change_mapping(
                MappingAction::Scoped {
                    identity,
                    ranges: vec![range],
                    target: Some(target),
                    alias: None,
                    category: None,
                },
                cx,
            ),
            PanelDrag::Entity { identity, .. } if identity != target => {
                self.change_mapping(MappingAction::Merge(identity, target), cx)
            }
            _ => {}
        }
        self.after_drop(had_popup, cx);
        cx.notify();
    }
    /// Drop a dragged mention on "New entity": separate it with a new alias.
    pub(in crate::review::mapping) fn drop_as_new_entity(
        &mut self,
        drag: &PanelDrag,
        cx: &mut Context<Self>,
    ) {
        self.pii.mapping.dragging = None;
        self.pii.mapping.drop_target = None;
        self.pii.mapping.drag_in_panel = false;
        let had_popup = self.pii.popup.is_some();
        if let PanelDrag::Mention {
            identity, range, ..
        } = drag.clone()
            && self.pii.review.identity_count(identity) > 1
        {
            self.change_mapping(
                MappingAction::Scoped {
                    identity,
                    ranges: vec![range],
                    target: None,
                    alias: None,
                    category: None,
                },
                cx,
            );
        }
        self.after_drop(had_popup, cx);
        cx.notify();
    }
    /// Track the row under a drag so it can show "Link to ALIAS".
    pub(in crate::review::mapping) fn drag_over_target(
        &mut self,
        target: DropTarget,
        cx: &mut Context<Self>,
    ) {
        if self.pii.mapping.drop_target != Some(target) {
            self.pii.mapping.drop_target = Some(target);
            cx.notify();
        }
    }

    /// Document undo/redo without leaving the panel.
    pub(in crate::review::mapping) fn undo_last_step(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |e, cx| e.undo_step(window, cx));
    }
    pub(in crate::review::mapping) fn redo_last_step(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |e, cx| e.redo_step(window, cx));
    }

    /// Reveal a mention from the panel and open its popup there:
    /// the panel's own scroll lands it in view, then the popup anchors to it.
    pub(in crate::review::mapping) fn open_from_panel(
        &mut self,
        annotation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A narrow window showing the source switches back to the Markdown.
        self.original_selected = false;
        self.panel_select_mention(annotation, cx);
        self.activate_annotation(annotation, window, cx);
    }

    /// Expand or collapse every identity the panel lists. A mention under
    /// the keyboard cursor hands it to its identity's row when collapsing.
    pub(crate) fn set_all_identities_expanded(&mut self, expand: bool, cx: &mut Context<Self>) {
        let entries = self.replacement_entries(cx);
        if expand {
            let ids = entries
                .iter()
                .filter(|(_, item)| matches!(item, PanelItem::Header))
                .map(|(row, _)| row.id);
            self.pii.mapping.expanded.extend(ids);
        } else {
            if let Some(PanelCursor::Mention(annotation)) = self.pii.mapping.cursor
                && let Some((row, _)) = entries
                    .iter()
                    .find(|(_, item)| item.mention() == Some(annotation))
            {
                self.pii.mapping.cursor = Some(PanelCursor::Header(row.id));
            }
            self.pii.mapping.expanded.clear();
        }
        cx.notify();
    }

    /// Expand or collapse one group; nothing is selected.
    pub(in crate::review::mapping) fn toggle_group(&mut self, id: u64, cx: &mut Context<Self>) {
        let expanded = &mut self.pii.mapping.expanded;
        if !expanded.remove(&id) {
            expanded.insert(id);
        }
        cx.notify();
    }

    pub(in crate::review::mapping) fn edit_header_alias(
        &mut self,
        id: u64,
        alias: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.pii.mapping.header_alias.clone();
        input.update(cx, |input, cx| input.set_value(alias, cx));
        self.pii.mapping.header_edit = Some(id);
        window.focus(&input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Rename the whole entity from its header; a rejected alias keeps editing.
    pub(in crate::review::mapping) fn commit_header_alias(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self
            .pii
            .mapping
            .header_alias
            .read(cx)
            .value()
            .trim()
            .to_owned();
        if self
            .pii
            .review
            .identity(id)
            .is_some_and(|i| i.alias != value)
        {
            self.change_mapping(MappingAction::Rename(id, value), cx);
            if self.pii.error.is_some() {
                cx.notify();
                return;
            }
        }
        self.cancel_header_alias(window, cx);
    }

    pub(in crate::review::mapping) fn cancel_header_alias(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pii.mapping.header_edit = None;
        window.focus(&self.pii.mapping.focus, cx);
        cx.notify();
    }

    /// A right-click on an identity's row: open or close its category menu.
    pub(in crate::review::mapping) fn toggle_category_menu(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let menu = &mut self.pii.mapping.panel_menu;
        *menu = (*menu != Some(PanelMenu::Category(id))).then_some(PanelMenu::Category(id));
        // Escape reaches the menu from the list; an open popup keeps
        // its focus and closes the menu first (#92).
        let popup = &self.pii.focus;
        if !(popup.is_focused(window) || popup.contains_focused(window, cx)) {
            window.focus(&self.pii.mapping.focus, cx);
        }
        cx.notify();
    }

    /// A click on an identity's row: open it on its first mention, or fold it.
    pub(in crate::review::mapping) fn toggle_identity_row(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pii.mapping.cursor = Some(PanelCursor::Header(id));
        if self.pii.mapping.expanded.remove(&id) {
            self.pii.dismiss_popup();
            self.pii.mapping.panel_selected = false;
            window.focus(&self.pii.mapping.focus, cx);
            cx.notify();
            return;
        }
        self.pii.mapping.expanded.insert(id);
        match self.identity_occurrences(id).into_iter().next() {
            Some((annotation, _)) => {
                self.open_from_panel(annotation, window, cx);
                self.pii.mapping.cursor = Some(PanelCursor::Header(id));
            }
            None => window.focus(&self.pii.mapping.focus, cx),
        }
        cx.notify();
    }

    /// A click on a kept mention's row: its popup opens on it, unticked.
    pub(in crate::review::mapping) fn open_kept_row(
        &mut self,
        start: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Like a kept marker in the text: the popup opens on it, unticked.
        self.original_selected = false;
        self.reveal_replacement_offset(start, cx);
        self.activate_annotation(KEPT_ID | start as u64, window, cx);
        cx.notify();
    }

    /// A pick in an identity's category menu: close it and change the category.
    pub(in crate::review::mapping) fn pick_identity_category(
        &mut self,
        id: u64,
        category: Category,
        current: Category,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.pii.mapping.panel_menu = None;
        if category != current {
            self.change_mapping(MappingAction::Category(id, category), cx);
        }
        cx.notify();
    }
}
