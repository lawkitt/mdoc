//! Before/after view: the original wording beside the
//! pseudonymized text, both formatted like the editor and read-only, scrolling
//! together. Live: it follows every change to the applied replacements, with
//! the editor's replacement motion.
mod actions;
mod view;
use super::text;
use super::{APPLIED_ID, CopyFrom};
// What the moved bodies reach as `super::` from `actions`.
use super::{check_hand_copies, restore};
use crate::{document::DocumentView, ui::style};
use gpui::{Context, Entity, FocusHandle, Pixels, Subscription};
#[cfg(test)]
use gpui::{Focusable, px};
use std::{collections::HashMap, ops::Range};

/// Below this width the view shows one column at a time.
const NARROW: f32 = 720.;
/// Where the column labels and Close float from the view's top.
const FLOAT_TOP: f32 = 12.;
/// A column label's height: 11 pt text and its border.
const LABEL_HEIGHT: f32 = 22.;

/// Which columns an open before/after view shows, from the view switcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompareMode {
    /// Both columns (one at a time in a narrow window).
    Side,
    Original,
    Pseudonymized,
    /// The restored AI reply, read-only.
    Reply,
}

/// The two read-only editors of an open before/after view.
pub(crate) struct CompareState {
    original: Entity<mdoc_editor::EditorState>,
    current: Entity<mdoc_editor::EditorState>,
    /// The Reply view's editor.
    reply: Entity<mdoc_editor::EditorState>,
    pub(super) left: gpui::ScrollHandle,
    pub(super) right: gpui::ScrollHandle,
    reply_scroll: gpui::ScrollHandle,
    /// Reply/review revisions and theme the Reply view shows.
    reply_key: Option<(u64, u64, style::Theme)>,
    /// Scroll offsets at the last render, to tell which pane moved.
    last: std::cell::Cell<(Pixels, Pixels)>,
    focus: FocusHandle,
    /// Editor/review revisions and theme shown; warm renders do no content scan.
    key: Option<(u64, u64, style::Theme)>,
    /// Per shown annotation id: its alias and its span in the original text.
    shown: HashMap<u64, (String, Range<usize>)>,
    /// The original text shown, and each mark's (original, pseudonymized) span.
    before: String,
    marks: Vec<(Range<usize>, Range<usize>)>,
    /// Opening hid the PDF preview; closing shows it again.
    hid_preview: bool,
    /// In a narrow window, the one column shown is the original.
    original_only: bool,
    pub(crate) mode: CompareMode,
    _subscriptions: [Subscription; 2],
}

/// A kept replacement: its input index and its span in the original and in
/// the pseudonymized text.
type Mark = (usize, Range<usize>, Range<usize>);

/// The whole original text and its marks.
pub(crate) fn compare_texts(after: &str, applied: &[(Range<usize>, &str)]) -> (String, Vec<Mark>) {
    let mut before = String::with_capacity(after.len());
    let mut marks = Vec::new();
    let mut at = 0;
    for (i, (range, original)) in applied.iter().enumerate() {
        if range.start < at || range.end > after.len() {
            continue;
        }
        before.push_str(&after[at..range.start]);
        let mark = before.len();
        before.push_str(original);
        marks.push((i, mark..before.len(), range.clone()));
        at = range.end;
    }
    before.push_str(&after[at..]);
    (before, marks)
}

/// Map an offset in the original text to the pseudonymized text, given each
/// mark's (original, pseudonymized) span; inside a mark it lands on its start.
fn to_after(offset: usize, marks: &[(Range<usize>, Range<usize>)]) -> usize {
    mapped_offset(offset, marks, false)
}

fn to_before(offset: usize, marks: &[(Range<usize>, Range<usize>)]) -> usize {
    mapped_offset(offset, marks, true)
}

fn mapped_offset(offset: usize, marks: &[(Range<usize>, Range<usize>)], reverse: bool) -> usize {
    let pair = |(before, after): &(Range<usize>, Range<usize>)| {
        if reverse {
            (after.clone(), before.clone())
        } else {
            (before.clone(), after.clone())
        }
    };
    let next = marks.partition_point(|mark| pair(mark).0.end <= offset);
    if let Some(mark) = marks.get(next) {
        let (from, to) = pair(mark);
        if from.start < offset {
            return to.start;
        }
    }
    let shift = next.checked_sub(1).map_or(0, |previous| {
        let (from, to) = pair(&marks[previous]);
        to.end as isize - from.end as isize
    });
    offset.saturating_add_signed(shift)
}

