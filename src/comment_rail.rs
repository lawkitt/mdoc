//! The comment rail (originals spec, Comments in Source): a DOCX's comments
//! beside its Source page at their passages, pushed down where they would
//! overlap, or markers in the page margin when the pane is narrow. Cards are
//! read-only and their text never enters the Markdown. The rail is the page
//! view's overlay ([`gpui_pdf::PageOverlay`]), so it scrolls with the pages;
//! the passages' tints are its marks ([`gpui_pdf::PageMark`]).
use crate::style::Theme;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, KeyDownEvent, Subscription, WeakEntity, Window,
    actions, div, prelude::*, px,
};
use gpui_pdf::{NormRect, PageLayout, PageMark, PageOverlay, PdfEvent, PdfView};
use mdoc_import::docx_preview::{CommentAnchor, DocxComment};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

actions!(
    comment_rail,
    [
        /// Scroll to the next comment and focus its card.
        NextComment,
        /// Scroll to the previous comment and focus its card.
        PreviousComment
    ]
);

/// The rail's width beside the pages, gap included.
const RAIL_WIDTH: f32 = 256.;
/// The narrowest page viewport that has a rail; narrower panes, a 640×480
/// window's included, show markers.
const RAIL_MIN_VIEWPORT: f32 = 720.;
/// Space between the page and the cards, and the cards' right margin (the
/// overlay scrollbar floats there).
const RAIL_GAP: f32 = 12.;
const RAIL_RIGHT: f32 = 14.;
/// Space between cards pushed down under one another.
const CARD_GAP: f32 = 8.;
/// A card's height before it is measured.
const CARD_GUESS: f32 = 96.;
const CHIP_GUESS: f32 = 24.;
const MARKER: f32 = 20.;
/// The widest card opened from a marker.
const MARKER_CARD: f32 = 340.;

/// A comment with its replies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Thread {
    pub comment: DocxComment,
    pub replies: Vec<DocxComment>,
    /// Where the comment sits, in page order; empty when it has no place on
    /// the page.
    pub anchors: Vec<CommentAnchor>,
}

/// Comments as threads: those with a place, in page order, then those with
/// none ("Not on the page"). A reply joins its thread's root comment, in
/// document order; its own range is the root's.
pub(crate) fn threads(
    comments: &[DocxComment],
    anchors: &[CommentAnchor],
) -> (Vec<Thread>, Vec<Thread>) {
    let index: HashMap<&str, usize> = comments
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    // A comment whose parent is missing is a root; in a cycle, the first
    // comment of it in document order is.
    let root = |start: usize| {
        let mut path = vec![start];
        let mut i = start;
        while let Some(parent) = comments[i]
            .parent
            .as_deref()
            .and_then(|p| index.get(p).copied())
        {
            if let Some(at) = path.iter().position(|&p| p == parent) {
                return path[at..].iter().copied().min().unwrap_or(parent);
            }
            path.push(parent);
            i = parent;
        }
        i
    };
    let mut threads: Vec<Thread> = Vec::new();
    let mut slot = HashMap::new();
    for (i, comment) in comments.iter().enumerate() {
        let r = root(i);
        if r == i {
            slot.insert(i, threads.len());
            threads.push(Thread {
                comment: comment.clone(),
                replies: Vec::new(),
                anchors: anchors
                    .iter()
                    .filter(|a| a.id == comment.id && !a.rects.is_empty())
                    .cloned()
                    .collect(),
            });
        }
    }
    for (i, comment) in comments.iter().enumerate() {
        let r = root(i);
        if r != i
            && let Some(&t) = slot.get(&r)
        {
            threads[t].replies.push(comment.clone());
        }
    }
    let (mut placed, nowhere): (Vec<_>, Vec<_>) =
        threads.into_iter().partition(|t| !t.anchors.is_empty());
    // Anchors come in page order; a thread sorts by its first one.
    let order = |t: &Thread| {
        anchors
            .iter()
            .position(|a| a.id == t.comment.id)
            .unwrap_or(usize::MAX)
    };
    placed.sort_by_key(order);
    (placed, nowhere)
}

/// Where cards go down a rail: each at its wanted top unless the card
/// above reaches past it. `wanted` is in rail order.
pub(crate) fn stack(start: f32, wanted: &[(f32, f32)]) -> Vec<f32> {
    let mut cursor = start;
    wanted
        .iter()
        .map(|&(top, height)| {
            let top = top.max(cursor);
            cursor = top + height + CARD_GAP;
            top
        })
        .collect()
}

