//! Enter and ⇧Enter (ADR 0060): Enter makes a new block, so the copied
//! Markdown breaks where the editor shows a break; ⇧Enter is a line break
//! inside the block.

use std::ops::Range;

use mdoc_markdown::lines::{blockquote_prefix, heading_level, list_prefix, task_prefix};

use crate::text::{line_bounds, line_starts, row_col, row_is_blank, trailing_backslashes};

/// Enter's one edit at a collapsed caret outside tables, code and math:
/// the replaced range, its text and the caret's offset into that text.
pub fn enter_edit(source: &str, cursor: usize) -> (Range<usize>, String, usize) {
    let (line_start, line_end) = line_bounds(source, cursor);
    let line = &source[line_start..line_end];
    let quote = blockquote_prefix(line);
    let body_start = line_start + quote.unwrap_or(0);
    let before_blank = source[body_start.min(cursor)..cursor].trim().is_empty();
    let at = |text: String| {
        let len = text.len();
        (cursor..cursor, text, len)
    };
    // The list item this line belongs to: its own, or the item a chain of
    // ⇧Enter hard breaks above continues.
    let item_line = if list_prefix(line).is_some() {
        Some(line_start..line_end)
    } else if quote.is_none() {
        continued_item(source, line_start)
    } else {
        None
    };
    let next_item = |item: &str| {
        let (prefix_len, indent, ordered, num) = list_prefix(item)?;
        let task = task_prefix(item);
        let ws = &item[..indent];
        let bullet = item.as_bytes()[indent] as char;
        Some((
            task.map_or(prefix_len, |(l, ..)| l),
            if task.is_some() {
                format!("\n{ws}{bullet} [ ] ")
            } else if ordered {
                format!("\n{ws}{}. ", num + 1)
            } else {
                format!("\n{ws}{bullet} ")
            },
        ))
    };
    // Enter right after a ⇧Enter break turns that break into Enter's own:
    // the dangling `\` would otherwise stay as literal text.
    if before_blank && let Some(bs) = hard_break_above(source, line_start) {
        let to = cursor.max(body_start);
        let text = match (&item_line, quote) {
            (Some(item), _) => next_item(&source[item.clone()]).map(|(_, t)| t),
            (None, Some(plen)) => {
                let prefix = &line[..plen];
                Some(format!("\n{}\n{prefix}", prefix.trim_end()))
            }
            (None, None) => None,
        }
        .unwrap_or_else(|| "\n\n".to_string());
        let len = text.len();
        // As below: keep a soft-joined next line out of the new paragraph.
        let (row, _) = row_col(source, cursor);
        if item_line.is_none()
            && quote.is_none()
            && source[cursor..line_end].trim().is_empty()
            && !row_is_blank(source, row + 1)
        {
            return (bs..to, text + "\n", len);
        }
        return (bs..to, text, len);
    }
    // List auto-continuation: Enter on a list/task item (or its
    // continuation line) opens the next item (same marker + indent;
    // ordered numbers increment). Enter on an *empty* item exits the list,
    // leaving a blank line so the next text isn't a lazy continuation.
    if let Some(item) = item_line
        && let Some((content_start, text)) = next_item(&source[item.clone()])
    {
        let own = item.start == line_start;
        if own && line.get(content_start..).unwrap_or("").trim().is_empty() {
            return (line_start..line_end, "\n".into(), 1);
        }
        return at(text);
    }
    // Blockquote: Enter starts a new quoted paragraph (`>` blank line,
    // then the same prefix); Enter on an empty quote line exits the
    // quote, taking the bare `>` line above with it.
    if let Some(plen) = quote {
        if !line[plen..].trim().is_empty() {
            let prefix = &line[..plen];
            return at(format!("\n{}\n{prefix}", prefix.trim_end()));
        }
        let prev_start = source[..line_start.saturating_sub(1)]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let prev = &source[prev_start..line_start.saturating_sub(1)];
        let bare_above =
            line_start > 0 && blockquote_prefix(prev).is_some_and(|p| prev[p..].trim().is_empty());
        let from = if bare_above { prev_start } else { line_start };
        return (from..line_end, "\n".into(), 1);
    }
    // A blank line, or the caret at a line's start: one newline adds a
    // row or splits soft-joined lines into paragraphs.
    if before_blank {
        return at("\n".into());
    }
    // At the end of a line soft-joined to the next one, also separate the
    // new paragraph from that line.
    let (row, _) = row_col(source, cursor);
    if source[cursor..line_end].trim().is_empty()
        && line_end < source.len()
        && !row_is_blank(source, row + 1)
    {
        return (cursor..cursor, "\n\n\n".into(), 2);
    }
    at("\n\n".into())
}

