//! The window's live views, most recently viewed first: each one's id, the
//! document it names, its saved record and its document view, which one is
//! active, and whether Home covers it. At most `LIMIT` documents stay live;
//! the least recently viewed beyond them is evicted. Generic over the view,
//! so the recency rules run without a window; the workspace view creates,
//! releases and focuses the documents.

use mdoc_store::{document_id::DocumentId, session_store::TabRecord};
use std::{collections::HashSet, path::PathBuf};

/// How many documents stay live for their undo and scroll.
pub(super) const LIMIT: usize = 8;

/// What the workspace view carries out for a live-view transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LiveViewsEffect {
    /// Activate this view: the next most recent, or a new placeholder.
    Activate(u64),
}

/// How eviction weighs a live view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Weight {
    /// Not a document (Home's placeholder): never counted nor evicted.
    Ignored,
    /// Counted, but kept while it works (a conversion, a prompt, a write).
    Busy,
    /// Counted, and evicted when it is the least recent beyond the limit.
    Idle,
}

pub(super) struct LiveView<V> {
    id: u64,
    record: TabRecord,
    /// The document this view holds, for finding it again.
    document: Option<DocumentId>,
    view: Option<V>,
    /// Last opened/saved path observed, independent of the capped recent list.
    recent_path: Option<PathBuf>,
}

impl<V> LiveView<V> {
    /// A tab outside any set, for tests of how tabs are labelled.
    #[cfg(test)]
    pub(super) fn detached(id: u64, record: TabRecord) -> Self {
        Self {
            id,
            record,
            document: None,
            view: None,
            recent_path: None,
        }
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    /// What the session saves for this tab.
    pub(super) fn record(&self) -> &TabRecord {
        &self.record
    }

    pub(super) fn document(&self) -> Option<&DocumentId> {
        self.document.as_ref()
    }

    /// The document view, once the tab was first shown.
    pub(super) fn view(&self) -> Option<&V> {
        self.view.as_ref()
    }
}

pub(super) struct LiveViews<V> {
    tabs: Vec<LiveView<V>>,
    active: u64,
    next_id: u64,
    /// Home fills the card instead of the active tab.
    home: bool,
}

impl<V> Default for LiveViews<V> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            home: false,
        }
    }
}

/// Recency order: `tabs[0]` is the most recently viewed.
impl<V> std::ops::Index<usize> for LiveViews<V> {
    type Output = LiveView<V>;

    fn index(&self, index: usize) -> &LiveView<V> {
        &self.tabs[index]
    }
}

impl<'a, V> IntoIterator for &'a LiveViews<V> {
    type Item = &'a LiveView<V>;
    type IntoIter = std::slice::Iter<'a, LiveView<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.tabs.iter()
    }
}

impl<V> LiveViews<V> {
    pub(super) fn len(&self) -> usize {
        self.tabs.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, LiveView<V>> {
        self.tabs.iter()
    }

    #[cfg(test)]
    pub(super) fn last(&self) -> Option<&LiveView<V>> {
        self.tabs.last()
    }

    /// Drop every tab, keeping the active id and the id counter.
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.tabs.clear();
    }

    pub(super) fn get(&self, id: u64) -> Option<&LiveView<V>> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut LiveView<V>> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    pub(super) fn position(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    pub(super) fn contains(&self, id: u64) -> bool {
        self.tabs.iter().any(|tab| tab.id == id)
    }

    pub(super) fn active(&self) -> u64 {
        self.active
    }

    pub(super) fn active_tab(&self) -> Option<&LiveView<V>> {
        self.get(self.active)
    }

    /// Whether Home was asked for. A placeholder tab also shows Home; the
    /// view knows which tabs are placeholders.
    pub(super) fn home(&self) -> bool {
        self.home
    }

    /// Add a view for `record` as the least recent; activating it brings it
    /// to the front. Returns its id.
    pub(super) fn push(&mut self, record: TabRecord) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let document = record.document();
        self.tabs.push(LiveView {
            id,
            record,
            document,
            view: None,
            recent_path: None,
        });
        id
    }

    /// Name new blanks and older unnamed session records.
    /// The view supplies current titles, including unopened restored tabs,
    /// before any unnamed blank is initialized. Existing names stay intact.
    pub(super) fn name_blank_tabs(&mut self, names: impl IntoIterator<Item = String>) {
        let mut used: HashSet<_> = names.into_iter().collect();
        let mut number = 1;
        for tab in &mut self.tabs {
            if !tab.record.blank || tab.record.name.is_some() {
                continue;
            }
            loop {
                let name = if number == 1 {
                    "Untitled.md".to_string()
                } else {
                    format!("Untitled {number}.md")
                };
                number += 1;
                if used.insert(name.clone()) {
                    tab.record.name = Some(name);
                    break;
                }
            }
        }
    }

