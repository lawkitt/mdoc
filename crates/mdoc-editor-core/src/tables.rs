//! Table commands (Word-style): insert, delete and duplicate rows; insert and
//! delete columns; column alignment; the table's style marker; deleting the
//! table. Each returns its one edit — the replaced range, its text and the
//! caret after it — and changes nothing it does not need to: no padding or
//! realigning beyond the rows a command rewrites.

use std::ops::Range;

use mdoc_markdown::syntax::{TableStyle, table_marker_text, table_style_marker};
use mdoc_markdown::tables::{Align, TableRegion, table_cell_ranges, table_cells};

use crate::Blocks;
use crate::text::{line_end, line_starts, row_col};

/// One edit with the caret after it.
pub type TableEdit = (Range<usize>, String, usize);

/// A column edit applied to every row of a table (insert/delete a cell at index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColEdit {
    Insert(usize),
    Delete(usize),
}

/// The table block holding line `row` as `(header_line, separator_line,
/// end_exclusive, columns)`, or `None` outside a table.
pub fn table_block(blocks: Blocks, row: usize) -> Option<(usize, usize, usize, usize)> {
    let region = blocks.table_at(row)?;
    Some((
        region.lines.start,
        region.lines.start + 1,
        region.lines.end,
        region.aligns.len().max(1),
    ))
}

/// The table position of `offset` as `(row, cell_index, offset_within_cell)`,
/// or `None` outside a table. Lets structural edits keep the caret put.
pub fn cell_pos(source: &str, blocks: Blocks, offset: usize) -> Option<(usize, usize, usize)> {
    // Prepaint asks on every frame; stay off the whole-document line table.
    let offset = offset.min(source.len());
    let (row, line_col) = row_col(source, offset);
    table_block(blocks, row)?;
    let row_start = offset - line_col;
    let row_end = source[row_start..]
        .find('\n')
        .map_or(source.len(), |i| row_start + i);
    let line = &source[row_start..row_end];
    let ranges = table_cell_ranges(line);
    let cell = ranges
        .iter()
        .position(|r| line_col <= r.end)
        .unwrap_or(ranges.len().saturating_sub(1));
    let in_cell = ranges
        .get(cell)
        .map_or(0, |r| line_col.saturating_sub(r.start).min(r.len()));
    Some((row, cell, in_cell))
}

/// Byte offset of `(row, cell, offset_within_cell)` in `source`, clamping
/// the cell + offset to what that row actually has.
pub fn caret_pos_for_cell(source: &str, row: usize, cell: usize, in_cell: usize) -> usize {
    let starts = line_starts(source);
    let Some(&row_start) = starts.get(row) else {
        return source.len();
    };
    let line = &source[row_start..line_end(source, row)];
    let ranges = table_cell_ranges(line);
    if ranges.is_empty() {
        return row_start;
    }
    let r = &ranges[cell.min(ranges.len() - 1)];
    // Keep the caret strictly inside the cell, before its closing pipe — an
    // empty cell's trimmed range collapses onto that pipe (the line end for the
    // last cell), which would drop the caret out of the rendered table.
    let bytes = line.as_bytes();
    let close = (r.end..bytes.len())
        .find(|&i| bytes[i] == b'|')
        .unwrap_or(bytes.len());
    let mut col = (r.start + in_cell).min(close.saturating_sub(1));
    // An offset carried over from another cell can fall inside a character.
    while !line.is_char_boundary(col) {
        col -= 1;
    }
    row_start + col
}

fn applied(source: &str, range: &Range<usize>, text: &str) -> String {
    source[..range.start].to_owned() + text + &source[range.end..]
}

/// Insert an empty row above/below the caret's row; the caret moves into the
/// new row's first cell. `None` outside a table.
pub fn insert_row(source: &str, blocks: Blocks, cursor: usize, below: bool) -> Option<TableEdit> {
    let (row, _) = row_col(source, cursor);
    let (header, sep, _end, cols) = table_block(blocks, row)?;
    // From the header a new row always lands below the separator (the first
    // body row); above/below a body row is literal.
    let after = if row == header {
        sep
    } else if below {
        row
    } else {
        (row - 1).max(sep)
    };
    let new_row = format!("\n|{}", "  |".repeat(cols));
    let pos = line_end(source, after);
    Some((pos..pos, new_row, pos + 3)) // first cell, after "\n| "
}

