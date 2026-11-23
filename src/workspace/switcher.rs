//! The recency switcher (`⌃Tab`): the live views, most recently viewed
//! first, opening on the one before the shown one. Tab moves on while the
//! modifier is held, and releasing it opens the pick, so a quick
//! press-and-release goes back and forth between the last two. GPUI-free.

pub(super) struct Switcher {
    views: Vec<u64>,
    index: usize,
}

impl Switcher {
    /// Open over `views`, most recent first. With a document shown (it is
    /// `views[0]`), the pick starts on the next one; on Home, on the first.
    /// Backwards starts from the least recent. None with nothing to switch
    /// to.
    pub(super) fn open(views: Vec<u64>, shown: bool, backwards: bool) -> Option<Self> {
        let others = views.len().saturating_sub(usize::from(shown));
        if others == 0 {
            return None;
        }
        let index = if backwards {
            views.len() - 1
        } else {
            usize::from(shown)
        };
        Some(Self { views, index })
    }

    pub(super) fn views(&self) -> &[u64] {
        &self.views
    }

    pub(super) fn picked(&self) -> u64 {
        self.views[self.index]
    }

    /// Move the pick `delta` rows, wrapping.
    pub(super) fn step(&mut self, delta: isize) {
        let len = self.views.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(len) as usize;
    }

    /// Pick view `id` (a click on its row).
    pub(super) fn pick(&mut self, id: u64) {
        if let Some(index) = self.views.iter().position(|view| *view == id) {
            self.index = index;
        }
    }

    /// View `id` was released while the switcher is open; false when none
    /// is left to pick.
    pub(super) fn forget(&mut self, id: u64) -> bool {
        let Some(index) = self.views.iter().position(|view| *view == id) else {
            return true;
        };
        self.views.remove(index);
        if self.views.is_empty() {
            return false;
        }
        if self.index > index || self.index == self.views.len() {
            self.index = self.index.saturating_sub(1);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_opens_on_the_view_before_the_shown_one_and_wraps() {
        assert!(
            Switcher::open(vec![1], true, false).is_none(),
            "nothing else"
        );
        assert!(Switcher::open(vec![], false, false).is_none());
        let mut switcher = Switcher::open(vec![3, 2, 1], true, false).unwrap();
        assert_eq!(switcher.picked(), 2);
        switcher.step(1);
        assert_eq!(switcher.picked(), 1);
        switcher.step(1);
        assert_eq!(switcher.picked(), 3);
        switcher.step(-1);
        assert_eq!(switcher.picked(), 1);
        // On Home the most recent is the first pick; backwards the oldest.
        assert_eq!(Switcher::open(vec![1], false, false).unwrap().picked(), 1);
        assert_eq!(
            Switcher::open(vec![3, 2, 1], true, true).unwrap().picked(),
            1
        );
        switcher.pick(2);
        assert_eq!(switcher.picked(), 2);
        assert_eq!(switcher.views(), [3, 2, 1]);
    }

    #[test]
    fn a_released_view_leaves_the_list_and_the_pick_stays_on_its_row() {
        let mut switcher = Switcher::open(vec![4, 3, 2, 1], true, false).unwrap();
        switcher.step(1);
        assert_eq!(switcher.picked(), 2);
        assert!(switcher.forget(4));
        assert_eq!(switcher.picked(), 2);
        assert!(switcher.forget(2));
        assert_eq!(switcher.picked(), 1);
        assert!(switcher.forget(9));
        assert!(switcher.forget(1));
        assert_eq!(switcher.picked(), 3);
        assert!(!switcher.forget(3));
    }
}
