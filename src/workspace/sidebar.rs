//! The sidebar's state (ADR 0048, 0027, 0028): expanded or collapsed to the
//! rail, collapsed by itself in a narrow window, its dragged width, the
//! collapsed rail's reveal of the full list, OS files dragged over it, and
//! the list's roving Tab stop. Generic over the focus handle, so its
//! rules run without a window; the workspace view renders it and carries out
//! the returned effects (timers, focus).

use std::time::{Duration, Instant};

/// How long a collapse or reveal slides.
pub(super) const SLIDE: Duration = Duration::from_millis(150);

/// The expanded sidebar's width range and default, in points (ADR 0027).
pub(super) const MIN_WIDTH: f32 = 160.;
pub(super) const MAX_WIDTH: f32 = 320.;
pub(super) const DEFAULT_WIDTH: f32 = 240.;
/// Below this window width the sidebar collapses by itself (ADR 0027).
pub(super) const NARROW_WINDOW: f32 = 900.;

/// The collapsed rail's hover reveal of the full document list (ADR 0027).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reveal {
    Hidden,
    Shown,
    /// Sliding back; unmounted when the animation ends.
    Hiding,
}

/// What the workspace view carries out for a sidebar transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidebarEffect {
    /// The sidebar looks different now.
    Repaint,
    /// Repaint once the collapse or expand slide has ended.
    EndSlide,
    /// Open (after hover intent) or close (after the grace period) the
    /// reveal; only the timer with the latest ticket acts (`fire`).
    Schedule { open: bool, ticket: u64 },
    /// Repaint, and unmount the reveal when its slide back ends
    /// (`finish_hiding`).
    FinishHiding(u64),
}

pub(super) struct Sidebar<H> {
    visible: bool,
    /// The user's pick, saved with the session; none: expanded.
    choice: Option<bool>,
    /// The window is narrower than `NARROW_WINDOW`.
    narrow: bool,
    /// Expanded by the user while the window is narrow; the window widening
    /// ends it, so narrowing again collapses it by itself.
    held_open: bool,
    /// The expanded width, within `MIN_WIDTH..=MAX_WIDTH`.
    width: f32,
    /// A drag of the edge: the width it started from, and whether it moved.
    resize: Option<(f32, bool)>,
    /// Bumped by explicit toggles so only they animate the sidebar width.
    generation: u64,
    /// When the current toggle's slide ends (ADR 0048).
    slide: Option<Instant>,
    reveal: Reveal,
    reveal_generation: u64,
    /// Opened by keyboard focus, so focus inside it also keeps it open.
    keyboard: bool,
    /// Pointer over (rail, reveal).
    hover: (bool, bool),
    /// The timer still allowed to open (`true`) or close (`false`) the reveal.
    pending: Option<(bool, u64)>,
    ticket: u64,
    /// The rail control that keyboard focus came from; Escape returns there.
    opener: Option<H>,
    /// A rail control focused by Escape that must not reveal again.
    escaped: Option<H>,
    /// An OS file drag over the sidebar (ADR 0028): the row under the pointer
    /// with the insertion index its half selects, the list under the pointer,
    /// and the collapsed rail under the pointer.
    drop_row: Option<(u64, usize)>,
    drop_list: bool,
    /// Files over the sidebar, none of which mdoc opens.
    drop_refused: bool,
    drag_rail: bool,
    /// The row the arrows last moved to: the list's one Tab stop.
    cursor: Option<u64>,
}

impl<H> Default for Sidebar<H> {
    fn default() -> Self {
        Self {
            visible: true,
            choice: None,
            narrow: false,
            held_open: false,
            width: DEFAULT_WIDTH,
            resize: None,
            generation: 0,
            slide: None,
            reveal: Reveal::Hidden,
            reveal_generation: 0,
            keyboard: false,
            hover: (false, false),
            pending: None,
            ticket: 0,
            opener: None,
            escaped: None,
            drop_row: None,
            drop_list: false,
            drop_refused: false,
            drag_rail: false,
            cursor: None,
        }
    }
}

