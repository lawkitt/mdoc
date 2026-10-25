//! The window's tabs in sidebar order (ADR 0048): each tab's id, saved
//! record and document view, which tab is active, and whether Home covers
//! it. Generic over the view, so the order rules run without a window; the
//! workspace view creates, releases and focuses the documents.

use mdoc_store::session_store::TabRecord;
use std::path::PathBuf;

/// What the workspace view carries out for a tab-set transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabsEffect {
    /// The sidebar order changed.
    Repaint,
    /// Activate this tab: the closed tab's neighbour, or a new placeholder.
    Activate(u64),
}

pub(super) struct Tab<V> {
    id: u64,
    record: TabRecord,
    /// The Markdown file this tab holds, for finding it by path.
    identity: Option<PathBuf>,
    view: Option<V>,
    /// Last opened/saved path observed, independent of the capped recent list.
    recent_path: Option<PathBuf>,
}

impl<V> Tab<V> {
    /// A tab outside any set, for tests of how tabs are labelled.
    #[cfg(test)]
    pub(super) fn detached(id: u64, record: TabRecord) -> Self {
        Self {
            id,
            record,
            identity: None,
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

    pub(super) fn identity(&self) -> Option<&PathBuf> {
        self.identity.as_ref()
    }

    /// The document view, once the tab was first shown.
    pub(super) fn view(&self) -> Option<&V> {
        self.view.as_ref()
    }
}

pub(super) struct TabSet<V> {
    tabs: Vec<Tab<V>>,
    active: u64,
    next_id: u64,
    /// Home fills the card instead of the active tab (ADR 0048).
    home: bool,
}

impl<V> Default for TabSet<V> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            home: false,
        }
    }
}

/// Sidebar order: `tabs[0]` is the first row.
impl<V> std::ops::Index<usize> for TabSet<V> {
    type Output = Tab<V>;

    fn index(&self, index: usize) -> &Tab<V> {
        &self.tabs[index]
    }
}

impl<'a, V> IntoIterator for &'a TabSet<V> {
    type Item = &'a Tab<V>;
    type IntoIter = std::slice::Iter<'a, Tab<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.tabs.iter()
    }
}

impl<V> TabSet<V> {
    pub(super) fn len(&self) -> usize {
        self.tabs.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, Tab<V>> {
        self.tabs.iter()
    }

    #[cfg(test)]
    pub(super) fn last(&self) -> Option<&Tab<V>> {
        self.tabs.last()
    }

    /// Drop every tab, keeping the active id and the id counter.
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.tabs.clear();
    }

    /// The tab at sidebar position `index`.
    pub(super) fn at(&self, index: usize) -> Option<&Tab<V>> {
        self.tabs.get(index)
    }

    pub(super) fn get(&self, id: u64) -> Option<&Tab<V>> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Tab<V>> {
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

    pub(super) fn active_tab(&self) -> Option<&Tab<V>> {
        self.get(self.active)
    }

    /// Whether Home was asked for. A placeholder tab also shows Home; the
    /// view knows which tabs are placeholders.
    pub(super) fn home(&self) -> bool {
        self.home
    }

    /// Add a tab for `record` at the end of the list; returns its id.
    pub(super) fn push(&mut self, record: TabRecord) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let identity = (!record.markdown.as_os_str().is_empty()).then(|| record.markdown.clone());
        self.tabs.push(Tab {
            id,
            record,
            identity,
            view: None,
            recent_path: None,
        });
        id
    }

    /// The tab was first shown: keep its document view.
    pub(super) fn open(&mut self, id: u64, view: V) {
        if let Some(tab) = self.get_mut(id) {
            tab.view = Some(view);
        }
    }

    /// Tab `id` joined case `case`, or left its case (none).
    pub(super) fn set_case(&mut self, id: u64, case: Option<String>) {
        if let Some(tab) = self.get_mut(id) {
            tab.record.case = case;
        }
    }

    /// Case `case` is gone: no tab belongs to it any more.
    pub(super) fn forget_case(&mut self, case: &str) {
        for tab in &mut self.tabs {
            if tab.record.case.as_deref() == Some(case) {
                tab.record.case = None;
            }
        }
    }

    /// The user acted on blank tab `id` (none: on every tab), so it is no
    /// longer a placeholder that Home may replace (ADR 0048).
    pub(super) fn keep_blank(&mut self, id: Option<u64>) {
        for tab in &mut self.tabs {
            if id.is_none_or(|id| id == tab.id) {
                tab.record.blank_disposable = false;
            }
        }
    }

