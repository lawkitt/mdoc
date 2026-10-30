//! Line-level Markdown prefixes: ATX headings, thematic breaks, blockquotes,
//! and list and task markers. Each answers for one source line.

/// ATX heading depth (1–6) if `line` is a heading: 1–6 leading `#` followed by
/// a space or end-of-line. `None` otherwise.
pub fn heading_level(line: &str) -> Option<u8> {
    let b = line.as_bytes();
    let mut n = 0;
    while n < b.len() && b[n] == b'#' {
        n += 1;
    }
    ((1..=6).contains(&n) && (n == b.len() || b[n] == b' ')).then_some(n as u8)
}

/// True if `line` is a thematic break (horizontal rule): three or more of the
/// same `-`, `*`, or `_`, separated only by optional spaces — e.g. `---`, `***`,
/// `- - -`. The editor paints a divider in its place (reveal-on-caret). Matches
/// CommonMark, ignoring the rare setext-underline ambiguity this app doesn't use.
pub fn thematic_break(line: &str) -> bool {
    let mut chars = line.bytes().filter(|b| !b.is_ascii_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, b'-' | b'*' | b'_') {
        return false;
    }
    let mut count = 1;
    for b in chars {
        if b != first {
            return false;
        }
        count += 1;
    }
    count >= 3
}

/// Byte length of a blockquote prefix: its `>` markers (nested `>>` too),
/// each with one optional following space. `None` if `line` is no quote.
pub fn blockquote_prefix(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    if b.first() != Some(&b'>') {
        return None;
    }
    let mut p = 0;
    while p < b.len() && b[p] == b'>' {
        p += 1;
        if p < b.len() && b[p] == b' ' {
            p += 1;
        }
    }
    Some(p)
}

/// If `line` is a list item, return `(prefix_len, indent, ordered, number)`.
/// `prefix_len` is the byte length of the leading whitespace plus the marker
/// (a bullet `-`/`*`/`+`, or digits then `.`/`)`) plus one space; `indent` is the
/// leading-whitespace length (nesting depth); `ordered`/`number` describe an
/// ordered item. The editor hides this prefix and paints a bullet/number,
/// revealing the raw prefix on caret.
pub fn list_prefix(line: &str) -> Option<(usize, usize, bool, u32)> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let indent = i;
    // Unordered: `-`/`*`/`+` then a space.
    if i < b.len() && matches!(b[i], b'-' | b'*' | b'+') && b.get(i + 1) == Some(&b' ') {
        return Some((i + 2, indent, false, 0));
    }
    // Ordered: one or more digits, then `.` or `)`, then a space.
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > ds && matches!(b.get(i), Some(b'.') | Some(b')')) && b.get(i + 1) == Some(&b' ') {
        let num = line[ds..i].parse::<u32>().unwrap_or(1);
        return Some((i + 2, indent, true, num));
    }
    None
}

/// If `line` is a GFM task item — a list item whose body starts with `[ ]`,
/// `[x]`, or `[X]` then a space — return `(prefix_len, indent, checked)`, where
/// `prefix_len` covers the list marker plus the checkbox. The editor hides this
/// prefix and paints a ☐/☑ box (parity with the reading view).
pub fn task_prefix(line: &str) -> Option<(usize, usize, bool)> {
    let (list_len, indent, ..) = list_prefix(line)?;
    let rest = &line.as_bytes()[list_len..];
    if rest.len() >= 4
        && rest[0] == b'['
        && rest[2] == b']'
        && rest[3] == b' '
        && matches!(rest[1], b' ' | b'x' | b'X')
    {
        return Some((list_len + 4, indent, matches!(rest[1], b'x' | b'X')));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thematic_break_detection() {
        for s in ["---", "***", "___", "- - -", "  ---  ", "----"] {
            assert!(thematic_break(s), "{s:?} should be a rule");
        }
        for s in ["--", "**", "", "text", "---x", "- -- text", "===", "> ---"] {
            assert!(!thematic_break(s), "{s:?} should not be a rule");
        }
    }

    #[test]
    fn heading_level_detection() {
        assert_eq!(heading_level("## Hi"), Some(2));
        assert_eq!(heading_level("#notaheading"), None);
    }

    #[test]
    fn blockquote_prefix_detection() {
        assert_eq!(blockquote_prefix("> quote"), Some(2));
        assert_eq!(blockquote_prefix(">no space"), Some(1));
        assert_eq!(blockquote_prefix(">> nested"), Some(3));
        assert_eq!(blockquote_prefix("not a quote"), None);
    }

    #[test]
    fn list_prefix_detection() {
        assert_eq!(list_prefix("- item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("* item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("+ item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("  - nested"), Some((4, 2, false, 0)));
        assert_eq!(list_prefix("3. third"), Some((3, 0, true, 3)));
        assert_eq!(list_prefix("  10) tenth"), Some((6, 2, true, 10)));
        // Not lists: italic, a bare dash, indented prose.
        assert_eq!(list_prefix("*italic*"), None);
        assert_eq!(list_prefix("-no space"), None);
        assert_eq!(list_prefix("  just indented"), None);
    }

    #[test]
    fn task_prefix_detection() {
        assert_eq!(task_prefix("- [ ] todo"), Some((6, 0, false)));
        assert_eq!(task_prefix("- [x] done"), Some((6, 0, true)));
        assert_eq!(task_prefix("- [X] done"), Some((6, 0, true)));
        assert_eq!(task_prefix("  - [ ] nested"), Some((8, 2, false)));
        // A plain list item is not a task.
        assert_eq!(task_prefix("- item"), None);
    }
}
