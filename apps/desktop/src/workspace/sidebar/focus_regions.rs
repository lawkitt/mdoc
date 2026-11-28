//! The window's F6 regions: the sidebar, then Home or the active
//! document's regions, and the control last focused in each when focus left
//! it. Generic over the focus handle, so the order rules run without a
//! window; the workspace view says which region holds focus and carries out
//! the returned effect.
use crate::ui;
use crate::workspace::{Workspace, sidebar, sidebar::state::Reveal};
use gpui::{App, Context, Window};

/// Where focus goes, for the workspace view to carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workspace) enum FocusEffect<H> {
    /// Focus this target: a single-target region (the editor).
    Focus(H),
    /// Enter `region`: its remembered control while that is still inside it,
    /// else its first control.
    Enter { region: H, last: Option<H> },
}

pub(in crate::workspace) struct FocusRegions<H> {
    sidebar: H,
    home: H,
    /// Each region's last focused control, most recent last.
    last: Vec<(H, H)>,
}

impl<H: Clone + PartialEq> FocusRegions<H> {
    pub(in crate::workspace) fn new(sidebar: H, home: H) -> Self {
        Self {
            sidebar,
            home,
            last: Vec::new(),
        }
    }

    pub(in crate::workspace) fn sidebar(&self) -> &H {
        &self.sidebar
    }

    pub(in crate::workspace) fn home(&self) -> &H {
        &self.home
    }

