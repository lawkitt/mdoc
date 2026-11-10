//! Stateless editing commands for hosts without an editor model of their own
//! (the phones, through `mdoc-ffi`): each takes the source and a UTF-8
//! selection and returns what the key or button does, deciding as the
//! desktop editor's handlers do with the same rules.
//!
//! `rendered` says whether the view hides Markdown markers (live preview);
//! with Show Markdown on it is `false`, as desktop's raw mode: no hidden
//! markers to skip, and tables are plain lines. Applying the edits and
//! recording them for Undo stay with the caller. The caret is the
//! selection's `end`.

use std::ops::Range;

use mdoc_markdown::lines::blockquote_prefix;
use mdoc_markdown::tables::Align;

use crate::cells::{self, CellMove};
use crate::markdown::{image_row, toggle_task_checkbox};
use crate::tables::{self as table_rules, TableEdit};
use crate::text::{line_end, line_starts, next_boundary, previous_boundary, row_col};
use crate::turn_into::TurnKind;
use crate::{BlockScan, Blocks, EditResult, TAB_INDENT, delete, enter, format};

/// An inline format a toggle wraps around the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineFormat {
    Bold,
    Italic,
    Strikethrough,
    Code,
    Highlight,
    Underline,
}

impl InlineFormat {
    /// The opening and closing markers.
    pub fn markers(self) -> (&'static str, &'static str) {
        match self {
            InlineFormat::Bold => ("**", "**"),
            InlineFormat::Italic => ("*", "*"),
            InlineFormat::Strikethrough => ("~~", "~~"),
            InlineFormat::Code => ("`", "`"),
            InlineFormat::Highlight => ("==", "=="),
            // Markdown has no underline — the `<u>` tag, which both views honor.
            InlineFormat::Underline => ("<u>", "</u>"),
        }
    }
}

/// A table command on the table holding the caret.
#[derive(Clone, Debug, PartialEq)]
pub enum TableCommand {
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    DuplicateRow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteColumn,
    /// The caret's column's alignment.
    Align(Align),
    /// The table's style by name (`grid`, `striped`, `header`, `minimal`;
    /// `None` is grid, the default).
    Style(Option<String>),
    DeleteTable,
}

fn sorted(selection: &Range<usize>) -> Range<usize> {
    selection.start.min(selection.end)..selection.start.max(selection.end)
}

/// Replace the selection with `text`, the caret after it.
fn type_over(selection: &Range<usize>, text: &str) -> EditResult {
    let sel = sorted(selection);
    let caret = sel.start + text.len();
    EditResult::one(sel, text, caret)
}

fn from_table((range, text, caret): TableEdit) -> EditResult {
    EditResult::one(range, text, caret)
}

/// Whether line `row` is part of a table the view draws: table-shaped text
/// in a code block or display math is literal text, not a table.
fn row_in_drawn_table(source: &str, blocks: Blocks, row: usize) -> bool {
    blocks.row_in_table(row) && !blocks.row_is_literal(source, row)
}

/// Whether the caret's row is an editable table row in a rendered view, where
/// desktop's table rules apply.
fn in_table_cell(source: &str, blocks: Blocks, caret: usize, rendered: bool) -> bool {
    let row = row_col(source, caret).0;
    rendered && blocks.row_in_table_cell(row) && row_in_drawn_table(source, blocks, row)
}

