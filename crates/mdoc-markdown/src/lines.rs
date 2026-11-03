//! Line-level Markdown: ATX headings, thematic breaks, blockquotes, list and
//! task markers, footnote definitions and raw HTML lines, each for one source
//! line, plus list numbering and nesting over a run of lines.

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

/// Byte length of the `[^label]:` prefix if `line` is a footnote definition, so
/// the editor can render the whole line muted (the reading view does the same).
/// `None` otherwise.
pub fn footnote_def(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find(']')?;
    // Label must be non-empty and the `]` immediately followed by `:`.
    (close > 0 && rest[close + 1..].starts_with(':')).then_some(2 + close + 2)
}

/// True if `line` looks like a block-level raw HTML line — `<tag …>`, `</tag>`,
/// or `<!-- … -->` at the start (after optional indentation). The editor renders
/// it muted, matching the reading view (raw HTML is shown literally, never run).
pub fn html_block(line: &str) -> bool {
    let t = line.trim_start().as_bytes();
    matches!(t, [b'<', rest, ..] if rest.is_ascii_alphabetic() || *rest == b'/' || *rest == b'!')
}

/// The ordered numbering the editor and the mobile apps paint: `(position, level)`
/// of every ordered list item (see the editor's `ordered_numbers`), plus, for each ordered item, the list it belongs to:
/// siblings share an id (a nested list, or a new list after a break, gets its
/// own), so their bodies can share one marker width. Non-items are `None`.
pub fn ordered_scan(lines: &[&str]) -> (Vec<(u32, usize)>, Vec<Option<u32>>) {
    let mut out = vec![(0u32, 0usize); lines.len()];
    let mut lists = vec![None; lines.len()];
    let mut next_list = 0u32;
    // One entry per open list level: (indent, is_ordered, next number, list id).
    let mut stack: Vec<(usize, bool, u32, u32)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((_, indent, ordered, _)) = list_prefix(line) else {
            // Indented text hangs under the innermost item; anything else —
            // including a blank line — ends the open lists.
            let ws = line
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let hanging =
                !line.trim().is_empty() && stack.last().is_some_and(|&(ind, ..)| ws > ind);
            if !hanging {
                stack.clear();
            }
            continue;
        };
        while stack.last().is_some_and(|&(ind, ..)| ind > indent) {
            stack.pop();
        }
        let depth = stack.len();
        match stack.last_mut() {
            Some((ind, ord, next, list)) if *ind == indent => {
                if *ord != ordered {
                    // A different marker type at the same indent starts a
                    // new list.
                    *ord = ordered;
                    *next = 1;
                    *list = next_list;
                    next_list += 1;
                }
                if ordered {
                    out[i] = (*next, depth - 1);
                    lists[i] = Some(*list);
                    *next += 1;
                }
            }
            _ => {
                stack.push((indent, ordered, 2, next_list));
                out[i] = (1, depth);
                if ordered {
                    lists[i] = Some(next_list);
                }
                next_list += 1;
            }
        }
    }
    (out, lists)
}

/// The structural nesting level of every list item, ordered or not, as
/// [`ordered_scan`] counts it: how many lists are open above the item, so a
/// host can indent by level whatever the source's spaces per level. A blank
/// line or prose ends the open lists; indented text hangs under the innermost
/// item. Non-items are `None`.
pub fn list_levels(lines: &[&str]) -> Vec<Option<usize>> {
    let mut out = vec![None; lines.len()];
    // One indent per open list level.
    let mut stack: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((_, indent, ..)) = list_prefix(line) else {
            let ws = line
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let hanging = !line.trim().is_empty() && stack.last().is_some_and(|&ind| ws > ind);
            if !hanging {
                stack.clear();
            }
            continue;
        };
        while stack.last().is_some_and(|&ind| ind > indent) {
            stack.pop();
        }
        if stack.last() != Some(&indent) {
            stack.push(indent);
        }
        out[i] = Some(stack.len() - 1);
    }
    out
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
    fn list_levels_are_structural() {
        let lines = [
            "- a",
            "    - b",
            "        1. c",
            "    - d",
            "- e",
            "",
            "1. f",
            "text",
        ];
        assert_eq!(
            list_levels(&lines),
            [
                Some(0),
                Some(1),
                Some(2),
                Some(1),
                Some(0),
                None,
                Some(0),
                None
            ]
        );
        // Ordered numbering restarts after a break and per nesting level.
        let (numbers, _) = ordered_scan(&["1. a", "1. b", "   1. c", "1. d", "", "5. e"]);
        assert_eq!(numbers, [(1, 0), (2, 0), (1, 1), (3, 0), (0, 0), (1, 0)]);
    }

    #[test]
    fn footnotes_and_html_lines() {
        assert_eq!(footnote_def("[^1]: note"), Some(5));
        assert_eq!(footnote_def("[^]: no"), None);
        assert!(html_block("<div>"));
        assert!(html_block("  <!-- c -->"));
        assert!(!html_block("a < b"));
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
