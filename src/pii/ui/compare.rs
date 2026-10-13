//! Before/after view (ADR 0041 #31): the original wording beside the
//! pseudonymized text, both formatted like the editor and read-only, scrolling
//! together. A snapshot: reopen it after editing.
use super::*;
use gpui::{AnyElement, Entity, Pixels, div, prelude::*};

/// The two read-only editors of an open before/after view.
pub(crate) struct CompareState {
    original: Entity<mdoc_editor::EditorState>,
    current: Entity<mdoc_editor::EditorState>,
    left: gpui::ScrollHandle,
    right: gpui::ScrollHandle,
    /// Scroll offsets at the last render, to tell which pane moved.
    last: std::cell::Cell<(Pixels, Pixels)>,
}

/// The whole original text and, for both sides, the replaced spans.
pub(crate) fn compare_texts(
    after: &str,
    applied: &[(Range<usize>, &str)],
) -> (String, Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut before = String::with_capacity(after.len());
    let mut before_marks = Vec::new();
    let mut after_marks = Vec::new();
    let mut at = 0;
    for (range, original) in applied {
        if range.start < at || range.end > after.len() {
            continue;
        }
        before.push_str(&after[at..range.start]);
        let mark = before.len();
        before.push_str(original);
        before_marks.push(mark..before.len());
        after_marks.push(range.clone());
        at = range.end;
    }
    before.push_str(&after[at..]);
    (before, before_marks, after_marks)
}

impl DocumentView {
    pub(crate) fn toggle_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.compare.take().is_some() || !self.has_applied_aliases() {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            cx.notify();
            return;
        }
        let text = self.editor.read(cx).text().to_owned();
        let applied: Vec<_> = self
            .pii
            .review
            .applied()
            .iter()
            .filter(|a| text.get(a.range.clone()) == Some(a.step.after.as_ref()))
            .map(|a| (a.range.clone(), a.step.original()))
            .collect();
        let (before, before_marks, after_marks) = compare_texts(&text, &applied);
        let theme = self.theme.get();
        let pane = |text: String,
                    marks: Vec<Range<usize>>,
                    accent: Hsla,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut editor = mdoc_editor::EditorState::new(window, cx);
                editor.set_markdown_style(crate::style::markdown_style(theme), cx);
                editor.set_text(text, cx);
                editor.set_read_only(true, cx);
                let annotations = marks
                    .into_iter()
                    .enumerate()
                    .map(|(i, range)| mdoc_editor::SourceAnnotation {
                        id: i as u64,
                        range,
                        color: Hsla { a: 0.14, ..accent },
                        active_color: Hsla { a: 0.14, ..accent },
                        border: Hsla { a: 0.5, ..accent },
                        text_color: Some(accent),
                    })
                    .collect();
                editor.set_annotations(editor.revision(), annotations, cx);
                editor
            })
        };
        let original = pane(before, before_marks, theme.proposed(), window, cx);
        let current = pane(text, after_marks, theme.applied(), window, cx);
        self.compare = Some(CompareState {
            original,
            current,
            left: gpui::ScrollHandle::new(),
            right: gpui::ScrollHandle::new(),
            last: Default::default(),
        });
        cx.notify();
    }
    pub(crate) fn compare_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.compare.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
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
        let column = |id: &'static str,
                      scroll: &gpui::ScrollHandle,
                      editor: &Entity<mdoc_editor::EditorState>| {
            div()
                .id(id)
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .p_6()
                .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                .child(editor.clone())
        };
        Some(
            div()
                .id("compare-view")
                .when(cfg!(test), |v| v.debug_selector(|| "compare-view".into()))
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_6()
                        .py_2()
                        .border_b_1()
                        .border_color(palette.border)
                        .text_size(px(12.))
                        .text_color(palette.header_muted)
                        .child(div().flex_1().child("Original"))
                        .child(div().flex_1().child("Pseudonymized"))
                        .child(
                            crate::ui::control("close-compare", "Close", theme, true)
                                .when(cfg!(test), |v| v.debug_selector(|| "close-compare".into()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_compare(window, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(column("compare-original", &state.left, &state.original))
                        .child(div().w(px(1.)).h_full().bg(palette.border))
                        .child(column("compare-current", &state.right, &state.current)),
                )
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
        let (before, before_marks, after_marks) = compare_texts(
            after,
            &[
                (2..10, "Anna Petrova"),
                (13..18, "Northwind Ltd"),
                (22..30, "Anna"),
            ],
        );
        assert_eq!(before, "| Anna Petrova | Northwind Ltd |\n\nAnna left.");
        assert_eq!(&before[before_marks[1].clone()], "Northwind Ltd");
        assert_eq!(&after[after_marks[2].clone()], "PERSON_1");
    }
}