/// Enter: a new block (ADR 0060). In a rendered table it moves to the cell
/// below, or leaves the table from its last row; in code and math it types a
/// newline; a selection becomes a paragraph break.
pub fn enter(source: &str, selection: Range<usize>, rendered: bool) -> EditResult {
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    let caret = selection.end.min(source.len());
    if in_table_cell(source, blocks, caret, rendered)
        && let Some(cell) = cells::cell_at(source, caret)
    {
        // The cell below, keeping the column; from the last row out below.
        if cell.row + 1 < cell.rows
            && let Some(moved) = cells::move_caret(source, caret..caret, CellMove::Down)
        {
            return moved;
        }
        let region = blocks
            .table_at(row_col(source, caret).0)
            .expect("a table row");
        let (range, text, offset) = enter::table_exit_edit(source, region.lines.end - 1);
        let to = range.start + offset;
        return EditResult::one(range, text, to);
    }
    let (row, _) = row_col(source, caret);
    if selection.start == selection.end
        && let Some(closed) = close_fence(source, blocks, row, caret)
    {
        return closed;
    }
    if blocks.row_is_literal(source, row) {
        return type_over(&selection, "\n");
    }
    if selection.start != selection.end {
        return type_over(&selection, "\n\n");
    }
    let (range, text, offset) = enter::enter_edit(source, caret);
    let to = range.start + offset;
    EditResult::one(range, text, to)
}

/// The phones' typing helper (ADR 0109): Return at the end of an opening
/// code fence nobody closed yet (```` ``` ````, a language after it or not)
/// closes it, the caret on the empty line between. Desktop's Enter types a
/// newline there.
fn close_fence(source: &str, blocks: Blocks, row: usize, caret: usize) -> Option<EditResult> {
    if blocks.in_fence(row) {
        return None;
    }
    let (start, end) = (line_starts(source)[row], line_end(source, row));
    let line = &source[start..end];
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let ticks = body.bytes().take_while(|&b| b == b'`').count();
    if ticks < 3
        || body[ticks..].contains('`')
        || caret < start + indent.len() + ticks
        || !source[caret..end].trim().is_empty()
        || blocks.fence_block_rows(source, row).1.is_some()
    {
        return None;
    }
    let text = format!("\n{indent}\n{indent}{}", &body[..ticks]);
    Some(EditResult::one(end..end, text, end + 1 + indent.len()))
}

/// ⇧Enter: a line break inside the block — a hard break in prose, lists
/// and quotes, `<br>` in a rendered table cell, a newline in code and math;
/// Enter where a break can't go (headings, no text before the caret).
pub fn shift_enter(source: &str, selection: Range<usize>, rendered: bool) -> EditResult {
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    let caret = selection.end.min(source.len());
    if in_table_cell(source, blocks, caret, rendered) {
        return cells::type_text(source, selection.clone(), "\n", false)
            .unwrap_or_else(|| type_over(&selection, "<br>"));
    }
    if blocks.row_is_literal(source, row_col(source, caret).0) {
        return type_over(&selection, "\n");
    }
    match enter::line_break(source, sorted(&selection).start) {
        Some(text) => type_over(&selection, &text),
        None => enter(source, selection, rendered),
    }
}

/// A rendered image row's whole line and its newline (what one ⌫ or ⌦
/// removes Word-style). An image's text in code or math is literal.
fn image_row_range(source: &str, blocks: Blocks, row: usize) -> Option<Range<usize>> {
    if blocks.row_is_literal(source, row) {
        return None;
    }
    let start = *line_starts(source).get(row)?;
    let end = line_end(source, row);
    image_row(&source[start..end])?;
    Some(start..(end + 1).min(source.len()))
}

fn delete_range(range: Range<usize>, caret: usize) -> EditResult {
    EditResult::one(range, "", caret)
}

