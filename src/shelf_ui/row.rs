//! One shelf row (ADR 0084): the title without `.md`, the case as a tinted
//! label (readiness in the same style, amber), an outlined Copy for AI on
//! ready rows, the preview of two wrapped lines with alias chips, and the
//! meta line "5 min ago · 1,297 words · 24 replaced". A row that is not
//! ready starts its meta line with its state, so the title keeps its room.

use super::ShelfView;
use crate::{
    shelf::{Preview, ShelfDocument},
    style::{self, Theme},
    ui,
};
use gpui::{
    AnyElement, Context, CursorStyle, FontStyle, FontWeight, HighlightStyle, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, SharedString, StyledText, TextRun, TextStyle, Window, div,
    prelude::*, px,
};
use mdoc_pii::Category;
use std::{ops::Range, time::SystemTime};

/// The preview's width: the shelf's, less its padding and the row's.
pub(crate) const PREVIEW_WIDTH: f32 = crate::menu_bar::SHELF_WIDTH - 2. * 8. - 2. * 8. - 2.;
const PREVIEW_SIZE: f32 = 12.;
const PREVIEW_LINES: usize = 2;

/// Heights the shelf sums to size its window.
pub(super) const READY_ROW: f32 = 92.;
pub(super) const WAITING_ROW: f32 = 58.;
const TITLE_LINE: f32 = 20.;
const PREVIEW_LINE: f32 = 16.;
const META_LINE: f32 = 18.;
const TITLE_SIZE: f32 = 13.;
/// What the title line keeps for the others: a case label at least this
/// wide, and the Copy for AI button; a long case label gives way first.
const CASE_LABEL_MIN: f32 = 64.;
const COPY_BUTTON: f32 = 92.;
const HEADER_GAP: f32 = 6.;
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
        Category::Organization if one => "organization",
        Category::Organization => "organizations",
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

/// What hovering "24 replaced" lists: the distinct names those mentions
/// are, "14 names: 6 people · 4 organizations · 4 IDs". Each count keeps
/// its noun on its line ("1 phone number"): a long list wraps at the dots.
pub(crate) fn breakdown(replaced: &[(Category, usize)]) -> String {
    let names: usize = replaced.iter().map(|(_, n)| n).sum();
    let categories = replaced
        .iter()
        .map(|(category, n)| format!("{n} {}", category_noun(*category, *n)).replace(' ', "\u{a0}"))
        .collect::<Vec<_>>()
        .join(" · ");
    match names {
        1 => format!("1\u{a0}name: {categories}"),
        n => format!("{n}\u{a0}names: {categories}"),
    }
}

/// What VoiceOver reads for a row: "<title>, <case>, ready".
pub(crate) fn row_label(document: &ShelfDocument) -> String {
    let mut label = document.display_title().to_string();
    if let Some(case) = &document.case_label {
        label.push_str(", ");
        label.push_str(case);
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

/// The preview's text runs over `base`: bold and italic in their own
/// fonts, as the row draws them (chips change colour, not width).
fn fit_runs(preview: &Preview, base: &TextStyle) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, style) in preview_runs(preview, Theme::Light) {
        if range.start > at {
            runs.push(base.to_run(range.start - at));
        }
        runs.push(base.clone().highlight(style).to_run(range.len()));
        at = range.end;
    }
    if at < preview.text.len() {
        runs.push(base.to_run(preview.text.len() - at));
    }
    runs
}

/// How many lines `preview` wraps to at `width`, shaped as drawn.
pub(crate) fn wrapped_lines(preview: &Preview, width: f32, window: &Window) -> usize {
    let mut base = window.text_style();
    base.font_size = px(PREVIEW_SIZE).into();
    let runs = fit_runs(preview, &base);
    window
        .text_system()
        .shape_text(
            preview.text.clone().into(),
            px(PREVIEW_SIZE),
            &runs,
            Some(px(width)),
            None,
        )
        .map_or(1, |lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum()
        })
}

/// How wide `text` is in the title's font, on one line.
fn title_width(text: &str, window: &Window) -> f32 {
    let mut style = window.text_style();
    style.font_size = px(TITLE_SIZE).into();
    style.font_weight = FontWeight::SEMIBOLD;
    let runs = [style.to_run(text.len())];
    window
        .text_system()
        .shape_text(text.to_string().into(), px(TITLE_SIZE), &runs, None, None)
        .ok()
        .and_then(|lines| {
            lines
                .first()
                .map(|line| line.size(px(TITLE_LINE)).width.as_f32())
        })
        .unwrap_or(0.)
}