    /// The regions in F6 order, each with whether it is a single focus
    /// target (the editor) rather than a group of controls: the sidebar,
    /// then Home or the document's own regions.
    pub(in crate::workspace) fn order(
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
    pub(in crate::workspace) fn note(&mut self, region: &H, focused: &H) {
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
    pub(in crate::workspace) fn cycle(
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
    pub(in crate::workspace) fn tab_into_editor(
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
    pub(in crate::workspace) fn forget(&mut self) {
        self.last.clear();
    }
}

impl Workspace {
    /// The window's F6 regions in order, each with whether it is a single
    /// focus target (the editor) rather than a group of controls.
    pub(in crate::workspace) fn regions(&self, cx: &App) -> Vec<(gpui::FocusHandle, bool)> {
        let home = self.showing_overview(cx);
        let document = self
            .active_view()
            .filter(|_| !home)
            .map(|view| view.read(cx).focus_regions(cx))
            .unwrap_or_default();
        self.focus_regions.order(home, document)
    }

    /// F6 / Shift+F6: leave the focused region for the next one, landing on
    /// the control last focused there, else its first control.
    pub(crate) fn cycle_region(&mut self, backwards: bool, window: &mut Window, cx: &mut App) {
        // Modal surfaces keep focus until closed.
        if self.settings.read(cx).open
            || self.project_ui.list_open
            || self.project_ui.explainer
            || self.folder_sheets.open()
            || self.new_project.is_some()
            || self.shell.palette.is_some()
            || self.shell.shortcuts.is_some()
        {
            return;
        }
        let regions = self.regions(cx);
        let current = self.note_region_focus(window, cx);
        let effect = self
            .focus_regions
            .cycle(&regions, current, backwards, || self.sidebar_landing(cx));
        match effect {
            None => {}
            Some(sidebar::focus_regions::FocusEffect::Focus(target)) => window.focus(&target, cx),
            Some(sidebar::focus_regions::FocusEffect::Enter { region, last }) => {
                if let Some(last) =
                    last.filter(|focus| self.region_holds(&region, focus, window, cx))
                {
                    // The tree's one Tab stop follows the row it lands on,
                    // so Tab leaves the tree from there.
                    if region == *self.focus_regions.sidebar() {
                        self.tree.set_cursor(None);
                    }
                    window.focus(&last, cx);
                } else if region == *self.focus_regions.sidebar() && !self.sidebar.visible() {
                    // The deferred title/reveal group precedes the sidebar
                    // container in window Tab order. Enter its first control
                    // explicitly, so Shift+Tab leaves from the region's edge.
                    let toggle = if self.sidebar.reveal() == Reveal::Hidden {
                        &self.rail_toggle_focus
                    } else {
                        &self.panel_toggle_focus
                    };
                    window.focus(toggle, cx);
                } else {
                    window.focus(&region, cx);
                    ui::cycle(window, cx, Some(&region), false);
                    if !region.contains_focused(window, cx) {
                        window.focus(&region, cx);
                    }
                }
            }
        }
    }

    /// Where F6 enters the sidebar, in the layout drawn now: on the collapsed
    /// rail its Home row, which opens the reveal; in the expanded tree (or
    /// the rail's open reveal) Home's row on Home, else the tree's selected
    /// row.
    pub(in crate::workspace) fn sidebar_landing(&self, cx: &mut App) -> Option<gpui::FocusHandle> {
        let list = self.sidebar.visible() || self.sidebar.reveal() == Reveal::Shown;
        if !list {
            return Some(self.rail_home_focus.clone());
        }
        if self.showing_home(cx) {
            return Some(self.panel_home_focus.clone());
        }
        self.tree_landing(cx)
    }

    /// The region holding focus. Comments sit inside the preview: the
    /// innermost wins. The document's notices strip and view switcher count
    /// as its toolbar.
    pub(in crate::workspace) fn region_index(
        &self,
        regions: &[(gpui::FocusHandle, bool)],
        window: &Window,
        cx: &App,
    ) -> Option<usize> {
        regions
            .iter()
            .rposition(|(region, _)| region.contains_focused(window, cx))
            .or_else(|| {
                let view = self.active_view().filter(|_| !self.showing_overview(cx))?;
                let view = view.read(cx);
                view.toolbar_members()
                    .iter()
                    .any(|member| member.contains_focused(window, cx))
                    .then(|| {
                        regions
                            .iter()
                            .position(|(region, _)| *region == view.toolbar_region)
                    })
                    .flatten()
            })
    }

    /// Whether `focus` sits in `region`, counting the document's toolbar
    /// members as its toolbar.
    pub(in crate::workspace) fn region_holds(
        &self,
        region: &gpui::FocusHandle,
        focus: &gpui::FocusHandle,
        window: &Window,
        cx: &App,
    ) -> bool {
        region.contains(focus, window)
            || self.active_view().is_some_and(|view| {
                let view = view.read(cx);
                view.toolbar_region == *region
                    && view
                        .toolbar_members()
                        .iter()
                        .any(|member| member.contains(focus, window))
            })
    }

    /// Remember the focused control as its region's last; returns that
    /// region's index. Comments sit inside the preview: the innermost wins.
    pub(in crate::workspace) fn note_region_focus(
        &mut self,
        window: &Window,
        cx: &App,
    ) -> Option<usize> {
        let regions = self.regions(cx);
        let current = self.region_index(&regions, window, cx);
        if let (Some(i), Some(focused)) = (current, window.focused(cx)) {
            self.focus_regions.note(&regions[i].0, &focused);
        }
        current
    }

    /// Esc in the sidebar or toolbar: close the More menu or the notices
    /// first, otherwise return to the editor.
    pub(in crate::workspace) fn escape_to_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.title.notices_open.get() {
            self.title.notices_open.set(false);
            cx.notify();
            return;
        }
        // In the tree, Esc first clears Unfiled's selection.
        if self.focus_regions.sidebar().contains_focused(window, cx)
            && self.clear_tree_selection(cx)
        {
            return;
        }
        if let Some(view) = self.active_view().filter(|_| !self.showing_overview(cx))
            && view.read(cx).more_open
        {
            view.update(cx, |view, cx| view.close_more(window, cx));
            return;
        }
        self.note_region_focus(window, cx);
        if !self.showing_overview(cx) {
            self.focus_active(window, cx);
        }
    }

    /// Tab / Shift+Tab outside the editor, menus and dialogs.
    /// With no control focused on Home it enters Home's controls. Otherwise
    /// focus follows window order, except that the editor, which is no Tab
    /// stop, is entered rather than skipped: Tab past the toolbar's last
    /// control, and Shift+Tab back from the region after it.
    pub(in crate::workspace) fn tab_from_window(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let from = self.note_region_focus(window, cx);
        if self.focus.is_focused(window) && self.showing_overview(cx) {
            let home = self.focus_regions.home().clone();
            window.focus(&home, cx);
            ui::cycle(window, cx, Some(&home), backwards);
            if home.contains_focused(window, cx) {
                return;
            }
            window.focus(&self.focus, cx);
        }
        let regions = self.regions(cx);
        ui::cycle(window, cx, None, backwards);
        let to = self.region_index(&regions, window, cx);
        if let Some(sidebar::focus_regions::FocusEffect::Focus(editor)) =
            sidebar::focus_regions::FocusRegions::tab_into_editor(&regions, from, to, backwards)
        {
            window.focus(&editor, cx);
        }
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