/// Delete the caret's table row (body rows only — the header + separator stay).
/// The caret keeps its cell + in-cell offset, landing on the row that takes the
/// deleted row's place. `None` outside a table body row.
pub fn delete_row(source: &str, blocks: Blocks, cursor: usize) -> Option<TableEdit> {
    let (row, cell, in_cell) = cell_pos(source, blocks, cursor)?;
    let (header, sep, end, _cols) = table_block(blocks, row)?;
    if row == header || row == sep {
        return None;
    }
    let start = line_starts(source)[row];
    let line_end = line_end(source, row);
    // Remove the line + its trailing newline; for the last line, eat the
    // preceding newline instead so no blank line is left behind.
    let range = if line_end < source.len() {
        start..line_end + 1
    } else {
        start.saturating_sub(1)..line_end
    };
    // Stay at the same cell/offset, on the row now at this position (shifted
    // up), or the header if no body rows remain.
    let target = if end <= sep + 2 {
        header
    } else {
        row.min(end - 2)
    };
    let caret = caret_pos_for_cell(&applied(source, &range, ""), target, cell, in_cell);
    Some((range, String::new(), caret))
}

/// Delete the whole table the caret is in — its grid lines plus an optional
/// `<!-- table:STYLE -->` marker line directly above — joining the surrounding
/// text. The caret lands where the table was.
pub fn delete_table(source: &str, blocks: Blocks, cursor: usize) -> Option<TableEdit> {
    let (row, _) = row_col(source, cursor);
    let (header, _sep, end, _cols) = table_block(blocks, row)?;
    let starts = line_starts(source);
    let mut first = header;
    if first > 0 && table_style_marker(&source[starts[first - 1]..starts[first] - 1]).is_some() {
        first -= 1;
    }
    let line_end_last = line_end(source, end - 1);
    // Remove the table's lines + the trailing newline; at the document end, eat
    // the preceding newline instead so no blank line is left behind.
    let range = if line_end_last < source.len() {
        starts[first]..line_end_last + 1
    } else {
        starts[first].saturating_sub(1)..line_end_last
    };
    let caret = range.start.min(source.len() - range.len());
    Some((range, String::new(), caret))
}

/// Duplicate the caret's row below itself (the header duplicates as the
/// first body row). The caret lands in the copy, same cell + offset.
pub fn duplicate_row(source: &str, blocks: Blocks, cursor: usize) -> Option<TableEdit> {
    let (row, cell, in_cell) = cell_pos(source, blocks, cursor)?;
    let (_header, sep, _end, _cols) = table_block(blocks, row)?;
    if row == sep {
        return None;
    }
    let starts = line_starts(source);
    let line = &source[starts[row]..line_end(source, row)];
    // A duplicated header lands below the separator (as a body row).
    let after = if row + 1 == sep { sep } else { row };
    let insert = format!("\n{line}");
    let pos = line_end(source, after);
    let range = pos..pos;
    let caret = caret_pos_for_cell(&applied(source, &range, &insert), after + 1, cell, in_cell);
    Some((range, insert, caret))
}

/// Set the alignment of column `cell` of the table holding the caret by
/// rewriting that table's `|---|` separator row; the caret stays put. `None`
/// outside a table or past its columns.
pub fn set_align(
    source: &str,
    blocks: Blocks,
    cursor: usize,
    cell: usize,
    align: Align,
) -> Option<TableEdit> {
    let (row, _) = row_col(source, cursor);
    let region = blocks.table_at(row)?;
    let mut aligns = region.aligns.clone();
    if cell >= aligns.len() {
        return None;
    }
    aligns[cell] = align;
    let sep_row = region.lines.start + 1;
    let mut new_sep = String::from("|");
    for a in &aligns {
        new_sep.push_str(match a {
            Align::Left => " :-- |",
            Align::Center => " :-: |",
            Align::Right => " --: |",
        });
    }
    let starts = line_starts(source);
    let sep_start = starts[sep_row];
    let sep_end = starts.get(sep_row + 1).map_or(source.len(), |&s| s - 1);
    let delta = new_sep.len() as isize - (sep_end - sep_start) as isize;
    let caret = if cursor >= sep_end {
        (cursor as isize + delta).max(0) as usize
    } else {
        cursor
    };
    Some((sep_start..sep_end, new_sep, caret))
}

