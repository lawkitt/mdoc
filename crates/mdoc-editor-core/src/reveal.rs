//! The reveal rule: which hidden Markdown markers show around the caret or a
//! selection in live preview.
//!
//! Every marker span (`**`, `[`, `](url)`, `#`, `>`, a list bullet, …) is
//! hidden on a line the caret is not on. On the caret's line the rule
//! decides:
//!
//! - [`RevealRule::Desktop`]: formatting markers (`**`, `*`, `~~`, `` ` ``,
//!   `<u>`, `<mark>`) never reveal — the styling is the feedback and the
//!   toggles edit it; links and other structural constructs reveal while the
//!   caret is in them; the line prefix reveals only with the caret inside
//!   it. A selection reveals inline markers on the lines it touches, never
//!   the line prefix behind its painted mark.
//! - [`RevealRule::CaretSpan`] (ADR 0109, the phones): the construct the
//!   caret touches shows all its markers, formatting ones included; the line
//!   prefix shows anywhere on the caret's line. A selection reveals every
//!   marker on the lines it touches.

use std::ops::Range;

use mdoc_markdown::lines::{blockquote_prefix, heading_level, list_prefix, task_prefix};
use mdoc_markdown::spans::{Span, SpanStyle, scan};

/// Which markers the caret and selection reveal (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RevealRule {
    #[default]
    Desktop,
    CaretSpan,
}

/// What one line reveals, in line-local byte columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineReveal {
    /// The caret's column when a collapsed caret is on this line.
    pub caret_col: Option<usize>,
    /// Markers ending within this many bytes show (the revealed prefix).
    pub reveal_prefix: usize,
    /// Markers ending within this many bytes stay hidden even inside the
    /// caret's construct: the line prefix while its mark (bullet, number,
    /// box, quote bar) is painted instead.
    pub hide_prefix: usize,
    /// A selection touches this line: inline markers show.
    pub reveal_inline: bool,
    /// The raw line prefix shows, so its painted mark does not.
    pub show_prefix: bool,
}

impl RevealRule {
    /// What a line reveals: `caret_col` is a collapsed caret's column on it,
    /// `selection_touches` whether a selection covers any of it, and
    /// `prefix_len` the length of its line prefix (`None` without one).
    pub fn line(
        self,
        caret_col: Option<usize>,
        selection_touches: bool,
        prefix_len: Option<usize>,
    ) -> LineReveal {
        let show_prefix = match self {
            // The caret sitting in the BODY keeps the list/quote mark; only a
            // caret inside the hidden prefix reveals the raw marker.
            RevealRule::Desktop => prefix_len
                .zip(caret_col)
                .is_some_and(|(plen, col)| col < plen),
            RevealRule::CaretSpan => {
                prefix_len.is_some() && (caret_col.is_some() || selection_touches)
            }
        };
        let plen = prefix_len.unwrap_or(0);
        LineReveal {
            caret_col,
            reveal_prefix: if show_prefix { plen } else { 0 },
            hide_prefix: if show_prefix { 0 } else { plen },
            reveal_inline: selection_touches,
            show_prefix,
        }
    }

    /// Whether the marker span `range` (styled `style`) stays hidden, given
    /// the caret's `construct` (see [`construct_at`]) and what its line
    /// reveals. A span that is not a marker is never hidden.
    pub fn marker_hidden(
        self,
        style: &SpanStyle,
        range: &Range<usize>,
        construct: &Range<usize>,
        line: &LineReveal,
    ) -> bool {
        // A marker is shown when it's inside the caret's construct OR within the
        // revealed line-level prefix (e.g. a blockquote's `>` while the caret is
        // anywhere on the line).
        let in_construct = construct.start <= range.start && range.end <= construct.end;
        let in_prefix = range.end <= line.reveal_prefix;
        let force = range.end <= line.hide_prefix;
        let revealed = line.reveal_inline || in_construct || in_prefix;
        let never = match self {
            RevealRule::Desktop => style.always_hide,
            RevealRule::CaretSpan => false,
        };
        style.hide && (never || force || !revealed)
    }
}

/// The maximal run of adjacent spans (a markdown construct: its markers + body)
/// containing source byte `c`, or an empty range when `c` is in plain text.
/// `spans` must be sorted by `range.start` and non-overlapping. Used to reveal
/// only the construct the caret sits in (#5).
pub fn construct_at(spans: &[Span], c: usize) -> Range<usize> {
    let mut i = 0;
    while i < spans.len() {
        let start = spans[i].range.start;
        let mut end = spans[i].range.end;
        let mut j = i + 1;
        while j < spans.len() && spans[j].range.start == end {
            end = spans[j].range.end;
            j += 1;
        }
        if start <= c && c <= end {
            return start..end;
        }
        i = j;
    }
    0..0
}

/// The byte length of a line's prefix: a task, list or quote marker, or a
/// heading's hashes with their space.
pub fn line_prefix_len(line: &str) -> Option<usize> {
    task_prefix(line)
        .map(|(l, ..)| l)
        .or_else(|| list_prefix(line).map(|(l, ..)| l))
        .or_else(|| blockquote_prefix(line))
        .or_else(|| {
            heading_level(line).map(|n| {
                let n = n as usize;
                n + usize::from(line[n..].starts_with(' '))
            })
        })
}

