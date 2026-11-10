//! The phones' table cells (ADR 0109): in live preview a table is the drawn
//! grid, edited cell by cell. A caret rests in one cell, between its pipes;
//! typing, ⌫, ⌦ and paste change only that cell's bytes; Return, Tab and the
//! arrows move between cells. Nothing pads or realigns, and no other cell's
//! bytes change. Desktop's editor keeps its own table navigation; the
//! structural commands (rows, columns, alignment) are [`crate::tables`]'s,
//! which both call.
//!
//! Inside a cell a backslash and the punctuation it escapes (`\|`, `\\`) are
//! one unit: the caret never rests between them and a delete takes both, so
//! no edit turns an escaped pipe into a cell border or the other way round.

use std::ops::Range;

use mdoc_markdown::tables::{Align, TableRegion, table_cell_breaks, table_cell_spans};
use unicode_segmentation::UnicodeSegmentation;

use crate::text::{line_end, line_starts, row_col, trailing_backslashes};
use crate::{BlockScan, Blocks, Edit, EditResult};

/// The cell of a drawn table that holds an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The grid row: 0 is the header, then the body rows (the `| --- |`
    /// row is not one).
    pub row: usize,
    pub column: usize,
    /// The table's grid rows and columns.
    pub rows: usize,
    pub columns: usize,
    /// Between the cell's pipes, its spaces included.
    pub span: Range<usize>,
    /// Its content without the spaces; an empty cell's is empty, at the end
    /// of its span.
    pub content: Range<usize>,
    /// The cell's column's alignment.
    pub align: Align,
}

/// A caret move in a table: Tab, ⇧Tab and the arrow keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellMove {
    /// The next cell (Tab), the next row's first after a row's last; from the
    /// table's last cell, out below it as Return leaves its last row.
    Next,
    /// The previous cell (⇧Tab); the first cell stays.
    Previous,
    /// The cell above, or the line above the table from the header.
    Up,
    /// The cell below, or the line below the table from its last row.
    Down,
    /// A character left, then the previous cell's end, then out above.
    Left,
    /// A character right, then the next cell's start, then out below.
    Right,
}

/// Where in a cell a move lands.
#[derive(Clone, Copy)]
enum Place {
    Start,
    End,
    /// Bytes into the content, as far as it goes.
    Into(usize),
}

/// A drawn table and the grid line, row and column an offset is in.
struct Located<'a> {
    region: &'a TableRegion,
    starts: Vec<usize>,
    /// The table's grid lines: the header, then the body rows.
    lines: Vec<usize>,
    row: usize,
    column: usize,
    /// The offset lies in a cell's span on its grid line (not on the
    /// `| --- |` row, the style marker or past the pipes).
    inside: bool,
}

impl Located<'_> {
    /// Each cell's span on grid row `row`, in the source's offsets.
    fn spans(&self, source: &str, row: usize) -> Vec<Range<usize>> {
        let line = self.lines[row];
        let start = self.starts[line];
        let end = line_end(source, line);
        table_cell_spans(&source[start..end])
            .into_iter()
            .map(|span| start + span.start..start + span.end)
            .collect()
    }

    fn cell(&self, source: &str) -> Option<Cell> {
        let span = self.spans(source, self.row).get(self.column)?.clone();
        Some(Cell {
            row: self.row,
            column: self.column,
            rows: self.lines.len(),
            columns: self.region.aligns.len(),
            content: content(source, &span),
            span,
            align: self
                .region
                .aligns
                .get(self.column)
                .copied()
                .unwrap_or(Align::Left),
        })
    }

    /// Where `place` is in the cell at `row`, `column` (the row's last cell
    /// when it has fewer).
    fn caret(&self, source: &str, row: usize, column: usize, place: Place) -> usize {
        let spans = self.spans(source, row);
        let Some(span) = spans.get(column.min(spans.len().saturating_sub(1))) else {
            return self.starts[self.lines[row]];
        };
        let content = content(source, span);
        if content.is_empty() {
            return empty_caret(source, span);
        }
        match place {
            Place::Start => content.start,
            Place::End => content.end,
            Place::Into(bytes) => {
                // Pulled back onto a caret stop, never past where it was.
                let at = (content.start + bytes).min(content.end) - span.start;
                let stop = units(&source[span.clone()])
                    .into_iter()
                    .rev()
                    .find(|&stop| stop <= at)
                    .unwrap_or(0);
                (span.start + stop).max(content.start)
            }
        }
    }

    /// The line just above the table (its style marker skipped): its end.
    fn above(&self) -> Option<usize> {
        let first = self.region.marker_line.unwrap_or(self.region.lines.start);
        (first > 0).then(|| self.starts[first] - 1)
    }

    /// The line just below the table: its start.
    fn below(&self) -> Option<usize> {
        self.starts.get(self.region.lines.end).copied()
    }
}