/// ⌫: the selection, or before the caret — a whole rendered image row, the
/// paragraph break at a paragraph's start (joining the two), or one
/// character. `None` at the document start.
///
/// Desktop also skips hidden formatting markers here
/// ([`delete::fmt_delete_range`]); under the phone's reveal rule
/// ([`crate::reveal::RevealRule::CaretSpan`]) the construct the caret
/// touches shows its markers, so the character before the caret is always
/// visible and is the one deleted. In live preview, deleting the last
/// character of a formatting construct (`**a**`) deletes its markers too,
/// as desktop does.
pub fn backspace(source: &str, selection: Range<usize>, rendered: bool) -> Option<EditResult> {
    if selection.start != selection.end
        && !(rendered && cells::cell_at(source, selection.end).is_some())
    {
        let sel = sorted(&selection);
        let caret = sel.start;
        return Some(delete_range(sel, caret));
    }
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    // In a drawn table's cell: only that cell's bytes (ADR 0109).
    if rendered && let Some(result) = cells::delete(source, blocks, selection.clone(), false) {
        return Some(result);
    }
    let off = selection.end.min(source.len());
    let (row, col) = row_col(source, off);
    if rendered
        && let Some(range) = image_row_range(source, blocks, row).or_else(|| {
            (col == 0 && row > 0)
                .then(|| image_row_range(source, blocks, row - 1))
                .flatten()
        })
    {
        let caret = range.start;
        return Some(delete_range(range, caret));
    }
    // At a paragraph's start, one ⌫ undoes Enter's paragraph break (ADR 0060).
    let body = blockquote_prefix(&source[off - col..line_end(source, row)]);
    if (col == 0 || Some(col) == body)
        && let Some((range, caret)) = delete::break_delete_range(source, blocks, row, false)
    {
        return Some(delete_range(range, caret));
    }
    let prev = previous_boundary(source, off);
    (prev != off).then(|| {
        let range = rendered
            .then(|| emptied_pair(source, blocks, prev..off))
            .flatten()
            .unwrap_or(prev..off);
        let caret = range.start;
        delete_range(range, caret)
    })
}

/// Deleting `deleted`, one character, empties a formatting construct (its
/// opening marker ends where the character starts and its closing marker
/// starts where it ends): the construct's whole range, so no empty `****`
/// is left behind, as desktop deletes it. `None` otherwise, and in code and
/// math, where markers are literal text.
fn emptied_pair(source: &str, blocks: Blocks, deleted: Range<usize>) -> Option<Range<usize>> {
    let (row, _) = row_col(source, deleted.start);
    if blocks.row_is_literal(source, row) {
        return None;
    }
    let start = line_starts(source)[row];
    let end = line_end(source, row);
    if deleted.end > end {
        return None;
    }
    crate::markdown::fmt_marker_pairs(&source[start..end])
        .into_iter()
        .find(|(open, close)| {
            open.end + start == deleted.start && close.start + start == deleted.end
        })
        .map(|(open, close)| open.start + start..close.end + start)
}

/// ⌦: as [`backspace`], forward — a rendered image row, the paragraph break
/// at a paragraph's end, or one character. `None` at the document end.
pub fn delete_forward(source: &str, selection: Range<usize>, rendered: bool) -> Option<EditResult> {
    if selection.start != selection.end
        && !(rendered && cells::cell_at(source, selection.end).is_some())
    {
        let sel = sorted(&selection);
        let caret = sel.start;
        return Some(delete_range(sel, caret));
    }
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    if rendered && let Some(result) = cells::delete(source, blocks, selection.clone(), true) {
        return Some(result);
    }
    let off = selection.end.min(source.len());
    let (row, _) = row_col(source, off);
    if rendered
        && let Some(range) = image_row_range(source, blocks, row).or_else(|| {
            (off == line_end(source, row))
                .then(|| image_row_range(source, blocks, row + 1))
                .flatten()
        })
    {
        let caret = range.start;
        return Some(delete_range(range, caret));
    }
    if off == line_end(source, row)
        && let Some((range, caret)) = delete::break_delete_range(source, blocks, row, true)
    {
        return Some(delete_range(range, caret));
    }
    let next = next_boundary(source, off);
    (next != off).then(|| {
        let range = rendered
            .then(|| emptied_pair(source, blocks, off..next))
            .flatten()
            .unwrap_or(off..next);
        let caret = range.start;
        delete_range(range, caret)
    })
}

/// Toggle `format` the phone's way ([`crate::toolbar::toggle_format`]): a
/// selection as desktop's toggle does, and a caret too — inside the format
/// its markers come off, in a word the word wraps, elsewhere an empty pair
/// goes in. `None` on a blank selection.
pub fn toggle_format(
    source: &str,
    selection: Range<usize>,
    format: InlineFormat,
) -> Option<EditResult> {
    crate::toolbar::toggle_format(source, selection, format)
}

