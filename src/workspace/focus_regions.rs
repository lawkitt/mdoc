//! The window's F6 regions (ADR 0062): the sidebar, then Home or the active
//! document's regions, and the control last focused in each when focus left
//! it. Generic over the focus handle, so the order rules run without a
//! window; the workspace view says which region holds focus and carries out
//! the returned effect.

/// Where focus goes, for the workspace view to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FocusEffect<H> {
    /// Focus this target: a single-target region (the editor).
    Focus(H),
    /// Enter `region`: its remembered control while that is still inside it,
    /// else its first control.
    Enter { region: H, last: Option<H> },
}

pub(super) struct FocusRegions<H> {
    sidebar: H,
    home: H,
    /// Each region's last focused control, most recent last.
    last: Vec<(H, H)>,
}

impl<H: Clone + PartialEq> FocusRegions<H> {
    pub(super) fn new(sidebar: H, home: H) -> Self {
        Self {
            sidebar,
            home,
            last: Vec::new(),
        }
    }

    pub(super) fn sidebar(&self) -> &H {
        &self.sidebar
    }

    pub(super) fn home(&self) -> &H {
        &self.home
    }

    /// The regions in F6 order, each with whether it is a single focus
    /// target (the editor) rather than a group of controls: the sidebar,
    /// then Home or the document's own regions.
    pub(super) fn order(
        &self,
        home: bool,
        document: impl IntoIterator<Item = (H, bool)>,
    ) -> Vec<(H, bool)> {
        let mut regions = vec![(self.sidebar.clone(), false)];
        if home {
            regions.push((self.home.clone(), false));
        } else {
            regions.extend(document);
        }
        regions
    }

    /// Remember `focused` as the last control in `region`. The region's own
    /// handle is never remembered: it means "no control yet".
    pub(super) fn note(&mut self, region: &H, focused: &H) {
        if focused != region {
            self.last.retain(|(r, _)| r != region);
            self.last.push((region.clone(), focused.clone()));
        }
    }

    /// F6 / Shift+F6 from region `current` (none: from outside every
    /// region): the next region, wrapping. The sidebar lands on
    /// `sidebar_landing` (the active tab's row in the layout drawn now),
    /// never on the control last focused there; other regions return to
    /// their remembered control.
    pub(super) fn cycle(
        &self,
        regions: &[(H, bool)],
        current: Option<usize>,
        backwards: bool,
        sidebar_landing: impl FnOnce() -> Option<H>,
    ) -> Option<FocusEffect<H>> {
        let n = regions.len();
        if n == 0 {
            return None;
        }
        let next = match current {
            Some(i) if backwards => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
            None if backwards => n - 1,
            None => 0,
        };
        let (region, single) = regions[next].clone();
        if single {
            return Some(FocusEffect::Focus(region));
        }
        let last = if region == self.sidebar {
            sidebar_landing()
        } else {
            self.last
                .iter()
                .find(|(r, _)| *r == region)
                .map(|(_, focus)| focus.clone())
        }
        .filter(|focus| *focus != region);
        Some(FocusEffect::Enter { region, last })
    }

    /// Tab / Shift+Tab moved focus from region `from` to region `to` in
    /// window order. The editor is no Tab stop, so it is entered rather than
    /// skipped: leaving a region for the editor's place in F6 order, or
    /// tabbing forward from outside every region (the find bar) into the
    /// region right after the editor.
    pub(super) fn tab_into_editor(
        regions: &[(H, bool)],
        from: Option<usize>,
        to: Option<usize>,
        backwards: bool,
    ) -> Option<FocusEffect<H>> {
        if from.is_some_and(|i| regions[i].1) {
            return None;
        }
        let n = regions.len();
        let editor = match (from, to) {
            (Some(i), Some(j)) if i == j => None,
            (Some(i), _) => {
                let next = if backwards {
                    (i + n - 1) % n
                } else {
                    (i + 1) % n
                };
                regions[next].1.then_some(next)
            }
            (None, Some(j)) if !backwards && j > 0 && regions[j - 1].1 => Some(j - 1),
            (None, _) => None,
        };
        editor.map(|i| FocusEffect::Focus(regions[i].0.clone()))
    }

