//! Line and character geometry over the source: rows, columns, line ends and
//! grapheme boundaries. A row is a logical line (between `\n`s).

use unicode_segmentation::UnicodeSegmentation;

/// Byte offset where each line starts (line 0 starts at 0; each line after a
/// `\n`). Always has at least one entry.
pub fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// The `(row, byte-column)` of a byte offset.
pub fn row_col(source: &str, offset: usize) -> (usize, usize) {
    // Counts newlines before `offset` without building the whole-document
    // `line_starts` table: layout asks for the caret row on every frame.
    let before = &source.as_bytes()[..offset.min(source.len())];
    let row = before.iter().filter(|&&b| b == b'\n').count();
    let start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    (row, offset - start)
}

/// Byte offset of the end of a row's text (before its `\n`, or the document
/// end for the last row).
pub fn line_end(source: &str, row: usize) -> usize {
    let starts = line_starts(source);
    starts.get(row + 1).map(|&s| s - 1).unwrap_or(source.len())
}

/// The `(start, end)` byte offsets of the line holding `offset`, its newline
/// excluded.
pub fn line_bounds(source: &str, offset: usize) -> (usize, usize) {
    let start = source[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |i| start + i);
    (start, end)
}

/// Whether logical line `row` holds only whitespace (or doesn't exist).
pub fn row_is_blank(source: &str, row: usize) -> bool {
    line_starts(source)
        .get(row)
        .is_none_or(|&s| source[s..line_end(source, row)].trim().is_empty())
}

/// The grapheme boundary before `offset` (0 at the start).
pub fn previous_boundary(source: &str, offset: usize) -> usize {
    source
        .grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

/// The grapheme boundary after `offset` (the end at the end).
pub fn next_boundary(source: &str, offset: usize) -> usize {
    source
        .grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(source.len())
}

/// How many `\\` end `text`: odd means the last one is unescaped.
pub fn trailing_backslashes(text: &str) -> usize {
    text.bytes().rev().take_while(|&b| b == b'\\').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_columns_and_line_ends() {
        let s = "ab\nцд\n";
        assert_eq!(line_starts(s), vec![0, 3, 8]);
        assert_eq!(row_col(s, 5), (1, 2));
        assert_eq!(line_end(s, 1), 7);
        assert_eq!(line_end(s, 2), 8);
        assert_eq!(line_bounds(s, 5), (3, 7));
        assert!(row_is_blank(s, 2) && row_is_blank(s, 9) && !row_is_blank(s, 0));
    }

    #[test]
    fn grapheme_boundaries_step_over_clusters() {
        let s = "a👍🏽b";
        assert_eq!(next_boundary(s, 1), 9);
        assert_eq!(previous_boundary(s, 9), 1);
        assert_eq!(trailing_backslashes("a\\\\"), 2);
    }
}
