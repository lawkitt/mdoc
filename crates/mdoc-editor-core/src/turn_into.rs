//! "Turn into": convert the blocks a selection touches between text,
//! headings, lists, to-dos, quotes, callouts, code and math (ADR 0110).

use std::ops::Range;

use mdoc_markdown::lines::{blockquote_prefix, heading_level, list_prefix, task_prefix};

use crate::markdown::alert_kind;
use crate::text::{line_starts, row_col};
use crate::{Blocks, Edit, EditResult};

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

/// The source's lines, found once per command: the per-line helpers in
/// [`crate::text`] each rescan the source, which a command over thousands
/// of blocks cannot afford.
struct Lines<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(source: &'a str) -> Self {
        Lines {
            source,
            starts: line_starts(source),
        }
    }

    fn last_row(&self) -> usize {
        self.starts.len() - 1
    }

    fn start(&self, row: usize) -> usize {
        self.starts[row]
    }

    fn end(&self, row: usize) -> usize {
        self.starts
            .get(row + 1)
            .map_or(self.source.len(), |&next| next - 1)
    }

    fn line(&self, row: usize) -> &'a str {
        &self.source[self.start(row)..self.end(row)]
    }

    /// Code (fences included) or display math, as
    /// [`Blocks::row_is_literal`] reads it.
    fn literal(&self, blocks: Blocks, row: usize) -> bool {
        blocks.in_fence(row)
            || self.line(row).trim_start().starts_with("```")
            || blocks.math_at(row).is_some()
    }

    fn quote_run(&self, row: usize) -> (usize, usize) {
        let is_q = |r: usize| blockquote_prefix(self.line(r)).is_some();
        let mut first = row;
        while first > 0 && is_q(first - 1) {
            first -= 1;
        }
        let mut last = row;
        while last < self.last_row() && is_q(last + 1) {
            last += 1;
        }
        (first, last)
    }

    fn kind_at(&self, blocks: Blocks, row: usize) -> TurnKind {
        if blocks.math_at(row).is_some() {
            return TurnKind::Math;
        }
        if row > self.last_row() {
            return TurnKind::Text;
        }
        // The renderer's own recognizers (task/list/heading/quote/alert), so
        // the checked kind always agrees with what WYSIWYG actually renders.
        let line = self.line(row);
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
            let (first, last) = self.quote_run(row);
            let callout = (first..=last).any(|r| {
                let l = self.line(r);
                let p = blockquote_prefix(l).unwrap_or(0);
                alert_kind(&l[p..]).is_some()
            });
            return if callout {
                TurnKind::Callout
            } else {
                TurnKind::Quote
            };
        }
        TurnKind::Text
    }

    /// The block holding `row`: fenced kinds (code, math) and quote runs
    /// are whole blocks, anything else is its line.
    fn span_at(&self, blocks: Blocks, row: usize) -> Span {
        let kind = self.kind_at(blocks, row);
        let line_at = |r: usize| self.line(r).to_string();
        let (first, last, body): (usize, usize, Vec<String>) = match (kind, blocks.math_at(row)) {
            (TurnKind::Code, _) => {
                // The opening fence is the first row above not inside one;
                // the closing fence the next fence line below it.
                let open = (0..=row).rev().find(|&r| !blocks.in_fence(r)).unwrap_or(0);
                let close = ((open + 1)..=self.last_row())
                    .find(|&r| self.line(r).trim_start().starts_with("```"));
                let last = close.unwrap_or(self.last_row());
                let body_end = close.map_or(self.last_row(), |c| c - 1);
                let body = ((open + 1)..=body_end).map(line_at).collect();
                (open, last, body)
            }
            (TurnKind::Math, Some(reg)) => {
                let first = reg.range.start;
                let last = reg.range.end.saturating_sub(1).max(first);
                let body = ((first + 1)..last).map(line_at).collect();
                (first, last, body)
            }
            (TurnKind::Quote | TurnKind::Callout, _) => {
                let (first, last) = self.quote_run(row);
                let body = (first..=last)
                    .filter_map(|r| {
                        let stripped = strip_block_prefix(self.line(r));
                        // Drop the `[!KIND]` marker token; keep any title
                        // text after it (and skip the line if that leaves
                        // nothing).
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
                vec![strip_block_prefix(self.line(row)).to_string()],
            ),
        };
        Span {
            first,
            last,
            kind,
            body,
        }
    }
}

/// The contiguous blockquote run containing `row` (first row..=last row),
/// by the renderer's `blockquote_prefix` test.
pub fn quote_run_rows(source: &str, row: usize) -> (usize, usize) {
    Lines::new(source).quote_run(row)
}

/// The "Turn into" kind of the block containing `row` — what the flyout
/// shows checked.
pub fn block_kind_at(source: &str, blocks: Blocks, row: usize) -> TurnKind {
    Lines::new(source).kind_at(blocks, row)
}

/// One block a "Turn into" rewrites: its rows, its kind and its text with
/// the kind's dressing removed.
struct Span {
    first: usize,
    last: usize,
    kind: TurnKind,
    body: Vec<String>,
}

/// The rows `selection` touches. A selection that ends at a line's start
/// does not touch that line.
pub fn selected_rows(source: &str, selection: Range<usize>) -> (usize, usize) {
    let start = selection.start.min(selection.end).min(source.len());
    let end = selection.start.max(selection.end).min(source.len());
    let first = row_col(source, start).0;
    let (last, col) = row_col(source, end);
    if end > start && col == 0 && last > first {
        return (first, last - 1);
    }
    (first, last)
}

/// The blocks `selection` touches, in order. Blank lines, a drawn table and
/// its style marker are skipped. A caret on a blank line is its block (an
/// empty item to type in); a selection of nothing else has none.
fn spans(lines: &Lines, blocks: Blocks, selection: Range<usize>) -> Vec<Span> {
    let (first, last) = selected_rows(lines.source, selection.clone());
    let skipped = |r: usize| {
        !lines.literal(blocks, r)
            && (lines.line(r).trim().is_empty()
                || blocks.row_in_table(r)
                || blocks.tables.iter().any(|t| t.marker_line == Some(r)))
    };
    let mut out = Vec::new();
    let mut row = first;
    while row <= last {
        if skipped(row) {
            row += 1;
            continue;
        }
        let span = lines.span_at(blocks, row);
        row = span.last + 1;
        out.push(span);
    }
    if out.is_empty() && selection.is_empty() && lines.line(first).trim().is_empty() {
        out.push(lines.span_at(blocks, first));
    }
    out
}

/// The kinds of the blocks `selection` touches, in order.
pub fn selected_kinds(source: &str, blocks: Blocks, selection: Range<usize>) -> Vec<TurnKind> {
    spans(&Lines::new(source), blocks, selection)
        .into_iter()
        .map(|s| s.kind)
        .collect()
}

/// "Turn into" over `selection` (ADR 0110): every block it touches becomes
/// `kind`; when every one is `kind` already, they all turn back into text.
/// `None` when nothing changes.
pub fn turn_selection(
    source: &str,
    blocks: Blocks,
    selection: Range<usize>,
    kind: TurnKind,
) -> Option<EditResult> {
    turn_selection_where(source, blocks, selection, kind, |k| k == kind)
}

/// [`turn_selection`] where `is_kind` says which kinds count as `kind`
/// already (the list button counts a numbered item as a list).
///
/// Blank lines in the selection are skipped, and between new list and task
/// items they go. Turned back into text, items that were adjacent get a
/// blank line between them, so each stays a paragraph. Code, math and a
/// callout wrap every block in one container. A blank line, a table or
/// another block skipped between the blocks stays as it is. A selection
/// keeps the changed blocks selected (a container's body); a caret lands
/// at the end of the new text, or on a code or math block's first line.
pub fn turn_selection_where(
    source: &str,
    blocks: Blocks,
    selection: Range<usize>,
    kind: TurnKind,
    is_kind: impl Fn(TurnKind) -> bool,
) -> Option<EditResult> {
    let lines = Lines::new(source);
    let sel = selection.start.min(selection.end)..selection.start.max(selection.end);
    let spans = spans(&lines, blocks, sel);
    let target = if kind != TurnKind::Text && spans.iter().all(|s| is_kind(s.kind)) {
        TurnKind::Text
    } else {
        kind
    };
    // No block (a table or blank lines only), or nothing to change.
    if spans.iter().all(|s| s.kind == target) {
        return None;
    }
    let start = lines.start(spans[0].first);
    let end = lines.end(spans[spans.len() - 1].last);
    let original = |s: &Span| &source[lines.start(s.first)..lines.end(s.last)];
    let between = |i: usize| &source[lines.end(spans[i - 1].last)..lines.start(spans[i].first)];
    let body = |s: &Span| {
        if s.body.is_empty() {
            vec![String::new()]
        } else {
            s.body.clone()
        }
    };
    let container = matches!(target, TurnKind::Code | TurnKind::Math | TurnKind::Callout);
    let text = if container {
        let mut lines: Vec<String> = Vec::new();
        for (i, s) in spans.iter().enumerate() {
            if i > 0 {
                // The lines between, without their first and last newline.
                let sep = between(i);
                let quoted = target == TurnKind::Callout;
                if sep.len() == 1 && quoted {
                    lines.push(">".to_string());
                } else if sep.len() > 1 {
                    let inner = &sep[1..sep.len() - 1];
                    lines.extend(inner.split('\n').map(|l| match l.trim().is_empty() {
                        true if quoted => ">".to_string(),
                        true => String::new(),
                        false if quoted => format!("> {l}"),
                        false => l.to_string(),
                    }));
                }
            }
            let own = body(s);
            lines.extend(match target {
                TurnKind::Callout => own.iter().map(|l| format!("> {l}")).collect(),
                _ => own,
            });
        }
        match target {
            TurnKind::Code => format!("```\n{}\n```", lines.join("\n")),
            TurnKind::Math => format!("$$\n{}\n$$", lines.join("\n")),
            _ => format!("> [!NOTE]\n{}", lines.join("\n")),
        }
    } else {
        let mut text = String::new();
        let mut number = 1u32;
        for (i, s) in spans.iter().enumerate() {
            if i > 0 {
                let sep = between(i);
                let changed = spans[i - 1].kind != target || s.kind != target;
                text.push_str(match target {
                    _ if !changed || !sep.trim().is_empty() => sep,
                    TurnKind::Bullet | TurnKind::Numbered | TurnKind::Todo => "\n",
                    TurnKind::Text if sep == "\n" => "\n\n",
                    TurnKind::Quote if sep == "\n" => "\n>\n",
                    _ => sep,
                });
            }
            if s.kind == target {
                let kept = original(s);
                if let Some((_, _, true, n)) = kept.lines().last().and_then(list_prefix) {
                    number = n.saturating_add(1);
                }
                text.push_str(kept);
                continue;
            }
            let body = body(s);
            text.push_str(&match target {
                TurnKind::Text => body.join("\n"),
                TurnKind::H1 => prefix_lines(&body, |_| "# ".into()),
                TurnKind::H2 => prefix_lines(&body, |_| "## ".into()),
                TurnKind::H3 => prefix_lines(&body, |_| "### ".into()),
                TurnKind::Bullet => prefix_lines(&body, |_| "- ".into()),
                TurnKind::Numbered => {
                    let from = number;
                    number = number.saturating_add(body.len() as u32);
                    prefix_lines(&body, |i| format!("{}. ", from.saturating_add(i as u32)))
                }
                TurnKind::Todo => prefix_lines(&body, |_| "- [ ] ".into()),
                _ => prefix_lines(&body, |_| "> ".into()),
            });
        }
        text
    };
    // A fenced kind's caret would park after the closing fence, revealing
    // the raw markers: it goes on the body's first line instead.
    let body_start = start + text.find('\n').map_or(0, |i| i + 1);
    let selection = if selection.start == selection.end {
        let caret = match target {
            TurnKind::Code | TurnKind::Math => body_start,
            _ => start + text.len(),
        };
        caret..caret
    } else {
        match target {
            TurnKind::Code | TurnKind::Math => {
                body_start..(start + text.rfind('\n').unwrap_or(text.len())).max(body_start)
            }
            TurnKind::Callout => body_start..start + text.len(),
            _ => start..start + text.len(),
        }
    };
    Some(EditResult {
        edits: line_edits(&source[start..end], &text, start),
        selection,
    })
}

/// `old`, which sits at `start` in the source, rewritten as `new`: one edit
/// for each line prefix put, changed or taken away and for each line added
/// or removed, so a line's own text is in no edit. What lies in an edit is
/// replaced for everything that follows the text (a mobile proposal there
/// is dropped), and a block command changes markers, not words. One edit
/// for the whole stretch when the lines do not pair up.
fn line_edits(old: &str, new: &str, start: usize) -> Vec<Edit> {
    let whole = || {
        vec![Edit {
            range: start..start + old.len(),
            text: new.to_string(),
        }]
    };
    // Neither ends with a line break (the stretch ends at a line's end):
    // each line is compared with one.
    let lines =
        |text: &str| -> Vec<String> { text.split('\n').map(|l| format!("{l}\n")).collect() };
    let (was, now) = (lines(old), lines(new));
    // How many bytes two lines share at their end, when that is more than
    // the line break and what differs before it is a marker's length.
    let shared = |a: &str, b: &str| -> Option<usize> {
        let mut n = a
            .bytes()
            .rev()
            .zip(b.bytes().rev())
            .take_while(|(x, y)| x == y)
            .count();
        while !a.is_char_boundary(a.len() - n) || !b.is_char_boundary(b.len() - n) {
            n -= 1;
        }
        const MARKER: usize = 16;
        (n > 1 && a.len() - n <= MARKER && b.len() - n <= MARKER).then_some(n)
    };
    let pairs = |a: &str, b: &str| a == b || shared(a, b).is_some();
    // From lines that do not pair: how many new lines to add, or old ones
    // to remove, before a pair comes.
    let ahead = |a: Option<&str>, b: Option<&str>, i: usize, j: usize| -> Option<(usize, usize)> {
        let (a, b) = (a?, b?);
        let added = now[j + 1..]
            .iter()
            .position(|next| pairs(a, next))
            .map(|k| k + 1);
        let removed = was[i + 1..]
            .iter()
            .position(|next| pairs(next, b))
            .map(|k| k + 1);
        match (added, removed) {
            (Some(n), Some(m)) if m <= n => Some((0, m)),
            (Some(n), _) => Some((n, 0)),
            (None, Some(m)) => Some((0, m)),
            (None, None) => None,
        }
    };
    let mut edits: Vec<Edit> = Vec::new();
    let mut push = |range: Range<usize>, text: &str| match edits.last_mut() {
        Some(last) if last.range.end == range.start => {
            last.range.end = range.end;
            last.text.push_str(text);
        }
        _ => edits.push(Edit {
            range,
            text: text.to_string(),
        }),
    };
    let (mut i, mut j, mut at) = (0, 0, 0);
    while i < was.len() || j < now.len() {
        let a = was.get(i).map(String::as_str);
        let b = now.get(j).map(String::as_str);
        match (a, b) {
            (Some(a), Some(b)) if a == b => {
                at += a.len();
                (i, j) = (i + 1, j + 1);
            }
            (Some(a), Some(b)) if shared(a, b).is_some() => {
                let n = shared(a, b).unwrap_or(0);
                push(at..at + a.len() - n, &b[..b.len() - n]);
                at += a.len();
                (i, j) = (i + 1, j + 1);
            }
            // Lines added (the old line pairs with a later new one) or
            // removed (a later old line pairs with the new one): whichever
            // run is shorter, so the lines that pair stay out of the edit.
            (a, b) if a.is_none() || b.is_none() || ahead(a, b, i, j).is_some() => {
                let (added, removed) = match (a, b) {
                    (None, _) => (1, 0),
                    (_, None) => (0, 1),
                    _ => ahead(a, b, i, j).unwrap_or((0, 0)),
                };
                for line in &now[j..j + added] {
                    push(at..at, line);
                }
                for line in &was[i..i + removed] {
                    push(at..at + line.len(), "");
                    at += line.len();
                }
                (i, j) = (i + removed, j + added);
            }
            (Some(a), Some(b)) => {
                push(at..at + a.len(), b);
                at += a.len();
                (i, j) = (i + 1, j + 1);
            }
            (None, None) => break,
            // Guarded out above: a lone side is a line added or removed.
            (Some(_), None) | (None, Some(_)) => return whole(),
        }
    }
    // The last line's break is not in the source: an edit that reaches it
    // gives it up, or takes the break before the edit instead.
    if let Some(last) = edits.last_mut()
        && last.range.end > old.len()
    {
        last.range.end = old.len();
        if last.text.ends_with('\n') {
            last.text.pop();
            if last.range.start > old.len() {
                last.range.start = old.len();
                last.text.insert(0, '\n');
            }
        } else if last.range.start > 0 && last.range.start <= old.len() {
            last.range.start -= 1;
        } else {
            return whole();
        }
    }
    for edit in &mut edits {
        edit.range = start + edit.range.start..start + edit.range.end;
    }
    edits.retain(|e| !e.range.is_empty() || !e.text.is_empty());
    // Checked against the text asked for: any doubt is one edit.
    let result = EditResult {
        edits,
        selection: 0..0,
    };
    let ordered = result
        .edits
        .windows(2)
        .all(|w| w[0].range.end < w[1].range.start);
    let mut padded = " ".repeat(start);
    padded.push_str(old);
    if ordered && result.apply(&padded)[start..] == *new {
        result.edits
    } else {
        whole()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockScan;
    use TurnKind::*;

    /// The text and selection after turning `selection` into `kind`.
    fn turn(source: &str, selection: Range<usize>, kind: TurnKind) -> Option<(String, String)> {
        let scan = BlockScan::new(source);
        let r = turn_selection(source, scan.view(), selection, kind)?;
        let out = r.apply(source);
        let selected = out[r.selection.clone()].to_string();
        Some((out, selected))
    }

    /// A block command edits markers and blank lines, never a line's own
    /// text: what an edit covers is replaced for whoever follows the text
    /// (a mobile proposal inside it is dropped).
    #[test]
    fn edits_leave_each_lines_text_alone() {
        let source = "Intro Anna\n\nOne Anna\n\nTwo Anna\ntwo more\n\n- Three Anna\n\nLast";
        let all = 0..source.len();
        for kind in [Bullet, Numbered, Todo, Quote, Text, H2, Code, Callout] {
            let scan = BlockScan::new(source);
            let Some(r) = turn_selection(source, scan.view(), all.clone(), kind) else {
                continue;
            };
            assert!(kind == Text || r.edits.len() > 1, "{kind:?}: {:?}", r.edits);
            for edit in &r.edits {
                assert!(
                    !source[edit.range.clone()].contains("Anna"),
                    "{kind:?}: {edit:?}"
                );
                assert!(!edit.text.contains("Anna"), "{kind:?}: {edit:?}");
            }
            assert!(
                r.edits
                    .windows(2)
                    .all(|w| w[0].range.end < w[1].range.start)
            );
        }
        // Blank lines in a run between two paragraphs go as a run.
        let source = "Alice\n\n\n\nPay Anna";
        let scan = BlockScan::new(source);
        let r = turn_selection(source, scan.view(), 0..source.len(), Todo).unwrap();
        assert_eq!(r.apply(source), "- [ ] Alice\n- [ ] Pay Anna");
        for edit in &r.edits {
            assert!(!source[edit.range.clone()].contains("Anna"), "{edit:?}");
        }
        assert_eq!(
            r.joined(source).unwrap().range,
            0..source.len() - "Pay Anna".len()
        );
        // One paragraph: only its prefix is put, and taken away again.
        let scan = BlockScan::new("Pay Anna");
        let r = turn_selection("Pay Anna", scan.view(), 3..3, Todo).unwrap();
        assert_eq!(
            r.edits,
            [Edit {
                range: 0..0,
                text: "- [ ] ".into()
            }]
        );
        let scan = BlockScan::new("- [ ] Pay Anna");
        let r = turn_selection("- [ ] Pay Anna", scan.view(), 8..8, Text).unwrap();
        assert_eq!(
            r.edits,
            [Edit {
                range: 0..6,
                text: String::new()
            }]
        );
    }

    #[test]
    fn line_edits_give_the_text_asked_for() {
        let cases = [
            ("a", "- a"),
            ("a\n\nb", "- a\n- b"),
            ("- a\n- b", "a\n\nb"),
            ("a\nb", "```\na\nb\n```"),
            ("```\na\n```", "a"),
            ("a\n\nb", "> [!NOTE]\n> a\n>\n> b"),
            ("x\ny", "completely\ndifferent\nlines here"),
            ("é\n\nü", "1. é\n2. ü"),
            ("", "- "),
        ];
        for (old, new) in cases {
            let edits = line_edits(old, new, 3);
            let r = EditResult {
                edits,
                selection: 0..0,
            };
            assert_eq!(
                r.apply(&format!("ab\n{old}\ntail")),
                format!("ab\n{new}\ntail"),
                "{old:?} -> {new:?}"
            );
        }
    }

    /// The text after turning the block at the start of `row` into `kind`.
    fn turn_row(source: &str, row: usize, kind: TurnKind) -> Option<String> {
        let at = line_starts(source)[row];
        turn(source, at..at, kind).map(|(out, _)| out)
    }

    fn all(source: &str) -> Range<usize> {
        0..source.len()
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
        assert_eq!(turn_row("a\n- b", 1, H2).as_deref(), Some("a\n## b"));
        assert_eq!(turn_row("> x\n> y", 0, Bullet).as_deref(), Some("- x\n- y"));
        assert_eq!(turn_row("> [!NOTE]\n> y", 1, Quote).as_deref(), Some("> y"));
        assert_eq!(turn_row("```\nx\n```", 1, Text).as_deref(), Some("x"));
        assert_eq!(turn_row("x", 0, Code).as_deref(), Some("```\nx\n```"));
        // The caret's block is that kind already: back to text.
        assert_eq!(turn_row("- a", 0, Bullet).as_deref(), Some("a"));
        assert_eq!(turn_row("a", 0, Text), None);
        // A fenced kind seats the caret on the body's first line.
        let scan = BlockScan::new("x");
        let r = turn_selection("x", scan.view(), 0..0, Math).unwrap();
        assert_eq!(r.selection, 3..3);
    }

    #[test]
    fn kinds_follow_the_renderer() {
        let s = "# h\n- [ ] t\n1. n\n> [!TIP]\n> c\n$$\nm\n$$";
        let scan = BlockScan::new(s);
        let kinds: Vec<TurnKind> = (0..8).map(|r| block_kind_at(s, scan.view(), r)).collect();
        assert_eq!(
            kinds,
            [H1, Todo, Numbered, Callout, Callout, Math, Math, Math]
        );
    }

    #[test]
    fn every_selected_block_turns_and_list_items_close_up() {
        let s = "Арендатор платит.\n\nАрендодатель чинит.\n\nСрок год.";
        assert_eq!(
            turn(s, all(s), Bullet),
            Some((
                "- Арендатор платит.\n- Арендодатель чинит.\n- Срок год.".into(),
                "- Арендатор платит.\n- Арендодатель чинит.\n- Срок год.".into()
            ))
        );
        assert_eq!(
            turn(s, all(s), Numbered).unwrap().0,
            "1. Арендатор платит.\n2. Арендодатель чинит.\n3. Срок год."
        );
        assert_eq!(
            turn(s, all(s), Todo).unwrap().0,
            "- [ ] Арендатор платит.\n- [ ] Арендодатель чинит.\n- [ ] Срок год."
        );
        // Headings and quotes keep the blank lines: each block turns alone.
        assert_eq!(
            turn(s, all(s), H2).unwrap().0,
            "## Арендатор платит.\n\n## Арендодатель чинит.\n\n## Срок год."
        );
        // A selection from the middle of one paragraph into another touches both.
        let from = s.find("платит").unwrap();
        let to = s.find("чинит").unwrap();
        assert_eq!(
            turn(s, from..to, Bullet).unwrap().0,
            "- Арендатор платит.\n- Арендодатель чинит.\n\nСрок год."
        );
    }

    #[test]
    fn blank_lines_at_the_selection_edges_are_skipped() {
        // Starting on a blank line no longer makes an empty item.
        let s = "Вступление.\n\nПервый.\n\nВторой.\n";
        let from = s.find("\n\nПервый").unwrap() + 1;
        let (out, selected) = turn(s, from..s.len(), Bullet).unwrap();
        assert_eq!(out, "Вступление.\n\n- Первый.\n- Второй.\n");
        assert_eq!(selected, "- Первый.\n- Второй.");
        // A selection ending at a line's start leaves that line alone.
        let to = s.find("Второй").unwrap();
        let from = s.find("Первый").unwrap();
        assert_eq!(
            turn(s, from..to, H1).unwrap().0,
            "Вступление.\n\n# Первый.\n\nВторой.\n"
        );
        // A caret on a blank line still starts an empty item there.
        assert_eq!(
            turn("а\n\nб", 3..3, Bullet),
            Some(("а\n- \nб".into(), String::new()))
        );
    }

    #[test]
    fn all_of_that_kind_turn_back_into_paragraphs() {
        let s = "- Первый пункт\n- Второй пункт\n\n- Третий";
        assert_eq!(
            turn(s, all(s), Bullet).unwrap().0,
            "Первый пункт\n\nВторой пункт\n\nТретий"
        );
        let s = "## Раз\n\n## Два";
        assert_eq!(turn(s, all(s), H2).unwrap().0, "Раз\n\nДва");
        // Mixed: everything becomes the kind; items that are already keep
        // their text, and the numbers run on from them.
        let s = "1. Один\nДва\n\nТри";
        assert_eq!(
            turn(s, all(s), Numbered).unwrap().0,
            "1. Один\n2. Два\n3. Три"
        );
        let s = "- [x] Сделано\nНовое";
        assert_eq!(
            turn(s, all(s), Todo).unwrap().0,
            "- [x] Сделано\n- [ ] Новое"
        );
    }

    #[test]
    fn a_container_wraps_the_selection_once() {
        let s = "строка один\n\nстрока два";
        let (out, selected) = turn(s, all(s), Code).unwrap();
        assert_eq!(out, "```\nстрока один\n\nстрока два\n```");
        assert_eq!(selected, "строка один\n\nстрока два");
        assert_eq!(
            turn(s, all(s), Math).unwrap().0,
            "$$\nстрока один\n\nстрока два\n$$"
        );
        let (out, selected) = turn("- раз\n- два", all("- раз\n- два"), Callout).unwrap();
        assert_eq!(out, "> [!NOTE]\n> раз\n>\n> два");
        assert_eq!(selected, "> раз\n>\n> два");
        // Every block code already: back to text.
        let s = "```\nкод\n```";
        assert_eq!(turn(s, all(s), Code).unwrap().0, "код");
    }

    #[test]
    fn a_selection_of_only_a_table_or_blank_lines_changes_nothing() {
        let s = "| а | б |\n| - | - |\n| 1 | 2 |\n\n\nконец";
        assert_eq!(turn(s, 0..s.find("\n\n").unwrap(), Bullet), None);
        let blank = s.find("\n\n").unwrap() + 1;
        assert_eq!(turn(s, blank..blank + 1, Bullet), None);
        // A caret in a table cell is no blank line either.
        assert_eq!(turn(s, 3..3, Bullet), None);
    }

    /// Thousands of blocks in one command: the lines are found once, not
    /// per block (a rescan per block would take minutes here).
    #[test]
    fn many_blocks_turn_in_one_pass() {
        let s = "пункт\n\n".repeat(20_000);
        let (out, _) = turn(&s, all(&s), Bullet).unwrap();
        assert_eq!(out, format!("{}\n\n", vec!["- пункт"; 20_000].join("\n")));
    }

    #[test]
    fn tables_between_selected_blocks_stay() {
        let s = "Перед\n\n| а | б |\n| - | - |\n| 1 | 2 |\n\nПосле";
        assert_eq!(
            turn(s, all(s), H3).unwrap().0,
            "### Перед\n\n| а | б |\n| - | - |\n| 1 | 2 |\n\n### После"
        );
        assert_eq!(
            turn(s, all(s), Bullet).unwrap().0,
            "- Перед\n\n| а | б |\n| - | - |\n| 1 | 2 |\n\n- После"
        );
        assert_eq!(
            selected_kinds(s, BlockScan::new(s).view(), all(s)),
            [Text, Text]
        );
    }
}