/// The marker text that gives `region` the style named `name` (`None` =
/// Grid, the default), keeping its drag-resized `cols=` widths.
pub fn style_marker(region: &TableRegion, name: Option<&str>) -> Option<String> {
    let style = TableStyle::from_name(name.unwrap_or("grid")).unwrap_or_default();
    table_marker_text(style, region.col_widths_attr.as_deref())
}

/// Rewrite (or insert/remove) the marker line above `region` to say
/// `marker` (`None` = no marker needed). `cell_pos` is the caret's table
/// position, which it keeps; without one the caret (`old_caret`) shifts by
/// the edit. `None` when neither a marker nor a new one exists.
pub fn rewrite_marker(
    source: &str,
    region: &TableRegion,
    marker: Option<String>,
    cell_pos: Option<(usize, usize, usize)>,
    old_caret: usize,
) -> Option<TableEdit> {
    let starts = line_starts(source);
    let (range, new) = match (region.marker_line, marker) {
        // Marker present → replace its line, or remove it (with the newline).
        (Some(m), Some(text)) => (starts[m]..line_end(source, m), text),
        (Some(m), None) => (starts[m]..line_end(source, m) + 1, String::new()),
        // No marker → insert one above the header.
        (None, Some(text)) => (
            starts[region.lines.start]..starts[region.lines.start],
            format!("{text}\n"),
        ),
        (None, None) => return None,
    };
    // The marker edit adds/removes one line ABOVE the table (or replaces in
    // place) — shift the caret's row index to keep it in its cell.
    let row_shift: isize = match (region.marker_line, new.is_empty()) {
        (Some(_), true) => -1,
        (None, false) => 1,
        _ => 0,
    };
    let edited = applied(source, &range, &new);
    let caret = match cell_pos {
        Some((row, cell, in_cell)) => {
            let same_row = (row as isize + row_shift).max(0) as usize;
            caret_pos_for_cell(&edited, same_row, cell, in_cell)
        }
        // Caret wasn't in this table (e.g. a border drag from elsewhere):
        // keep it where it was, shifted by the edit's byte delta — seating
        // it "in a cell" would land it on the marker line and reveal it.
        None => {
            let delta = new.len() as isize - (range.end - range.start) as isize;
            if old_caret >= range.end {
                (old_caret as isize + delta).max(0) as usize
            } else if old_caret > range.start {
                range.start + new.len()
            } else {
                old_caret
            }
        }
    };
    let caret = caret.min(edited.len());
    Some((range, new, caret))
}

/// Rewrite every row of the table holding line `row` to insert/delete a
/// cell, normalizing cell spacing. `None` (no edit) outside a table or when
/// a delete would remove the last column.
pub fn rewrite_table_columns(
    source: &str,
    blocks: Blocks,
    row: usize,
    edit: ColEdit,
) -> Option<(Range<usize>, String)> {
    let (header, sep, end, _cols) = table_block(blocks, row)?;
    let lines: Vec<&str> = source.split('\n').collect();
    let mut new_rows: Vec<String> = Vec::with_capacity(end - header);
    for (i, &line) in lines[header..end].iter().enumerate() {
        let is_sep = header + i == sep;
        let mut cells: Vec<String> = table_cells(line).iter().map(|s| (*s).to_string()).collect();
        match edit {
            ColEdit::Insert(at) => cells.insert(
                at.min(cells.len()),
                if is_sep { "---".into() } else { String::new() },
            ),
            ColEdit::Delete(c) => {
                if cells.len() <= 1 || c >= cells.len() {
                    return None; // never delete the last column
                }
                cells.remove(c);
            }
        }
        new_rows.push(format!("| {} |", cells.join(" | ")));
    }
    let starts = line_starts(source);
    let block_start = starts[header];
    let block_end = line_end(source, end - 1);
    Some((block_start..block_end, new_rows.join("\n")))
}

/// Insert an empty column left/right of the caret's column (a cell added to
/// every row; the separator gets a default-left marker). The caret stays in
/// its cell. `None` outside a table.
pub fn insert_column(
    source: &str,
    blocks: Blocks,
    cursor: usize,
    right: bool,
) -> Option<TableEdit> {
    let (row, cell, in_cell) = cell_pos(source, blocks, cursor)?;
    let at = if right { cell + 1 } else { cell };
    let (range, text) = rewrite_table_columns(source, blocks, row, ColEdit::Insert(at))?;
    // Inserting to the left shifts the caret's cell one column right.
    let new_cell = if right { cell } else { cell + 1 };
    let caret = caret_pos_for_cell(&applied(source, &range, &text), row, new_cell, in_cell);
    Some((range, text, caret))
}

