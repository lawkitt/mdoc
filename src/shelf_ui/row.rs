//! One shelf row (ADR 0084): the title without `.md`, the case as a tinted
//! label (readiness in the same style, amber), an outlined Copy for AI on
//! ready rows, the preview of two wrapped lines with alias chips, and the
//! meta line "5 min ago · 1,297 words · 14 replaced".

use super::ShelfView;
use crate::{
    shelf::{Preview, ShelfDocument},
    style::{self, Theme},
    ui,
};
use gpui::{
    AnyElement, Context, CursorStyle, FontStyle, FontWeight, HighlightStyle, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, SharedString, StyledText, div, prelude::*, px,
};
use mdoc_pii::Category;
use std::{ops::Range, time::SystemTime};

/// Heights the shelf sums to size its window.
pub(super) const READY_ROW: f32 = 90.;
pub(super) const WAITING_ROW: f32 = 54.;
const TITLE_LINE: f32 = 20.;
const PREVIEW_LINE: f32 = 16.;
const META_LINE: f32 = 15.;
const ROW_INNER_GAP: f32 = 3.;

/// A row's height: a ready row with prose shows its two preview lines.
pub(super) fn height(document: &ShelfDocument) -> f32 {
    if document.readiness.is_ready() && document.preview.is_some() {
        READY_ROW
    } else {
        WAITING_ROW
    }
}

/// "just now", "5 min ago", "3 h ago", "yesterday", then a date.
pub(crate) fn ago(edited: SystemTime, now: SystemTime) -> String {
    let seconds = now
        .duration_since(edited)
        .map_or(0, |elapsed| elapsed.as_secs());
    if seconds < 60 {
        return "just now".into();
    }
    if seconds < 3600 {
        return format!("{} min ago", seconds / 60);
    }
    let edited = chrono::DateTime::<chrono::Local>::from(edited);
    let now = chrono::DateTime::<chrono::Local>::from(now);
    let (day, today) = (edited.date_naive(), now.date_naive());
    if day == today {
        format!("{} h ago", seconds / 3600)
    } else if today.pred_opt() == Some(day) {
        "yesterday".into()
    } else if chrono::Datelike::year(&day) == chrono::Datelike::year(&today) {
        edited.format("%-d %b").to_string()
    } else {
        edited.format("%-d %b %Y").to_string()
    }
}

/// "1,240 words".
pub(crate) fn words(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if count == 1 { "word" } else { "words" })
}

fn category_noun(category: Category, count: usize) -> &'static str {
    let one = count == 1;
    match category {
        Category::Person if one => "person",
        Category::Person => "people",
        Category::Organization if one => "organisation",
        Category::Organization => "organisations",
        Category::Email if one => "email",
        Category::Email => "emails",
        Category::Phone if one => "phone number",
        Category::Phone => "phone numbers",
        Category::Address if one => "address",
        Category::Address => "addresses",
        Category::Identity if one => "ID",
        Category::Identity => "IDs",
        Category::Tax if one => "tax ID",
        Category::Tax => "tax IDs",
        Category::Bank if one => "bank account",
        Category::Bank => "bank accounts",
        Category::Date if one => "date",
        Category::Date => "dates",
        Category::Other => "other",
    }
}

