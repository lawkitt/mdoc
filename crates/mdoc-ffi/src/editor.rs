//! The editing rules the apps' edit mode calls (ADR 0109), from
//! `mdoc-editor-core`, the rules the desktop editor runs.
//!
//! Plain typing is applied natively; structural keys and buttons (Enter,
//! ⇧Enter, ⌫ and ⌦, formatting toggles, Tab, Turn into, task boxes, table
//! commands, paste) call these. Each call is stateless: it takes the text and
//! a selection and returns the edits and the selection after them. The app
//! applies them, and the document session records them for Undo.
//!
//! Every offset is a UTF-8 byte offset into the text, as in
//! [`crate::markdown`]. A selection's `end` is the caret; an offset past the
//! text or inside a character is pulled back to the character before it.
//! `rendered` is true in live preview (markers hidden) and false with Show
//! Markdown on, where nothing is hidden and tables are plain lines, as in
//! desktop's raw mode.

use std::ops::Range;

use mdoc_editor_core::cells::{self, CellMove};
use mdoc_editor_core::commands::{self, InlineFormat, TableCommand};
use mdoc_editor_core::reveal::{RevealRule, visible_markers};
use mdoc_editor_core::toolbar;
use mdoc_editor_core::turn_into::TurnKind;
use mdoc_editor_core::{Edit, EditResult};
use mdoc_markdown::tables::Align;

use crate::library::TextRange;
use crate::markdown::{MarkdownAlign, MarkdownTableStyle};

/// One replacement: the byte range it replaces and the new text. A change's
/// edits are sorted, don't overlap, and are in the offsets of the text
/// before any of them applies.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditorEdit {
    pub range: TextRange,
    pub text: String,
}

/// What a command does: its edits (none for a caret move) and the selection
/// after them, in the offsets of the edited text.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditorChange {
    pub edits: Vec<EditorEdit>,
    pub selection: TextRange,
}

/// An inline format the toolbar and ⌘B/⌘I toggle around the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EditorInlineFormat {
    Bold,
    Italic,
    Strikethrough,
    Code,
    Highlight,
    Underline,
}

/// A block kind "Turn into" converts between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EditorBlockKind {
    Text,
    Heading1,
    Heading2,
    Heading3,
    BulletList,
    NumberedList,
    Todo,
    Quote,
    Callout,
    CodeBlock,
    MathBlock,
}

/// A command on the table holding the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EditorTableCommand {
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    DuplicateRow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteColumn,
    /// The caret's column's alignment.
    Align {
        align: MarkdownAlign,
    },
    /// The table's look, written as its `<!-- table:STYLE -->` marker.
    Style {
        style: MarkdownTableStyle,
    },
    DeleteTable,
}

