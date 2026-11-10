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
///
/// An emphasis marker (`*`, `**`, `_`, `__`) counts only as the whole run
/// of its character there: `**` beside a selection is bold, not two italic
/// markers, so Italic on bold wraps it into `***bold italic***` and Italic
/// on that takes one `*` back off (ADR 0110). A run of three is both and
/// gives up whichever is asked for.
pub fn unwrap_edit(source: &str, sel: Range<usize>, open: &str, close: &str) -> Option<WrapEdit> {
    let (ol, cl) = (open.len(), close.len());
    let sel_text = &source[sel.clone()];
    if sel_text.len() >= ol + cl
        && sel_text.starts_with(open)
        && sel_text.ends_with(close)
        && emphasis_run_fits(source, open, sel.start, sel.end - cl)
    {
        // `**foo**` selected → strip the markers inside the selection.
        let inner = source[sel.start + ol..sel.end - cl].to_string();
        Some((sel.clone(), inner, sel.start..sel.end - ol - cl))
    } else if !sel.is_empty()
        && source[..sel.start].ends_with(open)
        && source[sel.end..].starts_with(close)
        && emphasis_run_fits(source, open, sel.start - ol, sel.end)
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

/// Whether the emphasis markers `open` found at `opening` and `closing`
/// are whole runs there, or runs of three (bold and italic). The shorter
/// run decides. Markers of other kinds always fit.
fn emphasis_run_fits(source: &str, open: &str, opening: usize, closing: usize) -> bool {
    let bytes = source.as_bytes();
    let Some(&c) = bytes.get(opening).filter(|c| matches!(c, b'*' | b'_')) else {
        return true;
    };
    if open.bytes().any(|o| o != c) {
        return true;
    }
    // The run of `c` holding the byte at `at`.
    let run = |at: usize| {
        let left = bytes[..at].iter().rev().take_while(|&&b| b == c).count();
        let right = bytes[at..].iter().take_while(|&&b| b == c).count();
        left + right
    };
    let run = run(opening).min(run(closing));
    run == open.len() || run == 3
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
    // becomes the new list's start: rewrite it to 1, which shows as `a.`.
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
        assert_eq!(
            toggle_wrap_pair("<u>x</u>", 3..4, "<u>", "</u>"),
            Some((0..8, "x".into(), 0..1))
        );
        assert_eq!(toggle_wrap_pair("ab", 1..1, "*", "*"), None);
    }

    /// The text and the selected text after a toggle.
    fn toggled(source: &str, sel: Range<usize>, marker: &str) -> (String, String) {
        let (range, text, sel) = toggle_wrap_pair(source, sel, marker, marker).unwrap();
        let mut out = source.to_string();
        out.replace_range(range, &text);
        let selected = out[sel].to_string();
        (out, selected)
    }

    fn word(source: &str, w: &str) -> Range<usize> {
        let at = source.find(w).unwrap();
        at..at + w.len()
    }

    #[test]
    fn italic_on_bold_is_bold_italic_and_back() {
        let s = "раз **два** три";
        let (out, selected) = toggled(s, word(s, "два"), "*");
        assert_eq!(
            (out.as_str(), selected.as_str()),
            ("раз ***два*** три", "два")
        );
        let (out, selected) = toggled(&out, word(&out, "два"), "*");
        assert_eq!(
            (out.as_str(), selected.as_str()),
            ("раз **два** три", "два")
        );
        // Bold on italic, and Bold off bold italic.
        let s = "раз *два* три";
        let (out, _) = toggled(s, word(s, "два"), "**");
        assert_eq!(out, "раз ***два*** три");
        let (out, _) = toggled(&out, word(&out, "два"), "**");
        assert_eq!(out, "раз *два* три");
        // Bold and italic still come off their own markers.
        assert_eq!(toggled("**два**", 2..8, "**").0, "два");
        assert_eq!(toggled("*два*", 1..7, "*").0, "два");
        // The markers selected with the word: Italic adds, not strips.
        assert_eq!(toggled("**два**", 0..10, "*").0, "***два***");
        // Underscores read the same way.
        let s = "__два__";
        let (out, _) = toggled(s, word(s, "два"), "_");
        assert_eq!(out, "___два___");
        assert_eq!(toggled(&out, word(&out, "два"), "_").0, "__два__");
        assert_eq!(toggled(&out, word(&out, "два"), "__").0, "_два_");
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
