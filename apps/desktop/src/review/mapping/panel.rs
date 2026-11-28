//! Searchable replacement overview with inline occurrence context.
mod actions;
mod view;
use super::{DropTarget, MappingAction, PanelDrag, PanelFilter, PanelMenu, checklist::Tick};
#[cfg(test)]
use crate::review::text::SetupCopy;
pub(super) use crate::review::text::replacement_summary;
use crate::{document::DocumentView, review::APPLIED_ID};
use gpui::App;
use mdoc_pii::{Category, merge_hints::MergeHint};
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
    /// Mentions kept as originals (unticked).
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
    /// Merge hints by the identity that shows them.
    hints: HashMap<u64, Vec<Arc<MergeHint>>>,
}
pub(super) struct EntriesCache {
    revision: (u64, u64),
    filter: PanelFilter,
    query: String,
    expanded: HashSet<u64>,
    dismissed: usize,
    entries: Arc<[PanelEntry]>,
}

/// Every panel row is this tall, so the list stays a uniform list.
pub(super) const ROW_HEIGHT: f32 = 34.;
/// One panel line: an entity row, or one of the expanded entity's mentions.
pub(super) type PanelEntry = (Arc<ReplacementRow>, PanelItem);
#[derive(Clone)]
pub(super) enum PanelItem {
    /// A category's label above its group headers: entities and mentions shown.
    /// Not focusable; navigation skips it.
    Category {
        entities: usize,
        mentions: usize,
    },
    Header,
    Mention(u64, Range<usize>),
    /// A mention whose original is kept: listed, unticked.
    Kept(Range<usize>),
    /// "Looks like … · Merge ×" under the header. Not a keyboard
    /// row: its Merge and × are Tab stops.
    Hint(Arc<MergeHint>),
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
        let listed: HashSet<u64> = rows.iter().map(|row| row.id).collect();
        let mut hints = HashMap::<u64, Vec<Arc<MergeHint>>>::new();
        for hint in review.merge_hints() {
            if listed.contains(&hint.from) && listed.contains(&hint.into) {
                hints.entry(hint.from).or_default().push(Arc::new(hint));
            }
        }
        let projection = Arc::new(PanelProjection {
            revision,
            rows,
            members,
            active_aliases,
            kept: kept_count,
            hints,
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
    pub(crate) fn replacement_entity_count(&self, cx: &App) -> usize {
        let query = self.replacement_query(cx);
        self.replacement_projection(cx)
            .rows
            .iter()
            .filter(|row| row.visible(&query, self.pii.mapping.filter))
            .count()
    }
    /// Names still to replace: the list's rows with a pending mention, the
    /// unit of the Review tab's count (#1314).
    pub(crate) fn pending_name_count(&self, cx: &App) -> usize {
        (self.replacement_projection(cx).rows.iter())
            .filter(|row| row.mentions > row.applied)
            .count()
    }
    /// Names with an applied replacement, in the same unit.
    pub(crate) fn applied_name_count(&self, cx: &App) -> usize {
        (self.replacement_projection(cx).rows.iter())
            .filter(|row| row.applied > 0)
            .count()
    }
    pub(super) fn kept_mention_count(&self, cx: &App) -> usize {
        self.replacement_projection(cx).kept
    }
    pub(in crate::review) fn project_alias_is_present(&self, alias: &str, cx: &App) -> bool {
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
            && cached.dismissed == mapping.dismissed_hint_count()
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
            for hint in projection.hints.get(&row.id).into_iter().flatten() {
                if !mapping.hint_dismissed(&hint.pair) {
                    entries.push(((*row).clone(), PanelItem::Hint(hint.clone())));
                }
            }
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
            dismissed: mapping.dismissed_hint_count(),
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

    #[cfg(test)]
    pub(crate) fn expanded_identities(&self) -> usize {
        self.pii.mapping.expanded.len()
    }

    #[cfg(test)]
    pub(crate) fn expand_all_focus(&self) -> gpui::FocusHandle {
        self.pii.mapping.expand_all_focus.clone()
    }

    /// Whether every identity the panel lists is expanded.
    pub(crate) fn all_identities_expanded(&self, cx: &App) -> bool {
        let entries = self.replacement_entries(cx);
        let mut headers = entries
            .iter()
            .filter(|(_, item)| matches!(item, PanelItem::Header))
            .peekable();
        headers.peek().is_some()
            && headers.all(|(row, _)| self.pii.mapping.expanded.contains(&row.id))
    }
}

#[cfg(test)]
mod tests {
    use super::SetupCopy;
    use mdoc_import::{model_download::Pending, ocr};
    use mdoc_store::settings::{Model, PiiModel};

    /// A model installed beside an earlier release's runtime (a 0.4.0-rc.3
    /// profile) is an update of the runtime's size; a first setup, a
    /// cancelled one and a ready model keep their words.
    #[test]
    fn an_earlier_runtime_asks_for_an_update() {
        let model = Some(Model::Pii(PiiModel::Fp16));
        let runtime = ocr::onnx_runtime_size();
        let full = 600_000_000 + runtime;
        let update = SetupCopy::new(
            model,
            Pending {
                model: 0,
                runtime,
                full,
                replaces_runtime: true,
            },
        );
        assert_eq!(update.title, "Update pseudonymization");
        assert_eq!(update.action, "Update & scan");
        assert_eq!(update.note, "Standard model · 185 MB runtime update");
        assert!(update.text.contains("newer shared runtime"));

        let first = Pending {
            model: 600_000_000,
            runtime,
            full,
            replaces_runtime: false,
        };
        let setup = SetupCopy::new(model, first);
        assert_eq!(setup.title, "Set up pseudonymization");
        assert_eq!(setup.action, "Download & scan");
        assert_eq!(setup.note, "Standard model · 785 MB download, once");
        // The model missing too is a setup, whatever runtime is there.
        let both = SetupCopy::new(
            model,
            Pending {
                replaces_runtime: true,
                ..first
            },
        );
        assert_eq!(both, setup);
        // Only the runtime missing, with none installed: as before.
        let runtime_only = SetupCopy::new(model, Pending { model: 0, ..first });
        assert_eq!(runtime_only.title, "Set up pseudonymization");
        assert_eq!(
            runtime_only.note,
            "Standard model · 185 MB left to download, once"
        );
        let ready = SetupCopy::new(
            model,
            Pending {
                full,
                ..Pending::default()
            },
        );
        assert_eq!(ready.title, "Set up pseudonymization");
        assert_eq!(ready.note, "Standard model · runs on this computer");
    }
}