/// `offset` in `text`, pulled back onto a character boundary.
fn offset(text: &str, offset: u64) -> usize {
    let mut i = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn selection(text: &str, range: TextRange) -> Range<usize> {
    offset(text, range.start)..offset(text, range.end)
}

fn change(result: EditResult) -> EditorChange {
    EditorChange {
        edits: result
            .edits
            .iter()
            .map(|Edit { range, text }| EditorEdit {
                range: TextRange::of(range),
                text: text.clone(),
            })
            .collect(),
        selection: TextRange::of(&result.selection),
    }
}

impl From<EditorBlockKind> for TurnKind {
    fn from(kind: EditorBlockKind) -> Self {
        match kind {
            EditorBlockKind::Text => TurnKind::Text,
            EditorBlockKind::Heading1 => TurnKind::H1,
            EditorBlockKind::Heading2 => TurnKind::H2,
            EditorBlockKind::Heading3 => TurnKind::H3,
            EditorBlockKind::BulletList => TurnKind::Bullet,
            EditorBlockKind::NumberedList => TurnKind::Numbered,
            EditorBlockKind::Todo => TurnKind::Todo,
            EditorBlockKind::Quote => TurnKind::Quote,
            EditorBlockKind::Callout => TurnKind::Callout,
            EditorBlockKind::CodeBlock => TurnKind::Code,
            EditorBlockKind::MathBlock => TurnKind::Math,
        }
    }
}

impl From<TurnKind> for EditorBlockKind {
    fn from(kind: TurnKind) -> Self {
        match kind {
            TurnKind::Text => EditorBlockKind::Text,
            TurnKind::H1 => EditorBlockKind::Heading1,
            TurnKind::H2 => EditorBlockKind::Heading2,
            TurnKind::H3 => EditorBlockKind::Heading3,
            TurnKind::Bullet => EditorBlockKind::BulletList,
            TurnKind::Numbered => EditorBlockKind::NumberedList,
            TurnKind::Todo => EditorBlockKind::Todo,
            TurnKind::Quote => EditorBlockKind::Quote,
            TurnKind::Callout => EditorBlockKind::Callout,
            TurnKind::Code => EditorBlockKind::CodeBlock,
            TurnKind::Math => EditorBlockKind::MathBlock,
        }
    }
}

/// Return: a new block (ADR 0060). Lists, tasks, numbers and quotes
/// continue, and an empty item ends its list; in a rendered table it moves
/// to the cell below, and in the last row adds a row with the caret in the
/// same column, or, on a last row whose cells are all empty, removes it and
/// leaves the table (a table of only a header keeps it; ADR 0110); in code
/// and math it types a newline.
#[uniffi::export]
pub fn editor_enter(text: String, selection: TextRange, rendered: bool) -> EditorChange {
    let sel = self::selection(&text, selection);
    change(commands::enter(&text, sel, rendered))
}

/// ⇧Return: a line break inside the block (a hard break, `<br>` in a table
/// cell, a newline in code); Return where a break can't go.
#[uniffi::export]
pub fn editor_shift_enter(text: String, selection: TextRange, rendered: bool) -> EditorChange {
    let sel = self::selection(&text, selection);
    change(commands::shift_enter(&text, sel, rendered))
}

/// ⌫: the selection, or before the caret — a rendered image's line, the
/// paragraph break at a paragraph's start, or one character (under the
/// phone's reveal rule the character before the caret always shows). Nil at
/// the start.
#[uniffi::export]
pub fn editor_backspace(
    text: String,
    selection: TextRange,
    rendered: bool,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::backspace(&text, sel, rendered).map(change)
}

/// ⌦: as [`editor_backspace`], forward. Nil at the end.
#[uniffi::export]
pub fn editor_delete_forward(
    text: String,
    selection: TextRange,
    rendered: bool,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::delete_forward(&text, sel, rendered).map(change)
}

/// Toggle `format` (the toolbar, ⌘B, ⌘I): inside a construct of that
/// format, the caret or a selection within it, its markers come off;
/// otherwise a selection is wrapped (the same text kept selected), a caret
/// in a word wraps the word, and a caret elsewhere gets an empty pair to
/// type into. Italic on a bold selection gives bold italic (`***x***`) and
/// back, as does Bold on an italic one (ADR 0110). Nil on a blank
/// selection, and in code or math.
#[uniffi::export]
pub fn editor_toggle_format(
    text: String,
    selection: TextRange,
    format: EditorInlineFormat,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    let format = match format {
        EditorInlineFormat::Bold => InlineFormat::Bold,
        EditorInlineFormat::Italic => InlineFormat::Italic,
        EditorInlineFormat::Strikethrough => InlineFormat::Strikethrough,
        EditorInlineFormat::Code => InlineFormat::Code,
        EditorInlineFormat::Highlight => InlineFormat::Highlight,
        EditorInlineFormat::Underline => InlineFormat::Underline,
    };
    commands::toggle_format(&text, sel, format).map(change)
}

/// Indent: the caret's list or quote item one level deeper, else spaces
/// over the selection. Nil in a rendered table (Tab moves between cells).
#[uniffi::export]
pub fn editor_indent(text: String, selection: TextRange, rendered: bool) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::indent(&text, sel, rendered).map(change)
}

/// Outdent: the caret's line one level out. Nil without an indent, or in a
/// rendered table.
#[uniffi::export]
pub fn editor_outdent(text: String, selection: TextRange, rendered: bool) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::outdent(&text, sel, rendered).map(change)
}

