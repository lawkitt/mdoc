//! Logical-order line breaking for right-to-left paragraphs.
//!
//! # Why this exists
//!
//! gpui shapes a paragraph as ONE long line and only then slices it into wrap
//! rows, walking the glyph table by ascending x
//! (`LineLayout::compute_wrap_boundaries`). Glyph order is *visual*, so in an
//! RTL paragraph the leftmost glyphs — the ones that end up in the first row —
//! are the paragraph's LAST words. A wrapped Persian note therefore reads
//! bottom-to-top, and no amount of alignment fixes it: the row contents
//! themselves are wrong.
//!
//! UAX #9 says to do it the other way round: break into lines in *logical*
//! order first, then reorder each line independently. That is what this module
//! does, and it needs nothing from gpui that isn't already public:
//!
//! - `TextSystem::shape_text` splits its input on `\n` and shapes each line
//!   separately, in order. So injecting a `\n` at each logical break makes the
//!   shaper do the per-line reordering for us, in the right sequence.
//! - Where the breaks GO is ours. gpui's `LineWrapper` walks the text (its
//!   offsets are logical, so it looked like the tool for this) but its
//!   `is_word_char` knows Latin, Cyrillic, Vietnamese and Bengali and not
//!   Arabic, so Persian takes its "CJK may not be space separated" path and
//!   every character becomes a break candidate — words split down the middle.
//!   Arabic script is space-separated, so [`wrap_at_words`] measures words.
//!
//! The whole trick is [`insert_breaks`]: turn one paragraph into a `\n`-joined
//! sequence of lines that already fit. Everything after that is ordinary
//! painting.

use std::ops::Range;

use crate::VisualMap;

use gpui::{
    App, Bounds, ContentMask, Pixels, Point, SharedString, TextAlign, TextRun, Window, WrappedLine,
    px,
};

/// Split `text` into lines at `breaks` (logical byte offsets, ascending) by
/// injecting `\n`, and grow `runs` to cover the injected bytes.
///
/// `shape_text` requires the runs to span every byte of the text it is handed,
/// including the `\n`s (it consumes one byte of run per line break), so a
/// break inside a run lengthens that run by one rather than splitting it — the
/// newline inherits the style of the text it interrupts, which is invisible
/// either way since a line break paints nothing.
///
/// Offsets at 0, at `text.len()`, or repeated are ignored: they would produce
/// an empty line, which would paint as a blank row the reader never asked for.
pub(crate) fn insert_breaks(
    text: &str,
    runs: &[TextRun],
    breaks: &[usize],
) -> (SharedString, Vec<TextRun>) {
    let mut wanted: Vec<usize> = breaks
        .iter()
        .copied()
        .filter(|ix| *ix > 0 && *ix < text.len() && text.is_char_boundary(*ix))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() {
        return (SharedString::from(text.to_string()), runs.to_vec());
    }

    let mut out = String::with_capacity(text.len() + wanted.len());
    let mut last = 0usize;
    for ix in &wanted {
        out.push_str(&text[last..*ix]);
        out.push('\n');
        last = *ix;
    }
    out.push_str(&text[last..]);

    // Walk the runs alongside the break list, widening whichever run contains
    // each break. A break exactly on a run boundary belongs to the run that
    // ENDS there, matching how the text was split above.
    let mut new_runs = Vec::with_capacity(runs.len());
    let mut run_start = 0usize;
    let mut next = 0usize;
    for run in runs {
        let run_end = run_start + run.len;
        let mut extra = 0usize;
        while next < wanted.len() && wanted[next] <= run_end {
            if wanted[next] > run_start {
                extra += 1;
            }
            next += 1;
        }
        let mut run = run.clone();
        run.len += extra;
        new_runs.push(run);
        run_start = run_end;
    }
    // Any break past the last run (runs that don't cover the text — gpui logs
    // and truncates in that case) has nowhere to go; the text still carries it.
    (SharedString::from(out), new_runs)
}

/// The byte range of each word in `text`, where a word runs up to and including
/// the spaces that follow it — trailing spaces belong to the line they end, the
/// same convention gpui's wrapper uses.
///
/// Only ASCII space and tab separate words. Notably NOT the zero-width
/// non-joiner (U+200C), which sits *inside* Persian words (می‌گیرد) and would
/// split them if treated as a break.
fn words(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        out.push(start..i);
    }
    out
}