/// What hovering "14 replaced" lists: "6 people · 4 organisations · 4 IDs".
pub(crate) fn breakdown(replaced: &[(Category, usize)]) -> String {
    replaced
        .iter()
        .map(|(category, n)| format!("{n} {}", category_noun(*category, *n)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What VoiceOver reads for a row: "<title>, <case>, ready".
pub(crate) fn row_label(document: &ShelfDocument) -> String {
    let mut label = document.display_title().to_string();
    if let Some(case) = &document.case {
        label.push_str(", ");
        label.push_str(&case.name);
    }
    label.push_str(", ");
    label.push_str(&document.readiness.label());
    label
}

/// The preview's styles as disjoint, ordered runs: chips in the editor's
/// applied colours, bold and italic kept.
pub(crate) fn preview_runs(preview: &Preview, theme: Theme) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut edges: Vec<usize> = preview
        .aliases
        .iter()
        .chain(&preview.bold)
        .chain(&preview.italic)
        .flat_map(|r| [r.start, r.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let within = |ranges: &[Range<usize>], at: usize| ranges.iter().any(|r| r.contains(&at));
    edges
        .windows(2)
        .filter_map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let alias = within(&preview.aliases, start);
            let bold = within(&preview.bold, start);
            let italic = within(&preview.italic, start);
            (alias || bold || italic).then(|| {
                (
                    start..end,
                    HighlightStyle {
                        color: alias.then(|| theme.applied_text()),
                        background_color: alias.then(|| theme.applied().opacity(0.16)),
                        font_weight: bold.then_some(FontWeight::SEMIBOLD),
                        font_style: italic.then_some(FontStyle::Italic),
                        ..Default::default()
                    },
                )
            })
        })
        .collect()
}

/// A small label: the case in its colour, or a readiness state in amber.
fn label(text: impl Into<SharedString>, color: Hsla) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(text.into())
        .into_any_element()
}

impl ShelfView {
    pub(super) fn document_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let document = &self.content.documents[index];
        let ready = document.readiness.is_ready();
        let copied = self.copied == Some(index);
        let header = div()
            .h(px(TITLE_LINE))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_middle()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(document.display_title().to_string()),
            )
            .when_some(document.case.as_ref(), |header, case| {
                header.child(label(
                    case.name.clone(),
                    style::case_color(case.hue(), theme),
                ))
            })
            .when(!ready, |header| {
                header.child(label(document.readiness.label(), theme.proposed()))
            })
            .child(div().flex_1())
            .when(ready, |header| {
                header.child(
                    ui::focus_ring(("shelf-copy", index), theme)
                        .role(gpui::Role::Button)
                        .aria_label("Copy for AI")
                        .key_context("UiControl")
                        .tab_stop(true)
                        .flex_shrink_0()
                        .h(px(22.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded_md()
                        .border_1()
                        .border_color(p.border)
                        .text_size(px(12.))
                        .text_color(if copied {
                            theme.applied_text()
                        } else {
                            p.header_fg
                        })
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.placeholder_bg))
                        .when(copied, |button| {
                            button.child(
                                gpui::svg()
                                    .data(ui::Icon::Check.data())
                                    .size(px(12.))
                                    .text_color(theme.applied_text()),
                            )
                        })
                        .child(if copied { "Copied" } else { "Copy for AI" })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.copy(index, window, cx);
                        })),
                )
            });
        let preview = document
            .preview
            .as_ref()
            .filter(|_| ready)
            .map(|preview| {
                div()
                    .id(("shelf-preview", index))
                    .h(px(2. * PREVIEW_LINE))
                    .text_size(px(12.))
                    .line_height(px(PREVIEW_LINE))
                    .text_color(p.header_fg.opacity(0.82))
                    .line_clamp(2)
                    .text_ellipsis()
                    // A preview is what a drag picks up (ADR 0084).
                    .cursor(CursorStyle::OpenHand)
                    .child(
                        StyledText::new(preview.text.clone())
                            .with_highlights(preview_runs(preview, theme)),
                    )
            });
        let mut meta = format!(
            "{} · {}",
            ago(document.edited, SystemTime::now()),
            words(document.words)
        );
        let replaced = document.replaced_count();
        let count = (ready && replaced > 0).then(|| {
            div()
                .id(("shelf-replaced", index))
                .child(format!("{replaced} replaced"))
                .tooltip(style::tooltip(breakdown(&document.replaced), theme))
        });
        if count.is_some() {
            meta.push_str(" · ");
        }
        let meta = div()
            .h(px(META_LINE))
            .flex()
            .flex_row()
            .items_center()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .whitespace_nowrap()
            .child(meta)
            .children(count);
        ui::focus_ring(("shelf-row", index), theme)
            .track_focus(&self.rows[index])
            .role(gpui::Role::Button)
            .aria_label(row_label(document))
            .key_context("UiControl")
            .w_full()
            .h(px(height(document)))
            .px(px(8.))
            .py(px(8.))
            .rounded_lg()
            .flex()
            .flex_col()
            .gap(px(ROW_INNER_GAP))
            .cursor_pointer()
            .hover(move |s| s.bg(p.placeholder_bg.opacity(0.7)))
            .child(header)
            .children(preview)
            .child(meta)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.mouse_down(index, event)
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.mouse_move(event, window, cx)
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.press = None;
                this.open(index, window, cx)
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn word_counts_group_thousands() {
        assert_eq!(words(1), "1 word");
        assert_eq!(words(1240), "1,240 words");
        assert_eq!(words(1_000_000), "1,000,000 words");
    }

    #[test]
    fn edits_read_as_time_ago_then_a_date() {
        let now = SystemTime::now();
        let before = |s: u64| now - Duration::from_secs(s);
        assert_eq!(ago(now, now), "just now");
        assert_eq!(ago(now + Duration::from_secs(30), now), "just now");
        assert_eq!(ago(before(5 * 60), now), "5 min ago");
        let local = chrono::DateTime::<chrono::Local>::from(now);
        let midnight = chrono::Timelike::num_seconds_from_midnight(&local) as u64;
        if midnight > 3 * 3600 {
            assert_eq!(ago(before(3 * 3600), now), "3 h ago");
        }
        assert_eq!(ago(before(midnight + 3600), now), "yesterday");
        let old = before(midnight + 3 * 86_400);
        let shown = ago(old, now);
        assert!(!shown.contains("ago") && shown.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn the_replaced_count_breaks_down_by_category() {
        assert_eq!(
            breakdown(&[
                (Category::Person, 6),
                (Category::Organization, 4),
                (Category::Identity, 4)
            ]),
            "6 people · 4 organisations · 4 IDs"
        );
        assert_eq!(breakdown(&[(Category::Person, 1)]), "1 person");
    }

    #[test]
    fn preview_runs_are_disjoint_and_layer_chips_over_bold() {
        let preview = Preview {
            text: "PERSON_1 must sign now".into(),
            aliases: vec![0..8; 1],
            bold: vec![0..13; 1],
            italic: vec![14..18; 1],
        };
        let runs = preview_runs(&preview, Theme::Light);
        let ranges: Vec<_> = runs.iter().map(|(r, _)| r.clone()).collect();
        assert_eq!(ranges, [0..8, 8..13, 14..18]);
        assert!(runs[0].1.background_color.is_some() && runs[0].1.font_weight.is_some());
        assert!(runs[1].1.background_color.is_none() && runs[1].1.font_weight.is_some());
        assert_eq!(runs[2].1.font_style, Some(FontStyle::Italic));
    }
}
