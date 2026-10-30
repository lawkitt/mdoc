//! GFM table recognition: regions, rows, cells and column alignment, plus
//! the `<!-- table:STYLE -->` marker line above a table.

use std::ops::Range;

use crate::spans::is_backslash_escaped;
use crate::syntax::{TableStyle, base_direction, table_col_widths, table_style_marker};

/// Per-column text alignment of a GFM table.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A detected GFM table region: the half-open range of logical line indices it
/// spans (header, separator, then body rows) and its per-column alignment, plus
/// an optional `<!-- table:STYLE -->` marker line directly above it.
#[derive(Clone, Debug)]
pub struct TableRegion {
    pub lines: Range<usize>,
    pub aligns: Vec<Align>,
    pub style: TableStyle,
    /// Index of the style-marker comment line above the header, if present — the
    /// editor hides it (revealed only when the caret lands on it).
    pub marker_line: Option<usize>,
    /// Explicit column widths (logical px) from the marker's `cols=` attribute
    /// — written by drag-to-resize; `None` = content-measured.
    pub col_widths_attr: Option<Vec<f32>>,
    /// The table reads right-to-left (#66), so its grid hugs the note column's
    /// RIGHT edge with the 22px gutter on the other side — the reader's
    /// `.mr(22).ml_auto()`. Column ORDER is not mirrored.
    pub rtl: bool,
}

