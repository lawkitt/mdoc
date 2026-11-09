//! Formatting toggles (bold, italic, code, strike, underline, highlight) and
//! indent / outdent.

use std::ops::Range;

use mdoc_markdown::lines::{blockquote_prefix, list_prefix};

/// A wrap toggle's edit: the replaced range, its text and the selection
/// after it.
pub type WrapEdit = (Range<usize>, String, Range<usize>);

/// Toggle an open/close marker pair (`<u>`/`</u>`, or a symmetric `**`)
/// around the selection `sel`. `None` (no-op) on an empty or blank
/// selection. Unwraps when the selection is already wrapped (markers just
/// inside or just outside it), otherwise wraps — keeping the same text
/// selected so presses toggle. Whitespace at either end stays outside new
/// markers: CommonMark does not render `**delta **` as bold (#77). Existing
/// markers unwrap as selected (`<u> delta </u>`) or once the whitespace is
/// trimmed (`**delta** `).
pub fn toggle_wrap_pair(
    source: &str,
    sel: Range<usize>,
    open: &str,
    close: &str,
) -> Option<WrapEdit> {
    if sel.start >= sel.end {
        return None;
    }
    let text = &source[sel.clone()];
    let start = sel.start + (text.len() - text.trim_start().len());
    let trimmed = start..start + text.trim().len();
    let unwrap = unwrap_edit(source, sel.clone(), open, close)
        .or_else(|| unwrap_edit(source, trimmed.clone(), open, close));
    match unwrap {
        Some(edit) => Some(edit),
        None if trimmed.is_empty() => None,
        None => {
            // Plain → wrap.
            let ol = open.len();
            Some((
                trimmed.clone(),
                format!("{open}{}{close}", &source[trimmed.clone()]),
                trimmed.start + ol..trimmed.end + ol,
            ))
        }
    }
}

/// The edit that strips `open`/`close` from `sel`, if they sit just
/// inside or just outside it: (range, replacement, new selection).
pub fn unwrap_edit(source: &str, sel: Range<usize>, open: &str, close: &str) -> Option<WrapEdit> {
    let (ol, cl) = (open.len(), close.len());
    let sel_text = &source[sel.clone()];
    if sel_text.len() >= ol + cl && sel_text.starts_with(open) && sel_text.ends_with(close) {
        // `**foo**` selected → strip the markers inside the selection.
        let inner = source[sel.start + ol..sel.end - cl].to_string();
        Some((sel.clone(), inner, sel.start..sel.end - ol - cl))
    } else if !sel.is_empty()
        && source[..sel.start].ends_with(open)
        && source[sel.end..].starts_with(close)
    {
        // `foo` selected with the markers just outside → strip them.
        Some((
            sel.start - ol..sel.end + cl,
            sel_text.to_string(),
            sel.start - ol..sel.end - ol,
        ))
    } else {
        None
    }
}

/// What Tab does outside a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndentEdit {
    /// Not a list or quote item: type these spaces over the selection.
    Spaces(String),
    /// Indent the caret's item: replace `range` with `text`; the caret lands
    /// at `caret`.
    Item {
        range: Range<usize>,
        text: String,
        caret: usize,
    },
}

/// Tab at `cursor`: on a list/quote item, indent the whole item one level
/// (`tab_indent` spaces at the line start, caret shifts with it); elsewhere
/// insert that many spaces at the caret (replacing any selection).
pub fn indent(source: &str, cursor: usize, tab_indent: usize) -> IndentEdit {
    let line_start = source[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |i| line_start + i);
    let line = &source[line_start..line_end];
    let item = list_prefix(line);
    let is_item = item.is_some() || blockquote_prefix(line).is_some();
    let indent = " ".repeat(tab_indent);
    if !is_item {
        return IndentEdit::Spaces(indent);
    }
    // Indenting an ordered item starts a NESTED list, so its number
    // becomes the new list's start: rewrite it to 1. (Both views
    // renumber the items after it, so only the start digit matters.)
    let (range, text) = match item {
        Some((_, ws, true, _)) => {
            let de = ws + line[ws..].bytes().take_while(u8::is_ascii_digit).count();
            (
                line_start..line_start + de,
                format!("{indent}{}1", &line[..ws]),
            )
        }
        _ => (line_start..line_start, indent),
    };
    let delta = text.len() as isize - (range.end - range.start) as isize;
    let caret = (cursor as isize + delta).max(line_start as isize) as usize;
    IndentEdit::Item { range, text, caret }
}

/// ⇧Tab at `cursor`: remove up to `tab_indent` leading spaces (or one
/// leading tab) from the caret's line. The removed range and the caret
/// after it; `None` when the line has no indent.
pub fn outdent(source: &str, cursor: usize, tab_indent: usize) -> Option<(Range<usize>, usize)> {
    let line_start = source[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line = &source[line_start..];
    let removed = if line.starts_with('\t') {
        1
    } else {
        line.bytes()
            .take(tab_indent)
            .take_while(|b| *b == b' ')
            .count()
    };
    if removed == 0 {
        return None;
    }
    let caret = cursor.saturating_sub(removed).max(line_start);
    Some((line_start..line_start + removed, caret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_toggles_and_keeps_edge_whitespace_outside() {
        assert_eq!(
            toggle_wrap_pair("alpha delta gamma", 6..12, "**", "**"),
            Some((6..11, "**delta**".into(), 8..13))
        );
        let s = "alpha **delta** gamma";
        assert_eq!(
            toggle_wrap_pair(s, 8..13, "**", "**"),
            Some((6..15, "delta".into(), 6..11))
        );
        assert_eq!(
            toggle_wrap_pair("<u>x</u>", 0..8, "<u>", "</u>"),
            Some((0..8, "x".into(), 0..1))
        );
        assert_eq!(toggle_wrap_pair("a  b", 1..3, "*", "*"), None);
        assert_eq!(toggle_wrap_pair("ab", 1..1, "*", "*"), None);
    }

    #[test]
    fn indent_nests_items_and_restarts_numbers() {
        assert_eq!(
            indent("- a", 3, 4),
            IndentEdit::Item {
                range: 0..0,
                text: "    ".into(),
                caret: 7
            }
        );
        assert_eq!(
            indent("3. a", 4, 4),
            IndentEdit::Item {
                range: 0..1,
                text: "    1".into(),
                caret: 8
            }
        );
        assert_eq!(indent("plain", 2, 4), IndentEdit::Spaces("    ".into()));
    }

    #[test]
    fn outdent_removes_one_level() {
        assert_eq!(outdent("      - a", 9, 4), Some((0..4, 5)));
        assert_eq!(outdent("\t- a", 4, 4), Some((0..1, 3)));
        assert_eq!(outdent("- a", 3, 4), None);
    }
}
