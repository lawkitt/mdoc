//! Source edits and input: keyboard/clipboard commands, undo grouping and IME.
//!
//! EditorState remains the single owner of source, selection and history. This
//! module keeps revision changes, diagnostic remapping and UTF-16 conversions
//! beside the input paths that use them. Structural/table edits use the same
//! history methods; painted geometry and pointer navigation stay in the parent.

use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window,
    point, px,
};
use unicode_segmentation::UnicodeSegmentation;

use mdoc_editor_core::history::{EditKind, Snapshot};
use mdoc_editor_core::paste::{PasteTarget, normalize_paste};
use mdoc_editor_core::tables as table_rules;
use mdoc_editor_core::{delete, enter, format, text};

use super::{
    Backspace, Bold, Code, Copy, Cut, Delete, EditorEvent, EditorState, Indent, Italic, LineBreak,
    Newline, Outdent, Paste, Redo, ShowCharacterPalette, Strike, TAB_INDENT, Underline, Undo,
    caret_line_pos, caret_off_marker_line, line_pos, markdown_syntax,
};

impl EditorState {
    /// A host metadata change shares Undo/Redo with text without changing source,
    /// selection or emitting a text Changed event. The zero-length edit denotes
    /// a new history state rather than history travel.
    pub fn checkpoint_metadata(&mut self, revision: u64, cx: &mut Context<Self>) -> bool {
        if revision != self.content_gen {
            return false;
        }
        self.begin_history(
            false,
            vec![super::SourceEdit {
                range: 0..0,
                new_len: 0,
            }],
        );
        self.content_gen += 1;
        self.history.set_last_edit(EditKind::Other);
        self.emit_transaction(cx);
        cx.notify();
        true
    }
    /// Replace sorted, non-overlapping source ranges atomically as one undo step.
    /// Refuses stale revisions and invalid UTF-8 geometry without touching text.
    /// Selection is remapped; the operation never resets the existing undo history.
    pub fn replace_ranges(
        &mut self,
        revision: u64,
        edits: &[(Range<usize>, String)],
        cx: &mut Context<Self>,
    ) -> bool {
        if revision != self.content_gen || edits.is_empty() {
            return false;
        }
        let mut end = 0;
        for (range, _) in edits {
            if range.start < end
                || range.start >= range.end
                || range.end > self.content.len()
                || !self.content.is_char_boundary(range.start)
                || !self.content.is_char_boundary(range.end)
            {
                return false;
            }
            end = range.end;
        }
        self.begin_history(
            false,
            edits
                .iter()
                .map(|(range, value)| super::SourceEdit {
                    range: range.clone(),
                    new_len: value.len(),
                })
                .collect(),
        );
        let remap = |offset: usize| {
            let mut delta = 0isize;
            for (range, replacement) in edits {
                if offset < range.start {
                    break;
                }
                if offset <= range.end {
                    return (range.start as isize + delta) as usize + replacement.len();
                }
                delta += replacement.len() as isize - range.len() as isize;
            }
            (offset as isize + delta) as usize
        };
        let selection = remap(self.selected_range.start)..remap(self.selected_range.end);
        let content = super::transactions::build_batch(&self.content, edits);
        for (range, replacement) in edits.iter().rev() {
            self.remap_diagnostics(range, replacement.len());
        }
        self.content = content;
        self.selected_range = selection;
        self.marked_range = None;
        self.content_gen += 1;
        self.history.set_last_edit(EditKind::Other);
        self.last_edit_keystroke = false;
        self.annotation_bounds.clear();
        self.emit_changed(cx);
        cx.notify();
        true
    }