/// The sub-runs covering `range`, so a slice of text can be measured with the
/// styles it actually has (a bold word is wider than the same word in regular).
fn slice_runs(runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for run in runs {
        let end = at + run.len;
        let lo = at.max(range.start);
        let hi = end.min(range.end);
        if lo < hi {
            let mut r = run.clone();
            r.len = hi - lo;
            out.push(r);
        }
        at = end;
        if at >= range.end {
            break;
        }
    }
    out
}

/// Greedy word wrap: the byte offsets where a new line should start.
///
/// Words are measured once each and summed, rather than re-measuring the whole
/// line as it grows — shaping is per-word for Arabic script (contextual forms
/// join within a word, never across a space), so the sum is exact and the pass
/// stays linear. A single word wider than `wrap_width` overflows rather than
/// being chopped mid-word: chopping is what the reader complained about.
fn wrap_at_words(
    text: &str,
    runs: &[TextRun],
    wrap_width: Pixels,
    font_size: Pixels,
    window: &Window,
) -> Vec<usize> {
    let mut breaks = Vec::new();
    let mut line_width = px(0.);
    let mut line_has_word = false;
    for word in words(text) {
        let measured = window.text_system().shape_line(
            SharedString::from(text[word.clone()].to_string()),
            font_size,
            &slice_runs(runs, word.clone()),
            None,
        );
        // Trailing spaces don't push a line over: they hang past the edge, as
        // they do in every text engine.
        let trimmed = text[word.clone()].trim_end();
        let ink = if trimmed.len() == word.len() {
            measured.width
        } else {
            window
                .text_system()
                .shape_line(
                    SharedString::from(trimmed.to_string()),
                    font_size,
                    &slice_runs(runs, word.start..word.start + trimmed.len()),
                    None,
                )
                .width
        };

        if line_has_word && line_width + ink > wrap_width {
            breaks.push(word.start);
            line_width = measured.width;
        } else {
            line_width += measured.width;
        }
        line_has_word = true;
    }
    breaks
}

/// One laid-out row: its span in the original text, plus the visual map that
/// turns a logical offset inside it into an x and back.
///
/// Public because both engines need it — the reader's [`RtlText`] and the
/// editor, which has its own line pipeline but the same problem.
pub struct Row {
    /// Byte offset in the ORIGINAL text where this row starts.
    pub start: usize,
    /// Byte length of the row's own text.
    pub len: usize,
    pub width: Pixels,
    pub map: VisualMap,
    /// The row's style runs, with row-local ranges. Carried because gpui can't
    /// paint a multi-styled RTL row correctly — see [`paint_row`].
    pub runs: Vec<(Range<usize>, TextRun)>,
    /// The shaped row, ready to paint.
    pub line: WrappedLine,
    /// The size it was shaped at. Carried so painting can re-shape a run
    /// without the caller passing a size that might not be this row's — a
    /// heading is not the body size, and getting that wrong is invisible until
    /// a styled heading paints at the wrong scale.
    pub font_size: Pixels,
}