/// A comment's date as the card shows it: "Sep 19, 2026".
pub(crate) fn short_date(date: &str) -> Option<String> {
    let mut parts = date.get(..10)?.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .get(month.parse::<usize>().ok()?.checked_sub(1)?)?;
    let day: u32 = day.parse().ok()?;
    year.parse::<u32>().ok()?;
    Some(format!("{month} {day}, {year}"))
}

fn author(comment: &DocxComment) -> &str {
    if comment.author.trim().is_empty() {
        "Unknown author"
    } else {
        comment.author.trim()
    }
}

/// Up to two initials of the author's name.
pub(crate) fn initials(comment: &DocxComment) -> String {
    let words: Vec<&str> = comment.author.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(1).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

fn replies_label(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 reply".into()),
        n => Some(format!("{n} replies")),
    }
}

/// What a screen reader says for a card: "Comment by <author>, <date>, on
/// '<passage>': <text>. N replies."
pub(crate) fn card_label(thread: &Thread) -> String {
    let c = &thread.comment;
    let mut label = format!("Comment by {}", author(c));
    if let Some(date) = short_date(&c.date) {
        label.push_str(&format!(", {date}"));
    }
    let passage: String = c.quote.chars().take(120).collect();
    if !passage.trim().is_empty() {
        label.push_str(&format!(", on '{}'", passage.trim()));
    }
    label.push_str(&format!(": {}", c.text.trim()));
    if !label.ends_with('.') {
        label.push('.');
    }
    if thread.comment.resolved {
        label.push_str(" Resolved.");
    }
    if let Some(replies) = replies_label(thread.replies.len()) {
        label.push_str(&format!(" {replies}."));
    }
    label
}

/// What ⌘C on a focused card copies: the comment and its replies.
fn card_text(thread: &Thread) -> String {
    let mut out = format!("{}: {}", author(&thread.comment), thread.comment.text);
    for reply in &thread.replies {
        out.push_str(&format!("\n{}: {}", author(reply), reply.text));
    }
    out
}

/// What the rail draws: the "Not on the page" row, or a thread's card (or
/// chip, for a resolved one that is closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Item {
    Nowhere,
    Thread(usize),
}

pub struct CommentRail {
    /// Threads with a place, in page order, then those without
    /// (`placed..`): an index in this list names a thread.
    threads: Vec<Thread>,
    placed: usize,
    pdf: WeakEntity<PdfView>,
    theme: Rc<Cell<Theme>>,
    /// The document's "comments" F6 region, around the cards or markers.
    region: FocusHandle,
    cards: Vec<FocusHandle>,
    markers: Vec<FocusHandle>,
    nowhere_focus: FocusHandle,
    /// The expanded card: in a narrow pane, the open marker's.
    open: Option<usize>,
    nowhere_open: bool,
    shown: bool,
    /// Measured heights of what the rail draws.
    heights: Rc<RefCell<HashMap<Item, f32>>>,
    /// Each thread's top and height at the last render, for scrolling to it.
    places: Vec<Option<(f32, f32)>>,
    _subscriptions: Vec<Subscription>,
}

impl CommentRail {
    pub fn new(
        comments: &[DocxComment],
        anchors: &[CommentAnchor],
        pdf: &Entity<PdfView>,
        theme: Rc<Cell<Theme>>,
        region: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (placed, nowhere) = threads(comments, anchors);
        let placed_count = placed.len();
        let threads: Vec<Thread> = placed.into_iter().chain(nowhere).collect();
        let cards: Vec<FocusHandle> = threads
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let markers = threads
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let mut subscriptions = vec![cx.subscribe(pdf, |rail, _, event: &PdfEvent, cx| {
            if let PdfEvent::MarkClicked(i) = event {
                rail.open = Some(*i);
                rail.marks_changed(cx);
            }
        })];
        for (i, card) in cards.iter().enumerate() {
            subscriptions.push(cx.on_focus(card, window, move |rail, _, cx| {
                rail.reveal(i, cx);
            }));
        }
        Self {
            places: vec![None; threads.len()],
            threads,
            placed: placed_count,
            pdf: pdf.downgrade(),
            theme,
            region,
            cards,
            markers,
            nowhere_focus: cx.focus_handle().tab_stop(true),
            open: None,
            nowhere_open: false,
            shown: false,
            heights: Default::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Expand the thread of comment `id` and open "Not on the page", for
    /// renders and tests.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn show_open(&mut self, id: Option<&str>, nowhere: bool, cx: &mut Context<Self>) {
        self.open = id.and_then(|id| self.threads.iter().position(|t| t.comment.id == id));
        self.nowhere_open = nowhere;
        self.marks_changed(cx);
    }

    /// Thread `i`'s card, to focus.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn card_focus(&self, i: usize) -> FocusHandle {
        self.cards[i].clone()
    }

