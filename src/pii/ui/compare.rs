//! Before/after view (ADR 0041 #31, ADR 0042): the original wording beside the
//! pseudonymized text, both formatted like the editor and read-only, scrolling
//! together. Live: it follows every change to the applied replacements, with
//! the editor's replacement motion.
use super::*;
use crate::ui;
use gpui::{AnyElement, Entity, Pixels, SharedString, Subscription, div, prelude::*};
use std::collections::{HashMap, HashSet};

/// Below this width the view shows one column at a time.
const NARROW: f32 = 720.;

/// Which columns an open before/after view shows (ADR 0046's view switcher).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompareMode {
    /// Both columns (one at a time in a narrow window).
    Side,
    Original,
    Pseudonymized,
}

/// The two read-only editors of an open before/after view.
pub(crate) struct CompareState {
    original: Entity<mdoc_editor::EditorState>,
    current: Entity<mdoc_editor::EditorState>,
    pub(super) left: gpui::ScrollHandle,
    pub(super) right: gpui::ScrollHandle,
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

#[cfg(test)]
impl CompareState {
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

impl DocumentView {
    pub(crate) fn toggle_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.compare.is_some() || !self.has_applied_aliases() {
            self.close_compare(window, cx);
            return;
        }
        let theme = self.theme.get();
        let pane = |window: &mut Window, cx: &mut Context<Self>| {
            let editor = cx.new(|cx| {
                let mut editor = mdoc_editor::EditorState::new(window, cx);
                editor.set_markdown_style(crate::style::markdown_style(theme), cx);
                editor.set_read_only(true, cx);
                editor
            });
            // A name clicked in either column selects its Replacements row.
            let subscription = cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
                let id = match event {
                    mdoc_editor::EditorEvent::ActivateAnnotation(id) => *id,
                    mdoc_editor::EditorEvent::ActivateAnnotations(ids) => match ids.first() {
                        Some(id) => *id,
                        None => return,
                    },
                    _ => return,
                };
                if !this.pii.mapping.open {
                    this.pii.mapping.open_panel(window.focused(cx));
                }
                this.panel_select_mention(id, cx);
            });
            (editor, subscription)
        };
        let (original, left) = pane(window, cx);
        let (current, right) = pane(window, cx);
        self.pii.dismiss_popup();
        let hid_preview = std::mem::take(&mut self.preview.visible);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.compare = Some(CompareState {
            original,
            current,
            left: gpui::ScrollHandle::new(),
            right: gpui::ScrollHandle::new(),
            last: Default::default(),
            focus,
            key: None,
            shown: HashMap::new(),
            before: String::new(),
            marks: Vec::new(),
            hid_preview,
            original_only: false,
            mode: CompareMode::Side,
            _subscriptions: [left, right],
        });
        self.refresh_compare(window, cx);
        cx.notify();
    }
    /// The view switcher's choice: `None` is Edit (ADR 0046).
    pub(crate) fn set_view_mode(
        &mut self,
        mode: Option<CompareMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wake_switcher(cx);
        match mode {
            None => {
                if self.compare.is_some() {
                    self.close_compare(window, cx);
                }
            }
            Some(mode) => {
                if self.compare.is_none() {
                    self.toggle_compare(window, cx);
                }
                if let Some(state) = self.compare.as_mut() {
                    state.mode = mode;
                }
                cx.notify();
            }
        }
    }
    /// Show the view pill, then let it fade after [`SWITCHER_IDLE`] unless
    /// the pointer stays on it. Pointer, scroll, zoom and view changes call
    /// this; repeated calls only restart the idle timer.
    pub(crate) fn wake_switcher(&mut self, cx: &mut Context<Self>) {
        let shown = self.switcher.shown();
        self.switcher.awake = true;
        self.switcher.settle(shown, cx);
        self.switcher.idle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SWITCHER_IDLE).await;
            let _ = this.update(cx, |this, cx| {
                let shown = this.switcher.shown();
                this.switcher.awake = false;
                this.switcher.idle = None;
                this.switcher.settle(shown, cx);
            });
        }));
    }
    /// The pointer near the bottom of the text wakes the pill.
    pub(crate) fn switcher_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let column = self.switcher.column.get();
        if column.contains(&position) && position.y > column.bottom() - px(SWITCHER_REACH) {
            self.wake_switcher(cx);
        }
    }
    /// The floating pill at the bottom of the document (ADR 0048): zoom
    /// always; Edit · Original · Pseudonymized · Side by side once aliases are
    /// applied (ADR 0046).
    pub(crate) fn view_switcher(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.source_only {
            return None;
        }
        let views = self.has_applied_aliases();
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let current = self.compare.as_ref().map(|state| state.mode);
        let segment = |id: &'static str, label: &'static str, mode: Option<CompareMode>| {
            let selected = current == mode;
            ui::control(id, label, theme, true)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .aria_toggled(if selected {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .px_3()
                .text_size(px(12.))
                .when(selected, |v| {
                    v.bg(palette.placeholder_bg).text_color(palette.header_fg)
                })
                .when(!selected, |v| v.text_color(palette.header_muted))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.set_view_mode(mode, window, cx)),
                )
        };
        let zoom_step = |id: &'static str, label: &'static str, delta: f32| {
            ui::control(id, label, theme, true)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .px_2()
                .text_size(px(13.))
                .text_color(palette.header_muted)
                .on_click(cx.listener(move |this, _, _, cx| this.set_zoom(this.zoom + delta, cx)))
        };
        let percent = format!("{}%", (self.zoom * 100.).round() as i32);
        // Fade only on a change of visibility; the first frame and Reduce
        // Motion jump straight to the resting state. Opacity is computed here
        // rather than with a keyed animation so the pill's element ids, and a
        // click in progress, survive the change.
        let progress = match self.switcher.changed_at {
            Some(at) if !cx.reduce_motion() => {
                (at.elapsed().as_secs_f32() / SWITCHER_FADE.as_secs_f32()).min(1.)
            }
            _ => 1.,
        };
        if progress < 1. {
            window.request_animation_frame();
        }
        let opacity = if self.switcher.shown() {
            progress
        } else {
            1. - progress
        };
        Some(
            div()
                .absolute()
                .bottom(px(16.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("view-switcher")
                        .when(cfg!(test), |v| v.debug_selector(|| "view-switcher".into()))
                        .role(gpui::Role::Group)
                        .aria_label("View")
                        .flex()
                        .items_center()
                        .gap_1()
                        .p_1()
                        .rounded_xl()
                        .bg(palette.bg)
                        .border_1()
                        .border_color(palette.border)
                        .shadow_md()
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            let shown = this.switcher.shown();
                            this.switcher.hovered = *hovered;
                            this.switcher.settle(shown, cx);
                            if !*hovered {
                                this.wake_switcher(cx);
                            }
                        }))
                        .when(views, |v| {
                            v.child(segment("view-edit", "Edit", None))
                                .child(segment(
                                    "view-original",
                                    "Original",
                                    Some(CompareMode::Original),
                                ))
                                .child(segment(
                                    "view-pseudonymized",
                                    "Pseudonymized",
                                    Some(CompareMode::Pseudonymized),
                                ))
                                .child(segment(
                                    "view-side",
                                    "Side by side",
                                    Some(CompareMode::Side),
                                ))
                                .child(div().w(px(1.)).h(px(16.)).mx_1().bg(palette.border))
                        })
                        .child(zoom_step("zoom-out", "−", -crate::document_view::ZOOM_STEP))
                        .child(
                            ui::control("zoom-reset", percent, theme, true)
                                .when(cfg!(test), |v| v.debug_selector(|| "zoom-reset".into()))
                                .aria_label("Reset zoom")
                                .min_w(px(48.))
                                .flex()
                                .justify_center()
                                .text_size(px(12.))
                                .on_click(cx.listener(|this, _, _, cx| this.set_zoom(1., cx))),
                        )
                        .child(zoom_step("zoom-in", "+", crate::document_view::ZOOM_STEP))
                        .opacity(opacity),
                )
                .into_any_element(),
        )
    }
    pub(crate) fn close_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.compare.take()
            && state.hid_preview
        {
            self.preview.visible = true;
        }
        // A popup opened by panel decisions meanwhile belongs to no visible text.
        self.pii.dismiss_popup();
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    /// Bring an open view up to date with the editor and its applied
    /// replacements; cheap when nothing changed. New replacements settle in and
    /// roll as in the editor, reverted ones fade out.
    pub(crate) fn refresh_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        // Focus handed back to the hidden editor (e.g. by closing the panel)
        // moves to the view, so Esc still reaches it.
        if self.editor.read(cx).focus_handle(cx).is_focused(window) {
            window.focus(&state.focus, cx);
        }
        let editor = self.editor.read(cx);
        let key = Some((
            editor.revision(),
            self.pii.review.revision(),
            self.theme.get(),
        ));
        if key == state.key {
            return;
        }
        let text = editor.text();
        let applied: Vec<_> = self
            .pii
            .review
            .applied()
            .iter()
            .filter(|a| text.get(a.range.clone()) == Some(a.step.after.as_ref()))
            .collect();
        if applied.is_empty() {
            self.close_compare(window, cx);
            return;
        }
        let text = text.to_owned();
        let pairs: Vec<_> = applied
            .iter()
            .map(|a| (a.range.clone(), a.step.original()))
            .collect();
        let (before, marks) = compare_texts(&text, &pairs);
        let theme = self.theme.get();
        let blue = theme.applied();
        let gold = theme.proposed();
        // Original wording is gold, aliases blue, as in the editor's review.
        let annotation = |id, range, accent: Hsla| mdoc_editor::SourceAnnotation {
            id,
            range,
            color: Hsla { a: 0.16, ..accent },
            active_color: Hsla { a: 0.34, ..accent },
            border: Hsla { a: 0.5, ..accent },
            text_color: Some(accent),
            underline: None,
        };
        let mut shown = HashMap::with_capacity(marks.len());
        let (mut left_marks, mut right_marks) = (Vec::new(), Vec::new());
        let mut added = HashSet::new();
        let mut rolls: Vec<(Range<usize>, SharedString)> = Vec::new();
        for (i, before_range, after_range) in &marks {
            let a = applied[*i];
            let id = APPLIED_ID | a.id;
            match state.shown.get(&id) {
                None => {
                    added.insert(id);
                    rolls.push((after_range.clone(), a.step.original().to_owned().into()));
                }
                Some((alias, _)) if alias.as_str() != a.step.after.as_ref() => {
                    rolls.push((after_range.clone(), alias.clone().into()));
                }
                Some(_) => {}
            }
            shown.insert(id, (a.step.after.to_string(), before_range.clone()));
            left_marks.push(annotation(id, before_range.clone(), gold));
            right_marks.push(annotation(id, after_range.clone(), blue));
        }
        let spans: Vec<_> = marks.into_iter().map(|(_, b, a)| (b, a)).collect();
        // Reverted names go back to their original wording: a ghost of the
        // chip fades from both columns and the alias rolls away. The original
        // text stays put, so its old spans still hold.
        let mut ghosts = Vec::new();
        if state.before == before {
            for (id, (alias, range)) in &state.shown {
                if shown.contains_key(id) {
                    continue;
                }
                let start = to_after(range.start, &spans);
                let right = start..start + range.len();
                ghosts.push((range.clone(), right.clone()));
                rolls.push((right, alias.clone().into()));
            }
        }
        let animate = state.key.is_some() && !cx.reduce_motion();
        let surface = theme.pdf_style().bg;
        let fill =
            |range: Range<usize>, c: Hsla| (range, Hsla { a: 0.22, ..c }, Hsla { a: 0.7, ..c });
        // The alias settles from gold into blue; its original glows in gold.
        let settle = |ids: HashSet<u64>, from: Hsla, to: Hsla| mdoc_editor::AnnotationFlash {
            ids,
            from: (Hsla { a: 0.22, ..from }, Hsla { a: 0.6, ..from }),
            peak: (Hsla { a: 0.3, ..to }, Hsla { a: 0.85, ..to }),
            duration: std::time::Duration::from_millis(700),
            mark: None,
        };
        let left_ghosts: Vec<_> = ghosts.iter().map(|(l, _)| fill(l.clone(), gold)).collect();
        let right_ghosts: Vec<_> = ghosts.into_iter().map(|(_, r)| fill(r, blue)).collect();
        let (original, current) = (state.original.clone(), state.current.clone());
        original.update(cx, |editor, cx| {
            editor.set_markdown_style(crate::style::markdown_style(theme), cx);
            if editor.text() != before {
                editor.set_text(before.clone(), cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, left_marks, cx);
            if animate {
                editor.flash_annotations(settle(added.clone(), gold, gold), cx);
                editor.flash_ranges(
                    mdoc_editor::RangeFlash {
                        revision,
                        ranges: left_ghosts,
                        duration: std::time::Duration::from_millis(700),
                    },
                    cx,
                );
            }
        });
        current.update(cx, |editor, cx| {
            editor.set_markdown_style(crate::style::markdown_style(theme), cx);
            if editor.text() != text {
                editor.set_text(text, cx);
            }
            let revision = editor.revision();
            editor.set_annotations(revision, right_marks, cx);
            if animate {
                editor.flash_annotations(settle(added, gold, blue), cx);
                editor.flash_ranges(
                    mdoc_editor::RangeFlash {
                        revision,
                        ranges: right_ghosts,
                        duration: std::time::Duration::from_millis(700),
                    },
                    cx,
                );
                editor.roll_text(
                    mdoc_editor::TextRoll {
                        revision,
                        items: rolls,
                        surface,
                        duration: std::time::Duration::from_millis(600),
                    },
                    cx,
                );
            }
        });
        let state = self.compare.as_mut().expect("checked above");
        state.key = key;
        state.shown = shown;
        state.before = before;
        state.marks = spans;
    }
    /// Show the replacement at `offset` (in the editor text) in both columns:
    /// scroll it into view when it is outside, then mark and flash it.
    pub(super) fn reveal_in_compare(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        let review = &self.pii.review;
        let id = review
            .applied()
            .iter()
            .find(|a| a.range.start == offset)
            .map(|a| APPLIED_ID | a.id);
        // A pending mention is plain text in both columns: a gold ghost
        // marks it instead.
        let pending = review
            .candidates()
            .iter()
            .find(|c| id.is_none() && c.range.start == offset)
            .map(|c| c.range.len());
        let left_offset = to_before(offset, &state.marks);
        let theme = self.theme.get();
        for (editor, scroll, at, accent) in [
            (&state.original, &state.left, left_offset, theme.proposed()),
            (&state.current, &state.right, offset, theme.applied()),
        ] {
            let viewport = scroll.bounds();
            if let Some(top) = editor.read(cx).offset_screen_top(at)
                && (top < viewport.top() || top + px(24.) > viewport.bottom())
            {
                let delta = top - viewport.top() - px(48.);
                let mut position = scroll.offset();
                position.y = (position.y - delta).clamp(-scroll.max_offset().y, px(0.));
                scroll.set_offset(position);
            }
            editor.update(cx, |editor, cx| {
                editor.set_active_annotation(id, cx);
                if let Some(len) = pending.filter(|_| !cx.reduce_motion()) {
                    let gold = theme.proposed();
                    let revision = editor.revision();
                    editor.flash_ranges(
                        mdoc_editor::RangeFlash {
                            revision,
                            ranges: vec![(
                                at..at + len,
                                Hsla { a: 0.4, ..gold },
                                Hsla { a: 0.95, ..gold },
                            )],
                            duration: std::time::Duration::from_millis(1200),
                        },
                        cx,
                    );
                }
                if let Some(id) = id.filter(|_| !cx.reduce_motion()) {
                    let strong = (Hsla { a: 0.45, ..accent }, Hsla { a: 0.95, ..accent });
                    editor.flash_annotations(
                        mdoc_editor::AnnotationFlash {
                            ids: HashSet::from([id]),
                            from: strong,
                            peak: strong,
                            duration: std::time::Duration::from_millis(900),
                            mark: None,
                        },
                        cx,
                    );
                }
            });
        }
        // Both columns moved on purpose; neither leads the next frame.
        state
            .last
            .set((state.left.offset().y, state.right.offset().y));
        cx.notify();
    }
    pub(crate) fn compare_view(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.compare.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        // A single-column mode shows one column full width; Side by side
        // shows one at a time only in a narrow window.
        let narrow = width < NARROW || state.mode != CompareMode::Side;
        // Scroll together: whichever pane moved since the last frame leads,
        // by the same fraction of its height (lines pair up, wrapping differs).
        let (left, right) = (state.left.offset().y, state.right.offset().y);
        let (last_left, last_right) = state.last.get();
        let follow = |from: &gpui::ScrollHandle, to: &gpui::ScrollHandle, y: Pixels| {
            let max = from.max_offset().y;
            let fraction = if max > px(0.) {
                f32::from(-y) / f32::from(max)
            } else {
                0.
            };
            let mut offset = to.offset();
            offset.y = -(to.max_offset().y * fraction.clamp(0., 1.));
            to.set_offset(offset);
        };
        if left != last_left {
            follow(&state.left, &state.right, left);
        } else if right != last_right {
            follow(&state.right, &state.left, right);
        }
        state
            .last
            .set((state.left.offset().y, state.right.offset().y));
        // A quiet floating label names each column; it dims once the column
        // scrolls under it. In a narrow window it switches columns.
        let switchable = narrow && state.mode == CompareMode::Side;
        let label = |id: &'static str, text: &'static str, scroll: &gpui::ScrollHandle| {
            let scrolled = scroll.offset().y < px(-8.);
            div()
                .id(id)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .absolute()
                .top(px(12.))
                .left(px(16.))
                .px_2()
                .py(px(2.))
                .rounded_md()
                .bg(palette.bg)
                .border_1()
                .border_color(palette.border)
                .text_size(px(11.))
                .text_color(palette.header_muted)
                .when(scrolled, |v| v.opacity(0.35))
                .hover(|s| s.opacity(1.))
                .child(if switchable {
                    format!("{text}  ⇄")
                } else {
                    text.into()
                })
                .when(switchable, |v| {
                    v.cursor_pointer()
                        .tooltip(crate::style::tooltip("Switch column".into(), theme))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.compare.as_mut() {
                                state.original_only = !state.original_only;
                            }
                            cx.notify();
                        }))
                })
        };
        let column = |id: &'static str,
                      scroll: &gpui::ScrollHandle,
                      editor: &Entity<mdoc_editor::EditorState>,
                      label: gpui::Stateful<gpui::Div>| {
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(
                    // Absolute, like the document scroll: intrinsic sizing must
                    // not measure (shape) the editor at extra widths.
                    div()
                        .id(id)
                        .absolute()
                        .inset_0()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .px_6()
                        .pt(px(44.))
                        // Room for the view switcher over the last lines.
                        .pb(px(72.))
                        .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                            this.wake_switcher(cx);
                            cx.notify();
                        }))
                        .child(editor.clone()),
                )
                .child(label)
        };
        let (show_left, show_right) = match state.mode {
            CompareMode::Original => (true, false),
            CompareMode::Pseudonymized => (false, true),
            CompareMode::Side => (
                !narrow || state.original_only,
                !narrow || !state.original_only,
            ),
        };
        let close = ui::floating_button(
            "close-compare",
            "Close before / after (Esc)",
            ui::Icon::Close,
            "Close",
            theme,
            true,
        )
        .when(cfg!(test), |v| v.debug_selector(|| "close-compare".into()))
        .on_click(cx.listener(|this, _, window, cx| this.close_compare(window, cx)));
        Some(
            div()
                .id("compare-view")
                .when(cfg!(test), |v| v.debug_selector(|| "compare-view".into()))
                .key_context("CompareView")
                .track_focus(&state.focus)
                .on_action(cx.listener(|this, _: &crate::CloseCompare, window, cx| {
                    this.close_compare(window, cx)
                }))
                .relative()
                .size_full()
                .flex()
                // A column not shown stays laid out, invisible, under the shown
                // one: its scroll keeps following, so switching columns keeps
                // the reading position (Codex review).
                .child(
                    column(
                        "compare-original",
                        &state.left,
                        &state.original,
                        label("compare-label-original", "Original", &state.left),
                    )
                    .when(!show_left, |v| v.absolute().inset_0().invisible()),
                )
                .when(show_left && show_right, |v| {
                    v.child(div().w(px(1.)).h_full().bg(palette.border))
                })
                .child(
                    column(
                        "compare-current",
                        &state.right,
                        &state.current,
                        label("compare-label-current", "Pseudonymized", &state.right),
                    )
                    .when(!show_right, |v| v.absolute().inset_0().invisible()),
                )
                .child(div().absolute().top(px(12.)).right(px(20.)).child(close))
                .into_any_element(),
        )
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