/// `span`'s content without its spaces: at the span's end when blank, as
/// [`mdoc_markdown::tables::table_cell_ranges`] reads it.
fn content(source: &str, span: &Range<usize>) -> Range<usize> {
    let text = &source[span.clone()];
    let start = span.start + (text.len() - text.trim_start().len());
    let end = span.end - (text.len() - text.trim_end().len());
    start..end.max(start)
}

/// An empty cell's caret: between its spaces when it has two (`| ‸ |`), so
/// typed text sits as a row written by hand does; else before its pipe.
fn empty_caret(source: &str, span: &Range<usize>) -> usize {
    if span.len() >= 2 && source.as_bytes()[span.end - 1] == b' ' {
        span.end - 1
    } else {
        span.end
    }
}

/// The caret stops in `text` (a cell's span), relative to it: grapheme
/// boundaries, a backslash and the punctuation it escapes counting as one,
/// and a `<br>` tag, which the grid draws as a line break, as one.
fn units(text: &str) -> Vec<usize> {
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let breaks = table_cell_breaks(text);
    let mut stops = vec![0];
    let mut k = 0;
    while k < graphemes.len() {
        let at = graphemes[k].0;
        if let Some(tag) = breaks.iter().find(|tag| tag.start == at) {
            while k < graphemes.len() && graphemes[k].0 < tag.end {
                k += 1;
            }
        } else {
            let escapes = graphemes[k].1 == "\\"
                && graphemes
                    .get(k + 1)
                    .is_some_and(|(_, g)| g.len() == 1 && g.as_bytes()[0].is_ascii_punctuation());
            k += if escapes { 2 } else { 1 };
        }
        stops.push(graphemes.get(k).map_or(text.len(), |(i, _)| *i));
    }
    stops
}

/// `offset` within `span`, moved forward onto a caret stop.
fn snap(source: &str, span: &Range<usize>, offset: usize) -> usize {
    let offset = offset.clamp(span.start, span.end);
    let rel = offset - span.start;
    units(&source[span.clone()])
        .into_iter()
        .find(|&stop| stop >= rel)
        .map_or(span.end, |stop| span.start + stop)
}

/// The drawn table holding `offset`, and where in it. `None` outside a
/// table, and for table-shaped code or math, which is literal text.
fn locate<'a>(source: &str, blocks: Blocks<'a>, offset: usize) -> Option<Located<'a>> {
    let offset = offset.min(source.len());
    let (line, _) = row_col(source, offset);
    let region = blocks
        .tables
        .iter()
        .find(|t| t.lines.contains(&line) || t.marker_line == Some(line))?;
    if blocks.row_is_literal(source, region.lines.start) {
        return None;
    }
    let starts = line_starts(source);
    let header = region.lines.start;
    let lines: Vec<usize> = std::iter::once(header)
        .chain(region.lines.clone().skip(2))
        .collect();
    let mut located = Located {
        region,
        starts,
        lines,
        row: 0,
        column: 0,
        inside: false,
    };
    if region.marker_line == Some(line) {
        return Some(located);
    }
    // The `| --- |` row's columns stand for the header's.
    if line != header + 1 {
        located.row = located.lines.iter().position(|&l| l == line)?;
    }
    let start = located.starts[line];
    let spans: Vec<Range<usize>> = table_cell_spans(&source[start..line_end(source, line)])
        .into_iter()
        .map(|span| start + span.start..start + span.end)
        .collect();
    let column = spans.iter().position(|span| offset <= span.end)?;
    located.column = column.min(located.spans(source, located.row).len().saturating_sub(1));
    located.inside =
        line != header + 1 && spans[column].start <= offset && offset <= spans[column].end;
    Some(located)
}