/// Turn every block the selection touches into `kind` (ADR 0110): blank
/// lines are skipped, and between new list or task items they go; a code
/// or math block or a callout wraps the selection in one. When every block
/// is `kind` already they all turn back into text. A selection keeps the
/// changed blocks selected. Nil when nothing changes.
#[uniffi::export]
pub fn editor_turn_into(
    text: String,
    selection: TextRange,
    kind: EditorBlockKind,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::turn_into(&text, sel, kind.into()).map(change)
}

/// The caret's block kind, which the formatting keyboard shows selected.
#[uniffi::export]
pub fn editor_block_kind(text: String, selection: TextRange) -> EditorBlockKind {
    let sel = self::selection(&text, selection);
    commands::block_kind(&text, sel).into()
}

/// Check or uncheck the caret line's task box. Nil when the line is not a
/// task.
#[uniffi::export]
pub fn editor_toggle_task(text: String, selection: TextRange) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::toggle_task(&text, sel).map(change)
}

/// Run a table command on the table holding the caret. A new row takes the
/// caret into its cell in the caret's column, a new column into its cell in
/// the caret's row (ADR 0110). Nil outside a table or where it does not
/// apply (deleting the header row or the last column).
#[uniffi::export]
pub fn editor_table_command(
    text: String,
    selection: TextRange,
    command: EditorTableCommand,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    let command = match command {
        EditorTableCommand::InsertRowAbove => TableCommand::InsertRowAbove,
        EditorTableCommand::InsertRowBelow => TableCommand::InsertRowBelow,
        EditorTableCommand::DeleteRow => TableCommand::DeleteRow,
        EditorTableCommand::DuplicateRow => TableCommand::DuplicateRow,
        EditorTableCommand::InsertColumnLeft => TableCommand::InsertColumnLeft,
        EditorTableCommand::InsertColumnRight => TableCommand::InsertColumnRight,
        EditorTableCommand::DeleteColumn => TableCommand::DeleteColumn,
        EditorTableCommand::Align { align } => TableCommand::Align(match align {
            MarkdownAlign::Left => Align::Left,
            MarkdownAlign::Center => Align::Center,
            MarkdownAlign::Right => Align::Right,
        }),
        EditorTableCommand::Style { style } => TableCommand::Style(match style {
            MarkdownTableStyle::Grid => None,
            MarkdownTableStyle::Striped => Some("striped".into()),
            MarkdownTableStyle::Header => Some("header".into()),
            MarkdownTableStyle::Minimal => Some("minimal".into()),
        }),
        EditorTableCommand::DeleteTable => TableCommand::DeleteTable,
    };
    commands::table_command(&text, sel, &command).map(change)
}

/// A caret move in a drawn table (ADR 0109): Tab, ⇧Tab, the arrow keys and
/// the table controls' move keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EditorTableMove {
    /// The next cell; from the last cell, out below the table as Return
    /// leaves its last row.
    Next,
    /// The previous cell; the first cell stays.
    Previous,
    /// The cell above, or out above the table.
    Up,
    /// The cell below, or out below the table.
    Down,
    /// A character left, then the previous cell.
    Left,
    /// A character right, then the next cell.
    Right,
}

/// Tab, ⇧Tab, an arrow key or a table control's move key in a drawn table
/// (live preview): the caret to another cell, or a character over. Nil
/// outside a drawn table, where the key does what it does elsewhere.
#[uniffi::export]
pub fn editor_table_move(
    text: String,
    selection: TextRange,
    by: EditorTableMove,
    rendered: bool,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    let by = match by {
        EditorTableMove::Next => CellMove::Next,
        EditorTableMove::Previous => CellMove::Previous,
        EditorTableMove::Up => CellMove::Up,
        EditorTableMove::Down => CellMove::Down,
        EditorTableMove::Left => CellMove::Left,
        EditorTableMove::Right => CellMove::Right,
    };
    commands::table_move(&text, sel, by, rendered).map(change)
}

/// Typing `typed` over the selection in a drawn table's cell (live
/// preview), as that cell's own bytes: a line break becomes `<br>`, a pipe
/// is escaped (`\|`), and backslashes keep each pipe what it was. Nil
/// outside a drawn table, where typing is plain.
#[uniffi::export]
pub fn editor_type_in_table(
    text: String,
    selection: TextRange,
    typed: String,
    rendered: bool,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    commands::type_in_table(&text, sel, &typed, rendered).map(change)
}