/// Delete the caret's column from every row; the caret stays near where the
/// column was. `None` outside a table, or on the last remaining column.
pub fn delete_column(source: &str, blocks: Blocks, cursor: usize) -> Option<TableEdit> {
    let (row, cell, in_cell) = cell_pos(source, blocks, cursor)?;
    let (range, text) = rewrite_table_columns(source, blocks, row, ColEdit::Delete(cell))?;
    let caret = caret_pos_for_cell(&applied(source, &range, &text), row, cell, in_cell);
    Some((range, text, caret))
}

/// The source of `region` with any `<!-- table:STYLE -->` marker above it
/// (what Copy table copies).
pub fn table_source_range(source: &str, region: &TableRegion) -> Option<Range<usize>> {
    let starts = line_starts(source);
    let first = region.marker_line.unwrap_or(region.lines.start);
    let &start = starts.get(first)?;
    Some(start..line_end(source, region.lines.end - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockScan;

    const T: &str = "| a | b |\n| --- | --- |\n| 1 | 2 |\nafter";

    fn run(source: &str, f: impl Fn(&str, Blocks) -> Option<TableEdit>) -> Option<(String, usize)> {
        let scan = BlockScan::new(source);
        let (range, text, caret) = f(source, scan.view())?;
        Some((applied(source, &range, &text), caret))
    }

    #[test]
    fn rows_insert_delete_and_duplicate() {
        let (out, caret) = run(T, |s, b| insert_row(s, b, 2, true)).unwrap();
        assert_eq!(out, "| a | b |\n| --- | --- |\n|  |  |\n| 1 | 2 |\nafter");
        assert_eq!(caret, 26);
        let body = T.find("2 |").unwrap();
        let (out, caret) = run(T, |s, b| delete_row(s, b, body)).unwrap();
        assert_eq!(out, "| a | b |\n| --- | --- |\nafter");
        assert_eq!(caret, caret_pos_for_cell(&out, 0, 1, 0));
        assert_eq!(run(T, |s, b| delete_row(s, b, 2)), None);
        let (out, _) = run(T, |s, b| duplicate_row(s, b, body)).unwrap();
        assert_eq!(out, "| a | b |\n| --- | --- |\n| 1 | 2 |\n| 1 | 2 |\nafter");
    }

    #[test]
    fn a_carried_cell_offset_lands_on_a_character() {
        let s = "| ab |\n| --- |\n| я |";
        let caret = caret_pos_for_cell(s, 2, 0, 1);
        assert!(s.is_char_boundary(caret));
        assert_eq!(&s[caret..], "я |");
    }

    #[test]
    fn columns_insert_and_delete_keep_the_caret_cell() {
        let (out, caret) = run(T, |s, b| insert_column(s, b, 2, false)).unwrap();
        assert_eq!(
            out,
            "|  | a | b |\n| --- | --- | --- |\n|  | 1 | 2 |\nafter"
        );
        assert_eq!(&out[caret..caret + 1], "a");
        let (out, _) = run(T, |s, b| delete_column(s, b, 2)).unwrap();
        assert_eq!(out, "| b |\n| --- |\n| 2 |\nafter");
        let one = "| a |\n| - |";
        assert_eq!(run(one, |s, b| delete_column(s, b, 2)), None);
    }

    #[test]
    fn align_style_and_delete_table() {
        let (out, _) = run(T, |s, b| set_align(s, b, 2, 1, Align::Right)).unwrap();
        assert_eq!(out, "| a | b |\n| :-- | --: |\n| 1 | 2 |\nafter");
        let scan = BlockScan::new(T);
        let region = &scan.tables[0];
        let marker = style_marker(region, Some("striped"));
        let (range, text, _) =
            rewrite_marker(T, region, marker.clone(), cell_pos(T, scan.view(), 2), 2).unwrap();
        assert_eq!(
            applied(T, &range, &text),
            format!("{}\n{T}", marker.unwrap())
        );
        let (out, caret) = run(T, |s, b| delete_table(s, b, 2)).unwrap();
        assert_eq!((out.as_str(), caret), ("after", 0));
        assert_eq!(table_source_range(T, region), Some(0..33));
    }
}