    /// Replace byte `range` with `text` as ONE recorded (undoable) edit, leaving the caret
    /// after the inserted text. Unlike [`Self::set_text`] this preserves — and extends — the
    /// undo history, so a host writing back a structural edit (e.g. a committed `$$…$$`
    /// formula) lands as a normal undo step rather than clobbering the history.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        // Snap to char boundaries (start down, end up) so a stale/shifted range — e.g. one
        // captured before a prior formula commit moved the bytes — can't panic mid-UTF-8.
        let len = self.content.len();
        let mut start = range.start.min(len);
        while start > 0 && !self.content.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = range.end.clamp(start, len);
        while end < len && !self.content.is_char_boundary(end) {
            end += 1;
        }
        let range = start..end;
        self.record_edit(&range, text);
        self.content.replace_range(range.clone(), text);
        self.remap_diagnostics(&range, text.len());
        let caret = range.start + text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        // Don't coalesce a following keystroke into this structural replacement.
        self.history.set_last_edit(EditKind::Other);
        self.emit_changed(cx);
        cx.notify();
    }

    /// Replace the whole document; resets the caret to the start.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content_gen += 1;
        self.content = text.into();
        // Diagnostic ranges belong to the old text; the host recomputes them.
        self.diagnostics.clear();
        self.menu = None;
        // Never park the loaded caret on a collapsed marker line (`<!-- table/
        // math:… -->`): the first focus would reveal it raw mid-interaction.
        // This is the ONE passive parking path — every other caret write is a
        // deliberate placement (which SHOULD reveal markers for editing).
        let caret = caret_off_marker_line(&self.content, 0);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        // A programmatic load isn't undoable to the prior document.
        self.history.reset();
        self.last_transaction = None;
        self.position_cache.borrow_mut().clear();
        cx.notify();
    }

    /// Keep diagnostics valid across an edit at `edited` (the replaced byte
    /// range) that inserted `new_len` bytes: spans before the edit are left
    /// alone, spans after it are shifted by the size delta, and spans that
    /// overlap the edited text are dropped (that text changed, so they're
    /// stale). The host still recomputes the edited region on its own schedule —
    /// this just keeps the *other* spans correct so they don't all flicker off
    /// on every keystroke.
    pub(super) fn remap_diagnostics(&mut self, edited: &Range<usize>, new_len: usize) {
        let delta = new_len as isize - (edited.end - edited.start) as isize;
        self.diagnostics.retain_mut(|d| {
            if d.range.end <= edited.start {
                true
            } else if d.range.start >= edited.end {
                d.range.start = (d.range.start as isize + delta) as usize;
                d.range.end = (d.range.end as isize + delta) as usize;
                true
            } else {
                false
            }
        });
    }

    // --- Editing -------------------------------------------------------------

    pub(super) fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            // Word-style image deletion: with the caret on an image row — or at
            // the start of the line just below one — remove the whole picture
            // (line + newline) as one edit, never stepping into its hidden
            // markdown character by character.
            let off = self.cursor_offset();
            let (row, col) = self.row_col(off);
            if let Some(range) = self.image_row_range(row).or_else(|| {
                (col == 0 && row > 0)
                    .then(|| self.image_row_range(row - 1))
                    .flatten()
            }) {
                self.replace_range(range, "", cx);
                self.emit_changed(cx);
                return;
            }
            // A plain-text field removes one character, whatever it is.
            if self.plain {
                let prev = self.previous_boundary(off);
                if off != prev {
                    self.select_to(prev, cx);
                    self.replace_text_in_range(None, "", window, cx);
                }
                return;
            }
            // At a paragraph's start, one ⌫ undoes Enter's paragraph break
            // (ADR 0060) instead of leaving two soft-joined lines.
            let body =
                markdown_syntax::blockquote_prefix(&self.content[off - col..self.line_end(row)]);
            if (col == 0 || Some(col) == body)
                && let Some((range, caret)) = self.break_delete_range(row, false)
            {
                self.delete_break(range, caret, cx);
                return;
            }
            // Cditor-style around hidden formatting markers: delete the
            // previous VISIBLE character (never a marker byte), and take an
            // emptied construct's marker pair with it.
            if let Some(range) = self.fmt_delete_range(off, true) {
                self.replace_range(range, "", cx);
                self.emit_changed(cx);
                return;
            }
            let prev = self.previous_boundary(off);
            if off == prev {
                return;
            }
            self.select_to(prev, cx);
            self.replace_text_in_range(None, "", window, cx);
            return;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            // Word-style, mirroring `backspace`: the caret on an image row — or
            // at the end of the line just above one — removes the whole picture.
            let off = self.cursor_offset();
            let (row, _) = self.row_col(off);
            if let Some(range) = self.image_row_range(row).or_else(|| {
                (off == self.line_end(row))
                    .then(|| self.image_row_range(row + 1))
                    .flatten()
            }) {
                self.replace_range(range, "", cx);
                self.emit_changed(cx);
                return;
            }
            if self.plain {
                let next = self.next_boundary(off);
                if off != next {
                    self.select_to(next, cx);
                    self.replace_text_in_range(None, "", window, cx);
                }
                return;
            }
            // At a paragraph's end, ⌦ joins the next paragraph (see backspace).
            if off == self.line_end(row)
                && let Some((range, caret)) = self.break_delete_range(row, true)
            {
                self.delete_break(range, caret, cx);
                return;
            }
            // Cditor-style around hidden formatting markers (see backspace).
            if let Some(range) = self.fmt_delete_range(off, false) {
                self.replace_range(range, "", cx);
                self.emit_changed(cx);
                return;
            }
            let next = self.next_boundary(off);
            if off == next {
                return;
            }
            self.select_to(next, cx);
            self.replace_text_in_range(None, "", window, cx);
            return;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.plain {
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        // Inside a table, a raw newline would split the row's `| … |` markup.
        // Enter instead moves to the cell directly below (next row, same column,
        // spreadsheet-style); in the last row it adds a row, or from an empty
        // last row exits onto a fresh line below the table (ADR 0110).
        if self.caret_in_table() {
            if let Some(off) = self.table_move_vertical(1)
                && self
                    .table_rows
                    .get(self.row_col(off).0)
                    .and_then(Option::as_ref)
                    .is_some_and(|t| !t.is_separator)
            {
                self.move_to(off, cx);
                return;
            }
            if let Some((range, text, caret)) = table_rules::return_in_last_row(
                &self.content,
                self.scan_data().blocks(),
                self.cursor_offset(),
            ) {
                let into = caret - range.start;
                self.edit(range, &text, into, window, cx);
                return;
            }
            let (row, _) = self.row_col(self.cursor_offset());
            let mut last = row;
            while self
                .table_rows
                .get(last + 1)
                .and_then(Option::as_ref)
                .is_some()
            {
                last += 1;
            }
            let starts = self.line_starts();
            let end = starts.get(last + 1).map_or(self.content.len(), |&s| s - 1);
            // A blank line ends the table: a line right below it would be read
            // as another row (GFM).
            self.edit(end..end, "\n\n", 2, window, cx);
            return;
        }
        let (row, _) = self.row_col(self.cursor_offset());
        // Code and display math are literal: every newline counts as typed.
        if self.row_is_literal(row) {
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        // ADR 0060: everywhere else Enter makes a new block, so the copied
        // Markdown breaks where the editor shows a break. A selection is
        // replaced by a plain paragraph break.
        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "\n\n", window, cx);
            return;
        }
        let (range, text, caret) = self.enter_edit();
        self.edit(range, &text, caret, window, cx);
    }

    /// Enter's one edit at a collapsed caret outside tables, code and math:
    /// the replaced range, its text and the caret's offset into that text.
    fn enter_edit(&self) -> (Range<usize>, String, usize) {
        enter::enter_edit(&self.content, self.cursor_offset())
    }

    /// Replace `range` with `text` as one undo step, leaving the caret
    /// `caret` bytes into the inserted text.
    fn edit(
        &mut self,
        range: Range<usize>,
        text: &str,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let start = range.start;
        self.selected_range = range;
        self.selection_reversed = false;
        self.replace_text_in_range(None, text, window, cx);
        if caret != text.len() {
            self.move_to(start + caret, cx);
        }
    }

    /// ⇧Enter: a line break inside the block (ADR 0060). In prose, lists and
    /// quotes it's a CommonMark hard break (`\` then a newline, the list or
    /// quote prefix kept); in a table cell `<br>`; in code and math a plain
    /// newline. Headings can't hold a break, and a break needs text before it
    /// on the line, so both fall back to Enter.
    pub(super) fn line_break(
        &mut self,
        _: &LineBreak,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        if self.plain {
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        if self.caret_in_table() {
            self.replace_text_in_range(None, "<br>", window, cx);
            return;
        }
        let (row, _) = self.row_col(self.cursor_offset());
        if self.row_is_literal(row) {
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        match enter::line_break(&self.content, self.selected_range.start) {
            Some(text) => self.replace_text_in_range(None, &text, window, cx),
            None => self.newline(&Newline, window, cx),
        }
    }

    /// Whether logical line `row` is literal text, where Enter types a plain
    /// newline: a fenced code block (fences included) or display math.
    fn row_is_literal(&self, row: usize) -> bool {
        self.scan_data().blocks().row_is_literal(&self.content, row)
    }

    /// What ⌫ at the body start of `row` (or ⌦, with `forward`, at its end)
    /// removes around a paragraph break (ADR 0060): the range and where the
    /// caret lands ([`delete::break_delete_range`]).
    fn break_delete_range(&self, row: usize, forward: bool) -> Option<(Range<usize>, usize)> {
        delete::break_delete_range(&self.content, self.scan_data().blocks(), row, forward)
    }

    /// Apply [`Self::break_delete_range`] as its own undo step, not part of a
    /// preceding ⌫/⌦ run.
    fn delete_break(&mut self, range: Range<usize>, caret: usize, cx: &mut Context<Self>) {
        self.history.set_last_edit(EditKind::Other);
        self.replace_range(range, "", cx);
        if self.cursor_offset() != caret {
            self.move_to(caret, cx);
        }
        self.emit_changed(cx);
    }

    /// Toggle an inline wrapping marker (`**` bold, `*` italic, `` ` `` code)
    /// around the selection — the symmetric case of [`Self::toggle_wrap_pair`].
    fn toggle_wrap(&mut self, marker: &str, cx: &mut Context<Self>) {
        self.toggle_wrap_pair(marker, marker, cx);
    }

    /// Toggle an open/close marker pair (`<u>`/`</u>`, or a symmetric `**`)
    /// around the selection. No-op on an empty selection. Unwraps when the
    /// selection is already wrapped (markers just inside or just outside it),
    /// otherwise wraps — keeping the same text selected so presses toggle.
    /// Whitespace at either end stays outside new markers: CommonMark does not
    /// render `**delta **` as bold (#77). Existing markers unwrap as selected
    /// (`<u> delta </u>`) or once the whitespace is trimmed (`**delta** `).
    fn toggle_wrap_pair(&mut self, open: &str, close: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some((range, new, new_sel)) =
            format::toggle_wrap_pair(&self.content, self.selected_range.clone(), open, close)
        else {
            return;
        };
        self.record_edit(&range, &new);
        self.content.replace_range(range.clone(), &new);
        self.selected_range = new_sel;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, new.len());
        self.emit_changed(cx);
        cx.notify();
    }

    pub(super) fn bold(&mut self, _: &Bold, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("**", cx);
    }

    pub(super) fn italic(&mut self, _: &Italic, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("*", cx);
    }

    pub(super) fn code(&mut self, _: &Code, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("`", cx);
    }

    pub(super) fn strike(&mut self, _: &Strike, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("~~", cx);
    }

    pub(super) fn underline(&mut self, _: &Underline, _: &mut Window, cx: &mut Context<Self>) {
        // Markdown has no underline — the `<u>` tag, which both views honor.
        self.toggle_wrap_pair("<u>", "</u>", cx);
    }

    /// Tab: on a list/quote item, indent the whole item one level (`tab_indent`
    /// spaces at the line start, caret shifts with it); elsewhere insert that many
    /// spaces at the caret (replacing any selection).
    pub(super) fn indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        // In a table, Tab moves to the next cell rather than indenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(true) {
                self.move_to(offset, cx);
            }
            return;
        }
        let (range, new_text, caret) =
            match format::indent(&self.content, self.cursor_offset(), TAB_INDENT) {
                format::IndentEdit::Spaces(spaces) => {
                    self.replace_text_in_range(None, &spaces, window, cx);
                    return;
                }
                format::IndentEdit::Item { range, text, caret } => (range, text, caret),
            };
        self.record_edit(&range, &new_text);
        self.content.replace_range(range.clone(), &new_text);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, new_text.len());
        self.emit_changed(cx);
        cx.notify();
    }

    /// Shift+Tab: outdent the caret's line — remove up to `tab_indent` leading
    /// spaces (or one leading tab) from the line start. No-op if there's none.
    pub(super) fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        // In a table, Shift+Tab moves to the previous cell rather than outdenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(false) {
                self.move_to(offset, cx);
            }
            return;
        }
        let Some((range, caret)) = format::outdent(&self.content, self.cursor_offset(), TAB_INDENT)
        else {
            return;
        };
        self.record_edit(&range, "");
        self.content.replace_range(range.clone(), "");
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, 0);
        self.emit_changed(cx);
        cx.notify();
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        // A copied FILE also carries its path as text; inserting that string
        // is never what a paste meant. Treat file and image clipboards as
        // not-ours: fall through so a host binding on the same keys
        // (mdoc's image/file paste) can run.
        let has_files = item.as_ref().is_some_and(|i| {
            i.entries()
                .iter()
                .any(|e| matches!(e, gpui::ClipboardEntry::ExternalPaths(_)))
        });
        match item.and_then(|i| i.text()).filter(|_| !has_files) {
            Some(text) => self.insert_pasted(text, window, cx),
            None => cx.propagate(),
        }
    }

    /// Insert `text` at the caret as a paste would, without the clipboard:
    /// a host that already holds the text (mdoc's Paste text as new tab,
    /// issue #300) needn't wait for this editor to take focus.
    pub fn insert_pasted(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let target = PasteTarget {
            plain: self.plain,
            table_cell: !self.plain && self.caret_in_table(),
            markdown: self.markdown_style.is_some(),
        };
        let text = normalize_paste(text, target);
        self.replace_text_in_range(None, &text, window, cx);
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            // Ordered items show the numbers written (ADR 0110), so the
            // source copies as the screen reads.
            let text = self.content[self.copy_range()].to_string();
            self.write_clipboard(text, cx);
        }
    }

    /// Copy the selection as the raw markdown ONLY — no host clipboard
    /// flavors — for pasting literal source into rich surfaces (the context
    /// menu's "Copy as Markdown"). Same selection rules and copy metadata as
    /// `copy`.
    pub(crate) fn copy_plain(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let text = self.content[self.copy_range()].to_string();
            self.write_clipboard(text, cx);
        }
    }

    /// Write a Copy/Cut payload as plain text, with the host's copy metadata.
    pub(super) fn write_clipboard(&self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(match &self.copy_metadata {
            Some(metadata) => ClipboardItem::new_string_with_metadata(text, metadata.clone()),
            None => ClipboardItem::new_string(text),
        });
    }

    /// What a copy takes: the selection — extended back over the first
    /// line's hidden list/task/quote prefix when the selection is multi-line
    /// and starts exactly at that line's body start. With markers painted
    /// (not text), "select the whole list" visually anchors AFTER the first
    /// `1. `, so a verbatim copy dropped the first marker while every other
    /// line kept its own. Raw mode (no markdown style) copies verbatim.
    fn copy_range(&self) -> std::ops::Range<usize> {
        let (start, end) = (
            self.selected_range.start.min(self.selected_range.end),
            self.selected_range.start.max(self.selected_range.end),
        );
        if self.markdown_style.is_none() || !self.content[start..end].contains('\n') {
            return start..end;
        }
        let line_start = self.content[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.content[line_start..]
            .find('\n')
            .map_or(self.content.len(), |i| line_start + i);
        let line = &self.content[line_start..line_end];
        let prefix_len = markdown_syntax::task_prefix(line)
            .map(|(l, ..)| l)
            .or_else(|| markdown_syntax::list_prefix(line).map(|(l, ..)| l))
            .or_else(|| markdown_syntax::blockquote_prefix(line));
        match prefix_len {
            Some(plen) if start == line_start + plen => line_start..end,
            _ => start..end,
        }
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.write_clipboard(self.content[self.selected_range.clone()].to_string(), cx);
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    pub(super) fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // --- Undo / redo ---------------------------------------------------------

    pub fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(prev) = self.history.undo(&self.content, self.selected_range.end) {
            self.travel_to(prev, cx);
        }
    }

    pub fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(next) = self.history.redo(&self.content, self.selected_range.end) {
            self.travel_to(next, cx);
        }
    }

    /// Restore a state Undo or Redo handed back.
    fn travel_to(&mut self, s: Snapshot, cx: &mut Context<Self>) {
        let changed = self.content != s.content;
        self.restore(s);
        if changed {
            self.emit_changed(cx);
        } else {
            self.emit_transaction(cx);
        }
        cx.notify();
    }

    pub(super) fn restore(&mut self, s: Snapshot) {
        // A snapshot swaps the whole text, so diagnostics would keep offsets
        // into the text being replaced — after undoing a batch of replacements
        // they'd land inside multi-byte characters. Remap them through the one
        // region that differs (common prefix/suffix kept).
        let (edited, new_len) = changed_region(&self.content, &s.content);
        self.remap_diagnostics(&edited, new_len);
        self.content_gen += 1;
        self.content = s.content;
        let caret = s.caret.min(self.content.len());
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    /// Snapshot the pre-edit state for undo, coalescing a run of single-grapheme
    /// inserts (or a run of deletes) into one undo step so typing isn't undone
    /// one character at a time.
    pub(super) fn record_edit(&mut self, range: &Range<usize>, new_text: &str) {
        self.content_gen += 1;
        // The forward caret (selection end), so undoing a backspace lands the
        // caret after the restored text rather than inside it.
        self.history
            .record(&self.content, self.selected_range.end, range, new_text);
        // A keystroke is one typed grapheme (incl. typed over a selection — that's
        // an auto-pair "wrap") or a single-char backspace. Multi-char edits (paste,
        // table ops, …) are not, so auto-pairing skips them.
        self.last_edit_keystroke = (new_text != "\n" && new_text.graphemes(true).count() == 1)
            || (new_text.is_empty() && self.content[range.clone()].graphemes(true).count() == 1);
    }

    /// Make the current state an undo point and start a new branch.
    fn push_undo_point(&mut self) {
        self.history
            .push_undo_point(&self.content, self.selected_range.end);
    }

    fn begin_history(&mut self, coalesce: bool, edits: Vec<super::SourceEdit>) {
        self.history
            .begin(coalesce, edits, &self.content, self.selected_range.end);
    }

    pub fn history_id(&self) -> u64 {
        self.history.history_id()
    }
    /// The states Undo and Redo reach from here, for hosts that travel the
    /// history by counting presses.
    pub fn history_path(&self) -> mdoc_history::HistoryPath {
        self.history.path()
    }
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
    /// Undo for hosts acting from outside the editor (e.g. a side panel).
    pub fn undo_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.undo(&Undo, window, cx);
    }
    pub fn redo_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.redo(&Redo, window, cx);
    }
    pub fn last_transaction(&self) -> Option<&std::sync::Arc<super::EditorTransaction>> {
        self.last_transaction.as_ref()
    }
    pub(super) fn emit_changed(&mut self, cx: &mut Context<Self>) {
        self.emit_transaction(cx);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }
    fn emit_transaction(&mut self, cx: &mut Context<Self>) {
        if let Some(transaction) = self.history.take_transaction(self.content_gen) {
            let transaction = std::sync::Arc::new(transaction);
            self.last_transaction = Some(transaction.clone());
            cx.emit(EditorEvent::Transaction(transaction));
        }
    }

    // --- UTF-16 + grapheme boundaries (IME / cursor movement) ----------------

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = self.utf16_resume(offset, false);
        for ch in self.content[utf8..].chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        self.utf16_anchor.set((self.content_gen, utf8, utf16));
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = self.utf16_resume(offset, true);
        for ch in self.content[utf8..].chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        self.utf16_anchor.set((self.content_gen, utf8, utf16));
        utf16
    }

    /// Where a conversion can start: the saved anchor when it's from the
    /// current content generation and at/before the target (`by_utf8` picks
    /// which unit the target is in), else the document start. IME composition
    /// converts monotonically-close offsets many times per keystroke, so this
    /// turns O(document) scans into O(distance-from-anchor).
    fn utf16_resume(&self, target: usize, by_utf8: bool) -> (usize, usize) {
        let (generation, utf8, utf16) = self.utf16_anchor.get();
        let anchor_pos = if by_utf8 { utf8 } else { utf16 };
        if generation == self.content_gen && anchor_pos <= target && utf8 <= self.content.len() {
            (utf8, utf16)
        } else {
            (0, 0)
        }
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    pub(super) fn previous_boundary(&self, offset: usize) -> usize {
        text::previous_boundary(&self.content, offset)
    }

    pub(super) fn next_boundary(&self, offset: usize) -> usize {
        text::next_boundary(&self.content, offset)
    }
}

