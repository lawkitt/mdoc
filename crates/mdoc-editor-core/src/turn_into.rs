//! "Turn into": convert the caret's block between text, headings, lists,
//! to-dos, quotes, callouts, code and math.

use std::ops::Range;

use mdoc_markdown::lines::{blockquote_prefix, heading_level, list_prefix, task_prefix};

use crate::Blocks;
use crate::markdown::alert_kind;
use crate::text::{line_end, line_starts};

/// A block kind "Turn into" converts between — flat-markdown natural: each
/// conversion is a line-prefix rewrite (fenced kinds wrap/unwrap the block's
/// lines).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnKind {
    Text,
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Todo,
    Quote,
    Callout,
    Code,
    Math,
}

impl TurnKind {
    pub const ALL: [TurnKind; 11] = [
        TurnKind::Text,
        TurnKind::H1,
        TurnKind::H2,
        TurnKind::H3,
        TurnKind::Bullet,
        TurnKind::Numbered,
        TurnKind::Todo,
        TurnKind::Quote,
        TurnKind::Callout,
        TurnKind::Code,
        TurnKind::Math,
    ];

    /// The menu label.
    pub fn label(self) -> &'static str {
        match self {
            TurnKind::Text => "Text",
            TurnKind::H1 => "Heading 1",
            TurnKind::H2 => "Heading 2",
            TurnKind::H3 => "Heading 3",
            TurnKind::Bullet => "Bulleted list",
            TurnKind::Numbered => "Numbered list",
            TurnKind::Todo => "To-do",
            TurnKind::Quote => "Quote",
            TurnKind::Callout => "Callout",
            TurnKind::Code => "Code block",
            TurnKind::Math => "Math block",
        }
    }
}

/// Strip a line's block dressing (heading hashes, list/todo bullet, ordered
/// number, quote `>`), leaving the text a "Turn into" conversion re-prefixes.
pub fn strip_block_prefix(line: &str) -> &str {
    // Composes the renderer's own recognizers so the strip grammar can't
    // drift from what WYSIWYG classifies (task/list/heading/quote).
    if let Some((p, ..)) = task_prefix(line) {
        return &line[p..];
    }
    if let Some((p, ..)) = list_prefix(line) {
        return &line[p..];
    }
    if let Some(n) = heading_level(line) {
        let after = &line[n as usize..];
        return after.strip_prefix(' ').unwrap_or(after);
    }
    if let Some(p) = blockquote_prefix(line) {
        return &line[p..];
    }
    line.trim_start()
}

