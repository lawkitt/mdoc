//! Searchable replacement overview with inline occurrence context.
use super::{DropTarget, MappingAction, PanelDrag, PanelFilter, PanelMenu, checklist::Tick};
use crate::{document_view::DocumentView, pii::ui::APPLIED_ID, settings_ui, style, ui};
use gpui::{
    AnyElement, App, Context, Hsla, SharedString, Window, div, prelude::*, px, uniform_list,
};
use mdoc_pii::Category;
use mdoc_store::settings;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

#[derive(Clone)]
pub(super) struct ReplacementRow {
    pub id: u64,
    pub original: Arc<str>,
    pub alias: String,
    pub category: Category,
    pub mentions: usize,
    pub applied: usize,
    /// Mentions kept as originals (unticked, ADR 0048).
    pub kept: usize,
    search_original: String,
    search_alias: String,
}

impl ReplacementRow {
    pub(super) fn tick(&self) -> Tick {
        match (self.mentions > 0, self.kept > 0) {
            (true, false) => Tick::On,
            (true, true) => Tick::Mixed,
            _ => Tick::Off,
        }
    }
    fn visible(&self, query: &str, filter: PanelFilter) -> bool {
        let shown = match filter {
            PanelFilter::All => true,
            PanelFilter::Proposed => self.mentions > self.applied,
            PanelFilter::Applied => self.applied > 0,
            PanelFilter::Kept => self.kept > 0,
        };
        shown
            && (query.is_empty()
                || self.search_original.contains(query)
                || self.search_alias.contains(query))
    }
}

/// Immutable derived data. A review/text revision rebuilds it once; expansion,
/// filtering, selection and hover do not rescan document occurrences.
pub(super) struct PanelProjection {
    revision: (u64, u64),
    rows: Vec<Arc<ReplacementRow>>,
    members: HashMap<u64, Vec<PanelItem>>,
    active_aliases: HashSet<String>,
    kept: usize,
}
pub(super) struct EntriesCache {
    revision: (u64, u64),
    filter: PanelFilter,
    query: String,
    expanded: HashSet<u64>,
    entries: Arc<[PanelEntry]>,
}

/// Every panel row is this tall, so the list stays a uniform list.
pub(super) const ROW_HEIGHT: f32 = 34.;
/// One panel line: an entity row, or one of the expanded entity's mentions.
pub(super) type PanelEntry = (Arc<ReplacementRow>, PanelItem);
#[derive(Clone)]
pub(super) enum PanelItem {
    /// A category's label above its group headers: entities and mentions shown
    /// (ADR 0046). Not focusable; navigation skips it.
    Category {
        entities: usize,
        mentions: usize,
    },
    Header,
    Mention(u64, Range<usize>),
    /// A mention whose original is kept: listed, unticked (ADR 0048).
    Kept(Range<usize>),
}
impl PanelItem {
    pub(super) fn mention(&self) -> Option<u64> {
        match self {
            Self::Mention(annotation, _) => Some(*annotation),
            _ => None,
        }
    }
}