/// The selection kept in one cell of a drawn table (live preview), its
/// caret's: never past the cell's pipes, between a backslash and what it
/// escapes, on the `| --- |` row or the style marker. Nil outside a drawn
/// table.
#[uniffi::export]
pub fn editor_table_selection(text: String, selection: TextRange) -> Option<TextRange> {
    let sel = self::selection(&text, selection);
    cells::keep_in_cell(&text, sel).map(|range| TextRange::of(&range))
}

/// The drawn table's cell holding the caret, for the table controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditorTableCell {
    /// The grid row: 0 is the header, then the body rows.
    pub row: u32,
    pub column: u32,
    pub rows: u32,
    pub columns: u32,
    /// The caret's column's alignment.
    pub align: MarkdownAlign,
}

/// A link at the caret: an inline `[text](url)` or a bare URL.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditorLink {
    /// The whole link.
    pub range: TextRange,
    pub text: String,
    pub url: String,
    /// An `http` or `https` URL, the only kind the app opens.
    pub web: bool,
}

/// What the editing toolbar and the formatting keyboard show for the
/// selection (ADR 0109).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditorCaretState {
    /// The inline formats the caret is inside (or the selection lies
    /// within): their buttons show on.
    pub formats: Vec<EditorInlineFormat>,
    /// Inline formats apply here: not in code or math.
    pub can_format: bool,
    /// The caret's block kind, selected in "Turn into".
    pub block: EditorBlockKind,
    /// The caret's line is a bulleted or numbered item, not a task.
    pub list: bool,
    /// The caret's line is a task: whether it is checked.
    pub task: Option<bool>,
    /// Indent nests the caret's list item under the item above it.
    pub can_indent: bool,
    /// The caret's line has an indent to take away.
    pub can_outdent: bool,
    /// The caret is in a table the view draws.
    pub in_table: bool,
    /// In live preview, the drawn table's cell holding the caret: the
    /// formatting keyboard shows the table controls (ADR 0109).
    pub cell: Option<EditorTableCell>,
    /// The link holding or touching the caret, for its Open and Edit chip.
    pub link: Option<EditorLink>,
}

impl From<InlineFormat> for EditorInlineFormat {
    fn from(format: InlineFormat) -> Self {
        match format {
            InlineFormat::Bold => EditorInlineFormat::Bold,
            InlineFormat::Italic => EditorInlineFormat::Italic,
            InlineFormat::Strikethrough => EditorInlineFormat::Strikethrough,
            InlineFormat::Code => EditorInlineFormat::Code,
            InlineFormat::Highlight => EditorInlineFormat::Highlight,
            InlineFormat::Underline => EditorInlineFormat::Underline,
        }
    }
}

/// What the toolbar shows for `selection`: one call per caret move.
#[uniffi::export]
pub fn editor_caret_state(text: String, selection: TextRange, rendered: bool) -> EditorCaretState {
    let sel = self::selection(&text, selection);
    let state = toolbar::caret_state(&text, sel, rendered);
    EditorCaretState {
        formats: state.formats.into_iter().map(Into::into).collect(),
        can_format: state.can_format,
        block: state.block.into(),
        list: state.list,
        task: state.task,
        can_indent: state.can_indent,
        can_outdent: state.can_outdent,
        in_table: state.in_table,
        cell: state.cell.map(|cell| EditorTableCell {
            row: u32::try_from(cell.row).unwrap_or(u32::MAX),
            column: u32::try_from(cell.column).unwrap_or(u32::MAX),
            rows: u32::try_from(cell.rows).unwrap_or(u32::MAX),
            columns: u32::try_from(cell.columns).unwrap_or(u32::MAX),
            align: match cell.align {
                Align::Left => MarkdownAlign::Left,
                Align::Center => MarkdownAlign::Center,
                Align::Right => MarkdownAlign::Right,
            },
        }),
        link: state.link.map(|link| EditorLink {
            range: TextRange::of(&link.range),
            text: link.text,
            url: link.url,
            web: link.web,
        }),
    }
}