impl<H: Clone + PartialEq> Sidebar<H> {
    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    pub(super) fn choice(&self) -> Option<bool> {
        self.choice
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn reveal(&self) -> Reveal {
        self.reveal
    }

    pub(super) fn reveal_generation(&self) -> u64 {
        self.reveal_generation
    }

    /// The rail control the keyboard reveal was opened from.
    pub(super) fn opener(&self) -> Option<&H> {
        self.opener.as_ref()
    }

    pub(super) fn cursor(&self) -> Option<u64> {
        self.cursor
    }

    pub(super) fn set_cursor(&mut self, row: Option<u64>) {
        self.cursor = row;
    }

    /// The saved pick from the last session.
    pub(super) fn restore(&mut self, visible: bool, choice: Option<bool>) {
        self.visible = visible;
        self.choice = choice;
    }

    /// Set the user's pick; it takes effect at the next `settle`. Expanded,
    /// it holds in a narrow window, as a toggle there does.
    #[cfg(test)]
    pub(super) fn choose(&mut self, expanded: Option<bool>) {
        self.choice = expanded;
        self.held_open = expanded == Some(true);
    }

    /// Each frame, for a window `window_width` wide: expanded unless the
    /// user collapsed it, or the window is narrow and the user has not
    /// expanded it since it narrowed. Collapsing or expanding by itself
    /// does not slide and leaves the user's pick alone. Returns whether it
    /// collapsed now.
    pub(super) fn settle(&mut self, window_width: f32) -> bool {
        self.narrow = window_width < NARROW_WINDOW;
        if !self.narrow {
            self.held_open = false;
        }
        let visible = match self.choice {
            Some(false) => false,
            _ => !self.narrow || self.held_open,
        };
        if visible && !self.visible {
            self.end_reveal();
        }
        let collapsed = self.visible && !visible;
        self.visible = visible;
        collapsed
    }

    /// Expand or collapse at `now`; any reveal ends at once. Expanding in a
    /// narrow window holds it open until the window widens.
    pub(super) fn toggle(&mut self, now: Instant) -> SidebarEffect {
        self.visible = !self.visible;
        self.choice = Some(self.visible);
        self.held_open = self.visible && self.narrow;
        self.generation += 1;
        // Only the sidebar layer animates; one repaint ends it.
        self.slide = Some(now + SLIDE);
        self.end_reveal();
        SidebarEffect::EndSlide
    }

    fn end_reveal(&mut self) {
        self.reveal = Reveal::Hidden;
        self.pending = None;
        self.hover = (false, false);
        self.keyboard = false;
        self.opener = None;
    }

    /// The expanded width in points.
    pub(super) fn width(&self) -> f32 {
        self.width
    }

    /// The width saved in settings; none or out of range is clamped.
    pub(super) fn restore_width(&mut self, saved: Option<u16>) {
        self.width = saved.map_or(DEFAULT_WIDTH, |width| clamp_width(f32::from(width)));
    }

    /// A drag of the expanded sidebar's edge began.
    pub(super) fn begin_resize(&mut self) {
        self.resize = Some((self.width, false));
    }

    pub(super) fn resizing(&self) -> bool {
        self.resize.is_some()
    }

    /// The edge moved to `width` points from the sidebar's left; whether the
    /// sidebar looks different now.
    pub(super) fn resize_to(&mut self, width: f32) -> bool {
        let Some((_, moved)) = &mut self.resize else {
            return false;
        };
        let width = clamp_width(width);
        if width == self.width {
            return false;
        }
        *moved = true;
        self.width = width;
        true
    }

    /// The drag ended: the whole-point width to save, if it changed.
    pub(super) fn end_resize(&mut self) -> Option<u16> {
        let (start, moved) = self.resize.take()?;
        self.width = self.width.round();
        (moved && self.width != start).then_some(self.width as u16)
    }

    /// Whether a toggle's slide is still running at `now`.
    pub(super) fn sliding(&self, now: Instant) -> bool {
        self.slide.is_some_and(|until| now < until)
    }

    /// The pointer entered or left the rail (`panel` false) or the reveal.
    pub(super) fn hover(&mut self, panel: bool, hovered: bool) -> Option<SidebarEffect> {
        if panel {
            self.hover.1 = hovered;
        } else {
            self.hover.0 = hovered;
        }
        self.update_reveal()
    }

    /// Open or close the reveal for the pointer, or for OS files dragged over
    /// the rail, which GPUI does not report as hover (ADR 0028).
    pub(super) fn update_reveal(&mut self) -> Option<SidebarEffect> {
        if self.visible {
            return None;
        }
        let pending = self.pending.map(|(open, _)| open);
        let hovered = self.hover.0 || self.hover.1 || self.drag_rail;
        match (hovered, self.reveal) {
            (true, Reveal::Shown) => self.pending = None,
            (true, Reveal::Hiding) => {
                self.show(false);
                return Some(SidebarEffect::Repaint);
            }
            (true, Reveal::Hidden) if pending != Some(true) => return Some(self.schedule(true)),
            (false, Reveal::Shown) if pending != Some(false) => {
                return Some(self.schedule(false));
            }
            (false, Reveal::Hidden) => self.pending = None,
            _ => {}
        }
        None
    }

    /// Open after hover intent, or close after the grace period. Only the
    /// most recently scheduled timer acts.
    pub(super) fn schedule(&mut self, open: bool) -> SidebarEffect {
        self.ticket += 1;
        self.pending = Some((open, self.ticket));
        SidebarEffect::Schedule {
            open,
            ticket: self.ticket,
        }
    }

    /// A scheduled timer ran out: whether it is still the one to act.
    pub(super) fn fire(&mut self, open: bool, ticket: u64) -> bool {
        if self.pending != Some((open, ticket)) {
            return false;
        }
        self.pending = None;
        true
    }

    /// The pointer, or keyboard focus inside a keyboard reveal, keeps the
    /// reveal open; the view adds menus and drags.
    pub(super) fn holds(&self, focus_inside: impl FnOnce() -> bool) -> bool {
        self.hover.0 || self.hover.1 || (self.keyboard && focus_inside())
    }

    pub(super) fn show(&mut self, keyboard: bool) {
        self.pending = None;
        if self.reveal != Reveal::Shown {
            self.reveal_generation += 1;
            self.reveal = Reveal::Shown;
            self.keyboard = keyboard;
        }
    }

    /// Slide the reveal back; with reduced motion it unmounts at once.
    /// Nothing to do unless it is shown.
    pub(super) fn dismiss(&mut self, reduce_motion: bool) -> Option<SidebarEffect> {
        self.pending = None;
        if self.reveal != Reveal::Shown {
            return None;
        }
        self.keyboard = false;
        self.opener = None;
        self.reveal_generation += 1;
        if reduce_motion {
            self.reveal = Reveal::Hidden;
            self.hover.1 = false;
            Some(SidebarEffect::Repaint)
        } else {
            self.reveal = Reveal::Hiding;
            Some(SidebarEffect::FinishHiding(self.reveal_generation))
        }
    }

    /// The slide back ended: unmount unless the reveal reopened since.
    pub(super) fn finish_hiding(&mut self, generation: u64) -> Option<SidebarEffect> {
        if self.reveal == Reveal::Hiding && self.reveal_generation == generation {
            self.reveal = Reveal::Hidden;
            self.hover.1 = false;
            Some(SidebarEffect::Repaint)
        } else {
            None
        }
    }

    /// Whether keyboard focus on a rail control may open the reveal: the
    /// sidebar is collapsed, the reveal is not shown, and Escape did not just
    /// return focus to a control that is still focused.
    pub(super) fn keyboard_may_open(&mut self, focused: impl Fn(&H) -> bool) -> bool {
        if self.visible {
            return false;
        }
        if self.escaped.as_ref().is_some_and(|focus| !focused(focus)) {
            self.escaped = None;
        }
        self.reveal != Reveal::Shown && self.escaped.is_none()
    }

    /// Keyboard focus on rail control `opener` reveals the full list.
    pub(super) fn open_for_keyboard(&mut self, opener: H) {
        self.show(true);
        self.keyboard = true;
        self.opener = Some(opener);
    }

    /// Whether focus leaving a keyboard reveal closes it.
    pub(super) fn keyboard_reveal(&self) -> bool {
        !self.visible && self.reveal == Reveal::Shown && self.keyboard
    }

    /// Escape returned focus to rail control `focus`; it must not reveal
    /// again while it keeps focus.
    pub(super) fn escape_to(&mut self, focus: H) {
        self.escaped = Some(focus);
    }

    /// A rail row closed. Returns whether it opened the reveal now shown.
    pub(super) fn forget_opener(&mut self, removed: Option<&H>) -> bool {
        let opener = removed.is_some() && self.opener.as_ref() == removed;
        if opener {
            self.opener = None;
        }
        opener && self.reveal == Reveal::Shown
    }

    /// OS files moved over the list's row `id`; `row` is the insertion it
    /// selects, none once the pointer left it.
    pub(super) fn drag_over_row(
        &mut self,
        id: u64,
        row: Option<(u64, usize)>,
    ) -> Option<SidebarEffect> {
        if row.is_some() && self.drop_row != row {
            self.drop_row = row;
        } else if row.is_none() && self.drop_row.is_some_and(|(row, _)| row == id) {
            self.drop_row = None;
        } else {
            return None;
        }
        Some(SidebarEffect::Repaint)
    }

    /// OS files moved over the list, `refused` when mdoc opens none of them.
    pub(super) fn drag_over_list(&mut self, over: bool, refused: bool) -> Option<SidebarEffect> {
        if self.drop_list == over && self.drop_refused == refused {
            return None;
        }
        self.drop_list = over;
        self.drop_refused = refused;
        Some(SidebarEffect::Repaint)
    }

    /// OS files moved onto or off the collapsed rail: they reveal the list
    /// like the pointer.
    pub(super) fn drag_over_rail(&mut self, over: bool) -> Option<SidebarEffect> {
        if self.drag_rail == over {
            return None;
        }
        self.drag_rail = over;
        self.update_reveal()
    }

    pub(super) fn drop_refused(&self) -> bool {
        self.drop_refused
    }

    /// Whether an OS drag left an insertion line, refusal or rail hover.
    pub(super) fn dragging(&self) -> bool {
        self.drag_rail || self.drop_list || self.drop_refused || self.drop_row.is_some()
    }

    /// Where files dropped on the list of `tabs` rows would open.
    pub(super) fn drop_index(&self, tabs: usize) -> Option<usize> {
        if self.drop_refused {
            return None;
        }
        self.drop_row
            .map(|(_, index)| index)
            .or(self.drop_list.then_some(tabs))
    }

    /// The drag ended; one over the rail may close the reveal.
    pub(super) fn clear_drag(&mut self) -> Option<SidebarEffect> {
        self.drop_row = None;
        self.drop_list = false;
        self.drop_refused = false;
        if std::mem::take(&mut self.drag_rail) {
            self.update_reveal()
        } else {
            None
        }
    }
}

fn clamp_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(MIN_WIDTH, MAX_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 1000.;
    const NARROW: f32 = 720.;

    fn collapsed() -> Sidebar<u8> {
        let mut sidebar = Sidebar::default();
        sidebar.restore(false, Some(false));
        sidebar
    }

    #[test]
    fn a_toggle_pins_the_pick_slides_once_and_ends_any_reveal() {
        let mut sidebar = collapsed();
        sidebar.show(false);
        let now = Instant::now();
        assert_eq!(sidebar.toggle(now), SidebarEffect::EndSlide);
        assert!(sidebar.visible());
        assert_eq!(sidebar.choice(), Some(true));
        assert_eq!(sidebar.generation(), 1);
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        assert!(sidebar.sliding(now));
        assert!(!sidebar.sliding(now + SLIDE));
        // Expanded, the pointer never reveals.
        assert_eq!(sidebar.hover(false, true), None);
        sidebar.choose(None);
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
    }

    #[test]
    fn a_narrow_window_collapses_it_and_widening_brings_it_back() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        assert!(sidebar.settle(NARROW_WINDOW - 1.));
        assert!(!sidebar.visible());
        assert!(!sidebar.settle(NARROW));
        // Collapsed by itself: no slide, and the user's pick is untouched.
        assert_eq!(sidebar.generation(), 0);
        assert_eq!(sidebar.choice(), None);
        sidebar.settle(NARROW_WINDOW);
        assert!(sidebar.visible());
        // A reveal open while narrow ends when the window widens.
        sidebar.settle(NARROW);
        sidebar.show(false);
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        // An explicit expand earlier in the session does not stop it.
        let now = Instant::now();
        sidebar.toggle(now);
        sidebar.toggle(now);
        assert_eq!(sidebar.choice(), Some(true));
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
    }