/// The cell of a drawn table holding `offset`; its `| --- |` row and style
/// marker stand for the header. `None` outside a drawn table.
pub fn cell_at(source: &str, offset: usize) -> Option<Cell> {
    let scan = BlockScan::new(source);
    locate(source, scan.view(), offset)?.cell(source)
}

/// `selection` kept in one cell of a drawn table, the cell of its caret
/// (`end`): an offset past the pipes, on the `| --- |` row or the style
/// marker goes to the cell's start, an offset inside an escape after it, an
/// empty cell's caret between its spaces. `None` outside a drawn table.
pub fn keep_in_cell(source: &str, selection: Range<usize>) -> Option<Range<usize>> {
    let scan = BlockScan::new(source);
    let located = locate(source, scan.view(), selection.end)?;
    let cell = located.cell(source)?;
    let caret = if !located.inside {
        located.caret(source, cell.row, cell.column, Place::Start)
    } else if cell.content.is_empty() {
        empty_caret(source, &cell.span)
    } else {
        snap(source, &cell.span, selection.end)
    };
    if selection.start == selection.end {
        return Some(caret..caret);
    }
    let anchor = snap(source, &cell.span, selection.start);
    Some(anchor..caret)
}

/// Moves the caret as Tab, ⇧Tab or an arrow key does in a drawn table:
/// no edit, except Tab from the last cell, which leaves the table below as
/// Return from its last row does (ADR 0060). `None` outside a drawn table.
pub fn move_caret(source: &str, selection: Range<usize>, by: CellMove) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    let caret = selection.end.min(source.len());
    let located = locate(source, blocks, caret)?;
    let cell = located.cell(source)?;
    let (row, column) = (cell.row, cell.column);
    let last_row = cell.rows - 1;
    let row_cells = |r: usize| located.spans(source, r).len();
    let stay = Some(caret);
    let to = match by {
        CellMove::Next if column + 1 < row_cells(row) => {
            Some(located.caret(source, row, column + 1, Place::Start))
        }
        CellMove::Next if row < last_row => Some(located.caret(source, row + 1, 0, Place::Start)),
        CellMove::Next => {
            let (range, text, offset) =
                crate::enter::table_exit_edit(source, located.region.lines.end - 1);
            let to = range.start + offset;
            return Some(EditResult::one(range, text, to));
        }
        CellMove::Previous if column > 0 => {
            Some(located.caret(source, row, column - 1, Place::Start))
        }
        CellMove::Previous if row > 0 => {
            let last = row_cells(row - 1).saturating_sub(1);
            Some(located.caret(source, row - 1, last, Place::Start))
        }
        CellMove::Previous => stay,
        CellMove::Up | CellMove::Down => {
            let into = Place::Into(caret.saturating_sub(cell.content.start));
            match by {
                CellMove::Up if row > 0 => Some(located.caret(source, row - 1, column, into)),
                CellMove::Up => located.above().or(stay),
                _ if row < last_row => Some(located.caret(source, row + 1, column, into)),
                _ => located.below().or(stay),
            }
        }
        CellMove::Right if located.inside && caret < cell.content.end => {
            let stops = units(&source[cell.span.clone()]);
            let next = stops
                .iter()
                .map(|s| cell.span.start + s)
                .find(|&s| s > caret)
                .unwrap_or(cell.span.end);
            Some(next.clamp(cell.content.start, cell.content.end))
        }
        CellMove::Right if column + 1 < row_cells(row) => {
            Some(located.caret(source, row, column + 1, Place::Start))
        }
        CellMove::Right if row < last_row => Some(located.caret(source, row + 1, 0, Place::Start)),
        CellMove::Right => located.below().or(stay),
        CellMove::Left if located.inside && caret > cell.content.start => {
            let stops = units(&source[cell.span.clone()]);
            let previous = stops
                .iter()
                .rev()
                .map(|s| cell.span.start + s)
                .find(|&s| s < caret)
                .unwrap_or(cell.span.start);
            Some(previous.clamp(cell.content.start, cell.content.end))
        }
        CellMove::Left if column > 0 => Some(located.caret(source, row, column - 1, Place::End)),
        CellMove::Left if row > 0 => {
            let last = row_cells(row - 1).saturating_sub(1);
            Some(located.caret(source, row - 1, last, Place::End))
        }
        CellMove::Left => located.above().or(stay),
    }?;
    Some(EditResult {
        edits: Vec::new(),
        selection: to..to,
    })
}