/// `title` as the row draws it, measured, and its width: whole when it
/// fits the room its row leaves, else cut in the middle with "…" (ADR 0084).
pub(crate) fn fit_title(document: &ShelfDocument, window: &Window) -> (String, f32) {
    let mut room = PREVIEW_WIDTH - HEADER_GAP;
    if document.case.is_some() {
        room -= CASE_LABEL_MIN + HEADER_GAP;
    }
    if document.readiness.is_ready() {
        room -= COPY_BUTTON + HEADER_GAP;
    }
    let title = document.display_title();
    let width = title_width(title, window);
    if width <= room {
        return (title.to_string(), width.ceil());
    }
    let chars: Vec<char> = title.chars().collect();
    if chars.len() < 3 {
        return (title.to_string(), room);
    }
    let cut = |keep: usize| -> String {
        let head = keep.div_ceil(2);
        let tail = keep / 2;
        let start: String = chars[..head].iter().collect();
        let end: String = chars[chars.len() - tail..].iter().collect();
        format!("{start}…{end}")
    };
    // The most characters kept that still fit.
    let (mut lo, mut hi) = (0, chars.len() - 1);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if title_width(&cut(mid), window) <= room {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let fitted = cut(lo);
    let width = title_width(&fitted, window);
    (fitted, width.ceil())
}

/// `preview` cut at byte `cut`, back to a word, ending in "…"; ranges past
/// the end are dropped or clipped, so no chip is cut in half.
fn cut_preview(preview: &Preview, cut: usize) -> Preview {
    let body = &preview.text[..cut];
    let at_word = body.rfind(' ').filter(|at| *at > cut / 2).unwrap_or(cut);
    let end = body[..at_word].trim_end().len();
    let clip = |ranges: &[Range<usize>]| -> Vec<Range<usize>> {
        ranges
            .iter()
            .filter(|r| r.start < end)
            .map(|r| r.start..r.end.min(end))
            .collect()
    };
    Preview {
        text: format!("{}…", &preview.text[..end]),
        aliases: preview
            .aliases
            .iter()
            .filter(|r| r.end <= end)
            .cloned()
            .collect(),
        bold: clip(&preview.bold),
        italic: clip(&preview.italic),
    }
}

/// `preview` as it fits two lines `width` wide with its real fonts: whole
/// when it fits, else cut at a word and ending in "…" (ADR 0084). Bold and
/// chips wrap wider than plain text, so the cut is measured, not counted.
pub(crate) fn fit_preview(preview: &Preview, width: f32, window: &Window) -> Preview {
    if wrapped_lines(preview, width, window) <= PREVIEW_LINES {
        return preview.clone();
    }
    let body_len = preview.text.trim_end_matches('…').len();
    let cuts: Vec<usize> = preview.text[..body_len]
        .char_indices()
        .map(|(at, _)| at)
        .filter(|at| *at > 0)
        .collect();
    // The longest cut that still fits.
    let (mut lo, mut hi) = (0, cuts.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if wrapped_lines(&cut_preview(preview, cuts[mid - 1]), width, window) <= PREVIEW_LINES {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    match lo {
        0 => cut_preview(preview, cuts.first().copied().unwrap_or(0)),
        n => cut_preview(preview, cuts[n - 1]),
    }
}

/// A small label: the case in its colour, or a readiness state in amber.
fn label(text: impl Into<SharedString>, color: Hsla) -> gpui::Div {
    div()
        .flex_shrink_0()
        .h(px(18.))
        .line_height(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(text.into())
}

impl ShelfView {
    pub(super) fn document_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let document = &self.content.documents[index];
        let ready = document.readiness.is_ready();
        let copied = self.copied == Some(index);
        let (title, title_width) = self
            .titles
            .get(index)
            .cloned()
            .unwrap_or_else(|| (document.display_title().to_string(), PREVIEW_WIDTH));
        let header = div()
            .h(px(TITLE_LINE))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(HEADER_GAP))
            .child(
                // Measured when the shelf opened: the title keeps its width
                // and the case label takes what is left.
                div()
                    .flex_shrink_0()
                    .w(px(title_width))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(TITLE_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when_some(document.case.as_ref(), |header, case| {
                header.child(
                    label(
                        document.case_label.clone().unwrap_or_default(),
                        style::case_color(case.hue(), theme),
                    )
                    .flex_shrink(1.)
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis(),
                )
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
        let fitted = self.fitted.get(index).and_then(Option::as_ref);
        let preview = fitted.filter(|_| ready).map(|preview| {
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
            let count = div()
                .id(("shelf-replaced", index))
                .child(format!("{replaced} replaced"))
                .tooltip(style::tooltip(breakdown(&document.replaced), theme));
            #[cfg(all(
                debug_assertions,
                feature = "headless-renders",
                any(target_os = "macos", target_os = "windows")
            ))]
            let count = {
                let counts = self.counts.clone();
                count.relative().child(
                    gpui::canvas(
                        move |bounds, _, _| {
                            let mut counts = counts.borrow_mut();
                            counts.retain(|(row, _)| *row != index);
                            counts.push((index, bounds));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            };
            count
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
            .when(!ready, |line| {
                line.child(label(document.readiness.label(), theme.proposed()).mr(px(6.)))
            })
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
    fn the_popup_takes_its_content_height_up_to_the_room_under_the_item() {
        use crate::shelf_ui::fitted_height;
        assert_eq!(fitted_height(250.2, 400.), 251.);
        assert_eq!(fitted_height(500., 400.), 400.);
    }

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
        let shown = |replaced: &[(Category, usize)]| breakdown(replaced).replace('\u{a0}', " ");
        assert_eq!(
            shown(&[
                (Category::Person, 6),
                (Category::Organization, 4),
                (Category::Identity, 4)
            ]),
            "14 names: 6 people · 4 organizations · 4 IDs"
        );
        assert_eq!(shown(&[(Category::Person, 1)]), "1 name: 1 person");
        // A wrap falls between categories, never inside one.
        assert_eq!(
            breakdown(&[(Category::Email, 1), (Category::Phone, 1)]),
            "2\u{a0}names: 1\u{a0}email · 1\u{a0}phone\u{a0}number"
        );
    }

    /// GUI run of #280: bold and chips wrap wider than plain text, so the
    /// two-line cut is measured with them and always ends in "…".
    #[gpui::test]
    fn previews_fit_two_measured_lines_and_end_in_an_ellipsis(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let text = "Without prejudice and subject to contract Claim number: 6001842/2026 \
                    (Employment Tribunal, Bristol) ORG_1 and PERSON_1 agree the terms below.";
        let preview = Preview {
            text: text.into(),
            aliases: vec![text.find("ORG_1").unwrap()..text.find("ORG_1").unwrap() + 5; 1],
            bold: vec![42..55; 1],
            italic: Vec::new(),
        };
        cx.update(|window, _| {
            for width in [120., 200., PREVIEW_WIDTH] {
                let fitted = fit_preview(&preview, width, window);
                assert!(
                    wrapped_lines(&fitted, width, window) <= PREVIEW_LINES,
                    "{width}"
                );
                if fitted.text != text {
                    assert!(fitted.text.ends_with('…'), "{}", fitted.text);
                    assert!(text.starts_with(fitted.text.trim_end_matches('…')));
                    assert!(fitted.aliases.iter().all(|r| r.end < fitted.text.len()));
                }
            }
            let short = Preview {
                text: "PERSON_1 signed.".into(),
                ..Preview::default()
            };
            assert_eq!(fit_preview(&short, PREVIEW_WIDTH, window), short);
        });
    }

    /// GUI run of #280: the title takes its width first; only a title
    /// longer than its room is cut, in the middle.
    #[gpui::test]
    fn titles_keep_their_width_and_cut_in_the_middle_only_when_long(cx: &mut gpui::TestAppContext) {
        use crate::shelf::OpenTab;
        let cx = cx.add_empty_window();
        let row = |title: &'static str| {
            ShelfDocument::of_tab(OpenTab {
                id: 1,
                title,
                text: "PERSON_1 signed the agreement today.",
                aliases: vec!["PERSON_1"],
                pending: 0,
                checking: false,
                names: Vec::new(),
                case: None,
                edited: SystemTime::UNIX_EPOCH,
            })
            .unwrap()
        };
        cx.update(|window, _| {
            let (short, width) = fit_title(&row("Settlement agreement.md"), window);
            assert_eq!(short, "Settlement agreement");
            assert!(width > 0. && width < PREVIEW_WIDTH - COPY_BUTTON);
            let long = row(
                "Settlement agreement between the parties, schedules, annexes and the final \
                 signed copy.md",
            );
            let (cut, width) = fit_title(&long, window);
            assert!(cut.contains('…') && cut.starts_with("Settlement") && cut.ends_with("copy"));
            assert!(width <= PREVIEW_WIDTH - COPY_BUTTON - 2. * HEADER_GAP + 1.);
        });
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