impl DocumentView {
    fn replacement_projection(&self, cx: &App) -> Arc<PanelProjection> {
        let mapping = &self.pii.mapping;
        let revision = (self.pii.review.revision(), self.editor.read(cx).revision());
        if let Some(cached) = mapping.projection.borrow().as_ref()
            && cached.revision == revision
        {
            return cached.clone();
        }
        let review = &self.pii.review;
        let mut rows = BTreeMap::<u64, ReplacementRow>::new();
        let mut members = HashMap::<u64, Vec<PanelItem>>::new();
        let mut add = |id, original: Arc<str>, applied: Option<bool>| {
            let Some(identity) = review.identity(id) else {
                return;
            };
            let row = rows.entry(id).or_insert_with(|| ReplacementRow {
                id,
                original,
                alias: identity.alias.clone(),
                category: identity.category,
                mentions: 0,
                applied: 0,
                kept: 0,
                search_original: String::new(),
                search_alias: String::new(),
            });
            match applied {
                Some(applied) => {
                    row.mentions += 1;
                    row.applied += applied as usize;
                }
                None => row.kept += 1,
            }
        };
        for c in review.candidates() {
            if let (Some(id), Some(group)) = (
                review.occurrence_identity(c.variant, &c.range),
                review.variant(c.variant),
            ) {
                add(id, group.original.clone(), Some(false));
                members
                    .entry(id)
                    .or_default()
                    .push(PanelItem::Mention(c.id, c.range.clone()));
            }
        }
        for a in review.applied() {
            add(
                a.step.identity,
                a.step.original_shared().clone(),
                Some(true),
            );
            members
                .entry(a.step.identity)
                .or_default()
                .push(PanelItem::Mention(APPLIED_ID | a.id, a.range.clone()));
        }
        let kept = self.kept_entries();
        let kept_count = kept.len();
        let mut orphans = Vec::new();
        for (range, original, identity) in kept {
            if let Some(id) = identity {
                members.entry(id).or_default().push(PanelItem::Kept(range));
                if review.identity(id).is_some() {
                    add(id, original, None);
                } else {
                    orphans.push((id, original));
                }
            }
        }
        for (id, original) in orphans {
            let Some(name) = mapping.kept_names.get(&original) else {
                continue;
            };
            let row = rows.entry(id).or_insert_with(|| ReplacementRow {
                id,
                original: original.clone(),
                alias: name.alias.clone(),
                category: name.category,
                mentions: 0,
                applied: 0,
                kept: 0,
                search_original: String::new(),
                search_alias: String::new(),
            });
            row.kept += 1;
        }
        for items in members.values_mut() {
            items.sort_by_key(|item| match item {
                PanelItem::Mention(_, range) | PanelItem::Kept(range) => range.start,
                _ => 0,
            });
        }
        let mut rows: Vec<_> = rows
            .into_values()
            .map(|mut row| {
                row.search_original = row.original.to_lowercase();
                row.search_alias = row.alias.to_lowercase();
                Arc::new(row)
            })
            .collect();
        rows.sort_by_key(|row| Category::ALL.iter().position(|c| *c == row.category));
        let active_aliases = rows
            .iter()
            .filter(|row| row.mentions > 0)
            .map(|row| row.alias.clone())
            .collect();
        let projection = Arc::new(PanelProjection {
            revision,
            rows,
            members,
            active_aliases,
            kept: kept_count,
        });
        *mapping.projection.borrow_mut() = Some(projection.clone());
        projection
    }

    /// Entity ids in the panel's row order (category, then entity).
    pub(super) fn panel_entity_order(&self, cx: &App) -> Vec<u64> {
        self.replacement_projection(cx)
            .rows
            .iter()
            .map(|row| row.id)
            .collect()
    }

    fn replacement_query(&self, cx: &App) -> String {
        self.pii
            .mapping
            .search
            .read(cx)
            .value()
            .trim()
            .to_lowercase()
    }
    pub(super) fn replacement_entity_count(&self, cx: &App) -> usize {
        let query = self.replacement_query(cx);
        self.replacement_projection(cx)
            .rows
            .iter()
            .filter(|row| row.visible(&query, self.pii.mapping.filter))
            .count()
    }
    pub(super) fn kept_mention_count(&self, cx: &App) -> usize {
        self.replacement_projection(cx).kept
    }
    pub(in crate::pii::ui) fn case_alias_is_present(&self, alias: &str, cx: &App) -> bool {
        self.replacement_projection(cx)
            .active_aliases
            .contains(alias)
    }