/// Nothing changes: the caret stays.
fn unchanged(caret: usize) -> EditResult {
    EditResult {
        edits: Vec::new(),
        selection: caret..caret,
    }
}

/// The cell holding the whole selection, when its caret is in a drawn
/// table: `Err` when the selection reaches past that cell's pipes.
fn selected_cell(
    source: &str,
    blocks: Blocks,
    selection: &Range<usize>,
) -> Option<Result<Cell, ()>> {
    let located = locate(source, blocks, selection.end)?;
    let cell = located.cell(source)?;
    let (start, end) = (
        selection.start.min(selection.end),
        selection.start.max(selection.end),
    );
    Some(
        if located.inside && cell.span.start <= start && end <= cell.span.end {
            Ok(cell)
        } else {
            Err(())
        },
    )
}

/// ⌫ (`forward` false) or ⌦ in a drawn table: the selection, or the unit
/// before (after) the caret, never past the cell's pipes. At the cell's
/// start (end), with only spaces before (after) the caret, nothing changes;
/// so does a selection reaching past the cell. `None` outside a drawn table.
pub fn delete(
    source: &str,
    blocks: Blocks,
    selection: Range<usize>,
    forward: bool,
) -> Option<EditResult> {
    let caret = selection.end.min(source.len());
    let Ok(cell) = selected_cell(source, blocks, &selection)? else {
        return Some(unchanged(caret));
    };
    let (start, end) = (
        selection.start.min(selection.end),
        selection.start.max(selection.end),
    );
    if start != end {
        return Some(cell_edit(source, &cell.span, start..end, ""));
    }
    let span = &cell.span;
    let range = if forward {
        if source[caret..span.end].trim().is_empty() {
            return Some(unchanged(caret));
        }
        let stops = units(&source[span.clone()]);
        let next = stops
            .iter()
            .map(|s| span.start + s)
            .find(|&s| s > caret)
            .unwrap_or(span.end);
        caret..next
    } else {
        if source[span.start..caret].trim().is_empty() {
            return Some(unchanged(caret));
        }
        let stops = units(&source[span.clone()]);
        let previous = stops
            .iter()
            .rev()
            .map(|s| span.start + s)
            .find(|&s| s < caret)
            .unwrap_or(span.start);
        previous..caret
    };
    Some(cell_edit(source, &cell.span, range, ""))
}

/// `typed` over the selection in a cell of a drawn table, as the cell's own
/// bytes: a line break becomes `<br>` (a pasted one a space, as desktop
/// pastes into a cell) and a pipe is escaped (`\|`), so the text stays in
/// its cell; backslashes before a pipe keep it a pipe or a border as it
/// was. A selection reaching past the cell changes nothing. `None` outside
/// a drawn table.
pub fn type_text(
    source: &str,
    selection: Range<usize>,
    typed: &str,
    pasted: bool,
) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let caret = selection.end.min(source.len());
    let Ok(cell) = selected_cell(source, scan.view(), &selection)? else {
        return Some(unchanged(caret));
    };
    let (start, end) = (
        selection.start.min(selection.end),
        selection.start.max(selection.end),
    );
    let text = typed.replace("\r\n", "\n").replace('\r', "\n");
    let text = if pasted {
        // Desktop's paste into a cell (`paste::normalize_paste`): one line,
        // its escapes undone and done again below.
        text.trim_end_matches('\n')
            .replace('\n', " ")
            .replace("\\|", "|")
    } else {
        text.replace('\n', "<br>")
    };
    Some(cell_edit(source, &cell.span, start..end, &text))
}