/// Lay `text` out as right-to-left rows: break it in LOGICAL order at word
/// boundaries, then shape each row on its own so the shaper reorders it
/// independently — the sequence gpui's own wrapping gets backwards.
///
/// `runs` must cover every byte of `text`. Rows come back in reading order, so
/// row 0 is the paragraph's first line whichever way the script runs.
pub fn layout_rows(
    text: &str,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
    font_size: Pixels,
    window: &Window,
) -> Vec<Row> {
    let breaks: Vec<usize> = match wrap_width {
        Some(w) => wrap_at_words(text, runs, w, font_size, window),
        None => Vec::new(),
    };
    let (broken, broken_runs) = insert_breaks(text, runs, &breaks);

    // Each row now fits, so shaping with no wrap width leaves the rows exactly
    // where we put them — and each is reordered on its own, which is the point.
    let lines = window
        .text_system()
        .shape_text(broken, font_size, &broken_runs, None, None)
        .map(|l| l.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    // `orig` walks the ORIGINAL text, where the injected breaks do not exist,
    // so it advances by the row length alone. `broken_at` walks the string we
    // built, which DOES carry them. Conflating the two slides every row after
    // the first by a byte per break.
    let mut rows = Vec::with_capacity(lines.len());
    let mut orig = 0usize;
    let mut broken_at = 0usize;
    for line in lines {
        let len = line.text.len();
        let mut row_runs = Vec::new();
        let mut at = 0usize;
        for run in slice_runs(&broken_runs, broken_at..broken_at + len) {
            let end = at + run.len;
            row_runs.push((at..end, run));
            at = end;
        }
        rows.push(Row {
            start: orig,
            len,
            width: line.width(),
            map: crate::shaped::map_of_wrapped(&line, len),
            runs: row_runs,
            line,
            font_size,
        });
        orig += len;
        broken_at += len + 1;
    }
    rows
}

/// Paint one row at `origin`, whose x is the row's LEFT edge.
///
/// A row with a single style goes through gpui's painter. A row with more than
/// one can't: `paint_line` walks glyphs in visual order but pulls decorations
/// from a forward-only iterator keyed on `glyph.index >= run_end`. In an RTL
/// row the first glyph carries the HIGHEST index, so that first step consumes
/// every run up to the last and the iterator never advances again — the whole
/// row is painted in the colour of whichever run covers the logically-last
/// character. That is why a link inside Persian text came out body-coloured.
///
/// So the row is painted once per style, each pass coloured uniformly for that
/// style (making the collapse harmless) and clipped to the boxes that style
/// occupies. The whole row is shaped every pass, with its real fonts — cursive
/// joining and kerning survive a style boundary INSIDE a word (`**می**گیرد`),
/// which shaping each run separately would break.
pub fn paint_row(
    row: &Row,
    origin: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if row.runs.len() < 2 {
        // Backgrounds (the inline-code tint) are a separate pass from the
        // glyphs — `paint` alone would drop them.
        let _ = row
            .line
            .paint_background(origin, line_height, TextAlign::Left, None, window, cx);
        let _ = row
            .line
            .paint(origin, line_height, TextAlign::Left, None, window, cx);
        return;
    }

    // Paint the row once PER STYLE, each pass coloured uniformly for that style
    // and CLIPPED to the boxes that style actually occupies.
    //
    // The uniform colour is what makes gpui's painter usable here at all: it
    // walks glyphs in visual order but pulls decorations from a forward-only
    // iterator keyed on `glyph.index >= run_end`, and in an RTL row the first
    // glyph carries the HIGHEST index — so the iterator jumps to the last run
    // and never advances, painting the whole row in one colour. If that colour
    // is the one we want for this pass, the collapse stops mattering.
    //
    // Only the decoration differs between passes; fonts, weights and sizes stay
    // the row's own, so every pass lays out exactly as the row did. Shaping each
    // run on its OWN instead gives it a slightly different width than it has in
    // context, and the pieces overlap and swallow the spaces between them.
    let text_system = window.text_system().clone();
    for (range, style) in &row.runs {
        let rects = row.map.rects_for_range(range.clone());
        if rects.is_empty() {
            continue;
        }
        let uniform: Vec<TextRun> = row
            .runs
            .iter()
            .map(|(_, r)| TextRun {
                color: style.color,
                background_color: style.background_color,
                underline: style.underline,
                strikethrough: style.strikethrough,
                ..r.clone()
            })
            .collect();
        let shaped = text_system.shape_line(row.line.text.clone(), row.font_size, &uniform, None);
        for (x0, x1) in rects {
            let clip = Bounds::from_corners(
                Point {
                    x: origin.x + px(x0),
                    y: origin.y,
                },
                Point {
                    x: origin.x + px(x1),
                    y: origin.y + line_height,
                },
            );
            // `with_content_mask`, not `paint_layer`: the latter pushes a
            // scene layer for z-order and clips nothing, so every pass painted
            // the whole row and the overdraw showed up as faux-bold text.
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                let _ =
                    shaped.paint_background(origin, line_height, TextAlign::Left, None, window, cx);
                let _ = shaped.paint(origin, line_height, TextAlign::Left, None, window, cx);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Hsla, font};

    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: font("Helvetica"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn breaks_split_text_and_widen_the_run_that_holds_them() {
        let text = "one two three";
        let (out, runs) = insert_breaks(text, &[run(text.len())], &[4, 8]);
        assert_eq!(out.as_ref(), "one \ntwo \nthree");
        // Runs must still cover every byte, newlines included, or `shape_text`
        // warns and drops the tail.
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), out.len());
    }

    #[test]
    fn a_break_falls_in_the_run_that_ends_at_it() {
        let text = "boldplain";
        // Two runs: "bold" then "plain". A break exactly at 4 belongs to the
        // first, so the newline inherits the style it interrupts.
        let (out, runs) = insert_breaks(text, &[run(4), run(5)], &[4]);
        assert_eq!(out.as_ref(), "bold\nplain");
        assert_eq!(runs[0].len, 5);
        assert_eq!(runs[1].len, 5);
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), out.len());
    }

    #[test]
    fn degenerate_offsets_never_make_an_empty_row() {
        let text = "hello";
        // 0 and len would each produce a blank line; duplicates would produce
        // one per repeat.
        let (out, runs) = insert_breaks(text, &[run(5)], &[0, 5, 2, 2]);
        assert_eq!(out.as_ref(), "he\nllo");
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), out.len());
    }

    /// Walk `runs` over `broken` exactly as `TextSystem::shape_text` does —
    /// per line, then consuming ONE byte of the run at the front for the `\n`
    /// it skips — and report the colour landing on each byte. This is the
    /// contract `insert_breaks` has to satisfy: get the newline's owner wrong
    /// and every colour after the first break slides.
    fn colours_per_byte(broken: &str, runs: &[TextRun]) -> Vec<Hsla> {
        let mut queue: Vec<TextRun> = runs.to_vec();
        let mut out = Vec::new();
        let mut q = 0usize;
        for (i, line) in broken.split('\n').enumerate() {
            if i > 0 {
                // The `\n` itself: shape_text charges it to the front run.
                if let Some(run) = queue.get_mut(q) {
                    run.len -= 1;
                    if run.len == 0 {
                        q += 1;
                    }
                }
                out.push(Hsla::default()); // placeholder for the newline byte
            }
            let mut taken = 0usize;
            while taken < line.len() {
                let Some(run) = queue.get_mut(q) else {
                    panic!(
                        "runs ran out with {} bytes of line left",
                        line.len() - taken
                    );
                };
                let take = (line.len() - taken).min(run.len);
                for _ in 0..take {
                    out.push(run.color);
                }
                run.len -= take;
                if run.len == 0 {
                    q += 1;
                }
                taken += take;
            }
        }
        out
    }

    #[test]
    fn a_break_does_not_slide_the_colours_after_it() {
        let text = "plain LINK plain";
        let blue = Hsla {
            h: 0.6,
            s: 1.0,
            l: 0.5,
            a: 1.0,
        };
        let base = gpui::TextStyle::default();
        let link = gpui::TextStyle {
            color: blue,
            ..base.clone()
        };
        let runs = [base.to_run(6), link.to_run(4), base.to_run(6)];
        // Break BEFORE the link and again after it, so the link sits on its own
        // row — the arrangement that shifts if the newline is charged wrong.
        let (broken, broken_runs) = insert_breaks(text, &runs, &[6, 11]);
        assert_eq!(broken.as_ref(), "plain \nLINK \nplain");
        let colours = colours_per_byte(&broken, &broken_runs);
        assert_eq!(colours.len(), broken.len());
        let link_at = broken.find("LINK").unwrap();
        for (i, c) in colours.iter().enumerate() {
            let in_link = (link_at..link_at + 4).contains(&i);
            assert_eq!(
                *c == blue,
                in_link,
                "byte {i} ({:?}) coloured wrong",
                &broken[i..(i + 1).min(broken.len())]
            );
        }
    }

    #[test]
    fn words_keep_trailing_spaces_and_never_split_on_a_non_joiner() {
        assert_eq!(words("one two"), vec![0..4, 4..7]);
        assert_eq!(words("a  b"), vec![0..3, 3..4]);
        // U+200C (ZWNJ) is 3 bytes and lives INSIDE Persian words: می‌گیرد is
        // one word, and breaking at it would split it visually.
        let w = "می\u{200C}گیرد دنیا";
        assert_eq!(words(w).len(), 2);
        assert!(w[words(w)[0].clone()].contains('\u{200C}'));
    }

    #[test]
    fn sliced_runs_cover_exactly_the_requested_range() {
        let runs = [run(4), run(6)];
        let got = slice_runs(&runs, 2..7);
        assert_eq!(got.iter().map(|r| r.len).sum::<usize>(), 5);
        assert_eq!(got.len(), 2, "the range straddles both runs");
        assert_eq!(slice_runs(&runs, 0..0).len(), 0);
    }

    #[test]
    fn no_breaks_leaves_the_paragraph_untouched() {
        let text = "سلام دنیا";
        let (out, runs) = insert_breaks(text, &[run(text.len())], &[]);
        assert_eq!(out.as_ref(), text);
        assert_eq!(runs[0].len, text.len());
    }

    #[test]
    fn breaks_land_on_char_boundaries_of_multibyte_text() {
        // "سلام دنیا" — the space is at byte 8, inside multibyte content.
        let text = "سلام دنیا";
        let (out, _) = insert_breaks(text, &[run(text.len())], &[9]);
        assert_eq!(out.as_ref(), "سلام \nدنیا");
        // A mid-codepoint offset is refused rather than panicking the shaper.
        let (same, _) = insert_breaks(text, &[run(text.len())], &[1]);
        assert_eq!(same.as_ref(), text);
    }
}
