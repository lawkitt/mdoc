//! ⌫ and ⌦ at block boundaries and around hidden formatting markers.

use std::ops::Range;

use mdoc_markdown::lines::blockquote_prefix;

use crate::Blocks;
use crate::markdown::fmt_marker_pairs;
use crate::text::{line_end, line_starts, next_boundary, previous_boundary, row_col};

/// What ⌫ at the body start of `row` (or ⌦, with `forward`, at its end)
/// removes around a paragraph break (ADR 0060): the range and where the
/// caret lands. Across one blank line the two paragraphs join, undoing
/// Enter. In a quote the blank line is a bare `>` and the next row's
/// prefix goes too; extra bare `>` rows go one at a time, as plain blank
/// rows already do. `None` outside that shape, across differing quote
/// depths, or where newlines are literal or structural (code, math,
/// tables).
pub fn break_delete_range(
    source: &str,
    blocks: Blocks,
    row: usize,
    forward: bool,
) -> Option<(Range<usize>, usize)> {
    let (above, below) = if forward {
        (row, row + 2)
    } else {
        (row.checked_sub(2)?, row)
    };
    let mid = above + 1;
    let starts = line_starts(source);
    let line = |r: usize| Some(&source[*starts.get(r)?..line_end(source, r)]);
    let prefix = |r: usize| line(r).and_then(blockquote_prefix);
    let depth = |r: usize| {
        line(r).map(|l| prefix(r).map(|p| l[..p].bytes().filter(|&b| b == b'>').count()))
    };
    if depth(below)? != depth(mid)? || depth(above)? != depth(mid)? {
        return None;
    }
    let blank = |r: usize| line(r).is_some_and(|l| l[prefix(r).unwrap_or(0)..].trim().is_empty());
    let structural = |r: usize| blocks.row_is_literal(source, r) || blocks.row_in_table(r);
    if !blank(mid) || [above, mid, below].into_iter().any(structural) {
        return None;
    }
    let quoted = prefix(mid).is_some();
    let far = if forward { below } else { above };
    if blank(far) {
        // An extra blank row: plain ones delete as ordinary newlines.
        if !quoted {
            return None;
        }
        let range = starts[mid]..starts[mid + 1];
        let caret = if forward {
            line_end(source, row)
        } else {
            starts[mid] + prefix(below).unwrap_or(0)
        };
        return Some((range, caret));
    }
    let range = line_end(source, above)..starts[below] + prefix(below).unwrap_or(0);
    Some((range.clone(), range.start))
}

/// Cditor-style deletion planning around hidden formatting markers: the
/// range a backspace (`back`) / forward delete should remove at `off`.
/// Skips the invisible marker bytes to the adjacent VISIBLE character —
/// and when removing it would empty its construct, the now-empty marker
/// pair goes too (deleting bold's last char deletes the bold). `None` =
/// no hidden markers involved (the plain grapheme deletion applies).
pub fn fmt_delete_range(
    source: &str,
    blocks: Blocks,
    off: usize,
    back: bool,
) -> Option<Range<usize>> {
    let (row, col) = row_col(source, off);
    let line_start = line_starts(source)[row];
    let line_end = line_end(source, row);
    let line = &source[line_start..line_end];
    // Inside a fenced code block the text is verbatim — no markers there.
    if blocks.in_fence(row) || line.trim_start().starts_with("```") {
        return None;
    }
    let pairs = fmt_marker_pairs(line);
    if pairs.is_empty() {
        return None;
    }
    let markers: Vec<&Range<usize>> = pairs.iter().flat_map(|(o, c)| [o, c]).collect();
    // Skip marker bytes in the deletion direction to the visible char.
    let mut edge = col;
    if back {
        while let Some(sp) = markers.iter().find(|sp| sp.end == edge) {
            edge = sp.start;
        }
    } else {
        while let Some(sp) = markers.iter().find(|sp| sp.start == edge) {
            edge = sp.end;
        }
    }
    // The visible grapheme adjacent to the (possibly skipped-to) edge —
    // staying on this line; a line join takes the default path.
    let (del_start, del_end) = if back {
        if edge == 0 {
            return None;
        }
        let p = previous_boundary(source, line_start + edge) - line_start;
        (p, edge)
    } else {
        if edge >= line.len() {
            return None;
        }
        let n = next_boundary(source, line_start + edge).min(line_end) - line_start;
        (edge, n)
    };
    // Would this empty a construct? Only a real PAIR collapses: ITS opener
    // ending at the deletion's start and ITS closer starting at the end
    // (adjacent different constructs must not fuse).
    let emptied = pairs
        .iter()
        .find(|(o, c)| o.end == del_start && c.start == del_end);
    let range = match emptied {
        Some((o, c)) => o.start..c.end,
        None if edge == col => return None, // no markers were involved
        None => del_start..del_end,
    };
    Some(line_start + range.start..line_start + range.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockScan;

    #[test]
    fn backspace_at_a_paragraph_start_joins_across_the_blank_line() {
        let s = "one\n\ntwo";
        let scan = BlockScan::new(s);
        assert_eq!(
            break_delete_range(s, scan.view(), 2, false),
            Some((3..5, 3))
        );
        assert_eq!(break_delete_range(s, scan.view(), 0, true), Some((3..5, 3)));
        let q = "> one\n>\n> two";
        let scan = BlockScan::new(q);
        assert_eq!(
            break_delete_range(q, scan.view(), 2, false),
            Some((5..10, 5))
        );
    }

    #[test]
    fn no_join_into_code_or_tables() {
        let s = "```\nx\n```\n\ntwo";
        let scan = BlockScan::new(s);
        assert_eq!(break_delete_range(s, scan.view(), 4, false), None);
    }

    #[test]
    fn deletes_skip_hidden_markers_and_take_an_emptied_pair() {
        let s = "a **b** c";
        let scan = BlockScan::new(s);
        // ⌫ after the closing `**` deletes `b` and, emptied, the pair.
        assert_eq!(fmt_delete_range(s, scan.view(), 7, true), Some(2..7));
        let s = "**bc**";
        let scan = BlockScan::new(s);
        assert_eq!(fmt_delete_range(s, scan.view(), 6, true), Some(3..4));
        assert_eq!(
            fmt_delete_range("plain", BlockScan::new("plain").view(), 3, true),
            None
        );
    }
}