impl CompareState {
    pub(crate) fn reply_editor(&self) -> &Entity<mdoc_editor::EditorState> {
        &self.reply
    }
    /// `range` of the Original column's `text` as a range of the editor's
    /// text, when the column shows that text and review (`now`: their
    /// revisions) and no restored name lies in or across `range`; outside
    /// the restored names the two texts are the same.
    pub(super) fn original_in_editor(
        &self,
        text: &str,
        range: Range<usize>,
        now: (u64, u64),
    ) -> Option<Range<usize>> {
        let shown = self
            .key
            .is_some_and(|(editor, review, _)| (editor, review) == now);
        let restored = (self.marks.iter())
            .any(|(before, _)| before.start < range.end && range.start < before.end);
        (shown && self.before == text && !restored)
            .then(|| to_after(range.start, &self.marks)..to_after(range.end, &self.marks))
    }
}

#[cfg(test)]
impl CompareState {
    /// The Reply view's text.
    pub(crate) fn reply_text(&self, cx: &gpui::App) -> String {
        self.reply.read(cx).text().to_owned()
    }
    /// Whether the Reply view, as last painted, shows the row holding
    /// `offset` inside its viewport.
    pub(crate) fn reply_shows(&self, offset: usize, cx: &gpui::App) -> bool {
        let viewport = self.reply_scroll.bounds();
        self.reply
            .read(cx)
            .offset_screen_top(offset)
            .is_some_and(|top| top >= viewport.top() && top + px(16.) <= viewport.bottom())
    }
    /// The Original and Pseudonymized columns' editors.
    pub(crate) fn columns(
        &self,
    ) -> (
        Entity<mdoc_editor::EditorState>,
        Entity<mdoc_editor::EditorState>,
    ) {
        (self.original.clone(), self.current.clone())
    }
    pub(super) fn current_focus(&self, cx: &gpui::App) -> FocusHandle {
        self.current.read(cx).focus_handle(cx)
    }
    /// Both columns' text and whether the pseudonymized one is animating.
    pub(super) fn texts(&self, cx: &gpui::App) -> (String, String, bool) {
        let current = self.current.read(cx);
        (
            self.original.read(cx).text().to_owned(),
            current.text().to_owned(),
            current.is_rolling_text() || current.is_fading_ranges(),
        )
    }
}

/// How long the view pill stays after the last pointer, scroll, zoom or view
/// change before it fades, so it does not rest over the text.
pub(crate) const SWITCHER_IDLE: std::time::Duration = std::time::Duration::from_millis(1500);
const SWITCHER_FADE: std::time::Duration = std::time::Duration::from_millis(200);
/// The pointer this close to the bottom of the text column wakes the pill.
const SWITCHER_REACH: f32 = 96.;

/// Whether the floating view pill shows. It rests hidden but still takes
/// clicks; the pointer reaches it only through the bottom band that wakes it.
#[derive(Default)]
pub(crate) struct SwitcherRest {
    pub(crate) awake: bool,
    pub(crate) hovered: bool,
    /// When it last appeared or began to fade; `None` before the first change.
    changed_at: Option<std::time::Instant>,
    idle: Option<gpui::Task<()>>,
    /// The Markdown column, measured while drawing.
    pub(crate) column: std::rc::Rc<std::cell::Cell<gpui::Bounds<Pixels>>>,
}

impl SwitcherRest {
    pub(crate) fn shown(&self) -> bool {
        self.awake || self.hovered
    }
    fn settle(&mut self, was_shown: bool, cx: &mut Context<DocumentView>) {
        if self.shown() != was_shown {
            self.changed_at = Some(std::time::Instant::now());
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_is_rebuilt_with_marks_on_both_sides() {
        let after = "| PERSON_1 | ORG_1 |\n\nPERSON_1 left.";
        let (before, marks) = compare_texts(
            after,
            &[
                (2..10, "Anna Petrova"),
                (13..18, "Northwind Ltd"),
                (22..30, "Anna"),
            ],
        );
        assert_eq!(before, "| Anna Petrova | Northwind Ltd |\n\nAnna left.");
        assert_eq!(&before[marks[1].1.clone()], "Northwind Ltd");
        assert_eq!(&after[marks[2].2.clone()], "PERSON_1");
    }

    #[test]
    fn offsets_map_between_the_columns() {
        // "Anna met Bo." → "PERSON_1 met PERSON_2."
        let marks = [(0..4, 0..8), (9..11, 13..21)];
        assert_eq!(to_after(5, &marks), 9);
        assert_eq!(to_after(9, &marks), 13);
        assert_eq!(to_after(11, &marks), 21);
        assert_eq!(to_after(2, &marks), 0);
        assert_eq!(to_before(13, &marks), 9);
        assert_eq!(to_before(21, &marks), 11);
    }
}