    #[cfg(test)]
    pub(crate) fn open(&self) -> Option<usize> {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn nowhere_open(&self) -> bool {
        self.nowhere_open
    }

    /// The marks over the original paper.
    #[cfg(test)]
    pub(crate) fn marks_for_test(&self) -> Vec<PageMark> {
        self.marks(false)
    }

    /// The comment threads, one per "💬 N".
    pub fn count(&self) -> usize {
        self.threads.len()
    }

    pub fn shown(&self) -> bool {
        self.shown && !self.threads.is_empty()
    }

    /// Show the comments beside the page, or the plain page.
    pub fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.shown = shown;
        let Some(pdf) = self.pdf.upgrade() else {
            return;
        };
        let on = self.shown();
        let rail = cx.entity();
        let weak = rail.downgrade();
        pdf.update(cx, |pdf, cx| {
            if on {
                pdf.set_overlay(
                    Some(PageOverlay {
                        view: rail.into(),
                        rail_width: Rc::new(|viewport| {
                            if viewport >= RAIL_MIN_VIEWPORT {
                                RAIL_WIDTH
                            } else {
                                0.
                            }
                        }),
                    }),
                    cx,
                );
                pdf.set_marks(
                    Some(Rc::new(move |themed, cx: &App| {
                        weak.upgrade()
                            .map_or_else(Vec::new, |rail| rail.read(cx).marks(themed))
                    })),
                    cx,
                );
            } else {
                pdf.set_overlay(None, cx);
                pdf.set_marks(None, cx);
            }
        });
        cx.notify();
    }

    /// The passages' tints: every open thread's lines, the expanded one's
    /// stronger and underlined. A resolved thread has none unless expanded.
    fn marks(&self, themed: bool) -> Vec<PageMark> {
        if !self.shown() {
            return Vec::new();
        }
        let colors = self.theme.get().comments(themed);
        let mut marks = Vec::new();
        for (i, thread) in self.threads[..self.placed].iter().enumerate() {
            let open = self.open == Some(i);
            if thread.comment.resolved && !open {
                continue;
            }
            for anchor in &thread.anchors {
                marks.push(PageMark {
                    page: anchor.page,
                    rects: anchor
                        .rects
                        .iter()
                        .map(|r| NormRect {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                        })
                        .collect(),
                    fill: if anchor.point {
                        colors.underline
                    } else if open {
                        colors.tint_active
                    } else {
                        colors.tint
                    },
                    underline: open.then_some(colors.underline),
                    key: i,
                });
            }
        }
        marks
    }

    /// The marks or the expanded card changed: both redraw.
    fn marks_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(pdf) = self.pdf.upgrade() {
            pdf.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    fn toggle(&mut self, i: usize, cx: &mut Context<Self>) {
        self.open = if self.open == Some(i) { None } else { Some(i) };
        self.marks_changed(cx);
    }

    /// Scroll the page so thread `i`'s card (or its line) is in view.
    fn reveal(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some((top, height)) = self.places.get(i).copied().flatten() else {
            return;
        };
        if let Some(pdf) = self.pdf.upgrade() {
            pdf.update(cx, |pdf, cx| pdf.reveal(top, top + height, cx));
        }
    }

    /// Next or previous comment: from the focused card, else from the top
    /// of the view, to the next thread in rail order, scrolled to and
    /// focused. In a narrow pane its card opens under its line.
    pub fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.threads.len();
        if n == 0 || !self.shown() {
            return;
        }
        let Some(pdf) = self.pdf.upgrade() else {
            return;
        };
        let layout = pdf.read(cx).layout();
        // Rail order: "Not on the page" first, then page order.
        let order: Vec<usize> = (self.placed..n).chain(0..self.placed).collect();
        let focused = self
            .cards
            .iter()
            .chain(&self.markers)
            .position(|f| f.contains_focused(window, cx))
            .map(|i| i % n);
        let at = match focused.and_then(|f| order.iter().position(|&o| o == f)) {
            Some(p) if forward => (p + 1) % n,
            Some(p) => (p + n - 1) % n,
            None => {
                let top = layout.visible.0;
                let first_below = order
                    .iter()
                    .position(|&i| i < self.placed && self.line(i, &layout).0 >= top);
                match first_below {
                    Some(p) if forward => p,
                    Some(p) => (p + n - 1) % n,
                    None if forward => 0,
                    None => n - 1,
                }
            }
        };
        let target = order[at];
        let narrow = layout.rail_width <= 0.;
        if target >= self.placed {
            self.nowhere_open = true;
        }
        if narrow {
            self.open = Some(target);
        }
        if target < self.placed {
            let (top, bottom) = self.line(target, &layout);
            pdf.update(cx, |pdf, cx| pdf.reveal(top, bottom + 60., cx));
        }
        window.focus(&self.cards[target], cx);
        self.marks_changed(cx);
    }