/// Tab: indent the caret's list or quote item one level, else type spaces
/// over the selection. `None` in a rendered table, where Tab moves between
/// cells (the host's navigation).
pub fn indent(source: &str, selection: Range<usize>, rendered: bool) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let caret = selection.end.min(source.len());
    if in_table_cell(source, scan.view(), caret, rendered) {
        return None;
    }
    Some(match format::indent(source, caret, TAB_INDENT) {
        format::IndentEdit::Spaces(spaces) => type_over(&selection, &spaces),
        format::IndentEdit::Item { range, text, caret } => EditResult::one(range, text, caret),
    })
}

/// ⇧Tab: outdent the caret's line one level. `None` without an indent, or
/// in a rendered table.
pub fn outdent(source: &str, selection: Range<usize>, rendered: bool) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let caret = selection.end.min(source.len());
    if in_table_cell(source, scan.view(), caret, rendered) {
        return None;
    }
    let (range, caret) = format::outdent(source, caret, TAB_INDENT)?;
    Some(delete_range(range, caret))
}

/// Turn the block holding the selection's start into `kind`. `None` when it
/// already is one.
pub fn turn_into(source: &str, selection: Range<usize>, kind: TurnKind) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let start = sorted(&selection).start.min(source.len());
    let row = row_col(source, start).0;
    let edit = crate::turn_into::turn_into(source, scan.view(), row, kind)?;
    let caret = edit.caret.unwrap_or(edit.range.start + edit.text.len());
    Some(EditResult::one(edit.range, edit.text, caret))
}

/// The "Turn into" kind of the block holding the caret.
pub fn block_kind(source: &str, selection: Range<usize>) -> TurnKind {
    let scan = BlockScan::new(source);
    let caret = sorted(&selection).start.min(source.len());
    crate::turn_into::block_kind_at(source, scan.view(), row_col(source, caret).0)
}

/// Check or uncheck the caret line's task box (one byte swapped, so the
/// selection stays). `None` when the line is not a task.
pub fn toggle_task(source: &str, selection: Range<usize>) -> Option<EditResult> {
    let caret = selection.end.min(source.len());
    let row = row_col(source, caret).0;
    let range = line_starts(source)[row]..line_end(source, row);
    let line = toggle_task_checkbox(&source[range.clone()])?;
    Some(EditResult {
        edits: vec![crate::Edit { range, text: line }],
        selection,
    })
}

/// Run `command` on the table holding the caret. `None` outside a table or
/// where the command does not apply (deleting the header row or the last
/// column, aligning from the delimiter row).
pub fn table_command(
    source: &str,
    selection: Range<usize>,
    command: &TableCommand,
) -> Option<EditResult> {
    let scan = BlockScan::new(source);
    let blocks = scan.view();
    let caret = selection.end.min(source.len());
    if !row_in_drawn_table(source, blocks, row_col(source, caret).0) {
        return None;
    }
    let edit = match command {
        TableCommand::InsertRowAbove => table_rules::insert_row(source, blocks, caret, false),
        TableCommand::InsertRowBelow => table_rules::insert_row(source, blocks, caret, true),
        TableCommand::DeleteRow => table_rules::delete_row(source, blocks, caret),
        TableCommand::DuplicateRow => table_rules::duplicate_row(source, blocks, caret),
        TableCommand::InsertColumnLeft => column_edit(source, blocks, caret, Some(false)),
        TableCommand::InsertColumnRight => column_edit(source, blocks, caret, Some(true)),
        TableCommand::DeleteColumn => column_edit(source, blocks, caret, None),
        TableCommand::Align(align) => {
            let (row, _, _) = table_rules::cell_pos(source, blocks, caret)?;
            if !blocks.row_in_table_cell(row) {
                return None;
            }
            let column = cells::cell_at(source, caret)?.column;
            table_rules::set_align(source, blocks, caret, column, *align)
        }
        TableCommand::Style(name) => {
            let region = blocks.table_at(row_col(source, caret).0)?;
            let marker = table_rules::style_marker(region, name.as_deref());
            let cell = table_rules::cell_pos(source, blocks, caret);
            table_rules::rewrite_marker(source, region, marker, cell, sorted(&selection).start)
        }
        TableCommand::DeleteTable => table_rules::delete_table(source, blocks, caret),
    }?;
    Some(from_table(edit))
}