    #[test]
    fn collapsed_by_the_user_it_stays_collapsed_when_the_window_widens() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(WIDE);
        sidebar.toggle(Instant::now());
        sidebar.settle(NARROW);
        sidebar.settle(WIDE);
        assert!(!sidebar.visible());
        assert_eq!(sidebar.choice(), Some(false));
        // Collapsed by the user while it was already narrow: the same.
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(NARROW);
        sidebar.toggle(Instant::now());
        sidebar.settle(NARROW);
        assert!(sidebar.visible());
        sidebar.toggle(Instant::now());
        sidebar.settle(WIDE);
        assert!(!sidebar.visible());
    }

    #[test]
    fn expanded_in_a_narrow_window_it_stays_until_the_window_widens() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
        assert_eq!(sidebar.toggle(Instant::now()), SidebarEffect::EndSlide);
        sidebar.settle(NARROW);
        sidebar.settle(NARROW - 100.);
        assert!(sidebar.visible());
        // Widening ends the hold; narrowing again collapses it by itself.
        sidebar.settle(WIDE);
        assert!(sidebar.visible());
        sidebar.settle(NARROW);
        assert!(!sidebar.visible());
    }

    #[test]
    fn dragging_the_edge_resizes_within_its_range_and_saves_once() {
        let mut sidebar = Sidebar::<u8>::default();
        assert_eq!(sidebar.width(), DEFAULT_WIDTH);
        // Without a drag, a move does nothing.
        assert!(!sidebar.resize_to(300.));
        assert_eq!(sidebar.end_resize(), None);
        sidebar.begin_resize();
        assert!(sidebar.resizing());
        assert!(sidebar.resize_to(280.4));
        assert!(!sidebar.resize_to(280.4));
        assert!(sidebar.resize_to(1000.));
        assert_eq!(sidebar.width(), MAX_WIDTH);
        assert!(sidebar.resize_to(10.));
        assert_eq!(sidebar.width(), MIN_WIDTH);
        assert!(sidebar.resize_to(280.4));
        assert_eq!(sidebar.end_resize(), Some(280));
        assert!(!sidebar.resizing());
        assert_eq!(sidebar.width(), 280.);
        // A drag that ends where it began saves nothing.
        sidebar.begin_resize();
        assert!(sidebar.resize_to(200.));
        assert!(sidebar.resize_to(280.));
        assert_eq!(sidebar.end_resize(), None);
        sidebar.begin_resize();
        assert_eq!(sidebar.end_resize(), None);
    }

    #[test]
    fn a_saved_width_is_clamped_to_the_range() {
        let mut sidebar = Sidebar::<u8>::default();
        sidebar.restore_width(Some(300));
        assert_eq!(sidebar.width(), 300.);
        sidebar.restore_width(Some(40));
        assert_eq!(sidebar.width(), MIN_WIDTH);
        sidebar.restore_width(Some(u16::MAX));
        assert_eq!(sidebar.width(), MAX_WIDTH);
        sidebar.restore_width(None);
        assert_eq!(sidebar.width(), DEFAULT_WIDTH);
    }

    #[test]
    fn hover_opens_after_intent_and_only_the_latest_timer_acts() {
        let mut sidebar = collapsed();
        let Some(SidebarEffect::Schedule { open: true, ticket }) = sidebar.hover(false, true)
        else {
            panic!("hover schedules an open");
        };
        // Leaving and coming back before the timer runs schedules again.
        assert_eq!(sidebar.hover(false, false), None);
        let Some(SidebarEffect::Schedule {
            open: true,
            ticket: later,
        }) = sidebar.hover(false, true)
        else {
            panic!("hover schedules an open");
        };
        assert!(!sidebar.fire(true, ticket));
        assert!(sidebar.fire(true, later));
        sidebar.show(false);
        assert_eq!(sidebar.reveal(), Reveal::Shown);
        // The pointer moves from the rail onto the reveal: it stays.
        assert_eq!(sidebar.hover(true, true), None);
        assert_eq!(sidebar.hover(false, false), None);
        assert!(sidebar.holds(|| false));
        let Some(SidebarEffect::Schedule {
            open: false,
            ticket,
        }) = sidebar.hover(true, false)
        else {
            panic!("leaving schedules a close");
        };
        assert!(!sidebar.holds(|| true));
        assert!(sidebar.fire(false, ticket));
        let generation = sidebar.reveal_generation();
        assert_eq!(
            sidebar.dismiss(false),
            Some(SidebarEffect::FinishHiding(generation + 1))
        );
        assert_eq!(sidebar.reveal(), Reveal::Hiding);
        // Hovering again while it slides back shows it at once.
        assert_eq!(sidebar.hover(false, true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.reveal(), Reveal::Shown);
        assert_eq!(sidebar.finish_hiding(generation + 1), None);
        assert!(sidebar.hover(false, false).is_some());
        assert_eq!(sidebar.dismiss(true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.reveal(), Reveal::Hidden);
        assert_eq!(sidebar.dismiss(true), None);
    }

    #[test]
    fn a_keyboard_reveal_returns_to_its_opener_and_escape_holds_it_shut() {
        let mut sidebar = collapsed();
        assert!(sidebar.keyboard_may_open(|_| false));
        sidebar.open_for_keyboard(7);
        assert_eq!(sidebar.opener(), Some(&7));
        assert!(sidebar.keyboard_reveal());
        assert!(sidebar.holds(|| true));
        assert!(!sidebar.holds(|| false));
        assert!(!sidebar.keyboard_may_open(|_| true));
        // Escape: focus goes back to the opener, which must not reveal again
        // until focus has left it.
        sidebar.escape_to(7);
        assert_eq!(sidebar.dismiss(true), Some(SidebarEffect::Repaint));
        assert_eq!(sidebar.opener(), None);
        assert!(!sidebar.keyboard_may_open(|focus| *focus == 7));
        assert!(sidebar.keyboard_may_open(|_| false));
        // Closing the opener's tab forgets it, and says the reveal was its.
        sidebar.open_for_keyboard(9);
        assert!(!sidebar.forget_opener(None));
        assert!(!sidebar.forget_opener(Some(&7)));
        assert!(sidebar.forget_opener(Some(&9)));
        assert_eq!(sidebar.opener(), None);
    }

    #[test]
    fn files_dragged_over_the_list_mark_where_they_open() {
        let mut sidebar = collapsed();
        let repaint = Some(SidebarEffect::Repaint);
        assert_eq!(sidebar.drop_index(3), None);
        assert_eq!(sidebar.drag_over_list(true, false), repaint);
        assert_eq!(sidebar.drag_over_list(true, false), None);
        assert_eq!(sidebar.drop_index(3), Some(3));
        assert_eq!(sidebar.drag_over_row(5, Some((5, 1))), repaint);
        assert_eq!(sidebar.drag_over_row(5, Some((5, 1))), None);
        assert_eq!(sidebar.drop_index(3), Some(1));
        // Leaving another row leaves this one's line.
        assert_eq!(sidebar.drag_over_row(6, None), None);
        assert_eq!(sidebar.drag_over_row(5, None), repaint);
        assert_eq!(sidebar.drag_over_list(true, true), repaint);
        assert!(sidebar.drop_refused());
        assert_eq!(sidebar.drop_index(3), None);
        // Over the rail, the drag reveals the list like hover.
        assert!(matches!(
            sidebar.drag_over_rail(true),
            Some(SidebarEffect::Schedule { open: true, .. })
        ));
        assert_eq!(sidebar.drag_over_rail(true), None);
        assert!(sidebar.dragging());
        // Ending it cancels the pending open.
        assert_eq!(sidebar.clear_drag(), None);
        assert!(!sidebar.dragging());
        assert_eq!(sidebar.clear_drag(), None);
    }
}