    /// The top and bottom of thread `i`'s first line, in overlay terms.
    fn line(&self, i: usize, layout: &PageLayout) -> (f32, f32) {
        let Some(anchor) = self.threads[i].anchors.first() else {
            return (0., 0.);
        };
        let (top, height) = layout.pages.get(anchor.page).copied().unwrap_or_default();
        let rect = anchor.rects[0];
        let line_top = top + rect.y * height;
        // A point's rectangle is a spot on the baseline.
        let line_height = if anchor.point { 0. } else { rect.h * height };
        (
            line_top - if anchor.point { 10. } else { 0. },
            line_top + line_height,
        )
    }

    fn thread_card(
        &self,
        i: usize,
        expanded: bool,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let colors = theme.comments(false);
        let thread = &self.threads[i];
        let c = &thread.comment;
        let resolved = c.resolved;
        let pdf = self.pdf.clone();
        let copy = card_text(thread);
        let person = |comment: &DocxComment, size: f32| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(size))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.placeholder_bg)
                        .text_color(p.header_fg)
                        .text_size(px(10.))
                        .child(initials(comment)),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(p.header_fg)
                        .child(author(comment).to_owned()),
                )
                .when_some(short_date(&comment.date), |row, date| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(date),
                    )
                })
        };
        crate::ui::focus_ring(("comment-card", i), theme)
            .track_focus(&self.cards[i])
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("comment-card-{i}"))
            })
            .key_context("CommentCard")
            .role(gpui::Role::Button)
            .aria_label(card_label(thread))
            .aria_expanded(expanded)
            .w(px(width))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .rounded(px(6.))
            .bg(colors.card)
            .border_1()
            .border_color(if expanded {
                colors.card_active
            } else {
                colors.card_border
            })
            .shadow_sm()
            .text_size(px(12.))
            .text_color(if resolved {
                p.header_muted
            } else {
                p.header_fg
            })
            .when(resolved, |v| v.opacity(0.85))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.))
                    .child(person(c, 20.))
                    .when(resolved, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.))
                                .text_color(p.header_muted)
                                .child("✓ resolved"),
                        )
                    }),
            )
            .child(
                div()
                    .when(!expanded, |v| v.line_clamp(2))
                    .child(if c.text.trim().is_empty() {
                        "Empty comment".to_owned()
                    } else {
                        c.text.clone()
                    }),
            )
            .when(expanded, |card| {
                card.children(thread.replies.iter().map(|reply| {
                    div()
                        .mt(px(4.))
                        .pl(px(8.))
                        .border_l_2()
                        .border_color(colors.card_border)
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(person(reply, 16.))
                        .child(div().child(reply.text.clone()))
                }))
            })
            .when(!expanded, |card| {
                card.when_some(replies_label(thread.replies.len()), |card, replies| {
                    card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(replies),
                    )
                })
            })
            .on_click(cx.listener(move |rail, _, _, cx| rail.toggle(i, cx)))
            .on_key_down(cx.listener(move |rail, event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if keys.key == "escape" {
                    // Back to the page; an open marker card closes.
                    let narrow = pdf
                        .upgrade()
                        .is_some_and(|pdf| pdf.read(cx).layout().rail_width <= 0.);
                    if narrow {
                        rail.open = None;
                        rail.marks_changed(cx);
                    }
                    if let Some(pdf) = pdf.upgrade() {
                        let focus = pdf.read(cx).focus_handle(cx);
                        window.focus(&focus, cx);
                    }
                    cx.stop_propagation();
                } else if keys.key == "c" && keys.modifiers.secondary(cx) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()));
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }

    /// A closed resolved thread on the rail: a quiet chip at its anchor.
    fn chip(&self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let colors = theme.comments(false);
        crate::ui::focus_ring(("comment-card", i), theme)
            .track_focus(&self.cards[i])
            .when(cfg!(test), |v| {
                v.debug_selector(move || format!("comment-card-{i}"))
            })
            .key_context("CommentCard")
            .role(gpui::Role::Button)
            .aria_label(card_label(&self.threads[i]))
            .aria_expanded(false)
            .px(px(8.))
            .py(px(2.))
            .rounded_full()
            .border_1()
            .border_color(colors.card_border)
            .text_size(px(11.))
            .text_color(p.header_muted)
            .cursor_pointer()
            .child("✓ resolved")
            .on_click(cx.listener(move |rail, _, _, cx| rail.toggle(i, cx)))
            .into_any_element()
    }

    fn nowhere_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let count = self.threads.len() - self.placed;
        crate::ui::focus_ring("comments-nowhere", theme)
            .track_focus(&self.nowhere_focus)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "comments-nowhere".into())
            })
            .role(gpui::Role::Button)
            .aria_label(format!("Not on the page ({count})"))
            .aria_expanded(self.nowhere_open)
            .px(px(8.))
            .py(px(3.))
            .rounded(px(4.))
            .text_size(px(12.))
            .text_color(p.header_fg)
            .cursor_pointer()
            .hover(move |v| v.bg(p.placeholder_bg))
            .child(format!(
                "{} Not on the page ({count})",
                if self.nowhere_open { "▾" } else { "▸" }
            ))
            .on_click(cx.listener(|rail, _, _, cx| {
                rail.nowhere_open = !rail.nowhere_open;
                cx.notify();
            }))
            .into_any_element()
    }

    fn height(&self, item: Item, guess: f32) -> f32 {
        self.heights.borrow().get(&item).copied().unwrap_or(guess)
    }

    /// A wide pane: cards beside the page, "Not on the page" at the top.
    fn rail(&mut self, layout: &PageLayout, cx: &mut Context<Self>) -> (Vec<AnyElement>, f32) {
        let x = layout.page_width + RAIL_GAP;
        let width = (layout.rail_width - RAIL_GAP - RAIL_RIGHT).max(80.);
        let mut items: Vec<(Item, f32, AnyElement)> = Vec::new();
        let mut cursor = 0.;
        if self.placed < self.threads.len() {
            let row = self.nowhere_row(cx);
            items.push((Item::Nowhere, cursor, row));
            cursor += self.height(Item::Nowhere, 24.) + CARD_GAP / 2.;
            if self.nowhere_open {
                for i in self.placed..self.threads.len() {
                    let expanded = self.open == Some(i);
                    let card = self.thread_card(i, expanded, width, cx);
                    self.places[i] = Some((cursor, self.height(Item::Thread(i), CARD_GUESS)));
                    items.push((Item::Thread(i), cursor, card));
                    cursor += self.height(Item::Thread(i), CARD_GUESS) + CARD_GAP;
                }
            }
        }
        let wanted: Vec<(f32, f32)> = (0..self.placed)
            .map(|i| {
                let chip = self.threads[i].comment.resolved && self.open != Some(i);
                let guess = if chip { CHIP_GUESS } else { CARD_GUESS };
                (
                    (self.line(i, layout).0 - 4.).max(0.),
                    self.height(Item::Thread(i), guess),
                )
            })
            .collect();
        let tops = stack(cursor, &wanted);
        let mut bottom = cursor;
        for i in 0..self.placed {
            let element = if self.threads[i].comment.resolved && self.open != Some(i) {
                self.chip(i, cx)
            } else {
                self.thread_card(i, self.open == Some(i), width, cx)
            };
            self.places[i] = Some((tops[i], wanted[i].1));
            bottom = tops[i] + wanted[i].1;
            items.push((Item::Thread(i), tops[i], element));
        }
        let heights = self.heights.clone();
        let keys: Vec<Item> = items.iter().map(|(item, ..)| *item).collect();
        let entity = cx.entity_id();
        let measure = div()
            .absolute()
            .top_0()
            .left_0()
            .size_0()
            .children(items.into_iter().map(|(_, top, element)| {
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(top))
                    .w(px(width))
                    .flex()
                    .flex_col()
                    .items_start()
                    .child(element)
                    .into_any_element()
            }))
            .on_children_prepainted(move |bounds, _, cx| {
                let mut heights = heights.borrow_mut();
                let mut changed = false;
                for (item, bounds) in keys.iter().zip(bounds) {
                    let height = f32::from(bounds.size.height);
                    if heights
                        .get(item)
                        .is_none_or(|old| (old - height).abs() > 0.5)
                    {
                        heights.insert(*item, height);
                        changed = true;
                    }
                }
                drop(heights);
                if changed {
                    cx.notify(entity);
                }
            })
            .into_any_element();
        (vec![measure], bottom)
    }

    /// A narrow pane: a marker per line in the page's right margin, its
    /// cards opening under the line.
    fn markers(&mut self, layout: &PageLayout, cx: &mut Context<Self>) -> (Vec<AnyElement>, f32) {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let themed = self
            .pdf
            .upgrade()
            .is_some_and(|pdf| theme == Theme::Dark && !pdf.read(cx).shows_original_paper());
        let colors = theme.comments(themed);
        // Threads whose first line is one line share a marker.
        let mut lines: Vec<(usize, f32, f32, Vec<usize>)> = Vec::new();
        for i in 0..self.placed {
            let anchor = &self.threads[i].anchors[0];
            let (top, bottom) = self.line(i, layout);
            match lines.last_mut() {
                Some((page, line_top, line_bottom, group))
                    if *page == anchor.page && (top - *line_top).abs() < 6. =>
                {
                    *line_bottom = line_bottom.max(bottom);
                    group.push(i);
                }
                _ => lines.push((anchor.page, top, bottom, vec![i])),
            }
        }
        let card_width = (layout.page_width - 16.).clamp(80., MARKER_CARD);
        let card_x = layout.page_width - card_width - 8.;
        let mut elements = Vec::new();
        // Open cards draw last, over the markers below their line.
        let mut open_cards = Vec::new();
        let mut bottom: f32 = 0.;
        if self.placed < self.threads.len() {
            open_cards.push(
                div()
                    .absolute()
                    .top(px(4.))
                    .left(px(card_x))
                    .w(px(card_width))
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(4.))
                    .child(self.nowhere_row(cx))
                    .when(self.nowhere_open, |v| {
                        v.children(
                            (self.placed..self.threads.len())
                                .map(|i| self.thread_card(i, true, card_width, cx))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .into_any_element(),
            );
        }
        for (_, top, line_bottom, group) in &lines {
            let first = group[0];
            let open = self.open.is_some_and(|o| group.contains(&o));
            let resolved = group.iter().all(|&i| self.threads[i].comment.resolved);
            let label = if group.len() == 1 {
                format!("Comment on this line: {}", card_label(&self.threads[first]))
            } else {
                format!("{} comments on this line", group.len())
            };
            let marker_top = (top + line_bottom) / 2. - MARKER / 2.;
            let group_for_click = group.clone();
            for &i in group {
                self.places[i] = Some((*top, line_bottom - top));
            }
            elements.push(
                crate::ui::focus_ring(("comment-marker", first), theme)
                    .track_focus(&self.markers[first])
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("comment-marker-{first}"))
                    })
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .aria_expanded(open)
                    .absolute()
                    .left(px(layout.page_width - MARKER - 6.))
                    .top(px(marker_top))
                    .h(px(MARKER))
                    .min_w(px(MARKER))
                    .px(px(4.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .when(!resolved, |v| {
                        v.bg(colors.marker).text_color(colors.marker_fg)
                    })
                    .when(resolved, |v| {
                        v.bg(colors.card)
                            .border_1()
                            .border_color(colors.card_border)
                            .text_color(p.header_muted)
                    })
                    .cursor_pointer()
                    .child(if resolved {
                        "✓".to_owned()
                    } else {
                        group.len().to_string()
                    })
                    .on_click(cx.listener(move |rail, _, _, cx| {
                        let open = rail.open.is_some_and(|o| group_for_click.contains(&o));
                        rail.open = if open { None } else { Some(group_for_click[0]) };
                        rail.marks_changed(cx);
                    }))
                    .into_any_element(),
            );
            if open {
                let card_top = line_bottom + 6.;
                let cards: Vec<AnyElement> = group
                    .iter()
                    .map(|&i| self.thread_card(i, true, card_width, cx))
                    .collect();
                open_cards.push(
                    div()
                        .absolute()
                        .left(px(card_x))
                        .top(px(card_top))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children(cards)
                        .into_any_element(),
                );
                bottom = bottom.max(card_top + CARD_GUESS * 2.);
            }
        }
        elements.extend(open_cards);
        (elements, bottom)
    }
}