/// The toolbar's list button: every block the selection touches becomes a
/// bulleted item, the blank lines between them gone; when each is a
/// bulleted or numbered item already, they all turn back into text
/// (ADR 0110). The changed blocks stay selected.
#[uniffi::export]
pub fn editor_toggle_list(text: String, selection: TextRange) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    toolbar::toggle_list(&text, sel).map(change)
}

/// The toolbar's task button: on one task line its box checks or unchecks;
/// otherwise every block the selection touches becomes a task, or, when
/// each is a task already, they all turn back into text (ADR 0110).
#[uniffi::export]
pub fn editor_task_button(text: String, selection: TextRange) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    toolbar::task_button(&text, sel).map(change)
}

/// The link sheet's Done: `[link_text](url)` over the link at the caret or
/// over the selection, the caret after it. An empty URL takes the caret's
/// link away and keeps its text. Nil with no URL and no link.
#[uniffi::export]
pub fn editor_set_link(
    text: String,
    selection: TextRange,
    link_text: String,
    url: String,
) -> Option<EditorChange> {
    let sel = self::selection(&text, selection);
    toolbar::set_link(&text, sel, &link_text, &url).map(change)
}

/// The formatting keyboard's Rule: `---` as a block of its own after the
/// caret's line, the caret below it.
#[uniffi::export]
pub fn editor_insert_rule(text: String, selection: TextRange) -> EditorChange {
    let sel = self::selection(&text, selection);
    change(toolbar::insert_rule(&text, sel))
}

/// The formatting keyboard's Table: an empty two-column table (a header
/// row and a body row) after the caret's line, the caret in its first cell.
#[uniffi::export]
pub fn editor_insert_table(text: String, selection: TextRange) -> EditorChange {
    let sel = self::selection(&text, selection);
    change(toolbar::insert_table(&text, sel))
}

/// Paste `pasted` (plain text, or [`crate::markdown::pasted_markdown`]'s
/// result) over the selection, normalised as desktop does: line endings, a
/// rendered table cell's line breaks and pipes, `$$` fences.
#[uniffi::export]
pub fn editor_paste(
    text: String,
    selection: TextRange,
    pasted: String,
    rendered: bool,
) -> EditorChange {
    let sel = self::selection(&text, selection);
    change(commands::paste(&text, sel, pasted, rendered))
}

