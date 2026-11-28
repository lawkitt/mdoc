//! Identity review and direct correction. All mutation uses editor history.
use super::Popup;
use crate::review::text;
use crate::{document::DocumentView, document::markdown_search};
#[cfg(test)]
use gpui::Window;
use gpui::{AppContext, Context, Entity, FocusHandle, Pixels, px};
use mdoc_pii::{Category, tracking::ReplacementPlan};
use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

mod actions;
mod checklist;
mod merge_hint;
mod panel;
mod panel_keys;
mod panel_rows;
mod popup;
mod selection_menu;
mod share;
pub(super) use popup::Scope;
mod popup_render;

/// What the workspace popup edits: a whole entity after a mapping change, or
/// one exact pending or applied occurrence.
#[derive(Clone)]
pub(super) enum Selection {
    Entity(u64),
    Candidate {
        variant: u64,
        range: Range<usize>,
    },
    Applied(u64),
    /// A kept mention: its entity, range and wording.
    Kept {
        identity: u64,
        range: Range<usize>,
        original: Arc<str>,
    },
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

/// A manual addition while it (and an immediate Apply) is the latest history.
#[derive(Clone)]
pub(super) struct Added {
    pub original: Arc<str>,
    after_add: u64,
    after_apply: Option<u64>,
}
/// A panel row being dragged: one mention, or a whole entity.
#[derive(Clone)]
pub(super) enum PanelDrag {
    Mention {
        identity: u64,
        range: Range<usize>,
        original: Arc<str>,
    },
    Entity {
        identity: u64,
        original: Arc<str>,
    },
}
impl PanelDrag {
    pub(super) fn identity(&self) -> u64 {
        match self {
            Self::Mention { identity, .. } | Self::Entity { identity, .. } => *identity,
        }
    }
}
/// Where a panel drag would drop.
/// Which panel rows the filter shows; the editor always shows all.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PanelFilter {
    #[default]
    All,
    Proposed,
    Applied,
    /// Entities unticked in this session: their originals stay.
    Kept,
}
impl PanelFilter {
    pub(super) const ALL: [Self; 4] = [Self::All, Self::Proposed, Self::Applied, Self::Kept];
    pub(super) fn shows(self, applied: bool) -> bool {
        match self {
            Self::All => true,
            Self::Proposed => !applied,
            Self::Applied => applied,
            Self::Kept => false,
        }
    }
}

/// The entity a kept wording belonged to, recorded when it was unticked,
/// for when the review no longer links the two.
#[derive(Clone, Debug)]
pub(super) struct KeptName {
    pub identity: u64,
    pub alias: String,
    pub category: Category,
}
/// The keyboard row in the panel list: a group header or a mention.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PanelCursor {
    Header(u64),
    Mention(u64),
}
/// A dropdown floating from the panel's toolbar or a group header.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelMenu {
    Filter,
    Category(u64),
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum DropTarget {
    Entity(u64),
    NewEntity,
}