impl Render for CommentRail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(pdf) = self.pdf.upgrade().filter(|_| self.shown()) else {
            return div().into_any_element();
        };
        let layout = pdf.read(cx).layout();
        let (elements, bottom) = if layout.rail_width > 0. {
            self.rail(&layout, cx)
        } else {
            self.markers(&layout, cx)
        };
        div()
            .relative()
            .w_full()
            .min_h(px(bottom))
            .child(
                // A point holding the region: it takes no clicks itself, so
                // the page under the overlay keeps its own.
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_0()
                    .track_focus(&self.region)
                    .key_context("CommentRail")
                    .children(elements),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_import::docx_preview::AnchorRect;

    fn comment(id: &str, parent: Option<&str>) -> DocxComment {
        DocxComment {
            id: id.into(),
            author: "Ann Lee".into(),
            date: "2026-09-19T10:00:00Z".into(),
            text: format!("Text {id}"),
            quote: "the clause".into(),
            parent: parent.map(Into::into),
            resolved: false,
        }
    }

    fn anchor(id: &str, page: usize, y: f32) -> CommentAnchor {
        CommentAnchor {
            id: id.into(),
            page,
            rects: vec![AnchorRect {
                x: 0.1,
                y,
                w: 0.3,
                h: 0.02,
            }],
            point: false,
        }
    }

    #[test]
    fn replies_join_their_root_and_threads_follow_the_page() {
        let comments = [
            comment("a", None),
            comment("b", None),
            comment("r1", Some("a")),
            comment("r2", Some("r1")),
            comment("lost", Some("missing")),
            comment("x", Some("y")),
            comment("y", Some("x")),
            comment("n", None),
        ];
        // `b` comes first on the page; `a` crosses a page break.
        let anchors = [
            anchor("b", 0, 0.1),
            anchor("a", 0, 0.5),
            anchor("r1", 0, 0.5),
            anchor("a", 1, 0.1),
            anchor("lost", 1, 0.3),
        ];
        let (placed, nowhere) = threads(&comments, &anchors);
        let ids = |t: &[Thread]| t.iter().map(|t| t.comment.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&placed), ["b", "a", "lost"]);
        let replies: Vec<_> = placed[1].replies.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(replies, ["r1", "r2"]);
        assert_eq!(placed[1].anchors.len(), 2, "both pages");
        // A cycle has a root all the same: the comment that starts it.
        assert_eq!(ids(&nowhere), ["x", "n"]);
        assert_eq!(nowhere[0].replies[0].id, "y");
    }

    #[test]
    fn cards_stack_below_the_one_above() {
        assert_eq!(
            stack(30., &[(0., 50.), (40., 20.), (200., 10.)]),
            [30., 30. + 50. + CARD_GAP, 200.]
        );
    }

    #[test]
    fn a_card_reads_its_author_date_passage_text_and_replies() {
        let thread = Thread {
            comment: comment("a", None),
            replies: vec![comment("r", Some("a")), comment("s", Some("a"))],
            anchors: Vec::new(),
        };
        assert_eq!(
            card_label(&thread),
            "Comment by Ann Lee, Sep 19, 2026, on 'the clause': Text a. 2 replies."
        );
        let mut lone = thread.clone();
        lone.replies.clear();
        lone.comment.author = String::new();
        lone.comment.date = String::new();
        lone.comment.resolved = true;
        assert_eq!(
            card_label(&lone),
            "Comment by Unknown author, on 'the clause': Text a. Resolved."
        );
        assert_eq!(initials(&thread.comment), "AL");
        assert_eq!(initials(&lone.comment), "?");
        assert_eq!(short_date("2026-02-03"), Some("Feb 3, 2026".into()));
        assert_eq!(short_date("2026-13-03"), None);
        assert_eq!(short_date(""), None);
        assert_eq!(
            card_text(&thread),
            "Ann Lee: Text a\nAnn Lee: Text r\nAnn Lee: Text s"
        );
    }
}