/// A column inserted right (`Some(true)`) or left of the caret's, or the
/// caret's deleted (`None`), as desktop's column commands rewrite the
/// table. The caret's column is the cells' reading
/// ([`cells::cell_at`]), which counts a last cell left without its closing
/// pipe; the caret stays in its cell.
fn column_edit(
    source: &str,
    blocks: Blocks,
    caret: usize,
    insert_right: Option<bool>,
) -> Option<TableEdit> {
    let cell = cells::cell_at(source, caret)?;
    let row = row_col(source, caret).0;
    let into = caret.saturating_sub(cell.content.start);
    let (edit, column) = match insert_right {
        Some(true) => (table_rules::ColEdit::Insert(cell.column + 1), cell.column),
        Some(false) => (table_rules::ColEdit::Insert(cell.column), cell.column + 1),
        None => (table_rules::ColEdit::Delete(cell.column), cell.column),
    };
    let (range, text) = table_rules::rewrite_table_columns(source, blocks, row, edit)?;
    let edited = source[..range.start].to_owned() + &text + &source[range.end..];
    let caret = table_rules::caret_pos_for_cell(&edited, row, column, into);
    Some((range, text, caret))
}

/// Tab, ⇧Tab or an arrow key in a rendered table (ADR 0109): the caret to
/// the next or previous cell, or a character or a cell over. `None` outside
/// a rendered table, where the key does what it does elsewhere.
pub fn table_move(
    source: &str,
    selection: Range<usize>,
    by: CellMove,
    rendered: bool,
) -> Option<EditResult> {
    rendered
        .then(|| cells::move_caret(source, selection, by))
        .flatten()
}

/// Typing `typed` over the selection in a rendered table's cell, as that
/// cell's own bytes: a line break as `<br>`, a pipe escaped. `None` outside
/// a rendered table, where typing is plain.
pub fn type_in_table(
    source: &str,
    selection: Range<usize>,
    typed: &str,
    rendered: bool,
) -> Option<EditResult> {
    rendered
        .then(|| cells::type_text(source, selection, typed, false))
        .flatten()
}

