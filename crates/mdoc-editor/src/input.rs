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

use super::{
    Backspace, Bold, Code, Copy, Cut, Delete, EditorEvent, EditorState, Indent, Italic, Newline,
    Outdent, Paste, Redo, ShowCharacterPalette, Strike, TAB_INDENT, Underline, Undo,
    caret_off_marker_line, line_pos, markdown_syntax,
};

/// Cap on undo history (full snapshots) to bound memory.
const UNDO_LIMIT: usize = 256;

/// A restorable editor state, for undo/redo. Stores the caret offset (not a
/// selection), so undo/redo place the caret rather than re-selecting text.
#[derive(Clone)]
pub(super) struct Snapshot {
    content: String,
    caret: usize,
    history_id: u64,
}

/// The last edit's kind, for coalescing a run of edits into one undo step.
/// `Insert(end)` is a single-grapheme insert whose caret ends at `end`.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum EditKind {
    Insert(usize),
    Delete,
    Other,
}

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
        self.last_edit = EditKind::Other;
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
        self.last_edit = EditKind::Other;
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
        self.last_edit = EditKind::Other;
        self.emit_changed(cx);
        cx.notify();
    }

    /// Replace the whole document; resets the caret to the start.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content_gen += 1;
        self.content = text.into();
        // Never park the loaded caret on a collapsed marker line (`<!-- table/
        // math:… -->`): the first focus would reveal it raw mid-interaction.
        // This is the ONE passive parking path — every other caret write is a
        // deliberate placement (which SHOULD reveal markers for editing).
        let caret = caret_off_marker_line(&self.content, 0);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        // A programmatic load isn't undoable to the prior document.
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.next_history_id += 1;
        self.history_id = self.next_history_id;
        self.pending_changes.clear();
        self.last_transaction = None;
        self.position_cache.borrow_mut().clear();
        self.last_edit = EditKind::Other;
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
        // Inside a table, a raw newline would split the row's `| … |` markup.
        // Enter instead moves to the cell directly below (next row, same column,
        // spreadsheet-style); from the last row it exits onto a fresh line below
        // the table.
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
            self.selected_range = end..end;
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        // List auto-continuation: Enter on a list/task item opens the next item
        // (same marker + indent; ordered numbers increment); Enter on an *empty*
        // item removes the marker, exiting the list. Only with a collapsed
        // selection — a selection is just replaced by the newline.
        if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
            let line_end = self.content[line_start..]
                .find('\n')
                .map_or(self.content.len(), |i| line_start + i);
            let line = &self.content[line_start..line_end];
            if let Some((prefix_len, indent, ordered, num)) = markdown_syntax::list_prefix(line) {
                let task = markdown_syntax::task_prefix(line);
                let content_start = task.map_or(prefix_len, |(l, ..)| l);
                let empty = line.get(content_start..).unwrap_or("").trim().is_empty();
                let cont = if empty {
                    None
                } else {
                    let ws = &line[..indent];
                    let bullet = line.as_bytes()[indent] as char;
                    Some(if task.is_some() {
                        format!("\n{ws}{bullet} [ ] ")
                    } else if ordered {
                        format!("\n{ws}{}. ", num + 1)
                    } else {
                        format!("\n{ws}{bullet} ")
                    })
                };
                match cont {
                    // Empty item: clear the marker, leaving an empty line.
                    None => {
                        self.selected_range = line_start..line_end;
                        self.replace_text_in_range(None, "", window, cx);
                    }
                    Some(text) => self.replace_text_in_range(None, &text, window, cx),
                }
                return;
            }
        }
        self.replace_text_in_range(None, "\n", window, cx);
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
    fn toggle_wrap_pair(&mut self, open: &str, close: &str, cx: &mut Context<Self>) {
        let sel = self.selected_range.clone();
        if sel.start >= sel.end {
            return;
        }
        let (ol, cl) = (open.len(), close.len());
        let sel_text = &self.content[sel.clone()];
        let (range, new, new_sel) =
            if sel_text.len() >= ol + cl && sel_text.starts_with(open) && sel_text.ends_with(close)
            {
                // `**foo**` selected → strip the markers inside the selection.
                let inner = self.content[sel.start + ol..sel.end - cl].to_string();
                (sel.clone(), inner, sel.start..sel.end - ol - cl)
            } else if self.content[..sel.start].ends_with(open)
                && self.content[sel.end..].starts_with(close)
            {
                // `foo` selected with the markers just outside → strip them.
                (
                    sel.start - ol..sel.end + cl,
                    sel_text.to_string(),
                    sel.start - ol..sel.end - ol,
                )
            } else {
                // Plain → wrap.
                (
                    sel.clone(),
                    format!("{open}{sel_text}{close}"),
                    sel.start + ol..sel.end + ol,
                )
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
        // In a table, Tab moves to the next cell rather than indenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(true) {
                self.move_to(offset, cx);
            }
            return;
        }
        let cursor = self.cursor_offset();
        let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.content[line_start..]
            .find('\n')
            .map_or(self.content.len(), |i| line_start + i);
        let line = &self.content[line_start..line_end];
        let item = markdown_syntax::list_prefix(line);
        let is_item = item.is_some() || markdown_syntax::blockquote_prefix(line).is_some();
        let indent = " ".repeat(TAB_INDENT);
        if !is_item {
            self.replace_text_in_range(None, &indent, window, cx);
            return;
        }
        // Indenting an ordered item starts a NESTED list, so its number
        // becomes the new list's start: rewrite it to 1. (Both views
        // renumber the items after it, so only the start digit matters.)
        let (range, new_text) = match item {
            Some((_, ws, true, _)) => {
                let de = ws + line[ws..].bytes().take_while(u8::is_ascii_digit).count();
                (
                    line_start..line_start + de,
                    format!("{indent}{}1", &line[..ws]),
                )
            }
            _ => (line_start..line_start, indent),
        };
        self.record_edit(&range, &new_text);
        let delta = new_text.len() as isize - (range.end - range.start) as isize;
        self.content.replace_range(range.clone(), &new_text);
        let caret = (cursor as isize + delta).max(line_start as isize) as usize;
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
        // In a table, Shift+Tab moves to the previous cell rather than outdenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(false) {
                self.move_to(offset, cx);
            }
            return;
        }
        let cursor = self.cursor_offset();
        let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line = &self.content[line_start..];
        let removed = if line.starts_with('\t') {
            1
        } else {
            line.bytes()
                .take(TAB_INDENT)
                .take_while(|b| *b == b' ')
                .count()
        };
        if removed == 0 {
            return;
        }
        let range = line_start..line_start + removed;
        self.record_edit(&range, "");
        self.content.replace_range(range.clone(), "");
        let caret = cursor.saturating_sub(removed).max(line_start);
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
            Some(text) => {
                // Normalize foreign line endings — a Windows/browser copy
                // carries \r\n (or bare \r), and a literal \r in the buffer
                // garbles rendering + desyncs the \n-based column math.
                let mut text = if text.contains('\r') {
                    text.replace("\r\n", "\n").replace('\r', "\n")
                } else {
                    text
                };
                // Inside a table cell, a raw paste of newlines/pipes would
                // break the `| … |` row markup (Enter is guarded the same
                // way): flatten newlines and escape pipes so the paste stays
                // one cell's content.
                if self.caret_in_table() && text.contains(['\n', '|']) {
                    // Unescape-then-escape so text already carrying `\|`
                    // doesn't double up into `\\|` (an escaped backslash
                    // followed by a live separator).
                    text = text
                        .trim_end_matches('\n')
                        .replace('\n', " ")
                        .replace("\\|", "|")
                        .replace('|', "\\|");
                } else if self.markdown_style.is_some() && text.contains("$$") {
                    // Words-attached `$$` fences / words-mixed pairs in pasted
                    // text split onto their own lines (issue #54) — same
                    // normalization typing gets.
                    if let std::borrow::Cow::Owned(n) = crate::syntax::normalize_math_fences(&text)
                    {
                        text = n;
                    }
                }
                self.replace_text_in_range(None, &text, window, cx);
            }
            None => cx.propagate(),
        }
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let range = self.copy_range();
            // Ordered markers copy at their DISPLAYED positions (still digit
            // markdown), so a paste counts the way the screen did.
            let text = if self.markdown_style.is_some() {
                markdown_syntax::renumber_copy(&self.content, range)
            } else {
                self.content[range].to_string()
            };
            self.write_clipboard(text, cx);
        }
    }

    /// Copy the selection as the raw markdown ONLY — no host clipboard
    /// flavors — for pasting literal source into rich surfaces (the context
    /// menu's "Copy as Markdown"). Same selection/renumber rules as `copy`.
    pub(crate) fn copy_plain(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let range = self.copy_range();
            let text = if self.markdown_style.is_some() {
                markdown_syntax::renumber_copy(&self.content, range)
            } else {
                self.content[range].to_string()
            };
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Write a Copy/Cut payload as plain text.
    pub(super) fn write_clipboard(&self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.undo_stack.pop() {
            self.pending_changes.push(super::HistoryChange {
                before: self.history_id,
                after: prev.history_id,
                edits: Vec::new(),
            });
            self.redo_stack.push(self.snapshot());
            let changed = self.content != prev.content;
            self.restore(prev);
            if changed {
                self.emit_changed(cx);
            } else {
                self.emit_transaction(cx);
            }
            self.last_edit = EditKind::Other;
            cx.notify();
        }
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.redo_stack.pop() {
            self.pending_changes.push(super::HistoryChange {
                before: self.history_id,
                after: next.history_id,
                edits: Vec::new(),
            });
            self.undo_stack.push(self.snapshot());
            let changed = self.content != next.content;
            self.restore(next);
            if changed {
                self.emit_changed(cx);
            } else {
                self.emit_transaction(cx);
            }
            self.last_edit = EditKind::Other;
            cx.notify();
        }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            content: self.content.clone(),
            // The forward caret (selection end), so undoing a backspace lands the
            // caret after the restored text rather than inside it.
            caret: self.selected_range.end,
            history_id: self.history_id,
        }
    }

    pub(super) fn restore(&mut self, s: Snapshot) {
        self.content_gen += 1;
        self.content = s.content;
        self.history_id = s.history_id;
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
        let kind = if new_text.is_empty() {
            EditKind::Delete
        } else if range.start == range.end
            && new_text != "\n"
            && new_text.graphemes(true).count() == 1
        {
            EditKind::Insert(range.start + new_text.len())
        } else {
            EditKind::Other
        };
        let coalesce = match (self.last_edit, kind) {
            (EditKind::Insert(end), EditKind::Insert(_)) => end == range.start,
            (EditKind::Delete, EditKind::Delete) => true,
            _ => false,
        };
        self.begin_history(
            coalesce,
            vec![super::SourceEdit {
                range: range.clone(),
                new_len: new_text.len(),
            }],
        );
        self.last_edit = kind;
        // A keystroke is one typed grapheme (incl. typed over a selection — that's
        // an auto-pair "wrap") or a single-char backspace. Multi-char edits (paste,
        // table ops, …) are not, so auto-pairing skips them.
        self.last_edit_keystroke = (new_text != "\n" && new_text.graphemes(true).count() == 1)
            || (new_text.is_empty() && self.content[range.clone()].graphemes(true).count() == 1);
    }

    fn begin_history(&mut self, coalesce: bool, edits: Vec<super::SourceEdit>) {
        if !coalesce {
            self.undo_stack.push(self.snapshot());
            if self.undo_stack.len() > UNDO_LIMIT {
                self.undo_stack.remove(0);
            }
        }
        self.redo_stack.clear();
        let before = self.history_id;
        self.next_history_id += 1;
        self.history_id = self.next_history_id;
        self.pending_changes.push(super::HistoryChange {
            before,
            after: self.history_id,
            edits,
        });
    }

    pub fn history_id(&self) -> u64 {
        self.history_id
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
        if !self.pending_changes.is_empty() {
            let mut retained: Vec<_> = self
                .undo_stack
                .iter()
                .chain(&self.redo_stack)
                .map(|s| s.history_id)
                .chain(std::iter::once(self.history_id))
                .collect();
            retained.sort_unstable();
            retained.dedup();
            let transaction = std::sync::Arc::new(super::EditorTransaction {
                revision: self.content_gen,
                changes: std::mem::take(&mut self.pending_changes),
                retained,
            });
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
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    pub(super) fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
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
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        // Composition updates retain the existing editor undo contract: only
        // the commit records a snapshot. Hosts still receive every exact change.
        self.content_gen += 1;
        let before = self.history_id;
        self.next_history_id += 1;
        self.history_id = self.next_history_id;
        self.pending_changes.push(super::HistoryChange {
            before,
            after: self.history_id,
            edits: vec![super::SourceEdit {
                range: range.clone(),
                new_len: new_text.len(),
            }],
        });
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
        let p = line_pos(line, map, self.display_col(row, col), lh)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SelectAll;

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