/// The marker ranges live preview shows for the selection under the phone's
/// reveal rule (ADR 0109): the construct the caret touches shows its
/// markers, and the line prefix (`#`, `>`, a bullet) shows on the caret's
/// line; a selection shows every marker on its lines. Every other marker
/// span stays hidden.
#[uniffi::export]
pub fn editor_visible_markers(text: String, selection: TextRange) -> Vec<TextRange> {
    let sel = self::selection(&text, selection);
    visible_markers(&text, sel, RevealRule::CaretSpan)
        .iter()
        .map(TextRange::of)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caret(offset: u64) -> TextRange {
        TextRange {
            start: offset,
            end: offset,
        }
    }

    fn range(start: u64, end: u64) -> TextRange {
        TextRange { start, end }
    }

    /// `text` with `change` applied, and the selection after it.
    fn applied(text: &str, change: &EditorChange) -> (String, TextRange) {
        let mut out = text.to_string();
        for edit in change.edits.iter().rev() {
            out.replace_range(
                edit.range.start as usize..edit.range.end as usize,
                &edit.text,
            );
        }
        (out, change.selection)
    }

    #[test]
    fn enter_continues_a_list_and_moves_down_a_table() {
        let c = editor_enter("- a".into(), caret(3), true);
        assert_eq!(applied("- a", &c), ("- a\n- ".into(), caret(6)));
        let t = "| a |\n| - |\n| b |";
        let c = editor_enter(t.into(), caret(2), true);
        assert!(c.edits.is_empty());
        assert_eq!(&t[c.selection.start as usize..][..1], "b");
    }

    #[test]
    fn shift_enter_makes_a_hard_break() {
        let c = editor_shift_enter("> a".into(), caret(3), true);
        assert_eq!(applied("> a", &c), ("> a\\\n> ".into(), caret(7)));
    }

    #[test]
    fn backspace_joins_paragraphs_and_stops_at_the_start() {
        let c = editor_backspace("one\n\ntwo".into(), caret(5), false).unwrap();
        assert_eq!(applied("one\n\ntwo", &c), ("onetwo".into(), caret(3)));
        assert_eq!(editor_backspace("x".into(), caret(0), true), None);
    }

    #[test]
    fn delete_forward_takes_the_shown_marker() {
        // The caret touching the bold shows its markers, so ⌦ deletes the `*`.
        let s = "**ab**";
        let c = editor_delete_forward(s.into(), caret(0), true).unwrap();
        assert_eq!(applied(s, &c).0, "*ab**");
        assert_eq!(editor_delete_forward("x".into(), caret(1), true), None);
    }

    #[test]
    fn enter_in_a_table_never_returns_a_caret_inside_a_character() {
        let t = "| ab |\n| --- |\n| я |";
        let c = editor_enter(t.into(), caret(3), true);
        assert!(t.is_char_boundary(c.selection.start as usize));
    }

    #[test]
    fn toggle_format_wraps_and_unwraps() {
        let c = editor_toggle_format("a b".into(), range(2, 3), EditorInlineFormat::Bold).unwrap();
        assert_eq!(applied("a b", &c), ("a **b**".into(), range(4, 5)));
        let c =
            editor_toggle_format("a **b**".into(), range(4, 5), EditorInlineFormat::Bold).unwrap();
        assert_eq!(applied("a **b**", &c).0, "a b");
        for (format, wrapped) in [
            (EditorInlineFormat::Italic, "*b*"),
            (EditorInlineFormat::Strikethrough, "~~b~~"),
            (EditorInlineFormat::Code, "`b`"),
            (EditorInlineFormat::Highlight, "==b=="),
            (EditorInlineFormat::Underline, "<u>b</u>"),
        ] {
            let c = editor_toggle_format("b".into(), range(0, 1), format).unwrap();
            assert_eq!(applied("b", &c).0, wrapped);
        }
        // A caret in a word wraps the word; inside bold, bold comes off.
        let c = editor_toggle_format("ab".into(), caret(1), EditorInlineFormat::Bold).unwrap();
        assert_eq!(applied("ab", &c), ("**ab**".into(), caret(3)));
        let c = editor_toggle_format("**ab**".into(), caret(3), EditorInlineFormat::Bold).unwrap();
        assert_eq!(applied("**ab**", &c), ("ab".into(), caret(1)));
        assert_eq!(
            editor_toggle_format("a  b".into(), range(1, 3), EditorInlineFormat::Bold),
            None
        );
    }

    #[test]
    fn the_caret_state_tells_the_toolbar() {
        let s = "- [ ] **due** [site](https://a.org)";
        let state = editor_caret_state(s.into(), caret(9), true);
        assert_eq!(state.formats, [EditorInlineFormat::Bold]);
        assert_eq!(state.task, Some(false));
        assert_eq!(state.block, EditorBlockKind::Todo);
        assert!(!state.list && state.can_format && !state.in_table && state.link.is_none());
        let link = editor_caret_state(s.into(), caret(16), true).link.unwrap();
        assert_eq!(
            &s[link.range.start as usize..link.range.end as usize],
            "[site](https://a.org)"
        );
        assert_eq!(
            (link.text.as_str(), link.url.as_str(), link.web),
            ("site", "https://a.org", true)
        );
        // In a character: pulled back, as every call does.
        let state = editor_caret_state("дa".into(), caret(1), false);
        assert_eq!(state.block, EditorBlockKind::Text);
    }

    #[test]
    fn toolbar_and_formatting_keyboard_commands() {
        let c = editor_toggle_list("a".into(), caret(1)).unwrap();
        assert_eq!(applied("a", &c).0, "- a");
        let c = editor_task_button("- [ ] a".into(), caret(7)).unwrap();
        assert_eq!(applied("- [ ] a", &c).0, "- [x] a");
        // Over several paragraphs: an item each, the selection kept on them,
        // and back to text (ADR 0110). Offsets are UTF-8 bytes.
        let s = "Первый.\n\nВторой.";
        let all = range(0, s.len() as u64);
        let c = editor_toggle_list(s.into(), all).unwrap();
        let (out, sel) = applied(s, &c);
        assert_eq!(out, "- Первый.\n- Второй.");
        assert_eq!(sel, range(0, out.len() as u64));
        let c = editor_toggle_list(out.clone(), sel).unwrap();
        assert_eq!(applied(&out, &c).0, "Первый.\n\nВторой.");
        let c = editor_task_button(s.into(), all).unwrap();
        assert_eq!(applied(s, &c).0, "- [ ] Первый.\n- [ ] Второй.");
        let c = editor_set_link(
            "go".into(),
            range(0, 2),
            "go".into(),
            "https://a.org".into(),
        )
        .unwrap();
        assert_eq!(applied("go", &c), ("[go](https://a.org)".into(), caret(19)));
        assert_eq!(
            editor_set_link("go".into(), caret(1), "x".into(), "".into()),
            None
        );
        let c = editor_insert_rule("a".into(), caret(1));
        assert_eq!(applied("a", &c), ("a\n\n---\n\n".into(), caret(8)));
        let c = editor_insert_table("".into(), caret(0));
        assert_eq!(
            applied("", &c),
            ("|  |  |\n| --- | --- |\n|  |  |\n\n".into(), caret(2))
        );
    }

    #[test]
    fn indent_and_outdent_a_list_item() {
        let c = editor_indent("1. a".into(), caret(4), true).unwrap();
        assert_eq!(applied("1. a", &c), ("    1. a".into(), caret(8)));
        let c = editor_outdent("    1. a".into(), caret(8), true).unwrap();
        assert_eq!(applied("    1. a", &c), ("1. a".into(), caret(4)));
        assert_eq!(editor_outdent("a".into(), caret(1), true), None);
        let t = "| a |\n| - |\n| b |";
        assert_eq!(editor_indent(t.into(), caret(2), true), None);
    }

    #[test]
    fn turn_into_and_block_kind() {
        let c = editor_turn_into("a".into(), caret(1), EditorBlockKind::Heading2).unwrap();
        assert_eq!(applied("a", &c), ("## a".into(), caret(4)));
        let c = editor_turn_into("a".into(), caret(1), EditorBlockKind::CodeBlock).unwrap();
        assert_eq!(applied("a", &c), ("```\na\n```".into(), caret(4)));
        assert_eq!(
            editor_block_kind("> [!TIP]\n> x".into(), caret(11)),
            EditorBlockKind::Callout
        );
        // Already that kind: back to text (ADR 0110).
        let c = editor_turn_into("- a".into(), caret(3), EditorBlockKind::BulletList).unwrap();
        assert_eq!(applied("- a", &c).0, "a");
        assert_eq!(
            editor_turn_into("a".into(), caret(1), EditorBlockKind::Text),
            None
        );
    }

    #[test]
    fn toggle_task_flips_the_box() {
        let c = editor_toggle_task("- [x] t".into(), caret(7)).unwrap();
        assert_eq!(applied("- [x] t", &c), ("- [ ] t".into(), caret(7)));
        assert_eq!(editor_toggle_task("t".into(), caret(0)), None);
    }

    #[test]
    fn table_commands_edit_the_caret_table() {
        let t = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let c =
            editor_table_command(t.into(), caret(2), EditorTableCommand::InsertRowBelow).unwrap();
        assert_eq!(
            applied(t, &c).0,
            "| a | b |\n| --- | --- |\n|  |  |\n| 1 | 2 |"
        );
        let c = editor_table_command(t.into(), caret(28), EditorTableCommand::DeleteRow).unwrap();
        assert_eq!(applied(t, &c).0, "| a | b |\n| --- | --- |");
        let c = editor_table_command(
            t.into(),
            caret(6),
            EditorTableCommand::Align {
                align: MarkdownAlign::Right,
            },
        )
        .unwrap();
        assert_eq!(applied(t, &c).0, "| a | b |\n| :-- | --: |\n| 1 | 2 |");
        let c = editor_table_command(
            t.into(),
            caret(2),
            EditorTableCommand::Style {
                style: MarkdownTableStyle::Striped,
            },
        )
        .unwrap();
        assert!(applied(t, &c).0.starts_with("<!-- table:striped -->\n"));
        let c = editor_table_command(t.into(), caret(2), EditorTableCommand::DeleteTable).unwrap();
        assert_eq!(applied(t, &c), (String::new(), caret(0)));
        for command in [
            EditorTableCommand::InsertRowAbove,
            EditorTableCommand::DuplicateRow,
            EditorTableCommand::InsertColumnLeft,
            EditorTableCommand::InsertColumnRight,
            EditorTableCommand::DeleteColumn,
        ] {
            assert!(
                editor_table_command(t.into(), caret(28), command).is_some(),
                "{command:?}"
            );
            assert_eq!(
                editor_table_command("plain".into(), caret(1), command),
                None
            );
        }
    }

    #[test]
    fn cells_are_edited_and_moved_through() {
        let t = "| a | b |\n| --- | --: |\n| 1 | 2 |";
        let b = t.find("b |").unwrap() as u64;
        let state = editor_caret_state(t.into(), caret(b), true);
        assert_eq!(
            state.cell,
            Some(EditorTableCell {
                row: 0,
                column: 1,
                rows: 2,
                columns: 2,
                align: MarkdownAlign::Right
            })
        );
        assert_eq!(editor_caret_state(t.into(), caret(b), false).cell, None);
        // Typing a pipe and a line break stays in the cell.
        let c = editor_type_in_table(t.into(), caret(b), "x|\ny".into(), true).unwrap();
        assert_eq!(applied(t, &c).0, t.replace("| b", "| x\\|<br>yb"));
        assert_eq!(
            editor_type_in_table(t.into(), caret(b), "|".into(), false),
            None
        );
        // Tab to the next row's first cell; Up back to the header.
        let c = editor_table_move(t.into(), caret(b), EditorTableMove::Next, true).unwrap();
        assert_eq!(&t[c.selection.start as usize..], "1 | 2 |");
        let c = editor_table_move(t.into(), c.selection, EditorTableMove::Up, true).unwrap();
        assert_eq!(
            &t[c.selection.start as usize..],
            "a | b |\n| --- | --: |\n| 1 | 2 |"
        );
        assert_eq!(
            editor_table_move(t.into(), caret(b), EditorTableMove::Next, false),
            None
        );
        // The `| --- |` row keeps no caret.
        let sep = t.find("--:").unwrap() as u64;
        assert_eq!(editor_table_selection(t.into(), caret(sep)), Some(caret(b)));
        assert_eq!(editor_table_selection("x".into(), caret(0)), None);
        // ⌫ at a cell's start changes nothing.
        let c = editor_backspace(t.into(), caret(b), true).unwrap();
        assert!(c.edits.is_empty());
    }

    #[test]
    fn paste_normalises_line_endings_and_cells() {
        let c = editor_paste("ab".into(), range(0, 1), "x\r\ny".into(), false);
        assert_eq!(applied("ab", &c), ("x\nyb".into(), caret(3)));
        let t = "| a |\n| - |";
        let c = editor_paste(t.into(), caret(3), "1|2".into(), true);
        assert_eq!(applied(t, &c).0, "| a1\\|2 |\n| - |");
    }

    #[test]
    fn visible_markers_follow_the_caret_span_rule() {
        let s = "- a **b** [l](u)";
        let shown = |sel| -> Vec<String> {
            editor_visible_markers(s.into(), sel)
                .iter()
                .map(|r| s[r.start as usize..r.end as usize].to_string())
                .collect()
        };
        assert_eq!(shown(caret(6)), ["- ", "**", "**"]);
        assert_eq!(shown(caret(2)), ["- "]);
        assert!(editor_visible_markers("a\n**b**".into(), caret(0)).is_empty());
    }

    #[test]
    fn selections_outside_the_text_or_inside_a_character_are_pulled_back() {
        // "д" is two bytes: offset 1 falls inside it.
        assert_eq!(selection("дa", range(1, 99)), 0..3);
        let c = editor_enter("- a".into(), caret(u64::MAX), true);
        assert_eq!(applied("- a", &c).0, "- a\n- ");
    }
}