/// Join `body` lines each carrying the prefix `p(index)` produces — the
/// assembly half of a "Turn into" conversion.
pub fn prefix_lines(body: &[String], p: impl Fn(usize) -> String) -> String {
    body.iter()
        .enumerate()
        .map(|(i, l)| format!("{}{l}", p(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The contiguous blockquote run containing `row` (first row..=last row),
/// by the renderer's `blockquote_prefix` test.
pub fn quote_run_rows(source: &str, row: usize) -> (usize, usize) {
    let starts = line_starts(source);
    let last_row = starts.len().saturating_sub(1);
    let is_q = |r: usize| blockquote_prefix(&source[starts[r]..line_end(source, r)]).is_some();
    let mut first = row;
    while first > 0 && is_q(first - 1) {
        first -= 1;
    }
    let mut last = row;
    while last < last_row && is_q(last + 1) {
        last += 1;
    }
    (first, last)
}

/// The "Turn into" kind of the block containing `row` — what the flyout
/// shows checked.
pub fn block_kind_at(source: &str, blocks: Blocks, row: usize) -> TurnKind {
    if blocks.math_at(row).is_some() {
        return TurnKind::Math;
    }
    let starts = line_starts(source);
    let Some(&start) = starts.get(row) else {
        return TurnKind::Text;
    };
    // The renderer's own recognizers (task/list/heading/quote/alert), so
    // the checked kind always agrees with what WYSIWYG actually renders.
    let line = &source[start..line_end(source, row)];
    if blocks.in_fence(row) || line.trim_start().starts_with("```") {
        return TurnKind::Code;
    }
    if task_prefix(line).is_some() {
        return TurnKind::Todo;
    }
    match heading_level(line) {
        Some(1) => return TurnKind::H1,
        Some(2) => return TurnKind::H2,
        Some(3) => return TurnKind::H3,
        _ => {}
    }
    if let Some((_, _, ordered, _)) = list_prefix(line) {
        return if ordered {
            TurnKind::Numbered
        } else {
            TurnKind::Bullet
        };
    }
    if blockquote_prefix(line).is_some() {
        // Callout if the caret's contiguous `>`-run carries a VALID
        // `[!KIND]` marker anywhere (marker line or body line).
        let (first, last) = quote_run_rows(source, row);
        for (r, &start) in starts.iter().enumerate().take(last + 1).skip(first) {
            let l = &source[start..line_end(source, r)];
            let p = blockquote_prefix(l).unwrap_or(0);
            if alert_kind(&l[p..]).is_some() {
                return TurnKind::Callout;
            }
        }
        return TurnKind::Quote;
    }
    TurnKind::Text
}

/// A "Turn into" edit: replace `range` with `text`. `caret` is where a
/// fenced kind seats the caret (the body's first line) instead of after the
/// replaced text; `None` leaves it after the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnIntoEdit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: Option<usize>,
}

/// Convert the block containing `row` to `kind`: one edit rewriting the
/// block's lines; fenced kinds (code, math) and quote runs convert as whole
/// blocks. `None` when the block already is `kind`.
pub fn turn_into(source: &str, blocks: Blocks, row: usize, kind: TurnKind) -> Option<TurnIntoEdit> {
    let cur = block_kind_at(source, blocks, row);
    if cur == kind {
        return None;
    }
    let starts = line_starts(source);
    let last_row = starts.len().saturating_sub(1);
    let line_at = |r: usize| source[starts[r]..line_end(source, r)].to_string();
    // The block's line span + its content with the current dressing removed.
    let (first, last, mut body): (usize, usize, Vec<String>) = match cur {
        TurnKind::Code => {
            let (open, close) = blocks.fence_block_rows(source, row);
            let last = close.unwrap_or(last_row);
            let body_end = close.map(|c| c.saturating_sub(1)).unwrap_or(last_row);
            let body = ((open + 1)..=body_end).map(line_at).collect();
            (open, last, body)
        }
        TurnKind::Math => {
            let reg = blocks.math_at(row)?;
            let first = reg.range.start;
            let last = reg.range.end.saturating_sub(1).max(first);
            let body = ((first + 1)..last).map(line_at).collect();
            (first, last, body)
        }
        TurnKind::Quote | TurnKind::Callout => {
            let (first, last) = quote_run_rows(source, row);
            let body = (first..=last)
                .filter_map(|r| {
                    let line = line_at(r);
                    let stripped = strip_block_prefix(&line);
                    // Drop the `[!KIND]` marker token; keep any title text
                    // after it (and skip the line if that leaves nothing).
                    if let Some(rest) = stripped.trim_start().strip_prefix("[!") {
                        let after = rest.split_once(']').map(|(_, a)| a).unwrap_or("");
                        let after = after.trim_start_matches(['-', '+']).trim();
                        return (!after.is_empty()).then(|| after.to_string());
                    }
                    Some(stripped.to_string())
                })
                .collect();
            (first, last, body)
        }
        _ => (
            row,
            row,
            vec![strip_block_prefix(&line_at(row)).to_string()],
        ),
    };
    if body.is_empty() {
        body.push(String::new());
    }
    let text = match kind {
        TurnKind::Text => body.join("\n"),
        TurnKind::H1 => prefix_lines(&body, |_| "# ".into()),
        TurnKind::H2 => prefix_lines(&body, |_| "## ".into()),
        TurnKind::H3 => prefix_lines(&body, |_| "### ".into()),
        TurnKind::Bullet => prefix_lines(&body, |_| "- ".into()),
        TurnKind::Numbered => prefix_lines(&body, |i| format!("{}. ", i + 1)),
        TurnKind::Todo => prefix_lines(&body, |_| "- [ ] ".into()),
        TurnKind::Quote => prefix_lines(&body, |_| "> ".into()),
        TurnKind::Callout => format!("> [!NOTE]\n{}", prefix_lines(&body, |_| "> ".into())),
        TurnKind::Code => format!("```\n{}\n```", body.join("\n")),
        TurnKind::Math => format!("$$\n{}\n$$", body.join("\n")),
    };
    let range = starts[first]..line_end(source, last);
    // Fenced kinds: the caret parks after the closing fence, revealing the
    // raw markers (reveal-on-caret) — seat it on the body's first line
    // instead.
    let caret = matches!(kind, TurnKind::Code | TurnKind::Math)
        .then(|| range.start + text.find('\n').map_or(0, |i| i + 1));
    Some(TurnIntoEdit { range, text, caret })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockScan;

    fn turn(source: &str, row: usize, kind: TurnKind) -> Option<String> {
        let scan = BlockScan::new(source);
        let edit = turn_into(source, scan.view(), row, kind)?;
        let mut out = source.to_string();
        out.replace_range(edit.range, &edit.text);
        Some(out)
    }

    #[test]
    fn strip_block_prefix_cases() {
        assert_eq!(strip_block_prefix("# Title"), "Title");
        assert_eq!(strip_block_prefix("### Deep"), "Deep");
        assert_eq!(strip_block_prefix("- [x] done"), "done");
        assert_eq!(strip_block_prefix("- item"), "item");
        assert_eq!(strip_block_prefix("12. nth"), "nth");
        assert_eq!(strip_block_prefix("> quoted"), "quoted");
        assert_eq!(strip_block_prefix(">bare"), "bare");
        assert_eq!(strip_block_prefix("plain"), "plain");
        // Renderer-grammar forms the old hand-rolled version missed.
        assert_eq!(strip_block_prefix("* [ ] star task"), "star task");
        assert_eq!(strip_block_prefix("+ [x] plus task"), "plus task");
        assert_eq!(strip_block_prefix("3) paren"), "paren");
        assert_eq!(strip_block_prefix("#### H4"), "H4");
        // Not block prefixes: mid-word hash runs, `#tag`, a lone dash.
        assert_eq!(strip_block_prefix("#tag"), "#tag");
        assert_eq!(strip_block_prefix("-dash"), "-dash");
    }

    #[test]
    fn converts_lines_and_blocks() {
        assert_eq!(turn("a\n- b", 1, TurnKind::H2).as_deref(), Some("a\n## b"));
        assert_eq!(
            turn("> x\n> y", 0, TurnKind::Bullet).as_deref(),
            Some("- x\n- y")
        );
        assert_eq!(
            turn("> [!NOTE]\n> y", 1, TurnKind::Quote).as_deref(),
            Some("> y")
        );
        assert_eq!(turn("```\nx\n```", 1, TurnKind::Text).as_deref(), Some("x"));
        assert_eq!(turn("x", 0, TurnKind::Code).as_deref(), Some("```\nx\n```"));
        assert_eq!(turn("- a", 0, TurnKind::Bullet), None);
        let scan = BlockScan::new("x");
        let edit = turn_into("x", scan.view(), 0, TurnKind::Math).unwrap();
        assert_eq!(edit.caret, Some(3));
    }

    #[test]
    fn kinds_follow_the_renderer() {
        let s = "# h\n- [ ] t\n1. n\n> [!TIP]\n> c\n$$\nm\n$$";
        let scan = BlockScan::new(s);
        let kinds: Vec<TurnKind> = (0..8).map(|r| block_kind_at(s, scan.view(), r)).collect();
        use TurnKind::*;
        assert_eq!(
            kinds,
            [H1, Todo, Numbered, Callout, Callout, Math, Math, Math]
        );
    }
}