#[cfg(test)]
mod decision_tests;
#[cfg(test)]
mod tests;
/// Replacements panel and word-popup state. Every transition is a method here.
pub(super) struct MappingUi {
    pub open: bool,
    projection: std::cell::RefCell<Option<Arc<panel::PanelProjection>>>,
    entries_cache: std::cell::RefCell<Option<panel::EntriesCache>>,
    scope: Scope,
    pub(super) pickers: Pickers,
    pub(super) target_index: Option<usize>,
    pub(super) target_scroll: gpui::UniformListScrollHandle,
    field_error: Option<String>,
    remembered: std::collections::HashMap<u64, usize>,
    /// The popup's mention as (entity, index among its live and kept
    /// mentions): undo and redo flip kept and live without reordering them.
    active_place: Option<(u64, usize)>,
    bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    /// The open floating menu, which may hang below the popup card.
    menu_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    pub(super) popup_revision: Option<u64>,
    /// Mentions kept by the last Keep and the history state it created.
    kept: Option<(usize, u64)>,
    /// The panel mention row under the pointer.
    pub(super) hovered: Option<u64>,
    /// The latest manual addition, for Cancel addition.
    added: Option<Added>,
    /// A popup scope chip under the pointer previews its outline.
    pub(super) scope_hover: Option<Scope>,
    /// The panel row being dragged and the row it would drop on.
    pub(super) dragging: Option<PanelDrag>,
    pub(super) drop_target: Option<DropTarget>,
    /// The drag pointer is over the panel (shows the new-alias area).
    pub(super) drag_in_panel: bool,
    /// The alias cue to play: (generation, include the category chip).
    pub(super) cue: Option<(u64, bool)>,
    cue_generation: u64,
    /// The first detected-mention popup in this tab already played the cue.
    cued_detected: bool,
    selected: Option<Selection>,
    search: Entity<markdown_search::SearchInput>,
    pub(super) alias: Entity<markdown_search::SearchInput>,
    target: Entity<markdown_search::SearchInput>,
    /// Mentions applied by the last Apply replacements and the state it created.
    applied: Option<(usize, u64)>,
    /// Groups expanded in the panel, independent of selection.
    pub(super) expanded: std::collections::HashSet<u64>,
    /// Kept wordings and their entity, recorded at untick.
    pub(super) kept_names: std::collections::HashMap<Arc<str>, KeptName>,
    pub(super) filter: PanelFilter,
    pub(super) cursor: Option<PanelCursor>,
    /// A ⌥↑/↓ move is held: `dragging`/`drop_target` describe it.
    pub(super) key_moving: bool,
    /// The selection came from the panel: no popup.
    pub(super) panel_selected: bool,
    pub(super) panel_menu: Option<PanelMenu>,
    /// The group whose alias is being renamed in place, and its draft.
    pub(super) header_edit: Option<u64>,
    pub(super) header_alias: Entity<markdown_search::SearchInput>,
    /// Hides the footer's after-action notice once it times out.
    notice: Option<gpui::Task<()>>,
    previous_focus: Option<FocusHandle>,
    scroll: gpui::ScrollHandle,
    list_scroll: gpui::UniformListScrollHandle,
    pub(super) focus: FocusHandle,
    /// The header's expand/collapse-all toggle.
    pub(super) expand_all_focus: FocusHandle,
    /// The popup's open category or owner menu, which holds focus while
    /// open.
    pub(super) popup_menu_focus: FocusHandle,
    /// The panel's filter button and its menu, likewise.
    pub(super) filter_focus: FocusHandle,
    pub(super) filter_menu_focus: FocusHandle,
    /// Merge hints dismissed in this tab or its Project, by the wordings that
    /// matched. Forgetting removes affected pairs and clears caches.
    dismissed_hints: std::collections::HashSet<(Arc<str>, Arc<str>)>,
    /// The fuller wording of the last hint Merge and the state it created.
    merged: Option<(Arc<str>, u64)>,
    /// Each shown hint's Merge and × Tab stops, by (from, into) identity.
    pub(super) hint_focus:
        std::cell::RefCell<std::collections::HashMap<(u64, u64), [FocusHandle; 2]>>,
}
impl MappingUi {
    pub(super) fn invalidate_source_edit(&mut self, cx: &mut Context<DocumentView>) {
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
        self.target_index = None;
        self.field_error = None;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
    }
    #[cfg(test)]
    pub(super) fn scope(&self) -> Scope {
        self.scope
    }
    pub(super) fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }
    // Pickers float over the popup, so only one is open at a time.
    pub(super) fn toggle_category_picker(&mut self) {
        let open = !self.pickers.category;
        self.pickers = Pickers::default();
        self.pickers.category = open;
    }
    pub(super) fn toggle_owner_picker(&mut self) {
        let open = !self.pickers.owner;
        self.pickers = Pickers::default();
        self.pickers.owner = open;
    }
    pub(super) fn show_alias_choices(&mut self) {
        self.pickers = Pickers::default();
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
    /// Track the hovered mention row; true when it changed.
    pub(super) fn hover_mention(&mut self, annotation: u64, hovered: bool) -> bool {
        let next = if hovered {
            Some(annotation)
        } else if self.hovered == Some(annotation) {
            None
        } else {
            self.hovered
        };
        std::mem::replace(&mut self.hovered, next) != next
    }
    pub(super) fn record_kept(&mut self, count: usize, history: u64) {
        self.kept = Some((count, history));
    }
    pub(super) fn record_merged(&mut self, wording: Arc<str>, history: u64) {
        self.merged = Some((wording, history));
    }
    /// The wording the most recent history step merged into, while current.
    pub(super) fn merged_at(&self, history: u64) -> Option<&Arc<str>> {
        self.merged
            .as_ref()
            .filter(|(_, at)| *at == history)
            .map(|(wording, _)| wording)
    }
    /// Hide the hint for `pair`; false when it was hidden already.
    pub(super) fn dismiss_hint(&mut self, pair: (Arc<str>, Arc<str>)) -> bool {
        let pair = if pair.0 <= pair.1 {
            pair
        } else {
            (pair.1, pair.0)
        };
        self.dismissed_hints.insert(pair)
    }
    pub(super) fn hint_dismissed(&self, pair: &(Arc<str>, Arc<str>)) -> bool {
        self.dismissed_hints.contains(pair)
            || self
                .dismissed_hints
                .contains(&(pair.1.clone(), pair.0.clone()))
    }
    pub(super) fn dismissed_hint_count(&self) -> usize {
        self.dismissed_hints.len()
    }
    /// Irreversible forgetting also discards hint wordings and cached rows;
    /// moving this tab must not carry a forgotten name into a new Project.
    pub(super) fn forget_project_originals(&mut self, originals: &[String]) {
        let forgotten = |wording: &Arc<str>| originals.iter().any(|o| o == wording.as_ref());
        self.dismissed_hints
            .retain(|(from, into)| !forgotten(from) && !forgotten(into));
        if self
            .merged
            .as_ref()
            .is_some_and(|(wording, _)| forgotten(wording))
        {
            self.merged = None;
        }
        self.projection.get_mut().take();
        self.entries_cache.get_mut().take();
    }
    /// Every dismissed pair, sorted, for saving into a new Project.
    pub(super) fn dismissed_hint_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<_> = self
            .dismissed_hints
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        pairs.sort_unstable();
        pairs
    }
    pub(super) fn record_applied(&mut self, count: usize, history: u64) {
        self.applied = Some((count, history));
    }
    /// Mentions applied by the most recent history step, while it is current.
    pub(super) fn applied_at(&self, history: u64) -> Option<usize> {
        self.applied
            .filter(|(_, at)| *at == history)
            .map(|(count, _)| count)
    }
    /// The after-action notice is showing (its timer has not fired).
    pub(super) fn notice_visible(&self) -> bool {
        self.notice.is_some()
    }
    /// Mentions kept by the most recent history step, while it is current.
    pub(super) fn kept_at(&self, history: u64) -> Option<usize> {
        self.kept
            .filter(|(_, at)| *at == history)
            .map(|(count, _)| count)
    }
    pub(super) fn record_added(&mut self, original: Arc<str>, history: u64) {
        self.added = Some(Added {
            original,
            after_add: history,
            after_apply: None,
        });
    }
    /// An Apply right after the addition joins what Cancel addition reverts.
    pub(super) fn record_apply(&mut self, before: u64, after: u64) {
        if let Some(added) = &mut self.added
            && added.after_apply.is_none()
            && added.after_add == before
        {
            added.after_apply = Some(after);
        }
    }
    /// The addition and how many undo steps cancel it, while still current.
    pub(super) fn added_at(&self, history: u64) -> Option<(&Added, usize)> {
        let added = self.added.as_ref()?;
        if added.after_apply == Some(history) {
            Some((added, 2))
        } else if added.after_add == history && added.after_apply.is_none() {
            Some((added, 1))
        } else {
            None
        }
    }
    pub(super) fn clear_added(&mut self) {
        self.added = None;
    }
    /// Play the alias cue for a manual addition, or once per tab for a
    /// detected mention; `chip` also cues the category chip.
    pub(super) fn request_cue(&mut self, manual: bool, chip: bool) {
        if !manual && std::mem::replace(&mut self.cued_detected, true) {
            self.cue = None;
            return;
        }
        self.cue_generation += 1;
        self.cue = Some((self.cue_generation, chip));
    }
    pub(super) fn mark_popup_revision(&mut self, revision: u64) {
        self.popup_revision = Some(revision);
    }
    pub fn new(
        theme: std::rc::Rc<std::cell::Cell<crate::ui::style::Theme>>,
        cx: &mut Context<DocumentView>,
    ) -> Self {
        let mut input = |placeholder| {
            let input = cx.new(|cx| {
                markdown_search::SearchInput::new(cx)
                    .with_theme(theme.clone())
                    .with_key_context(match placeholder {
                        text::ALIAS => "PseudonymReplacement",
                        text::FIND_ALIAS_OR_ORIGINAL => "ReplacementSearch",
                        _ => "SettingsInput",
                    })
                    .with_placeholder(placeholder)
            });
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            if placeholder == text::ALIAS {
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
            projection: Default::default(),
            entries_cache: Default::default(),
            scope: Scope::Entity,
            pickers: Pickers::default(),
            target_index: None,
            target_scroll: gpui::UniformListScrollHandle::new(),
            field_error: None,
            remembered: Default::default(),
            active_place: None,
            bounds: Rc::new(Cell::new(None)),
            menu_bounds: Rc::new(Cell::new(None)),
            popup_revision: None,
            kept: None,
            hovered: None,
            added: None,
            scope_hover: None,
            dragging: None,
            drop_target: None,
            drag_in_panel: false,
            cue: None,
            cue_generation: 0,
            cued_detected: false,
            selected: None,
            search: input(text::FIND_ALIAS_OR_ORIGINAL),
            alias: input(text::ALIAS),
            target: input(text::FIND_AN_IDENTITY),
            applied: None,
            expanded: Default::default(),
            kept_names: Default::default(),
            filter: PanelFilter::All,
            cursor: None,
            key_moving: false,
            panel_selected: false,
            panel_menu: None,
            header_edit: None,
            header_alias: input(text::NEW_ALIAS),
            notice: None,
            previous_focus: None,
            scroll: gpui::ScrollHandle::new(),
            list_scroll: gpui::UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            expand_all_focus: cx.focus_handle().tab_stop(true),
            popup_menu_focus: cx.focus_handle(),
            filter_focus: cx.focus_handle().tab_stop(true),
            filter_menu_focus: cx.focus_handle(),
            dismissed_hints: Default::default(),
            merged: None,
            hint_focus: Default::default(),
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
/// Pending mentions a mapping change also applies, as source ranges.
#[derive(Clone)]
pub(super) enum Applying {
    Nothing,
    All,
    Ranges(std::collections::HashSet<(usize, usize)>),
}
/// Text corrections and occurrence assignments staged by one mapping change.
struct MappingChange {
    plans: Vec<ReplacementPlan>,
    assignments: Vec<(Range<usize>, u64)>,
}