    /// Cached flat presentation over the immutable grouped projection. The
    /// virtual list and keyboard navigation use the exact same row ordering.
    pub(super) fn replacement_entries(&self, cx: &App) -> Arc<[PanelEntry]> {
        let mapping = &self.pii.mapping;
        let query = self.replacement_query(cx);
        let revision = (self.pii.review.revision(), self.editor.read(cx).revision());
        if let Some(cached) = mapping.entries_cache.borrow().as_ref()
            && cached.revision == revision
            && cached.filter == mapping.filter
            && cached.query == query
            && cached.expanded == mapping.expanded
        {
            return cached.entries.clone();
        }
        let projection = self.replacement_projection(cx);
        let rows: Vec<_> = projection
            .rows
            .iter()
            .filter(|row| row.visible(&query, mapping.filter))
            .collect();
        let mut entries = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if i == 0 || rows[i - 1].category != row.category {
                let group = rows[i..].iter().take_while(|r| r.category == row.category);
                let (entities, mentions) =
                    group.fold((0, 0), |(e, m), r| (e + 1, m + r.mentions + r.kept));
                entries.push(((*row).clone(), PanelItem::Category { entities, mentions }));
            }
            entries.push(((*row).clone(), PanelItem::Header));
            if mapping.expanded.contains(&row.id)
                && let Some(items) = projection.members.get(&row.id)
            {
                for item in items {
                    let shown = match item {
                        PanelItem::Mention(annotation, _) => {
                            mapping.filter.shows(annotation & APPLIED_ID != 0)
                                || mapping.filter == PanelFilter::Kept
                        }
                        PanelItem::Kept(_) => {
                            !matches!(mapping.filter, PanelFilter::Proposed | PanelFilter::Applied)
                        }
                        _ => false,
                    };
                    if shown {
                        entries.push(((*row).clone(), item.clone()));
                    }
                }
            }
        }
        let entries: Arc<[PanelEntry]> = entries.into();
        *mapping.entries_cache.borrow_mut() = Some(EntriesCache {
            revision,
            query,
            filter: mapping.filter,
            expanded: mapping.expanded.clone(),
            entries: entries.clone(),
        });
        entries
    }

    /// Keep the active mention's panel row in view, e.g. after ‹ › navigation.
    pub(super) fn reveal_active_row(&self, cx: &App) {
        let Some(annotation) = self.active_annotation() else {
            return;
        };
        if let Some(index) = self
            .replacement_entries(cx)
            .iter()
            .position(|(_, item)| item.mention() == Some(annotation))
        {
            self.pii
                .mapping
                .list_scroll
                .scroll_to_item(index, gpui::ScrollStrategy::Nearest);
        }
    }

    fn show_pii_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    /// The panel while the selected model is missing: consent, progress and
    /// retry in place of the review (ADR 0026).
    fn pii_setup_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let model = self
            .preferences
            .borrow()
            .snapshot()
            .ok()
            .map(|p| settings::Model::Pii(p.pseudonymization.model));
        let panel = self.model_panel.read(cx);
        let progress = model.and_then(|m| panel.progress_of(m));
        let cancelling = panel.cancelling();
        let needs_setup = model.is_none_or(|m| panel.needs_setup(m));
        let pending = model.map_or_else(Default::default, |m| {
            panel.pending[settings_ui::Panel::index(m)]
        });
        let idle = panel.working.is_none() && !mdoc_import::model_work::busy();
        let error = self.pii.error.clone();
        let primary = if !needs_setup {
            "Pseudonymize"
        } else if error.is_some() {
            "Retry"
        } else {
            "Download & scan"
        };
        let card = ui::state_card("pii-setup-card", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "pii-setup-card".into()))
            .max_w_full()
            .child(ui::card_title("Set up pseudonymization"))
            .child(ui::card_text(
                "Detects names, organizations and identifiers on this computer. Review the whole document before sharing.",
                theme,
            ));
        let card = if let Some(state) = progress {
            card.child(ui::setup_progress(
                "pii-setup-progress",
                &state,
                cancelling,
                theme,
            ))
            .child(
                ui::card_actions().child(
                    settings_ui::control("cancel-pii-setup", "Cancel", theme, !cancelling)
                        .track_focus(&self.card_focus.clone().tab_stop(!cancelling))
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "cancel-pii-setup".into())
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model_panel
                                .update(cx, |panel, cx| panel.cancel_setup(cx))
                        })),
                ),
            )
        } else {
            card.when_some(error, |v, e| v.child(ui::card_error(e, theme)))
                .child(
                    ui::card_actions()
                        .child(
                            ui::primary_button(
                                "pii-setup-primary",
                                primary,
                                theme,
                                idle || !needs_setup,
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "pii-setup-primary".into())
                            })
                            .track_focus(&self.card_focus.clone().tab_stop(idle || !needs_setup))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if !needs_setup {
                                        this.start_pii_scan(cx);
                                    } else if idle {
                                        this.setup_pii_model(window, cx);
                                    }
                                },
                            )),
                        )
                        .child(
                            settings_ui::control("pii-setup-cancel", "Cancel", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "pii-setup-cancel".into())
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_replacements(window, cx)
                                })),
                        )
                        .child(
                            ui::link_button("choose-pii-model", "Choose model…", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "choose-pii-model".into())
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_pii_settings(window, cx)
                                })),
                        ),
                )
        };
        let note = match model {
            Some(m) if pending.total() > 0 => {
                format!(
                    "{} · {}, once",
                    crate::models::short_name(m),
                    pending.download_label()
                )
            }
            Some(m) => format!("{} · runs on this computer", crate::models::short_name(m)),
            None => String::new(),
        };
        ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            // A card inset in the document, like the content card (ADR 0048).
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.pii.mapping.focus)
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                // An open row or filter menu closes before the panel (#92).
                if !this.close_panel_menu(window, cx) {
                    this.close_replacements(window, cx);
                }
                cx.stop_propagation();
            }))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .w(self.replacements_panel_width(window) - px(16.))
            .p_3()
            .child(
                card.child(ui::card_note(note, theme))
                    .when_some(model, |card, model| {
                        card.child(ui::model_attribution("pii-model-attribution", model, theme))
                    }),
            )
            .into_any_element()
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
    pub(super) fn drop_on_entity(&mut self, drag: &PanelDrag, target: u64, cx: &mut Context<Self>) {
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
    pub(super) fn drop_as_new_entity(&mut self, drag: &PanelDrag, cx: &mut Context<Self>) {
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
    pub(super) fn drag_over_target(&mut self, target: DropTarget, cx: &mut Context<Self>) {
        if self.pii.mapping.drop_target != Some(target) {
            self.pii.mapping.drop_target = Some(target);
            cx.notify();
        }
    }

    /// Document undo/redo without leaving the panel (ADR 0033).
    pub(super) fn undo_last_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.undo_step(window, cx));
    }
    pub(super) fn redo_last_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.redo_step(window, cx));
    }

    /// Expand or collapse every listed identity; its name follows the state.
    fn expand_all_control(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let expanded = self.all_identities_expanded(cx);
        let (label, icon) = if expanded {
            ("Collapse all identities", ui::Icon::CollapseAll)
        } else {
            ("Expand all identities", ui::Icon::ExpandAll)
        };
        ui::header_icon(
            "replacement-expand-all",
            label,
            icon,
            self.theme.get(),
            false,
        )
        .track_focus(&self.pii.mapping.expand_all_focus)
        .when(cfg!(test), |v| {
            v.debug_selector(|| "replacement-expand-all".into())
        })
        .on_click(
            cx.listener(move |this, _, _, cx| this.set_all_identities_expanded(!expanded, cx)),
        )
    }

    /// The panel-only filter; a dot marks anything but All (ADR 0033).
    fn filter_control(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme.get();
        let filter = self.pii.mapping.filter;
        let button = ui::header_icon(
            "replacement-filter",
            format!("Show: {}", filter.label()),
            ui::Icon::Filter,
            theme,
            filter != PanelFilter::All,
        )
        .track_focus(&self.pii.mapping.filter_focus)
        .when(cfg!(test), |v| {
            v.debug_selector(|| "replacement-filter".into())
        })
        .relative()
        .when(filter != PanelFilter::All, |v| {
            v.child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(theme.applied()),
            )
        })
        .on_click(cx.listener(|this, _, window, cx| {
            let menu = &mut this.pii.mapping.panel_menu;
            *menu = (*menu != Some(PanelMenu::Filter)).then_some(PanelMenu::Filter);
            // The menu takes focus on the current filter (ADR 0062).
            if this.pii.mapping.panel_menu.is_some() {
                let row = format!("replacement-filter-{}", this.pii.mapping.filter.label());
                window.focus(
                    &this.pii.popup_control_focus(SharedString::from(row), cx),
                    cx,
                );
            }
            cx.notify();
        }));
        let menu = (self.pii.mapping.panel_menu == Some(PanelMenu::Filter)).then(|| {
            let surface = self.menu_surface("replacement-filter-menu");
            self.focus_menu(
                surface,
                &self.pii.mapping.filter_menu_focus,
                &self.pii.mapping.filter_focus,
                |this, _, _| {
                    this.pii.mapping.panel_menu = None;
                    true
                },
                cx,
            )
            .w(px(160.))
            .children(PanelFilter::ALL.into_iter().map(|option| {
                self.menu_row(
                    SharedString::from(format!("replacement-filter-{}", option.label())),
                    option.label(),
                    "",
                    Some(option == filter),
                    true,
                    cx,
                )
                // A choice hands focus to the list it filters.
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pii.mapping.filter = option;
                    this.pii.mapping.panel_menu = None;
                    window.focus(&this.pii.mapping.focus, cx);
                    cx.notify();
                }))
            }))
            .into_any_element()
        });
        // Anchored at the button's left; the menu snaps inside the window.
        self.with_dropdown(button, menu)
    }
    pub(crate) fn replacements_panel_width(&self, window: &Window) -> gpui::Pixels {
        if self.pii.mapping.open && self.can_copy_markdown() {
            px((self.chrome_width(window) * 0.4).clamp(260., 380.))
        } else {
            px(0.)
        }
    }
    pub(crate) fn replacements_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.pii.mapping.open || !self.can_copy_markdown() {
            return None;
        }
        if self.pii.setup {
            return Some(self.pii_setup_panel(window, cx));
        }
        // Inspector (ADR 0048 variant C): Review | Share.
        if self.inspector_share {
            return Some(self.share_panel(window, cx));
        }
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let mapping = &self.pii.mapping;
        let entries = self.replacement_entries(cx);
        let count = entries.len();
        let bounds = mapping.bounds.clone();
        let busy = self.pii.scanning();
        let pending = self.pii.review.remaining();
        let history = self.editor.read(cx).history_id();
        // The latest step's after-action notice, newest kind first (ADR 0033).
        let notice = mapping
            .notice_visible()
            .then(|| {
                if let Some((added, _)) = mapping.added_at(history) {
                    Some((format!("Added “{}”", added.original), "Cancel", true))
                } else if let Some(kept) = mapping.kept_at(history) {
                    Some((format!("Kept {kept}"), "Undo", false))
                } else {
                    mapping
                        .applied_at(history)
                        .map(|n| (format!("Applied {n}"), "Undo", false))
                }
            })
            .flatten();
        // A mention of a multi-mention entity dragged over the panel can get a
        // new alias: the next free token of its entity's category.
        let new_alias = ((cx.has_active_drag() && mapping.drag_in_panel) || mapping.key_moving)
            .then_some(mapping.dragging.as_ref())
            .flatten()
            .filter(|d| {
                matches!(d, PanelDrag::Mention { .. })
                    && self.pii.review.identity_count(d.identity()) > 1
            })
            .and_then(|d| self.pii.review.identity(d.identity()))
            .map(|identity| self.pii.review.next_alias(identity.category));
        let rows_height = px(ROW_HEIGHT * count as f32);
        let total_entities = entries
            .iter()
            .filter(|(_, item)| matches!(item, PanelItem::Header))
            .count();
        let kept = self.kept_mention_count(cx);
        let applied = self.pii.review.applied().len();
        let summary = {
            let mut parts = Vec::new();
            if pending > 0 {
                parts.push(format!("{pending} to replace"));
            }
            if applied > 0 {
                parts.push(format!("{applied} applied"));
            }
            if kept > 0 {
                parts.push(format!("{kept} kept"));
            }
            if parts.is_empty() {
                "Nothing to replace yet".to_string()
            } else {
                parts.join(" · ")
            }
        };
        let upgrade = self.pii.review.applied().iter().any(|a| {
            self.pii
                .review
                .identity(a.step.identity)
                .is_some_and(|i| a.step.after.as_ref() != i.alias)
        });
        let panel = ui::panel("identity-panel", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "identity-panel".into()))
            .on_drag_move::<PanelDrag>(cx.listener(
                |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                    let inside = e.bounds.contains(&e.event.position);
                    let mapping = &mut this.pii.mapping;
                    if mapping.drag_in_panel != inside {
                        mapping.drag_in_panel = inside;
                        if !inside && mapping.drop_target == Some(DropTarget::NewEntity) {
                            mapping.drop_target = None;
                        }
                        cx.notify();
                    }
                },
            ))
            // A card inset in the document, like the content card (ADR 0048).
            .m_2()
            .rounded_lg()
            .shadow_none()
            .overflow_hidden()
            .key_context("IdentityPanel UiPanel UiMenu")
            .tab_group()
            .tab_stop(false)
            .track_focus(&mapping.focus)
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .w(self.replacements_panel_width(window) - px(16.))
            .relative()
            .child(
                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            // An open row or filter menu keeps Tab inside the panel;
            // otherwise Tab moves on to the next region (ADR 0062).
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                if this.pii.mapping.panel_menu.is_some() {
                    ui::cycle(window, cx, Some(&this.pii.mapping.focus), false);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                if this.pii.mapping.panel_menu.is_some() {
                    ui::cycle(window, cx, Some(&this.pii.mapping.focus), true);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                // An open row or filter menu closes before the panel (#92).
                if !this.close_panel_menu(window, cx) {
                    this.close_replacements(window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &mdoc_editor::Undo, window, cx| {
                this.undo_last_step(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &mdoc_editor::Redo, window, cx| {
                this.redo_last_step(window, cx);
                cx.stop_propagation();
            }))
            // Releasing ⌥ drops a keyboard move (ADR 0033).
            .on_modifiers_changed(cx.listener(|this, e: &gpui::ModifiersChangedEvent, _, cx| {
                if this.pii.mapping.key_moving && !e.modifiers.alt {
                    this.finish_key_move(cx);
                }
            }))
            .child(self.inspector_tabs(total_entities, cx))
            .child(
                div().flex_shrink_0().px_3().pb_2().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .h(px(30.))
                        .pl_2()
                        .rounded_md()
                        .border_1()
                        .border_color(palette.border)
                        .child(
                            gpui::svg()
                                .data(ui::Icon::Search.data())
                                .size(px(13.))
                                .flex_shrink_0()
                                .text_color(palette.header_muted),
                        )
                        .child(div().flex_1().min_w_0().child(mapping.search.clone()))
                        .child(self.filter_control(cx))
                        .child(self.expand_all_control(cx)),
                ),
            )
            .child(div().h(px(1.)).flex_shrink_0().bg(palette.border))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .map(|v| {
                        // While the new-alias area shows, the list keeps its rows'
                        // height (shrinking if needed) and the area takes the rest.
                        if new_alias.is_some() {
                            v.flex_basis(rows_height).flex_shrink(1.)
                        } else {
                            v.flex_1()
                        }
                    })
                    .min_h_0()
                    .when(count == 0, |v| v.flex_grow(0.).flex_basis(px(0.)))
                    .child(
                        uniform_list(
                            "replacement-list",
                            count,
                            cx.processor(move |this, indices: Range<usize>, _, cx| {
                                indices
                                    .map(|index| this.panel_row(index, entries[index].clone(), cx))
                                    .collect()
                            }),
                        )
                        .flex_1()
                        .min_h_0()
                        .track_scroll(&mapping.list_scroll),
                    )
                    .child(self.wheel_steps(cx)),
            )
            .when_some(new_alias, |v, alias| {
                // The free space under the rows becomes a contoured drop area
                // that names the alias a separated mention would get.
                let accent = theme.applied();
                let over = mapping.drop_target == Some(DropTarget::NewEntity);
                v.child(
                    div()
                        .id("replacement-new-entity")
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "replacement-new-entity".into())
                        })
                        .flex_grow(1.)
                        .min_h(px(72.))
                        .mx_3()
                        .my_2()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .rounded_lg()
                        .border_1()
                        .border_dashed()
                        .border_color(if over {
                            accent
                        } else {
                            Hsla {
                                a: 0.5,
                                ..palette.header_muted
                            }
                        })
                        .when(over, |v| v.bg(Hsla { a: 0.08, ..accent }))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.header_muted)
                                .child("Drop to give it a new alias"),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(accent)
                                .child(format!("→ {alias}")),
                        )
                        .on_drag_move::<PanelDrag>(cx.listener(
                            |this, e: &gpui::DragMoveEvent<PanelDrag>, _, cx| {
                                if e.bounds.contains(&e.event.position) {
                                    this.drag_over_target(DropTarget::NewEntity, cx);
                                }
                            },
                        ))
                        .on_drop(cx.listener(|this, drag: &PanelDrag, _, cx| {
                            this.drop_as_new_entity(drag, cx)
                        })),
                )
            })
            .when(count == 0, |v| {
                let searching = !mapping.search.read(cx).value().trim().is_empty();
                // While scanning: quiet skeleton rows; rows arrive when the
                // scan ends (ADR 0048 decision 22).
                let body = if busy && !searching {
                    div()
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "replacement-skeleton".into())
                        })
                        .flex()
                        .flex_col()
                        .px_3()
                        .pt_3()
                        .gap(px(14.))
                        .children(
                            [0.62, 0.48, 0.7, 0.4, 0.56, 0.66]
                                .into_iter()
                                .enumerate()
                                .map(|(i, width)| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .when(i == 0 || i == 4, |v| v.mt_1())
                                        .child(
                                            div()
                                                .size(px(14.))
                                                .rounded(px(4.))
                                                .bg(palette.placeholder_bg),
                                        )
                                        .child(
                                            div()
                                                .h(px(9.))
                                                .w(gpui::relative(width))
                                                .rounded_full()
                                                .bg(palette.placeholder_bg),
                                        )
                                }),
                        )
                        .into_any_element()
                } else {
                    div()
                        .px_3()
                        .py_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(if searching {
                            "No matching replacements."
                        } else {
                            "Nothing found. Select text to add it by hand."
                        })
                        .into_any_element()
                };
                v.child(body).child(div().flex_1())
            })
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(palette.border)
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "replacement-summary".into())
                                    })
                                    .flex_1()
                                    .min_w(px(96.))
                                    .text_size(px(12.))
                                    .text_color(palette.header_fg)
                                    .child(summary),
                            )
                            .when(upgrade && pending == 0, |v| {
                                v.child(
                                    ui::link_button(
                                        "apply-identity-map",
                                        "Update aliases",
                                        theme,
                                        !busy,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "apply-identity-map".into())
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.apply_replacements(cx)),
                                    ),
                                )
                            })
                            .child(
                                ui::link_button("review-rescan", "↻ Rescan", theme, !busy)
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "review-rescan".into())
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.pii.scanning() {
                                            this.start_pii_scan(cx);
                                        }
                                    })),
                            ),
                    )
                    .when_some(notice, |v, (text, action, addition)| {
                        v.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(palette.header_muted)
                                        .child(text),
                                )
                                .child(
                                    ui::link_button("replacement-notice-undo", action, theme, true)
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "replacement-notice-undo".into())
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if addition {
                                                this.cancel_addition(window, cx)
                                            } else {
                                                this.undo_last_step(window, cx)
                                            }
                                        })),
                                ),
                        )
                    })
                    .when_some(self.pii.error.clone(), |v, error| {
                        v.child(
                            div()
                                .text_size(px(11.))
                                .text_color(style::markdown_style(theme).alert_warning)
                                .child(error),
                        )
                    })
                    .children(self.case_names_section(cx))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child("Review for missed identifiers before sharing."),
                    ),
            );
        Some(self.panel_actions(panel, cx).into_any_element())
    }
}