    /// The tab was first shown: keep its document view.
    pub(super) fn open(&mut self, id: u64, view: V) {
        if let Some(tab) = self.get_mut(id) {
            tab.view = Some(view);
        }
    }

    /// The user acted on blank tab `id` (none: on every tab), so it is no
    /// longer a placeholder that Home may replace.
    pub(super) fn keep_blank(&mut self, id: Option<u64>) {
        for tab in &mut self.tabs {
            if id.is_none_or(|id| id == tab.id) {
                tab.record.blank_disposable = false;
            }
        }
    }

    /// Tab `id` is shown as `name` until its document loads.
    pub(super) fn set_name(&mut self, id: u64, name: Option<String>) {
        if let Some(tab) = self.get_mut(id) {
            tab.record.name = name;
        }
    }

    /// Tab `id` now holds `document`. Returns the document it held.
    pub(super) fn set_document(
        &mut self,
        id: u64,
        document: Option<DocumentId>,
    ) -> Option<DocumentId> {
        let tab = self.get_mut(id)?;
        std::mem::replace(&mut tab.document, document)
    }

    /// Tab `id` shows `path` now. Returns whether that is a different path
    /// from the last one seen, so it counts as opened again.
    pub(super) fn note_recent(&mut self, id: u64, path: Option<&PathBuf>) -> bool {
        let Some(tab) = self.get_mut(id) else {
            return false;
        };
        let opened = path != tab.recent_path.as_ref();
        if opened {
            tab.recent_path = path.cloned();
        }
        opened
    }