/// Paste `text` over the selection, normalised as desktop does: line
/// endings, a rendered table cell's newlines and pipes, words-attached
/// `$$` fences in a rendered view.
pub fn paste(source: &str, selection: Range<usize>, text: String, rendered: bool) -> EditResult {
    if rendered && let Some(result) = cells::type_text(source, selection.clone(), &text, true) {
        return result;
    }
    let scan = BlockScan::new(source);
    let caret = selection.end.min(source.len());
    let target = crate::paste::PasteTarget {
        plain: false,
        table_cell: in_table_cell(source, scan.view(), caret, rendered),
        markdown: rendered,
    };
    type_over(&selection, &crate::paste::normalize_paste(text, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str, result: Option<EditResult>) -> Option<(String, Range<usize>)> {
        let r = result?;
        Some((r.apply(source), r.selection))
    }

    fn caret(c: usize) -> Range<usize> {
        c..c
    }

    #[test]
    fn enter_follows_desktop() {
        let s = "- a";
        assert_eq!(
            run(s, Some(enter(s, caret(3), true))),
            Some(("- a\n- ".into(), caret(6)))
        );
        let s = "```\nx\n```";
        assert_eq!(
            run(s, Some(enter(s, caret(5), true))).unwrap().0,
            "```\nx\n\n```"
        );
        // The closing fence takes a plain newline too.
        assert_eq!(
            run(s, Some(enter(s, caret(9), true))).unwrap().0,
            "```\nx\n```\n"
        );
        let s = "one two";
        assert_eq!(
            run(s, Some(enter(s, 3..4, true))),
            Some(("one\n\ntwo".into(), caret(5)))
        );
    }

    #[test]
    fn a_typed_opening_fence_closes_itself() {
        for rendered in [true, false] {
            // At the text's end, with a language or without one.
            for (s, out) in [
                ("```", "```\n\n```"),
                (
                    "```rust",
                    "```rust\n\n```rust"[..12].to_string().leak() as &str,
                ),
            ] {
                let r = enter(s, caret(s.len()), rendered);
                assert_eq!(r.apply(s), out, "{s:?}");
                assert_eq!(r.selection, caret(s.len() + 1));
            }
            // Indented, before more text, and with four backticks.
            let s = "  ````\nnext";
            let r = enter(s, caret(6), rendered);
            assert_eq!(r.apply(s), "  ````\n  \n  ````\nnext");
            assert_eq!(r.selection, caret(9));
            // A fence a later one closes, a closing fence, inline code and a
            // caret before the backticks get a plain newline.
            for (s, at) in [
                ("```\nx\n```", 3),
                ("```\nx\n```", 9),
                ("```a``` b", 9),
                ("```", 1),
            ] {
                let r = enter(s, caret(at), rendered);
                assert!(
                    !r.apply(s).ends_with("\n```")
                        || s.ends_with("```") && at != s.len()
                        || r.apply(s).matches("```").count() == s.matches("```").count(),
                    "{s:?} at {at}: {:?}",
                    r.apply(s)
                );
                assert_eq!(
                    r.apply(s).matches('`').count(),
                    s.matches('`').count(),
                    "{s:?} at {at}"
                );
            }
        }
    }

    #[test]
    fn enter_in_a_rendered_table_moves_down_then_leaves() {
        let s = "| a |\n| - |\n| b |";
        // From the header to the body row, skipping the delimiter.
        let r = enter(s, caret(2), true);
        assert!(r.edits.is_empty());
        assert_eq!(&s[r.selection.start..r.selection.start + 1], "b");
        // From the last row out of the table.
        assert_eq!(
            run(s, Some(enter(s, caret(14), true))).unwrap().0,
            format!("{s}\n\n")
        );
        // With Show Markdown on a table is plain lines.
        assert_eq!(
            run(s, Some(enter(s, caret(5), false))).unwrap().0,
            "| a |\n\n\n\n| - |\n| b |"
        );
    }

    #[test]
    fn shift_enter_breaks_inside_the_block() {
        let s = "- a";
        assert_eq!(
            run(s, Some(shift_enter(s, caret(3), true))).unwrap().0,
            "- a\\\n  "
        );
        let s = "# h";
        assert_eq!(
            run(s, Some(shift_enter(s, caret(3), true))).unwrap().0,
            "# h\n\n"
        );
        let s = "| a |\n| - |\n| b |";
        assert_eq!(
            run(s, Some(shift_enter(s, caret(15), true))).unwrap().0,
            "| a |\n| - |\n| b<br> |"
        );
    }

    #[test]
    fn backspace_and_delete_at_boundaries() {
        let s = "one\n\ntwo";
        assert_eq!(
            run(s, backspace(s, caret(5), true)),
            Some(("onetwo".into(), caret(3)))
        );
        assert_eq!(
            run(s, delete_forward(s, caret(3), false)),
            Some(("onetwo".into(), caret(3)))
        );
        let s = "a **b** c";
        // The caret touching the bold shows its markers (CaretSpan), so ⌫
        // deletes the visible `*` in both views.
        for rendered in [true, false] {
            assert_eq!(
                run(s, backspace(s, caret(7), rendered)),
                Some(("a **b* c".into(), caret(6)))
            );
        }
        assert_eq!(backspace(s, caret(0), true), None);
        assert_eq!(delete_forward(s, caret(9), true), None);
        let s = "x\n![](a.png)\ny";
        assert_eq!(
            run(s, backspace(s, caret(13), true)),
            Some(("x\ny".into(), caret(2)))
        );
        assert_eq!(
            run(s, backspace(s, 0..2, true)),
            Some(("![](a.png)\ny".into(), caret(0)))
        );
    }

    #[test]
    fn deletes_take_the_character_the_reveal_rule_shows() {
        use crate::reveal::{RevealRule, visible_markers};
        for (s, off) in [("**ab**", 6), ("**ab**", 0), ("x *y* z", 5), ("x *y* z", 2)] {
            let shown = visible_markers(s, off..off, RevealRule::CaretSpan);
            let back = backspace(s, caret(off), true).map(|r| r.edits[0].range.clone());
            let fwd = delete_forward(s, caret(off), true).map(|r| r.edits[0].range.clone());
            for deleted in [back, fwd].into_iter().flatten() {
                // A deleted marker byte is one the view shows.
                let marker = shown
                    .iter()
                    .any(|m| m.start <= deleted.start && deleted.end <= m.end);
                let hidden_marker = crate::markdown::fmt_marker_pairs(s)
                    .iter()
                    .flat_map(|(o, c)| [o.clone(), c.clone()])
                    .any(|m| m.start <= deleted.start && deleted.end <= m.end);
                assert!(
                    !hidden_marker || marker,
                    "{s:?} at {off} deletes hidden {deleted:?}"
                );
            }
        }
    }

    #[test]
    fn an_emptied_construct_goes_with_its_last_character() {
        assert_eq!(
            run("a **b** c", backspace("a **b** c", caret(5), true)),
            Some(("a  c".into(), caret(2)))
        );
        assert_eq!(
            run("x ~~y~~", delete_forward("x ~~y~~", caret(4), true)),
            Some(("x ".into(), caret(2)))
        );
        // Not emptied, Show Markdown, or literal code: one character.
        assert_eq!(
            run("**bc**", backspace("**bc**", caret(4), true))
                .unwrap()
                .0,
            "**b**"
        );
        assert_eq!(
            run("**b**", backspace("**b**", caret(3), false)).unwrap().0,
            "****"
        );
        let s = "```\n**b**\n```";
        assert_eq!(
            run(s, backspace(s, caret(7), true)).unwrap().0,
            "```\n****\n```"
        );
    }

    #[test]
    fn code_blocks_stay_literal() {
        // An image's text in a code block deletes one character.
        let s = "```\n![](a.png)\n```";
        assert_eq!(
            run(s, backspace(s, caret(14), true)).unwrap().0,
            "```\n![](a.png\n```"
        );
        assert_eq!(
            run(s, delete_forward(s, caret(3), true)).unwrap().0,
            "```![](a.png)\n```"
        );
        // Table-shaped code: Enter and ⇧Enter type newlines, paste keeps its
        // pipes, and table commands don't apply.
        let s = "```\n| a |\n| - |\n| b |\n```";
        assert_eq!(
            run(s, Some(enter(s, caret(7), true))).unwrap().0,
            "```\n| a\n |\n| - |\n| b |\n```"
        );
        assert_eq!(
            run(s, Some(shift_enter(s, caret(7), true))).unwrap().0,
            "```\n| a\n |\n| - |\n| b |\n```"
        );
        assert_eq!(
            run(s, Some(paste(s, caret(7), "x|y".into(), true)))
                .unwrap()
                .0,
            "```\n| ax|y |\n| - |\n| b |\n```"
        );
        assert_eq!(
            table_command(s, caret(7), &TableCommand::InsertRowBelow),
            None
        );
        assert_eq!(
            indent(s, caret(7), true).map(|r| r.apply(s)),
            Some("```\n| a     |\n| - |\n| b |\n```".into())
        );
    }

    #[test]
    fn table_enter_lands_on_a_character() {
        let s = "| ab |\n| --- |\n| я |";
        // One byte into `ab` would be inside `я`: it lands before it; after
        // `ab` it lands after `я`.
        for (from, rest) in [(3, "я |"), (4, " |")] {
            let r = enter(s, caret(from), true);
            assert!(r.edits.is_empty());
            assert_eq!(&s[r.selection.start..], rest);
        }
    }

    #[test]
    fn formats_indent_turn_into_and_tasks() {
        let s = "a bc d";
        assert_eq!(
            run(s, toggle_format(s, 2..4, InlineFormat::Highlight)),
            Some(("a ==bc== d".into(), 4..6))
        );
        // A caret between words: an empty pair to type into (the phone's rule).
        assert_eq!(
            run(s, toggle_format(s, 1..1, InlineFormat::Bold)),
            Some(("a**** bc d".into(), caret(3)))
        );
        let s = "- a";
        assert_eq!(
            run(s, indent(s, caret(3), true)),
            Some(("    - a".into(), caret(7)))
        );
        assert_eq!(
            run("    - a", outdent("    - a", caret(7), true)),
            Some(("- a".into(), caret(3)))
        );
        assert_eq!(outdent(s, caret(3), true), None);
        assert_eq!(
            run(s, turn_into(s, caret(3), TurnKind::H1)),
            Some(("# a".into(), caret(3)))
        );
        assert_eq!(block_kind(s, caret(1)), TurnKind::Bullet);
        let s = "- [ ] t";
        assert_eq!(
            run(s, toggle_task(s, caret(7))),
            Some(("- [x] t".into(), caret(7)))
        );
        assert_eq!(toggle_task("t", caret(1)), None);
    }

    /// A row left without its closing pipe still has its last cell: the
    /// column commands act on the column the caret is in.
    #[test]
    fn column_commands_count_a_last_cell_without_its_pipe() {
        let s = "| a | b |\n| - | - |\n| x | y";
        let y = s.rfind('y').unwrap();
        let out = table_command(s, caret(y), &TableCommand::DeleteColumn)
            .unwrap()
            .apply(s);
        assert_eq!(out, "| a |\n| - |\n| x |");
        let out = table_command(s, caret(y), &TableCommand::InsertColumnLeft)
            .unwrap()
            .apply(s);
        assert_eq!(out, "| a |  | b |\n| - | --- | - |\n| x |  | y |");
        let out = table_command(s, caret(y), &TableCommand::Align(Align::Right))
            .unwrap()
            .apply(s);
        assert!(out.contains("| :-- | --: |"), "{out}");
    }

    #[test]
    fn table_commands_and_paste() {
        let s = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let (out, _) = run(
            s,
            table_command(s, caret(2), &TableCommand::InsertColumnRight),
        )
        .unwrap();
        assert_eq!(out, "| a |  | b |\n| --- | --- | --- |\n| 1 |  | 2 |");
        let (out, _) = run(
            s,
            table_command(s, caret(2), &TableCommand::Align(Align::Center)),
        )
        .unwrap();
        assert_eq!(out, "| a | b |\n| :-: | :-- |\n| 1 | 2 |");
        let (out, _) = run(
            s,
            table_command(s, caret(2), &TableCommand::Style(Some("minimal".into()))),
        )
        .unwrap();
        assert!(out.starts_with("<!-- table:minimal -->\n| a |"), "{out}");
        assert_eq!(
            table_command("plain", caret(1), &TableCommand::DeleteRow),
            None
        );
        assert_eq!(table_command(s, caret(2), &TableCommand::DeleteRow), None);
        assert_eq!(
            run(s, Some(paste(s, caret(2), "x|y\n".into(), true)))
                .unwrap()
                .0,
            "| x\\|ya | b |\n| --- | --- |\n| 1 | 2 |"
        );
        assert_eq!(
            run("ab", Some(paste("ab", 0..1, "x\r\ny".into(), false))),
            Some(("x\nyb".into(), caret(3)))
        );
    }
}