impl EntityInputHandler for EditorState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.record_edit(&range, new_text);
        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        let caret = range.start + new_text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_x = None;
        // Keep unaffected diagnostics valid across the edit (shift those after
        // it, drop those it overlapped); the host recomputes the edited region.
        self.remap_diagnostics(&range, new_text.len());
        self.reveal_caret = true;
        self.emit_changed(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        // Composition updates retain the existing editor undo contract: only
        // the commit records a snapshot. Hosts still receive every exact change.
        // The state a composition starts from is an undo point too, so Undo
        // still reaches a host's step just before it (a decision, ADR 0079).
        if self.marked_range.is_none() && !new_text.is_empty() {
            self.push_undo_point();
        }
        self.content_gen += 1;
        self.history.advance(vec![super::SourceEdit {
            range: range.clone(),
            new_len: new_text.len(),
        }]);
        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.start)
            .unwrap_or_else(|| {
                let caret = range.start + new_text.len();
                caret..caret
            });
        self.remap_diagnostics(&range, new_text.len());
        self.reveal_caret = true;
        self.emit_changed(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (row, col) = self.row_col(range.start);
        let lh = self.line_h(row);
        let line = self.wrapped.get(row)?;
        let map = self.bidi_map(row);
        // The caret's own position draws by its affinity.
        let upstream = range.start == self.cursor_offset() && self.caret_upstream();
        let p = caret_line_pos(line, map, self.display_col(row, col), lh, upstream)?;
        let top = bounds.top() + self.line_tops.get(row).copied().unwrap_or(px(0.)) + p.y;
        let x = bounds.left() + p.x + self.row_origin_x(row);
        // Span the whole range when it stays on one wrap row (the common IME
        // composition), so the candidate window anchors under the marked TEXT
        // rather than a zero-width bar at its start. Multi-row ranges keep the
        // start-anchored bar.
        let (erow, ecol) = self.row_col(range.end);
        let x2 = if erow == row && range.end > range.start {
            line_pos(line, map, self.display_col(row, ecol), lh)
                .filter(|e| e.y == p.y)
                .map(|e| bounds.left() + e.x + self.row_origin_x(row))
                .unwrap_or(x)
        } else {
            x
        };
        Some(Bounds::from_corners(
            point(x, top),
            point(x2.max(x), top + lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

/// The byte range of `old` that differs from `new` (after the longest common
/// prefix and suffix, both on char boundaries) and that region's length in
/// `new`, for remapping offsets through a whole-text swap.
fn changed_region(old: &str, new: &str) -> (Range<usize>, usize) {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !(old.is_char_boundary(pre) && new.is_char_boundary(pre)) {
        pre -= 1;
    }
    let max_suf = a.len().min(b.len()) - pre;
    let mut suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suf)
        .take_while(|(x, y)| x == y)
        .count();
    while !(old.is_char_boundary(a.len() - suf) && new.is_char_boundary(b.len() - suf)) {
        suf -= 1;
    }
    (pre..a.len() - suf, b.len() - suf - pre)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SelectAll;

    #[test]
    fn changed_region_keeps_common_ends_on_char_boundaries() {
        assert_eq!(changed_region("abc", "abc"), (3..3, 0));
        assert_eq!(changed_region("a Иван b", "a [P1] b"), (2..10, 4));
        // "д" and "ж" share their first UTF-8 byte; the region can't start mid-letter.
        assert_eq!(changed_region("ад", "аж"), (2..4, 2));
        assert_eq!(changed_region("ab", "aab"), (1..1, 1));
    }

    #[gpui::test]
    fn undoing_replacements_remaps_diagnostics_off_character_middles(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            let text = "Иван Петров и Мария Сидорова пишут";
            e.set_text(text, cx);
            let word = text.find("пишут").unwrap();
            e.set_diagnostics(
                vec![crate::Diagnostic {
                    range: word..text.len(),
                }],
                cx,
            );
            let ivan = 0..text.find(' ').unwrap();
            let maria = text.find("Мария").unwrap()..text.find(" пишут").unwrap();
            assert!(e.replace_ranges(
                e.revision(),
                &[(ivan, "[P1]".into()), (maria, "[P2]".into())],
                cx,
            ));
            e.undo(&Undo, window, cx);
            assert_eq!(e.text(), text);
            let d = &e.diagnostics()[0].range;
            assert_eq!(&text[d.clone()], "пишут");
            e.redo(&Redo, window, cx);
            let d = &e.diagnostics()[0].range;
            assert_eq!(&e.text()[d.clone()], "пишут");
        });
    }

    /// The GFM rendering of `source`: what an AI reading the copied
    /// Markdown sees.
    fn rendered(source: &str) -> String {
        markdown::to_html_with_options(source, &markdown::Options::gfm()).unwrap()
    }

    /// An editor with key bindings, focused on `text` with the caret at its end.
    fn typing<'a>(
        cx: &'a mut gpui::TestAppContext,
        text: &str,
    ) -> (gpui::Entity<EditorState>, &'a mut gpui::VisualTestContext) {
        cx.update(crate::bind_keys);
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            e.set_text(text, cx);
            e.set_cursor(text.len(), cx);
            e.focus(window, cx);
        });
        (editor, cx)
    }

    fn text_of(editor: &gpui::Entity<EditorState>, cx: &mut gpui::VisualTestContext) -> String {
        editor.read_with(cx, |e, _| e.text().to_string())
    }

    #[gpui::test]
    fn enter_starts_a_paragraph(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = typing(cx, "");
        cx.simulate_input("Alpha");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Beta");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta");
        assert_eq!(rendered(&text_of(&editor, cx)), "<p>Alpha</p>\n<p>Beta</p>");

        // Splitting mid-line, and at a line soft-joined to the next one.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("AlphaBeta\nGamma", cx);
            e.set_cursor(5, cx);
        });
        cx.simulate_keystrokes("enter");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta\nGamma");
        editor.update_in(cx, |e, _, cx| {
            e.set_cursor(e.text().find("\nGamma").unwrap(), cx)
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("New");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta\n\nNew\n\nGamma");
        assert_eq!(rendered(&text_of(&editor, cx)).matches("<p>").count(), 4);

        // At a line's start, one newline splits soft-joined lines.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("Alpha\nBeta", cx);
            e.set_cursor(6, cx);
        });
        cx.simulate_keystrokes("enter");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta");

        // A heading ends at Enter too.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("# Title", cx);
            e.set_cursor(7, cx);
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Body");
        assert_eq!(
            rendered(&text_of(&editor, cx)),
            "<h1>Title</h1>\n<p>Body</p>"
        );
    }

    #[gpui::test]
    fn backspace_and_delete_undo_a_paragraph_break(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = typing(cx, "Alpha");
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("backspace");
        assert_eq!(
            text_of(&editor, cx),
            "Alpha",
            "one ⌫ removes the whole break"
        );

        editor.update_in(cx, |e, _, cx| {
            e.set_text("Alpha\n\nBeta", cx);
            e.set_cursor(7, cx);
        });
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&editor, cx), "AlphaBeta");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 5);

        editor.update_in(cx, |e, _, cx| {
            e.set_text("Alpha\n\nBeta", cx);
            e.set_cursor(5, cx);
        });
        cx.simulate_keystrokes("delete");
        assert_eq!(text_of(&editor, cx), "AlphaBeta");

        // Extra blank rows go one at a time; code keeps its newlines.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("Alpha\n\n\nBeta", cx);
            e.set_cursor(8, cx);
        });
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta");
        editor.update_in(cx, |e, _, cx| {
            e.set_text("```\na\n\nb\n```", cx);
            e.set_cursor(7, cx);
        });
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&editor, cx), "```\na\nb\n```");
    }

    #[gpui::test]
    fn shift_enter_is_a_line_break(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = typing(cx, "Alpha");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("Beta");
        assert_eq!(text_of(&editor, cx), "Alpha\\\nBeta");
        assert_eq!(rendered(&text_of(&editor, cx)), "<p>Alpha<br />\nBeta</p>");

        // A list item keeps its body indented; a quote keeps its prefix.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("- one", cx);
            e.set_cursor(5, cx);
        });
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        assert_eq!(text_of(&editor, cx), "- one\\\n  two");
        assert_eq!(
            rendered(&text_of(&editor, cx)),
            "<ul>\n<li>one<br />\ntwo</li>\n</ul>"
        );
        editor.update_in(cx, |e, _, cx| {
            e.set_text("> one", cx);
            e.set_cursor(5, cx);
        });
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "> one\\\n> ");

        // No break without text before it, or in a heading: Enter instead.
        editor.update_in(cx, |e, _, cx| {
            e.set_text("# Title", cx);
            e.set_cursor(7, cx);
        });
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "# Title\n\n");
        editor.update_in(cx, |e, _, cx| e.set_text("", cx));
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "\n");
    }

    #[gpui::test]
    fn enter_in_lists_quotes_and_code(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = typing(cx, "- one");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("two");
        assert_eq!(text_of(&editor, cx), "- one\n- two");
        // An empty item exits with a blank line, so the next text isn't a
        // lazy continuation of the list.
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("After");
        assert_eq!(text_of(&editor, cx), "- one\n- two\n\nAfter");
        assert_eq!(
            rendered(&text_of(&editor, cx)),
            "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n<p>After</p>"
        );

        editor.update_in(cx, |e, _, cx| {
            e.set_text("> one", cx);
            e.set_cursor(5, cx);
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("two");
        assert_eq!(text_of(&editor, cx), "> one\n>\n> two");
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("After");
        assert_eq!(text_of(&editor, cx), "> one\n>\n> two\n\nAfter");
        assert_eq!(
            rendered(&text_of(&editor, cx)),
            "<blockquote>\n<p>one</p>\n<p>two</p>\n</blockquote>\n<p>After</p>"
        );

        editor.update_in(cx, |e, _, cx| {
            e.set_text("```\nlet a = 1;\n```", cx);
            e.set_cursor(14, cx);
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("b");
        assert_eq!(text_of(&editor, cx), "```\nlet a = 1;\nb\n```");
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "```\nlet a = 1;\nb\n\n```");
    }

    #[gpui::test]
    fn enter_review_regressions(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = typing(cx, "");
        let set = |cx: &mut gpui::VisualTestContext, text: &str, caret: usize| {
            editor.update_in(cx, |e, _, cx| {
                e.set_text(text, cx);
                e.set_cursor(caret, cx);
            });
        };
        // Read-only: Enter changes nothing and keeps the caret on a boundary.
        set(cx, "я\nBeta", 2);
        editor.update(cx, |e, cx| e.set_read_only(true, cx));
        cx.simulate_keystrokes("enter enter shift-enter");
        assert_eq!(text_of(&editor, cx), "я\nBeta");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 2);
        editor.update(cx, |e, cx| e.set_read_only(false, cx));

        // ⇧Enter after a typed `\` uses it as the break instead of escaping it.
        set(cx, "Alpha\\Beta", 6);
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "Alpha\\\nBeta");
        assert_eq!(rendered(&text_of(&editor, cx)), "<p>Alpha<br />\nBeta</p>");

        // A list item's continuation line continues the list, and keeps its
        // indent on another ⇧Enter.
        set(cx, "- one", 5);
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("2b");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("three");
        assert_eq!(text_of(&editor, cx), "- one\\\n  two\\\n  2b\n- three");
        assert_eq!(
            rendered(&text_of(&editor, cx)),
            "<ul>\n<li>one<br />\ntwo<br />\n2b</li>\n<li>three</li>\n</ul>"
        );
        // Enter right after ⇧Enter turns the break into Enter's own.
        set(cx, "- one", 5);
        cx.simulate_keystrokes("shift-enter enter");
        assert_eq!(text_of(&editor, cx), "- one\n- ");
        set(cx, "Alpha", 5);
        cx.simulate_keystrokes("shift-enter enter");
        assert_eq!(text_of(&editor, cx), "Alpha\n\n");

        // Headings behind a list or quote prefix, and empty tasks, take Enter.
        set(cx, "- # Title", 9);
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "- # Title\n- ");
        set(cx, "> # Title", 9);
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "> # Title\n>\n> ");
        set(cx, "- [ ] one\n- [ ] ", 16);
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(text_of(&editor, cx), "- [ ] one\n\n");

        // ⌦ removes extra blank rows one at a time.
        set(cx, "Alpha\n\n\nBeta", 5);
        cx.simulate_keystrokes("delete");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta");

        // A joining ⌫ is its own undo step.
        set(cx, "Alpha\n\n\nBeta", 8);
        cx.simulate_keystrokes("backspace backspace");
        assert_eq!(text_of(&editor, cx), "AlphaBeta");
        editor.update_in(cx, |e, window, cx| e.undo(&Undo, window, cx));
        assert_eq!(text_of(&editor, cx), "Alpha\n\nBeta");

        // ⌫ and ⌦ undo a quoted paragraph break.
        set(cx, "> one", 5);
        cx.simulate_keystrokes("enter backspace");
        assert_eq!(text_of(&editor, cx), "> one");
        set(cx, "> one\n>\n> two", 10);
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&editor, cx), "> onetwo");
        set(cx, "> one\n>\n> two", 5);
        cx.simulate_keystrokes("delete");
        assert_eq!(text_of(&editor, cx), "> onetwo");

        // Round 2: repeated Enter in an empty document.
        set(cx, "", 0);
        cx.simulate_keystrokes("enter enter enter");
        assert_eq!(text_of(&editor, cx), "\n\n\n");
        // Enter after ⇧Enter keeps a soft-joined next line separate.
        set(cx, "Alpha\nBeta", 5);
        cx.simulate_keystrokes("shift-enter enter");
        cx.simulate_input("New");
        assert_eq!(text_of(&editor, cx), "Alpha\n\nNew\n\nBeta");
        // Extra bare `>` rows go one at a time, both ways.
        set(cx, "> one\n>\n>\n> two", 5);
        cx.simulate_keystrokes("delete");
        assert_eq!(text_of(&editor, cx), "> one\n>\n> two");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 5);
        set(cx, "> one\n>\n>\n> two", 12);
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&editor, cx), "> one\n>\n> two");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 10);
        // Differing quote depths don't join.
        set(cx, "> outer\n>\n>> inner", 13);
        cx.simulate_keystrokes("backspace");
        assert_ne!(text_of(&editor, cx), "> outerinner");
        // Inside the `> ` prefix isn't the body start.
        set(cx, "> one\n>\n> two", 9);
        cx.simulate_keystrokes("backspace");
        assert_ne!(text_of(&editor, cx), "> onetwo");
    }

    #[gpui::test]
    fn enter_and_shift_enter_in_a_table(cx: &mut gpui::TestAppContext) {
        let source = "| A | B |\n| --- | --- |\n| one | two |";
        let (editor, cx) = typing(cx, source);
        cx.simulate_resize(gpui::size(px(700.), px(400.)));
        editor.update(cx, |e, cx| {
            e.set_markdown_style(markdown_syntax::search_style(), cx)
        });
        // Table rows come from layout.
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        editor.update_in(cx, |e, _, cx| {
            e.set_cursor(source.find("one").unwrap() + 3, cx)
        });
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(
            text_of(&editor, cx),
            "| A | B |\n| --- | --- |\n| one<br> | two |"
        );
        // From the last row, Enter adds a row, the caret in its column
        // (ADR 0110).
        cx.simulate_keystrokes("enter");
        assert_eq!(
            text_of(&editor, cx),
            "| A | B |\n| --- | --- |\n| one<br> | two |\n|  |  |"
        );
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        // From that empty row, Enter removes it and leaves the table with a
        // blank line, so the next text isn't read as another row.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("After");
        let text = text_of(&editor, cx);
        assert!(text.ends_with("| two |\n\nAfter"), "{text:?}");
        assert!(
            rendered(&text).ends_with("</table>\n<p>After</p>"),
            "{}",
            rendered(&text)
        );
    }

    #[gpui::test]
    fn ctrl_word_and_document_moves(cx: &mut gpui::TestAppContext) {
        // Windows caret moves: Ctrl+Backspace/Delete and Ctrl(+Shift)+Home/End.
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            e.set_text("Иван Петров пишет", cx);
            e.set_cursor(e.text().find(" пишет").unwrap(), cx);
            e.delete_word_left(&crate::DeleteWordLeft, window, cx);
            assert_eq!(e.text(), "Иван  пишет");
            e.delete_word_right(&crate::DeleteWordRight, window, cx);
            assert_eq!(e.text(), "Иван пишет", "Ctrl+Delete stops at the next word");
            e.undo(&Undo, window, cx);
            assert_eq!(e.text(), "Иван Петров пишет", "one deleting run, one undo");

            e.set_cursor(0, cx);
            e.delete_word_left(&crate::DeleteWordLeft, window, cx);
            assert_eq!(e.text(), "Иван Петров пишет", "nothing before the start");

            e.set_text("one\ntwo", cx);
            e.set_cursor(0, cx);
            e.next_word_start_action(&crate::NextWordStart, window, cx);
            assert_eq!(e.cursor(), 3, "the line end is a stop");
            e.next_word_start_action(&crate::NextWordStart, window, cx);
            assert_eq!(e.cursor(), 4);
            e.next_word_start_action(&crate::NextWordStart, window, cx);
            assert_eq!(e.cursor(), 7);
            e.set_text("Иван Петров пишет", cx);

            e.document_end(&crate::DocumentEnd, window, cx);
            assert_eq!(e.cursor(), e.text().len());
            e.select_to_document_start(&crate::SelectToDocumentStart, window, cx);
            assert_eq!(e.selection(), 0..e.text().len());
            e.document_start(&crate::DocumentStart, window, cx);
            assert_eq!(e.cursor(), 0);
            e.select_to_document_end(&crate::SelectToDocumentEnd, window, cx);
            assert_eq!(e.selection(), 0..e.text().len());
        });
    }

    #[cfg(not(target_os = "macos"))]
    #[gpui::test]
    fn ctrl_arrows_move_by_word(cx: &mut gpui::TestAppContext) {
        cx.update(crate::bind_keys);
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            e.set_text("one two three", cx);
            e.set_cursor(0, cx);
            e.focus(window, cx);
        });
        // Windows stops at word starts, not ends.
        cx.simulate_keystrokes("ctrl-right ctrl-right");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 8);
        cx.simulate_keystrokes("ctrl-shift-left");
        assert_eq!(editor.read_with(cx, |e, _| e.selection()), 4..8);
        cx.simulate_keystrokes("ctrl-shift-right");
        assert_eq!(editor.read_with(cx, |e, _| e.cursor()), 8);
        cx.simulate_keystrokes("ctrl-end ctrl-backspace");
        assert_eq!(
            editor.read_with(cx, |e, _| e.text().to_string()),
            "one two "
        );
    }

    #[gpui::test]
    fn math_is_ordinary_text_for_arrows_and_deletion(cx: &mut gpui::TestAppContext) {
        // ADR 0031: mdoc has no math editor, so the caret enters math and
        // Backspace edits it character by character instead of deferring.
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            e.set_markdown_style(crate::markdown_syntax::search_style(), cx);
            e.set_text("a $x$ b", cx);
            e.set_cursor(3, cx);
            e.right(&crate::Right, window, cx);
            assert_eq!(e.cursor(), 4, "the caret moves inside the formula");
            e.set_cursor(5, cx);
            e.backspace(&Backspace, window, cx);
            assert_eq!(e.text(), "a $x b", "only the closing `$` is deleted");

            e.set_text("$$\nx\n$$\nafter", cx);
            e.set_cursor(e.text().find("after").unwrap(), cx);
            e.backspace(&Backspace, window, cx);
            assert_eq!(e.text(), "$$\nx\n$$after", "the block stays");
        });
    }

    #[gpui::test]
    fn metadata_checkpoint_preserves_source_selection_and_emits_only_transactions(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        let changed = std::rc::Rc::new(std::cell::Cell::new(0));
        let events = changed.clone();
        let _subscription = cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &EditorEvent, _| {
                if matches!(event, EditorEvent::Changed) {
                    events.set(events.get() + 1);
                }
            })
        });
        editor.update(cx, |e, cx| {
            e.set_text("я😀 source", cx);
            e.selected_range = 2..6;
        });
        cx.run_until_parked();
        let before = changed.get();
        editor.update_in(cx, |e, window, cx| {
            let revision = e.revision();
            let history = e.history_id();
            assert!(!e.checkpoint_metadata(revision + 1, cx));
            assert_eq!(e.history_id(), history);
            assert!(e.checkpoint_metadata(revision, cx));
            assert_eq!(e.text(), "я😀 source");
            assert_eq!(e.selection(), 2..6);
            assert_ne!(e.history_id(), history);
            e.undo(&Undo, window, cx);
            assert_eq!(e.history_id(), history);
            assert_eq!(e.text(), "я😀 source");
            e.redo(&Redo, window, cx);
            assert_eq!(e.text(), "я😀 source");
        });
        cx.run_until_parked();
        assert_eq!(changed.get(), before);
    }

    #[gpui::test]
    fn read_only_blocks_typing_paste_cut_and_undo_but_copies(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Anna\n- [ ] task", cx);
            editor.set_read_only(true, cx);
            editor.replace_text_in_range(None, "x", window, cx);
            cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
            editor.paste(&Paste, window, cx);
            editor.select_all(&SelectAll, window, cx);
            editor.indent(&Indent, window, cx);
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), "Anna\n- [ ] task");
            editor.copy_plain(window, cx);
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .contains("Anna")
            );
            // The host still sets the text it shows.
            editor.set_text("Boris", cx);
            assert_eq!(editor.text(), "Boris");
        });
    }
    #[gpui::test]
    fn copy_metadata_tags_every_copy_and_cut(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Anna Petrova", cx);
            editor.select_all(&SelectAll, window, cx);
            editor.copy(&Copy, window, cx);
            assert_eq!(cx.read_from_clipboard().unwrap().metadata(), None);
            editor.set_copy_metadata(Some("tag".into()));
            type CopyFn = fn(&mut EditorState, &mut Window, &mut Context<EditorState>);
            let copies: [CopyFn; 3] = [
                |e, w, cx| e.copy(&Copy, w, cx),
                |e, w, cx| e.copy_plain(w, cx),
                |e, w, cx| e.cut(&Cut, w, cx),
            ];
            for copy in copies {
                cx.write_to_clipboard(ClipboardItem::new_string("before".into()));
                editor.select_all(&SelectAll, window, cx);
                copy(editor, window, cx);
                let item = cx.read_from_clipboard().unwrap();
                assert_eq!(item.text().as_deref(), Some("Anna Petrova"));
                assert_eq!(item.metadata().map(String::as_str), Some("tag"));
                editor.set_text("Anna Petrova", cx);
            }
            editor.set_copy_metadata(None);
            editor.select_all(&SelectAll, window, cx);
            editor.copy(&Copy, window, cx);
            assert_eq!(cx.read_from_clipboard().unwrap().metadata(), None);
        });
    }
    #[gpui::test]
    fn unicode_typing_selection_and_clipboard_keep_undo_groups(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("", cx);
            editor.replace_text_in_range(None, "я", window, cx);
            editor.replace_text_in_range(None, "👩‍💻", window, cx);
            assert_eq!(editor.selection(), editor.text().len()..editor.text().len());
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), "");
            editor.redo(&Redo, window, cx);
            assert_eq!(editor.text(), "я👩‍💻");

            editor.select_all(&SelectAll, window, cx);
            editor.copy_plain(window, cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("я👩‍💻")
            );
            cx.write_to_clipboard(ClipboardItem::new_string("paste\n日本語".into()));
            editor.paste(&Paste, window, cx);
            assert_eq!(editor.text(), "paste\n日本語");
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), "я👩‍💻");
            // A structural edit after undo starts a new branch; redo cannot
            // resurrect the discarded paste.
            editor.replace_range(0..editor.text().len(), "branch", cx);
            editor.redo(&Redo, window, cx);
            assert_eq!(editor.text(), "branch");
        });
    }

    #[gpui::test]
    fn undo_reaches_the_step_before_a_composition(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Anna", cx);
            // A host's step (a decision), then a dead-key composition.
            editor.replace_range(0..4, "PERSON_1", cx);
            let decided = editor.history_id();
            editor.set_cursor(editor.text().len(), cx);
            editor.replace_and_mark_text_in_range(None, "´", None, window, cx);
            editor.replace_text_in_range(None, "é", window, cx);
            assert_eq!(editor.text(), "PERSON_1é");
            let presses = editor.history_path().steps_to(decided).unwrap();
            assert!(presses < 0);
            for _ in 0..presses.unsigned_abs() {
                editor.undo(&Undo, window, cx);
            }
            assert_eq!(editor.history_id(), decided);
            assert_eq!(editor.text(), "PERSON_1");
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.text(), "Anna");
        });
    }

    #[gpui::test]
    fn ime_ranges_follow_composition_and_source_revisions(cx: &mut gpui::TestAppContext) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_text("a😀б", cx);
            let mut actual = None;
            assert_eq!(
                editor
                    .text_for_range(1..3, &mut actual, window, cx)
                    .as_deref(),
                Some("😀")
            );
            assert_eq!(actual, Some(1..3));
            editor.replace_and_mark_text_in_range(Some(1..3), "漢", None, window, cx);
            assert_eq!(editor.text(), "a漢б");
            assert_eq!(editor.marked_text_range(window, cx), Some(1..2));
            editor.replace_and_mark_text_in_range(None, "漢字", None, window, cx);
            assert_eq!(editor.text(), "a漢字б");
            assert_eq!(editor.marked_text_range(window, cx), Some(1..3));
            editor.replace_text_in_range(None, "日本", window, cx);
            assert_eq!(editor.text(), "a日本б");
            assert_eq!(editor.marked_text_range(window, cx), None);
            assert_eq!(
                editor.selected_text_range(false, window, cx).unwrap().range,
                3..3
            );

            // Conversion anchors must follow undo and programmatic loads,
            // rather than retaining offsets into an earlier source revision.
            editor.undo(&Undo, window, cx);
            assert_eq!(
                editor
                    .text_for_range(1..3, &mut actual, window, cx)
                    .as_deref(),
                Some("漢字")
            );
            editor.set_text("😀x", cx);
            assert_eq!(
                editor
                    .text_for_range(0..2, &mut actual, window, cx)
                    .as_deref(),
                Some("😀")
            );
            assert_eq!(actual, Some(0..2));
        });
    }
}