/// Detect GFM table regions in `content` (W4c). A region is a row line (trimmed
/// text starts with `|`) immediately followed by a separator row
/// (`| --- | :--: |`), then any further row lines. Returns regions in order.
pub fn table_regions(content: &str) -> Vec<TableRegion> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if is_table_row(lines[i])
            && let Some(aligns) = separator_aligns(lines[i + 1])
        {
            let start = i;
            let mut end = i + 2; // header + separator
            // Stop before a new table jammed directly below: a row that is itself
            // immediately followed by a `|---|` separator is the next table's
            // header, not a body row of this one — otherwise adjacent tables (no
            // blank line between) merge into one grid and the second separator
            // shows up as `---` cells.
            while end < lines.len()
                && is_table_row(lines[end])
                && separator_aligns(lines.get(end + 1).copied().unwrap_or("")).is_none()
            {
                end += 1;
            }
            // A `<!-- table:STYLE -->` comment on the line directly above sets the
            // table's visual style (and is hidden by the editor).
            let (style, marker_line) = match start
                .checked_sub(1)
                .map(|m| (m, table_style_marker(lines[m])))
            {
                Some((m, Some(s))) => (s, Some(m)),
                _ => (TableStyle::Grid, None),
            };
            let col_widths_attr = marker_line.and_then(|m| table_col_widths(lines[m]));
            // Writing direction of the whole table (#66) — the reader runs
            // `base_direction` over the table's byte span, so join the region's
            // lines and ask the same question: the first strong character
            // anywhere in the table decides (pipes, dashes and digits are
            // neutral under UAX #9). Once per scan, not per frame.
            let rtl = base_direction(&lines[start..end].join("\n")).is_rtl();
            out.push(TableRegion {
                lines: start..end,
                aligns,
                style,
                marker_line,
                col_widths_attr,
                rtl,
            });
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// A table row is a line whose trimmed text starts with `|`.
pub fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Split a `| a | b |` row into trimmed cell strings (the bounding pipes drop the
/// empty leading/trailing cells they'd otherwise create).
pub fn table_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    split_unescaped_pipes(t).map(str::trim).collect()
}

/// Split `s` at its UNESCAPED `|` bytes — a `\|` (odd run of preceding
/// backslashes) is literal cell content per GFM, not a cell separator. The
/// escape stays in the returned slices (cells are verbatim source), so cell
/// ranges and caret math keep their exact source mapping, and a rewrite that
/// re-joins the cells round-trips the escape.
fn split_unescaped_pipes(s: &str) -> impl Iterator<Item = &str> {
    let b = s.as_bytes();
    let mut cuts = vec![0usize];
    for (i, &c) in b.iter().enumerate() {
        if c == b'|' && !is_backslash_escaped(b, i) {
            cuts.push(i + 1);
        }
    }
    cuts.push(s.len() + 1);
    (0..cuts.len() - 1).map(move |k| &s[cuts[k]..cuts[k + 1] - 1])
}

/// The byte range of each cell's *trimmed content* within `line` (line-local), in
/// the same order as [`table_cells`]. Lets the editor place the caret inside a
/// rendered cell and hit-test a click back to a source offset. An empty cell is a
/// zero-width range at its content position.
pub fn table_cell_ranges(line: &str) -> Vec<Range<usize>> {
    let (Some(first), Some(last)) = (line.find('|'), line.rfind('|')) else {
        return Vec::new();
    };
    if last <= first {
        return Vec::new();
    }
    let base = first + 1; // start of the inter-pipe region in `line`
    let inner = &line[base..last];
    let mut out = Vec::new();
    let mut seg = 0; // offset of the current cell within `inner`
    for piece in split_unescaped_pipes(inner) {
        let lead = piece.len() - piece.trim_start().len();
        let trail = piece.len() - piece.trim_end().len();
        let start = base + seg + lead;
        out.push(start..(base + seg + piece.len() - trail).max(start));
        seg += piece.len() + 1; // + the `|`
    }
    out
}

/// If `line` is a table separator row (every cell is dashes with optional
/// alignment colons), return its per-column alignment, else `None`.
fn separator_aligns(line: &str) -> Option<Vec<Align>> {
    if !is_table_row(line) {
        return None;
    }
    let cells = table_cells(line);
    if cells.is_empty() {
        return None;
    }
    cells
        .iter()
        .map(|c| {
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            let dashes = c.trim_matches(':');
            (!dashes.is_empty() && dashes.bytes().all(|b| b == b'-')).then_some(
                match (left, right) {
                    (true, true) => Align::Center,
                    (false, true) => Align::Right,
                    _ => Align::Left,
                },
            )
        })
        .collect()
}

/// Whether a table row is the Markdown alignment/separator row. Search treats
/// it as structural chrome, just as the WYSIWYG table renderer does.
pub fn is_table_separator(line: &str) -> bool {
    separator_aligns(line).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_gfm_table() {
        let md = "intro\n\n| A | B |\n| --- | :-: |\n| 1 | 2 |\n| 3 | 4 |\n\nafter";
        let regions = table_regions(md);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].lines, 2..6); // header, separator, 2 body rows
        assert_eq!(regions[0].aligns, vec![Align::Left, Align::Center]);
    }

    #[test]
    fn cells_split_and_trim() {
        assert_eq!(table_cells("| a | b |"), vec!["a", "b"]);
        assert_eq!(table_cells("|  |  |"), vec!["", ""]);
    }

    #[test]
    fn escaped_pipes_are_cell_content() {
        // GFM: `\|` is a literal pipe in the cell, not a separator — the
        // reader (GFM parser) agrees; a blind split miscounted cells and
        // column rewrites then corrupted the row.
        assert_eq!(
            table_cells(r"| left \| right | ok |"),
            vec![r"left \| right", "ok"]
        );
        // A double backslash escapes the backslash — that pipe IS a separator.
        assert_eq!(table_cells(r"| a \\| b |"), vec![r"a \\", "b"]);
        // Ranges walk the same split: two ranges, first spans the escape.
        let line = r"| left \| right | ok |";
        let ranges = table_cell_ranges(line);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&line[ranges[0].clone()], r"left \| right");
        assert_eq!(&line[ranges[1].clone()], "ok");
    }

    #[test]
    fn separator_required_and_alignment() {
        // Pipes in prose without a separator row are not a table.
        assert!(table_regions("a | b\nc | d").is_empty());
        assert!(table_regions("| not a table\njust text").is_empty());
        // Right alignment from `---:`.
        assert_eq!(
            table_regions("| h |\n| ---: |\n| x |")[0].aligns,
            vec![Align::Right]
        );
    }

    #[test]
    fn table_style_marker_and_region() {
        assert_eq!(
            table_style_marker("<!-- table:striped -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(
            table_style_marker("<!--table:minimal-->"),
            Some(TableStyle::Minimal)
        );
        assert_eq!(table_style_marker("<!-- table:bogus -->"), None);
        assert_eq!(table_style_marker("<!-- a comment -->"), None);
        assert_eq!(table_style_marker("| a | b |"), None);

        // The marker above a table sets its style + is recorded (not in `lines`).
        let r = table_regions("<!-- table:header -->\n| h |\n| --- |\n| x |");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].style, TableStyle::Header);
        assert_eq!(r[0].marker_line, Some(0));
        assert_eq!(r[0].lines, 1..4);

        // No marker → default Grid.
        let g = table_regions("| h |\n| --- |\n| x |");
        assert_eq!(g[0].style, TableStyle::Grid);
        assert_eq!(g[0].marker_line, None);
    }

    #[test]
    fn adjacent_tables_split_into_two_regions() {
        // Two tables jammed together with no blank line between must not merge: the
        // second table's header + `|---|` start a new region (otherwise the second
        // separator rendered as `---` cells in one merged grid).
        let r = table_regions(
            "| a | b |\n| --- | --- |\n| 1 | 2 |\n| c | d |\n| --- | --- |\n| 3 | 4 |",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].lines, 0..3);
        assert_eq!(r[1].lines, 3..6);
    }
}