/// ⇧Enter's text in prose, lists and quotes, replacing the selection that
/// starts at `start`: a CommonMark hard break (`\` then a newline, the list
/// or quote prefix kept). `None` where ⇧Enter falls back to Enter: headings
/// can't hold a break, and a break needs text before it on the line. Tables,
/// code and math are the caller's: `<br>` in a cell, a plain newline in
/// literal text.
pub fn line_break(source: &str, start: usize) -> Option<String> {
    let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |i| line_start + i);
    let line = &source[line_start..line_end];
    let quote = blockquote_prefix(line);
    let item = list_prefix(line).map(|(l, ..)| l);
    let task = task_prefix(line).map(|(l, ..)| l);
    let body = line_start + task.or(item).or(quote).unwrap_or(0);
    if heading_level(&source[body.min(line_end)..line_end]).is_some()
        || source[body.min(start)..start].trim().is_empty()
    {
        return None;
    }
    let continuation = match (item, quote) {
        (Some(len), _) => " ".repeat(len),
        (None, Some(len)) => line[..len].to_string(),
        // A continuation line keeps its indent (a list item's body).
        (None, None) => line[..line.len() - line.trim_start().len()].to_string(),
    };
    // A `\` already before the caret becomes the break; another one
    // would escape it into a literal backslash.
    let marker = if trailing_backslashes(&source[line_start..start]) % 2 == 1 {
        ""
    } else {
        "\\"
    };
    Some(format!("{marker}\n{continuation}"))
}

/// The offset of the `\` ending the line above `line_start` when that line
/// ends in a hard break (an unescaped `\` after text).
pub fn hard_break_above(source: &str, line_start: usize) -> Option<usize> {
    let prev_end = line_start.checked_sub(1)?;
    let prev_start = source[..prev_end].rfind('\n').map_or(0, |i| i + 1);
    let prev = &source[prev_start..prev_end];
    let quote = blockquote_prefix(prev).unwrap_or(0);
    (trailing_backslashes(prev) % 2 == 1 && !prev[quote..prev.len() - 1].trim().is_empty())
        .then(|| prev_end - 1)
}

/// The list item a line continues through ⇧Enter hard breaks: the byte
/// range of the nearest item line above, reached only through lines that
/// each end in a hard break.
pub fn continued_item(source: &str, mut line_start: usize) -> Option<Range<usize>> {
    while let Some(bs) = hard_break_above(source, line_start) {
        let end = bs + 1;
        let start = source[..end].rfind('\n').map_or(0, |i| i + 1);
        if list_prefix(&source[start..end]).is_some() {
            return Some(start..end);
        }
        line_start = start;
    }
    None
}

/// Enter on a table's last row: the edit that leaves the table onto a fresh
/// line below it (`last_row` is the table's last line). A blank line ends
/// the table: a line right below it would be read as another row (GFM).
pub fn table_exit_edit(source: &str, last_row: usize) -> (Range<usize>, String, usize) {
    let starts = line_starts(source);
    let end = starts.get(last_row + 1).map_or(source.len(), |&s| s - 1);
    (end..end, "\n\n".into(), 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter(source: &str, cursor: usize) -> String {
        let (range, text, _) = enter_edit(source, cursor);
        let mut out = source.to_string();
        out.replace_range(range, &text);
        out
    }

    #[test]
    fn enter_continues_and_ends_lists_and_quotes() {
        assert_eq!(enter("- a", 3), "- a\n- ");
        assert_eq!(enter("3. a", 4), "3. a\n4. ");
        assert_eq!(enter("- [x] a", 7), "- [x] a\n- [ ] ");
        assert_eq!(enter("- a\n- ", 6), "- a\n\n");
        assert_eq!(enter("> a", 3), "> a\n>\n> ");
        assert_eq!(enter("> a\n>\n> ", 8), "> a\n\n");
        assert_eq!(enter("para", 4), "para\n\n");
        assert_eq!(enter("one\ntwo", 3), "one\n\n\n\ntwo");
    }

    #[test]
    fn enter_after_a_hard_break_replaces_it() {
        let s = "- a\\\n  ";
        assert_eq!(enter(s, s.len()), "- a\n- ");
    }

    #[test]
    fn shift_enter_keeps_the_prefix_and_falls_back_on_headings() {
        assert_eq!(line_break("- a", 3).as_deref(), Some("\\\n  "));
        assert_eq!(line_break("> a", 3).as_deref(), Some("\\\n> "));
        assert_eq!(line_break("a\\", 2).as_deref(), Some("\n"));
        assert_eq!(line_break("# a", 3), None);
        assert_eq!(line_break("- ", 2), None);
    }

    #[test]
    fn table_exit_adds_a_blank_line_after_the_grid() {
        let s = "| a |\n| - |\n| b |\nafter";
        assert_eq!(table_exit_edit(s, 2), (17..17, "\n\n".into(), 2));
    }
}
