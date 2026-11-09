//! The block structure the editing rules consult: fenced code, display math
//! and tables, by line.

use mdoc_markdown::tables::{TableRegion, table_regions};

use crate::markdown::{MathRegion, math_regions};
use crate::text::{line_end, line_starts};

/// Whether each line STARTS inside a fenced code block (an odd count of
/// ```` ``` ```` fences above it).
pub fn fence_parity(lines: &[&str]) -> Vec<bool> {
    let mut fence_odd = Vec::with_capacity(lines.len());
    let mut odd = false;
    for l in lines {
        fence_odd.push(odd);
        if l.trim_start().starts_with("```") {
            odd = !odd;
        }
    }
    fence_odd
}

/// A borrowed view of one source's block structure. The desktop editor lends
/// its per-revision scan; a stateless caller lends a [`BlockScan`].
#[derive(Clone, Copy)]
pub struct Blocks<'a> {
    /// Per line: whether it starts inside a fenced code block.
    pub fence_odd: &'a [bool],
    pub math: &'a [MathRegion],
    pub tables: &'a [TableRegion],
}

impl<'a> Blocks<'a> {
    /// Whether line `row` starts inside a fenced code block.
    pub fn in_fence(&self, row: usize) -> bool {
        self.fence_odd.get(row).copied().unwrap_or(false)
    }

    /// Whether logical line `row` is literal text, where Enter types a plain
    /// newline: a fenced code block (fences included) or display math.
    pub fn row_is_literal(&self, source: &str, row: usize) -> bool {
        let starts = line_starts(source);
        let Some(&start) = starts.get(row) else {
            return false;
        };
        self.in_fence(row)
            || source[start..line_end(source, row)]
                .trim_start()
                .starts_with("```")
            || self.math.iter().any(|m| m.range.contains(&row))
    }

    /// The table region holding line `row`.
    pub fn table_at(&self, row: usize) -> Option<&'a TableRegion> {
        self.tables.iter().find(|t| t.lines.contains(&row))
    }

    /// Whether line `row` is part of a table's grid.
    pub fn row_in_table(&self, row: usize) -> bool {
        self.table_at(row).is_some()
    }

    /// Whether line `row` is an editable table row (in a grid, not its
    /// `|---|` separator).
    pub fn row_in_table_cell(&self, row: usize) -> bool {
        self.table_at(row).is_some_and(|t| row != t.lines.start + 1)
    }

    /// The display-math region holding line `row`.
    pub fn math_at(&self, row: usize) -> Option<&'a MathRegion> {
        self.math.iter().find(|m| m.range.contains(&row))
    }

    /// The fenced code block containing `row`: its opening fence row plus the
    /// closing fence row (`None` when the block runs unclosed to the end).
    pub fn fence_block_rows(&self, source: &str, row: usize) -> (usize, Option<usize>) {
        let starts = line_starts(source);
        let last_row = starts.len().saturating_sub(1);
        let open = (0..=row).rev().find(|&r| !self.in_fence(r)).unwrap_or(0);
        let close = ((open + 1)..=last_row).find(|&r| {
            source[starts[r]..line_end(source, r)]
                .trim_start()
                .starts_with("```")
        });
        (open, close)
    }
}

/// An owned scan of one source's block structure, for callers without a
/// cache of their own.
#[derive(Clone, Default)]
pub struct BlockScan {
    pub fence_odd: Vec<bool>,
    pub math: Vec<MathRegion>,
    pub tables: Vec<TableRegion>,
}

impl BlockScan {
    pub fn new(source: &str) -> Self {
        let lines: Vec<&str> = source.split('\n').collect();
        BlockScan {
            fence_odd: fence_parity(&lines),
            math: math_regions(source),
            tables: table_regions(source),
        }
    }

    pub fn view(&self) -> Blocks<'_> {
        Blocks {
            fence_odd: &self.fence_odd,
            math: &self.math,
            tables: &self.tables,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_rows_are_code_and_math() {
        let s = "a\n```\ncode\n```\n$$\nx\n$$\n| h |\n| - |\n| c |";
        let scan = BlockScan::new(s);
        let b = scan.view();
        let literal: Vec<bool> = (0..11).map(|r| b.row_is_literal(s, r)).collect();
        assert_eq!(
            literal,
            [
                false, true, true, true, true, true, true, false, false, false, false
            ]
        );
        assert!(b.row_in_table_cell(7) && !b.row_in_table_cell(8) && b.row_in_table(8));
        assert_eq!(b.fence_block_rows(s, 2), (1, Some(3)));
    }
}