    /// Make `id` the active view, the most recent, and leave Home. False for
    /// an unknown view.
    pub(super) fn activate(&mut self, id: u64) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        let tab = self.tabs.remove(index);
        self.tabs.insert(0, tab);
        self.active = id;
        self.home = false;
        true
    }

    /// The views `weigh` counts, most recent first.
    pub(super) fn recent(&self, weigh: impl Fn(&LiveView<V>) -> Weight) -> Vec<u64> {
        (self.tabs.iter())
            .filter(|tab| weigh(tab) != Weight::Ignored)
            .map(|tab| tab.id)
            .collect()
    }

    /// The views to evict so that at most `limit` counted ones stay: the
    /// least recent idle ones, never the active one.
    pub(super) fn evictable(
        &self,
        limit: usize,
        weigh: impl Fn(&LiveView<V>) -> Weight,
    ) -> Vec<u64> {
        let weights: Vec<_> = self.tabs.iter().map(|tab| (tab.id, weigh(tab))).collect();
        let mut over = (weights.iter())
            .filter(|(_, weight)| *weight != Weight::Ignored)
            .count()
            .saturating_sub(limit);
        let mut evicted = Vec::new();
        for (id, weight) in weights.iter().rev() {
            if over == 0 {
                break;
            }
            if *weight == Weight::Idle && *id != self.active {
                evicted.push(*id);
                over -= 1;
            }
        }
        evicted
    }

    pub(super) fn show_home(&mut self) {
        self.home = true;
    }

    /// Release view `id`. The last one leaves a placeholder for Home;
    /// releasing the active view activates the next most recent.
    pub(super) fn remove(&mut self, id: u64) -> Option<(LiveView<V>, Option<LiveViewsEffect>)> {
        let index = self.position(id)?;
        let tab = self.tabs.remove(index);
        let effect = if self.tabs.is_empty() {
            Some(LiveViewsEffect::Activate(self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            })))
        } else if self.active == id {
            Some(LiveViewsEffect::Activate(
                self.tabs[index.min(self.tabs.len() - 1)].id,
            ))
        } else {
            None
        };
        Some((tab, effect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> DocumentId {
        DocumentId::File(path.into())
    }

    fn markdown(path: &str) -> TabRecord {
        TabRecord {
            markdown: path.into(),
            ..TabRecord::default()
        }
    }

    #[test]
    fn tabs_get_new_ids_in_order_and_markdown_names_their_document() {
        let mut tabs = LiveViews::<u8>::default();
        let a = tabs.push(markdown("/cases/a.md"));
        let b = tabs.push(TabRecord::default());
        assert!(b > a);
        assert_eq!(tabs.get(a).unwrap().document(), Some(&file("/cases/a.md")));
        assert_eq!(tabs.get(b).unwrap().document(), None);
        assert_eq!(tabs.get(a).unwrap().view(), None);
        tabs.open(a, 7);
        assert_eq!(tabs.get(a).unwrap().view(), Some(&7));
        assert!(tabs.activate(b));
        assert!(!tabs.activate(99));
        assert_eq!(tabs.active(), b);
        tabs.show_home();
        assert!(tabs.home());
        assert!(tabs.activate(a));
        assert!(!tabs.home());
        assert_eq!(tabs.active_tab().map(LiveView::id), Some(a));
    }

    #[test]
    fn releasing_activates_the_next_most_recent_and_the_last_leaves_a_placeholder() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b, c] = [(); 3].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        tabs.activate(c);
        tabs.activate(b);
        let (closed, effect) = tabs.remove(b).unwrap();
        assert_eq!(closed.id(), b);
        assert_eq!(effect, Some(LiveViewsEffect::Activate(c)));
        tabs.activate(c);
        assert_eq!(
            tabs.remove(c).unwrap().1,
            Some(LiveViewsEffect::Activate(a))
        );
        tabs.activate(a);
        assert!(tabs.remove(99).is_none());
        let Some((_, Some(LiveViewsEffect::Activate(placeholder)))) = tabs.remove(a) else {
            panic!("the last tab leaves a placeholder");
        };
        assert_eq!(tabs.len(), 1);
        let record = tabs.get(placeholder).unwrap().record();
        assert!(record.blank && record.blank_disposable);
    }

    #[test]
    fn records_change_only_through_named_transitions() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b] = [(); 2].map(|_| {
            tabs.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            })
        });
        tabs.keep_blank(Some(a));
        assert!(!tabs.get(a).unwrap().record().blank_disposable);
        assert!(tabs.get(b).unwrap().record().blank_disposable);
        tabs.keep_blank(None);
        assert!(!tabs.get(b).unwrap().record().blank_disposable);
        assert_eq!(tabs.set_document(a, Some(file("/cases/a.md"))), None);
        assert_eq!(
            tabs.set_document(a, Some(file("/cases/b.md"))),
            Some(file("/cases/a.md"))
        );
        assert_eq!(tabs.set_document(99, None), None);
        let path = PathBuf::from("/cases/b.md");
        assert!(tabs.note_recent(a, Some(&path)));
        assert!(!tabs.note_recent(a, Some(&path)));
        assert!(tabs.note_recent(a, None));
        assert!(!tabs.note_recent(99, Some(&path)));
    }

    #[test]
    fn closing_an_inactive_tab_keeps_the_active_one() {
        let mut tabs = LiveViews::<u8>::default();
        let [a, b] = [(); 2].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        assert_eq!(tabs.remove(b).unwrap().1, None);
        assert_eq!(tabs.active(), a);
    }

    #[test]
    fn viewing_brings_a_view_to_the_front_and_eviction_takes_the_least_recent_idle() {
        let mut tabs = LiveViews::<u8>::default();
        let ids: Vec<u64> = (0..10).map(|_| tabs.push(TabRecord::default())).collect();
        for id in &ids {
            tabs.activate(*id);
        }
        let all = |_: &LiveView<u8>| Weight::Idle;
        assert_eq!(
            tabs.recent(all),
            ids.iter().rev().copied().collect::<Vec<_>>()
        );
        // Ten views, a limit of eight: the two viewed first go.
        assert_eq!(tabs.evictable(LIMIT, all), [ids[0], ids[1]]);
        // A busy one is kept, so the next least recent goes in its place.
        let busy = ids[0];
        let weigh = |tab: &LiveView<u8>| {
            if tab.id() == busy {
                Weight::Busy
            } else {
                Weight::Idle
            }
        };
        assert_eq!(tabs.evictable(LIMIT, weigh), [ids[1], ids[2]]);
        // Uncounted views neither count nor go; the active one never goes.
        let placeholder = ids[1];
        let weigh = |tab: &LiveView<u8>| {
            if tab.id() == placeholder {
                Weight::Ignored
            } else {
                Weight::Idle
            }
        };
        assert_eq!(tabs.evictable(LIMIT, weigh), [ids[0]]);
        tabs.activate(ids[0]);
        assert_eq!(tabs.evictable(LIMIT, all), [ids[1], ids[2]]);
        assert_eq!(tabs.evictable(1, all).len(), 9);
        assert!(!tabs.evictable(0, all).contains(&ids[0]));
        assert_eq!(tabs.position(ids[0]), Some(0));
        assert_eq!(tabs.position(ids[9]), Some(1));
    }
}