    /// Tab `id` now holds the Markdown file `path`. Returns the file it held.
    pub(super) fn set_identity(&mut self, id: u64, path: Option<PathBuf>) -> Option<PathBuf> {
        let tab = self.get_mut(id)?;
        std::mem::replace(&mut tab.identity, path)
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

    /// Make `id` the active tab and leave Home. False for an unknown tab.
    pub(super) fn activate(&mut self, id: u64) -> bool {
        if !self.contains(id) {
            return false;
        }
        self.active = id;
        self.home = false;
        true
    }

    pub(super) fn show_home(&mut self) {
        self.home = true;
    }

    /// The tab `delta` places from the active one, wrapping.
    pub(super) fn cycle(&self, delta: isize) -> Option<TabsEffect> {
        if self.tabs.is_empty() {
            return None;
        }
        let index = self.position(self.active).unwrap_or(0);
        let next = (index as isize + delta).rem_euclid(self.tabs.len() as isize) as usize;
        Some(TabsEffect::Activate(self.tabs[next].id))
    }

    /// Move tab `from` to tab `to`'s place; nothing when either is gone.
    pub(super) fn reorder(&mut self, from: u64, to: u64) -> Option<TabsEffect> {
        let from = self.position(from)?;
        let to = self.position(to)?;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        Some(TabsEffect::Repaint)
    }

    /// Move the `created` tabs, keeping their sidebar order, before tab
    /// `anchor`, or to the end when it is gone.
    pub(super) fn place(&mut self, created: &[u64], anchor: Option<u64>) {
        let (moved, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.tabs)
            .into_iter()
            .partition(|tab| created.contains(&tab.id));
        self.tabs = kept;
        let index = anchor
            .and_then(|anchor| self.position(anchor))
            .unwrap_or(self.tabs.len());
        self.tabs.splice(index..index, moved);
    }

    /// Close tab `id`. The last tab leaves a placeholder for Home (ADR
    /// 0048); closing the active tab activates the one in its place, else
    /// the one before it.
    pub(super) fn remove(&mut self, id: u64) -> Option<(Tab<V>, Option<TabsEffect>)> {
        let index = self.position(id)?;
        let tab = self.tabs.remove(index);
        let effect = if self.tabs.is_empty() {
            Some(TabsEffect::Activate(self.push(TabRecord {
                blank: true,
                blank_disposable: true,
                ..TabRecord::default()
            })))
        } else if self.active == id {
            Some(TabsEffect::Activate(
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

    fn markdown(path: &str) -> TabRecord {
        TabRecord {
            markdown: path.into(),
            ..TabRecord::default()
        }
    }

    #[test]
    fn tabs_get_new_ids_in_order_and_markdown_names_their_identity() {
        let mut tabs = TabSet::<u8>::default();
        let a = tabs.push(markdown("/cases/a.md"));
        let b = tabs.push(TabRecord::default());
        assert!(b > a);
        assert_eq!(tabs.get(a).unwrap().identity(), Some(&"/cases/a.md".into()));
        assert_eq!(tabs.get(b).unwrap().identity(), None);
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
        assert_eq!(tabs.active_tab().map(Tab::id), Some(a));
    }

    #[test]
    fn closing_activates_the_tab_in_its_place_and_the_last_leaves_a_placeholder() {
        let mut tabs = TabSet::<u8>::default();
        let [a, b, c] = [(); 3].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(b);
        let (closed, effect) = tabs.remove(b).unwrap();
        assert_eq!(closed.id(), b);
        assert_eq!(effect, Some(TabsEffect::Activate(c)));
        tabs.activate(c);
        assert_eq!(tabs.remove(c).unwrap().1, Some(TabsEffect::Activate(a)));
        tabs.activate(a);
        assert!(tabs.remove(99).is_none());
        let Some((_, Some(TabsEffect::Activate(placeholder)))) = tabs.remove(a) else {
            panic!("the last tab leaves a placeholder");
        };
        assert_eq!(tabs.len(), 1);
        let record = tabs.get(placeholder).unwrap().record();
        assert!(record.blank && record.blank_disposable);
    }

    #[test]
    fn records_change_only_through_named_transitions() {
        let mut tabs = TabSet::<u8>::default();
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
        tabs.set_case(a, Some("case-1".into()));
        tabs.set_case(b, Some("case-1".into()));
        tabs.forget_case("case-2");
        assert_eq!(
            tabs.get(a).unwrap().record().case.as_deref(),
            Some("case-1")
        );
        tabs.forget_case("case-1");
        assert!(tabs.iter().all(|tab| tab.record().case.is_none()));
        assert_eq!(tabs.set_identity(a, Some("/cases/a.md".into())), None);
        assert_eq!(
            tabs.set_identity(a, Some("/cases/b.md".into())),
            Some("/cases/a.md".into())
        );
        assert_eq!(tabs.set_identity(99, None), None);
        let path = PathBuf::from("/cases/b.md");
        assert!(tabs.note_recent(a, Some(&path)));
        assert!(!tabs.note_recent(a, Some(&path)));
        assert!(tabs.note_recent(a, None));
        assert!(!tabs.note_recent(99, Some(&path)));
    }

    #[test]
    fn closing_an_inactive_tab_keeps_the_active_one() {
        let mut tabs = TabSet::<u8>::default();
        let [a, b] = [(); 2].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        assert_eq!(tabs.remove(b).unwrap().1, None);
        assert_eq!(tabs.active(), a);
    }

    #[test]
    fn cycling_wraps_and_reordering_and_placing_move_tabs() {
        let mut tabs = TabSet::<u8>::default();
        assert_eq!(tabs.cycle(1), None);
        let [a, b, c, d] = [(); 4].map(|_| tabs.push(TabRecord::default()));
        tabs.activate(a);
        assert_eq!(tabs.cycle(-1), Some(TabsEffect::Activate(d)));
        assert_eq!(tabs.cycle(1), Some(TabsEffect::Activate(b)));
        let order = |tabs: &TabSet<u8>| tabs.iter().map(Tab::id).collect::<Vec<_>>();
        assert_eq!(tabs.reorder(d, a), Some(TabsEffect::Repaint));
        assert_eq!(order(&tabs), [d, a, b, c]);
        assert_eq!(tabs.reorder(d, 99), None);
        tabs.place(&[c, d], Some(b));
        assert_eq!(order(&tabs), [a, d, c, b]);
        tabs.place(&[a], Some(a));
        assert_eq!(order(&tabs), [d, c, b, a]);
        assert_eq!(tabs.at(0).map(Tab::id), Some(d));
        assert_eq!(tabs.position(b), Some(2));
    }
}