/// The marker ranges of `source` that show for `selection` under `rule`, in
/// source byte offsets, sorted. Every other marker span is hidden. Only lines
/// the caret or selection touches reveal anything.
pub fn visible_markers(
    source: &str,
    selection: Range<usize>,
    rule: RevealRule,
) -> Vec<Range<usize>> {
    let (lo, hi) = (
        selection.start.min(selection.end).min(source.len()),
        selection.start.max(selection.end).min(source.len()),
    );
    let collapsed = lo == hi;
    let mut out = Vec::new();
    let mut line_start = source[..lo].rfind('\n').map_or(0, |i| i + 1);
    loop {
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |i| line_start + i);
        let line = &source[line_start..line_end];
        let caret_col = collapsed.then(|| lo - line_start);
        let mut spans = scan(line);
        spans.sort_by_key(|s| s.range.start);
        let construct = caret_col.map_or(0..0, |c| construct_at(&spans, c));
        let reveal = rule.line(caret_col, !collapsed, line_prefix_len(line));
        out.extend(
            spans
                .iter()
                .filter(|s| s.style.hide)
                .filter(|s| !rule.marker_hidden(&s.style, &s.range, &construct, &reveal))
                .map(|s| line_start + s.range.start..line_start + s.range.end),
        );
        if line_end >= hi || line_end >= source.len() {
            break;
        }
        line_start = line_end + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(source: &str, selection: Range<usize>, rule: RevealRule) -> Vec<&str> {
        visible_markers(source, selection, rule)
            .into_iter()
            .map(|r| &source[r])
            .collect()
    }

    #[test]
    fn desktop_never_reveals_formatting_markers() {
        let s = "a **bold** b";
        assert!(shown(s, 5..5, RevealRule::Desktop).is_empty());
        // A link reveals its syntax with the caret inside.
        let s = "a [t](u) b";
        assert_eq!(shown(s, 3..3, RevealRule::Desktop).concat(), "[](u)");
        assert!(shown(s, 0..0, RevealRule::Desktop).is_empty());
    }

    #[test]
    fn caret_span_reveals_the_touched_construct_only() {
        let s = "a **bold** and *it*";
        assert_eq!(shown(s, 5..5, RevealRule::CaretSpan), ["**", "**"]);
        // Touching the construct's edge counts.
        assert_eq!(shown(s, 2..2, RevealRule::CaretSpan), ["**", "**"]);
        assert_eq!(shown(s, 10..10, RevealRule::CaretSpan), ["**", "**"]);
        // Plain text between constructs reveals nothing.
        assert!(shown(s, 12..12, RevealRule::CaretSpan).is_empty());
        assert_eq!(shown(s, 17..17, RevealRule::CaretSpan), ["*", "*"]);
        // Every formatting kind reveals.
        for (line, caret) in [
            ("~~s~~", 3),
            ("`c`", 1),
            ("<u>u</u>", 3),
            ("<mark>m</mark>", 6),
        ] {
            assert!(
                !shown(line, caret..caret, RevealRule::CaretSpan).is_empty(),
                "{line:?}"
            );
        }
    }

    #[test]
    fn caret_span_shows_the_line_prefix_anywhere_on_the_line() {
        let s = "- item **b**\n> quote\nnext";
        assert_eq!(shown(s, 3..3, RevealRule::CaretSpan).concat(), "- ");
        assert_eq!(shown(s, 16..16, RevealRule::CaretSpan).concat(), "> ");
        // Desktop keeps the painted bullet unless the caret is in the prefix.
        assert!(shown(s, 3..3, RevealRule::Desktop).is_empty());
        assert_eq!(shown(s, 1..1, RevealRule::Desktop).concat(), "- ");
        // Another line's prefix stays hidden.
        assert!(shown(s, 25..25, RevealRule::CaretSpan).is_empty());
    }

    #[test]
    fn headings_reveal_their_hashes_on_the_caret_line() {
        let s = "## Title\nbody";
        assert_eq!(shown(s, 5..5, RevealRule::CaretSpan).concat(), "## ");
        assert!(shown(s, 11..11, RevealRule::CaretSpan).is_empty());
    }

    #[test]
    fn a_selection_reveals_its_lines() {
        let s = "a **b** c\n- d *e*\nf **g**";
        // Selecting across the first two lines: every marker there shows
        // under CaretSpan, inline-only under Desktop; the third line none.
        assert_eq!(
            shown(s, 2..13, RevealRule::CaretSpan),
            ["**", "**", "- ", "*", "*"]
        );
        assert!(shown(s, 2..13, RevealRule::Desktop).is_empty());
    }

    #[test]
    fn line_rules_match_desktop_painting() {
        let d = RevealRule::Desktop.line(Some(4), false, Some(2));
        assert_eq!(
            (d.reveal_prefix, d.hide_prefix, d.show_prefix),
            (0, 2, false)
        );
        let d = RevealRule::Desktop.line(Some(1), false, Some(2));
        assert_eq!(
            (d.reveal_prefix, d.hide_prefix, d.show_prefix),
            (2, 0, true)
        );
        let d = RevealRule::Desktop.line(None, true, Some(2));
        assert_eq!((d.hide_prefix, d.reveal_inline), (2, true));
        let c = RevealRule::CaretSpan.line(Some(4), false, Some(2));
        assert_eq!(
            (c.reveal_prefix, c.hide_prefix, c.show_prefix),
            (2, 0, true)
        );
        let c = RevealRule::CaretSpan.line(None, false, Some(2));
        assert_eq!(
            (c.reveal_prefix, c.hide_prefix, c.show_prefix),
            (0, 2, false)
        );
    }
}