/// `range` of the cell at `span` replaced with `text`, its pipes escaped:
/// a backslash before a pipe stays a backslash, and the pipe after the
/// text (the cell's border, or an escaped one) stays what it was. The
/// caret after it.
fn cell_edit(source: &str, span: &Range<usize>, range: Range<usize>, text: &str) -> EditResult {
    let mut out = String::with_capacity(text.len() + 2);
    let mut run = trailing_backslashes(&source[span.start..range.start]);
    for ch in text.chars() {
        match ch {
            '\\' => {
                out.push('\\');
                run += 1;
            }
            '|' => {
                if run % 2 == 1 {
                    out.push('\\');
                }
                out.push_str("\\|");
                run = 0;
            }
            _ => {
                out.push(ch);
                run = 0;
            }
        }
    }
    let rest = &source[range.end..];
    let slashes = rest.bytes().take_while(|&b| b == b'\\').count();
    if rest[slashes..].starts_with('|') {
        let was = trailing_backslashes(&source[span.start..range.end]) + slashes;
        if was % 2 != (run + slashes) % 2 {
            out.push('\\');
        }
    }
    let to = range.start + out.len();
    EditResult {
        edits: vec![Edit { range, text: out }],
        selection: to..to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockScan;

    const T: &str = "intro\n\n| a | bb |\n| --- | :-: |\n| 1 |  |\n| x \\| y | z |\n\nafter";

    fn at(source: &str, needle: &str) -> usize {
        source.find(needle).unwrap()
    }

    fn moved(source: &str, caret: usize, by: CellMove) -> usize {
        let result = move_caret(source, caret..caret, by).unwrap();
        assert!(result.edits.is_empty(), "{by:?} edited");
        result.selection.end
    }

    fn typed(source: &str, caret: usize, text: &str) -> String {
        type_text(source, caret..caret, text, false)
            .unwrap()
            .apply(source)
    }

    fn deleted(source: &str, caret: usize, forward: bool) -> (String, usize) {
        let scan = BlockScan::new(source);
        let result = delete(source, scan.view(), caret..caret, forward).unwrap();
        (result.apply(source), result.selection.end)
    }

    #[test]
    fn cells_are_found_by_offset() {
        let cell = cell_at(T, at(T, "bb")).unwrap();
        assert_eq!(
            (cell.row, cell.column, cell.rows, cell.columns),
            (0, 1, 3, 2)
        );
        assert_eq!(&T[cell.content.clone()], "bb");
        assert_eq!(&T[cell.span.clone()], " bb ");
        assert_eq!(cell.align, Align::Center);
        let cell = cell_at(T, at(T, "x \\|")).unwrap();
        assert_eq!((cell.row, cell.column), (2, 0));
        assert_eq!(&T[cell.content.clone()], "x \\| y");
        // The empty cell: its content at the end of its span.
        let cell = cell_at(T, at(T, "|  |") + 2).unwrap();
        assert_eq!((cell.row, cell.column), (1, 1));
        assert!(cell.content.is_empty());
        // The `| --- |` row stands for the header.
        assert_eq!(cell_at(T, at(T, ":-:")).unwrap().row, 0);
        assert_eq!(cell_at(T, 2), None);
        assert_eq!(cell_at("```\n| a |\n| - |\n```", 5), None);
    }

    #[test]
    fn a_caret_is_kept_in_its_cell() {
        let keep = |c: usize| keep_in_cell(T, c..c).unwrap().end;
        // In a cell's spaces: where it is.
        let b = at(T, "bb");
        assert_eq!(keep(b - 1), b - 1);
        // Before the row's first pipe, on the `| --- |` row: a cell's start.
        let row = at(T, "| 1");
        assert_eq!(keep(row), row + 2);
        assert_eq!(keep(at(T, ":-:")), b);
        // Between a backslash and its pipe: after the pipe.
        let escape = at(T, "\\|");
        assert_eq!(keep(escape + 1), escape + 2);
        // An empty cell: between its spaces.
        let empty = at(T, "|  |");
        assert_eq!(keep(empty + 3), empty + 2);
        // A selection's anchor is kept in the caret's cell.
        assert_eq!(keep_in_cell(T, at(T, "a |")..b), Some(b - 1..b));
        assert_eq!(keep_in_cell(T, 0..0), None);
        // The style marker stands for the header's first cell.
        let marked = "<!-- table:striped -->\n| a |\n| - |";
        assert_eq!(keep_in_cell(marked, 3..3), Some(25..25));
    }

    #[test]
    fn tab_and_arrows_move_between_cells() {
        let a = at(T, "a |");
        let b = at(T, "bb");
        let one = at(T, "1 |");
        let empty = at(T, "|  |") + 2;
        let x = at(T, "x \\|");
        let z = at(T, "z |");
        assert_eq!(moved(T, a, CellMove::Next), b);
        assert_eq!(moved(T, b, CellMove::Next), one);
        assert_eq!(moved(T, one, CellMove::Next), empty);
        assert_eq!(moved(T, one, CellMove::Previous), b);
        assert_eq!(moved(T, a, CellMove::Previous), a);
        // Arrows step a character, then over to the next cell.
        assert_eq!(moved(T, b, CellMove::Right), b + 1);
        assert_eq!(moved(T, b + 2, CellMove::Right), one);
        assert_eq!(moved(T, one, CellMove::Left), b + 2);
        assert_eq!(moved(T, a, CellMove::Left), at(T, "\n| a"));
        // A `<br>` is one step.
        let broken = "| a<br>b |\n| - |";
        let tag = at(broken, "<br>");
        assert_eq!(moved(broken, tag, CellMove::Right), tag + 4);
        assert_eq!(moved(broken, tag + 4, CellMove::Left), tag);
        assert_eq!(
            keep_in_cell(broken, tag + 2..tag + 2),
            Some(tag + 4..tag + 4)
        );
        // An escape is one step.
        let escape = at(T, "\\|");
        assert_eq!(moved(T, escape, CellMove::Right), escape + 2);
        assert_eq!(moved(T, escape + 2, CellMove::Left), escape);
        // Up and down keep the column, past the `| --- |` row, and leave
        // the table at its edges.
        assert_eq!(moved(T, b + 1, CellMove::Down), empty);
        assert_eq!(moved(T, one, CellMove::Up), a);
        assert_eq!(moved(T, z, CellMove::Down), at(T, "\nafter"));
        assert_eq!(moved(T, b, CellMove::Up), at(T, "\n| a"));
        assert_eq!(moved(T, z + 1, CellMove::Right), at(T, "\nafter"));
        assert_eq!(moved(T, x, CellMove::Up), one);
        // Tab from the last cell leaves the table as Return does.
        let result = move_caret(T, z..z, CellMove::Next).unwrap();
        assert_eq!(result.apply(T), T.replace("z |\n", "z |\n\n\n"));
        assert_eq!(move_caret("plain", 1..1, CellMove::Next), None);
    }

    #[test]
    fn deletes_stay_in_the_cell() {
        let b = at(T, "bb");
        assert_eq!(deleted(T, b, false), (T.into(), b));
        assert_eq!(deleted(T, b + 2, true), (T.into(), b + 2));
        assert_eq!(deleted(T, b + 1, false).0, T.replace("| bb", "| b"));
        // An escape goes whole.
        let escape = at(T, "\\|");
        assert_eq!(
            deleted(T, escape + 2, false).0,
            T.replace("x \\| y", "x  y")
        );
        assert_eq!(deleted(T, escape, true).0, T.replace("x \\| y", "x  y"));
        // A selection past the cell changes nothing.
        let scan = BlockScan::new(T);
        let across = delete(T, scan.view(), b..at(T, "1 |"), false).unwrap();
        assert!(across.edits.is_empty());
    }

    #[test]
    fn typing_escapes_pipes_and_breaks_lines() {
        let b = at(T, "bb");
        assert_eq!(typed(T, b, "a|b"), T.replace("| bb", "| a\\|bbb"));
        assert_eq!(typed(T, b, "x\ny"), T.replace("| bb", "| x<br>ybb"));
        // A backslash before a typed pipe stays a backslash.
        assert_eq!(typed(T, b, "\\|"), T.replace("| bb", "| \\\\\\|bb"));
        // A backslash typed before the border keeps it a border.
        let end = b + 2;
        assert_eq!(typed(T, end + 1, "\\"), T.replace("bb |", "bb \\\\|"));
        // Pasted: one line, as desktop pastes into a cell.
        let pasted = type_text(T, b..b, "1|2\\|3\n4\n", true).unwrap().apply(T);
        assert_eq!(pasted, T.replace("| bb", "| 1\\|2\\|3 4bb"));
        assert_eq!(type_text("plain", 1..1, "|", false), None);
    }

    /// Typing at any caret stop of any cell, any text, changes only the
    /// bytes between that cell's pipes, and the table keeps its cells.
    #[test]
    fn typing_changes_only_the_cell() {
        let sources = [
            T,
            "| a\\\\ | \\| |\n| - | - |\n| я | `c|d` |",
            "<!-- table:minimal -->\n|  |  |\n| --- | --- |\n|  |  |",
        ];
        let texts = ["x", "|", "\\", "\\|", "a\nb", "\\\\", "||", "я|\\"];
        for source in sources {
            let scan = BlockScan::new(source);
            let table = &scan.tables[0];
            let starts = line_starts(source);
            let lines: Vec<usize> = std::iter::once(table.lines.start)
                .chain(table.lines.clone().skip(2))
                .collect();
            for &line in &lines {
                let start = starts[line];
                let spans = table_cell_spans(&source[start..line_end(source, line)]);
                for span in spans {
                    let span = start + span.start..start + span.end;
                    for stop in units(&source[span.clone()]) {
                        let caret = span.start + stop;
                        for text in texts {
                            for pasted in [false, true] {
                                let result = type_text(source, caret..caret, text, pasted).unwrap();
                                let out = result.apply(source);
                                let edit = &result.edits[0];
                                assert!(
                                    span.start <= edit.range.start && edit.range.end <= span.end
                                );
                                let grown = out.len() - source.len();
                                assert_eq!(
                                    &out[..span.start],
                                    &source[..span.start],
                                    "{source:?} {caret} {text:?}"
                                );
                                assert_eq!(
                                    &out[span.end + grown..],
                                    &source[span.end..],
                                    "{source:?} {caret} {text:?}"
                                );
                                let before =
                                    table_cell_spans(&source[start..line_end(source, line)]).len();
                                let after =
                                    table_cell_spans(&out[start..line_end(&out, line)]).len();
                                assert_eq!(before, after, "{source:?} {caret} {text:?} -> {out:?}");
                            }
                        }
                        for forward in [false, true] {
                            let (out, _) = deleted(source, caret, forward);
                            let shrunk = source.len() - out.len();
                            assert_eq!(&out[..span.start], &source[..span.start]);
                            assert_eq!(&out[span.end - shrunk..], &source[span.end..]);
                            let after = table_cell_spans(&out[start..line_end(&out, line)]).len();
                            let before =
                                table_cell_spans(&source[start..line_end(source, line)]).len();
                            assert_eq!(before, after, "{source:?} {caret} {forward} -> {out:?}");
                        }
                    }
                }
            }
        }
    }
}