    /// Forget every remembered control.
    #[cfg(test)]
    pub(super) fn forget(&mut self) {
        self.last.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regions: 0 sidebar, 9 Home, then a document's toolbar (1), editor (2,
    // single) and preview (3).
    fn document(regions: &FocusRegions<u32>) -> Vec<(u32, bool)> {
        regions.order(false, [(1, false), (2, true), (3, false)])
    }

    #[test]
    fn order_is_the_sidebar_then_home_or_the_document() {
        let regions = FocusRegions::new(0, 9);
        assert_eq!(regions.order(true, [(1, false)]), [(0, false), (9, false)]);
        assert_eq!(
            document(&regions),
            [(0, false), (1, false), (2, true), (3, false)]
        );
    }

    #[test]
    fn f6_wraps_and_returns_to_the_last_control_of_a_region() {
        let mut regions = FocusRegions::new(0, 9);
        let order = document(&regions);
        let enter = |region, last| Some(FocusEffect::Enter { region, last });
        let none = || None;
        // From outside every region: the first, or backwards the last.
        assert_eq!(regions.cycle(&order, None, false, none), enter(0, None));
        assert_eq!(regions.cycle(&order, None, true, none), enter(3, None));
        // The editor is a single target.
        assert_eq!(
            regions.cycle(&order, Some(1), false, none),
            Some(FocusEffect::Focus(2))
        );
        // A remembered control.
        regions.note(&1, &10);
        regions.note(&1, &11);
        regions.note(&3, &3);
        assert_eq!(
            regions.cycle(&order, Some(0), false, none),
            enter(1, Some(11))
        );
        // The region's own handle is never a remembered control.
        assert_eq!(regions.cycle(&order, Some(2), false, none), enter(3, None));
        regions.forget();
        assert_eq!(regions.cycle(&order, Some(0), false, none), enter(1, None));
        assert_eq!(regions.cycle(&[], None, false, none), None);
    }

    #[test]
    fn f6_into_the_sidebar_lands_on_its_landing_not_the_last_control() {
        let mut regions = FocusRegions::new(0, 9);
        let order = document(&regions);
        let enter = |region, last| Some(FocusEffect::Enter { region, last });
        // Settings (40) was the sidebar's last control; the active tab's row
        // (41) wins.
        regions.note(&0, &40);
        assert_eq!(
            regions.cycle(&order, Some(3), false, || Some(41)),
            enter(0, Some(41))
        );
        assert_eq!(
            regions.cycle(&order, Some(1), true, || Some(41)),
            enter(0, Some(41))
        );
        // No row to land on: the region's first control.
        assert_eq!(
            regions.cycle(&order, Some(3), false, || None),
            enter(0, None)
        );
        assert_eq!(
            regions.cycle(&order, Some(3), false, || Some(0)),
            enter(0, None)
        );
    }

    #[test]
    fn tab_enters_the_editor_instead_of_skipping_it() {
        let regions = FocusRegions::new(0, 9);
        let order = document(&regions);
        let tab = |from, to, backwards| FocusRegions::tab_into_editor(&order, from, to, backwards);
        // Tab past the toolbar's last control; Shift+Tab back from the preview.
        assert_eq!(tab(Some(1), Some(3), false), Some(FocusEffect::Focus(2)));
        assert_eq!(tab(Some(3), Some(1), true), Some(FocusEffect::Focus(2)));
        // Moving within a region, or leaving the editor itself: no change.
        assert_eq!(tab(Some(1), Some(1), false), None);
        assert_eq!(tab(Some(2), Some(3), false), None);
        // From the sidebar to the toolbar: the editor is not next.
        assert_eq!(tab(Some(0), Some(1), false), None);
        // From the find bar (outside every region) into the preview.
        assert_eq!(tab(None, Some(3), false), Some(FocusEffect::Focus(2)));
        assert_eq!(tab(None, Some(3), true), None);
        assert_eq!(tab(None, Some(1), false), None);
    }
}
