//! Before/after view (ADR 0041 #31): the original wording beside the
//! pseudonymized text, line by line and read-only. Originals and aliases never
//! contain line breaks, so lines pair up and scroll together.
use super::*;
use gpui::{AnyElement, HighlightStyle, StyledText, div, prelude::*};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompareLine {
    pub before: String,
    pub before_marks: Vec<Range<usize>>,
    pub after: String,
    pub after_marks: Vec<Range<usize>>,
}

/// Pair each line of `after` with its original. `applied` are source-sorted
/// spans of `after` with the text they replaced.
pub(crate) fn compare_lines(after: &str, applied: &[(Range<usize>, &str)]) -> Vec<CompareLine> {
    let mut lines = Vec::new();
    let mut spans = applied.iter().peekable();
    let mut start = 0;
    for line in after.split('\n') {
        let end = start + line.len();
        let mut row = CompareLine {
            before: String::new(),
            before_marks: Vec::new(),
            after: line.to_string(),
            after_marks: Vec::new(),
        };
        let mut at = start;
        while let Some((range, original)) = spans.peek() {
            if range.start >= end || range.end > end {
                break;
            }
            row.before.push_str(&after[at..range.start]);
            let mark = row.before.len();
            row.before.push_str(original);
            row.before_marks.push(mark..row.before.len());
            row.after_marks.push(range.start - start..range.end - start);
            at = range.end;
            spans.next();
        }
        row.before.push_str(&after[at..end]);
        lines.push(row);
        start = end + 1;
    }
    lines
}

impl DocumentView {
    pub(crate) fn toggle_compare(&mut self, cx: &mut Context<Self>) {
        self.compare_open = !self.compare_open && self.has_applied_aliases();
        cx.notify();
    }
    pub(crate) fn compare_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let text = self.editor.read(cx).text();
        let applied: Vec<_> = self
            .pii
            .review
            .applied()
            .iter()
            .filter(|a| text.get(a.range.clone()) == Some(a.step.after.as_ref()))
            .map(|a| (a.range.clone(), a.step.original()))
            .collect();
        let lines = compare_lines(text, &applied);
        let marked = |line: &str, marks: &[Range<usize>], color: gpui::Hsla| {
            StyledText::new(line.to_string()).with_highlights(marks.iter().map(|m| {
                (
                    m.clone(),
                    HighlightStyle {
                        color: Some(color),
                        background_color: Some(Hsla { a: 0.14, ..color }),
                        ..Default::default()
                    },
                )
            }))
        };
        let (original, alias) = (theme.proposed(), theme.applied());
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
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_compare(cx))),
                    ),
            )
            .child(
                div()
                    .id("compare-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_6()
                    .py_2()
                    .text_size(px(14.))
                    .children(lines.into_iter().map(|line| {
                        div()
                            .flex()
                            .gap_4()
                            .min_h(px(20.))
                            .child(div().w_1_2().min_w_0().child(marked(
                                &line.before,
                                &line.before_marks,
                                original,
                            )))
                            .child(div().w_1_2().min_w_0().child(marked(
                                &line.after,
                                &line.after_marks,
                                alias,
                            )))
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_pair_up_with_their_originals() {
        let after = "PERSON_1 met ORG_1.\n\nPERSON_1 left.";
        let lines = compare_lines(
            after,
            &[
                (0..8, "Anna Petrova"),
                (13..18, "Northwind Ltd"),
                (21..29, "Anna"),
            ],
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].before, "Anna Petrova met Northwind Ltd.");
        assert_eq!(lines[0].before_marks, [0..12, 17..30]);
        assert_eq!(lines[0].after_marks, [0..8, 13..18]);
        assert_eq!(lines[1].before, "");
        assert_eq!(lines[2].before, "Anna left.");
        assert_eq!(lines[2].after_marks, vec![0..8_usize]);
    }
}
