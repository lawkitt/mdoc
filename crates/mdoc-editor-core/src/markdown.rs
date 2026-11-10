//! Markdown helpers the editing rules, search and spell exclusions share:
//! semantic spans, formatting-marker pairs, ordered numbering, task boxes,
//! image lines and display math. Recognition itself is `mdoc-markdown`'s.

use std::ops::Range;

use mdoc_markdown::lines::{list_prefix, ordered_scan, task_prefix};
use mdoc_markdown::spans::{inline_code_end, is_backslash_escaped, scan, scan_line};
use mdoc_markdown::syntax::{AlertKind, alert_prefix};

/// A source span as seen by the Markdown search projection. Unlike the
/// desktop editor's `hidden_runs`, this deliberately has no caret-dependent
/// reveal rules: search indexes the stable rendered text, not the transient
/// editing view.
#[derive(Clone, Debug)]
pub struct SemanticSpan {
    pub range: Range<usize>,
    pub hidden: bool,
    pub replacement: bool,
    pub decoded: Option<String>,
    /// Inline code (monospace) text.
    pub code: bool,
}

/// Scan one source slice using the editor's own Markdown scanner and expose
/// only the visibility decisions needed by the search index. Keeping this
/// beside `scan_line` prevents the search projection from growing a second,
/// subtly different inline-Markdown grammar.
pub fn semantic_spans_with(text: &str, start: usize, end: usize) -> Vec<SemanticSpan> {
    let mut spans = Vec::new();
    scan_line(text, start, end, &mut spans);
    spans.sort_by_key(|span| span.range.start);
    let mut out: Vec<SemanticSpan> = Vec::with_capacity(spans.len());
    let mut spans = spans.into_iter().peekable();
    while let Some(span) = spans.next() {
        // An escape reads as its character, mapped to the whole escape
        // (`\*` matches `*`, and replacing it replaces the backslash too).
        if span.style.escape
            && let Some(ch) = spans.next_if(|ch| ch.range.start == span.range.end)
        {
            out.push(SemanticSpan {
                range: span.range.start..ch.range.end,
                hidden: true,
                replacement: true,
                decoded: Some(text[ch.range].to_owned()),
                code: ch.style.code,
            });
            continue;
        }
        out.push(SemanticSpan {
            range: span.range,
            hidden: span.style.hide,
            replacement: span.style.replace.is_some(),
            decoded: span.style.replace.filter(|_| span.style.decoded),
            code: span.style.code,
        });
    }
    out
}

/// The always-hidden formatting-marker PAIRS on a line, as
/// `(opening range, closing range)` per construct — for the editor's
/// Cditor-style delete (skip the invisible bytes; collapse an emptied pair).
pub fn fmt_marker_pairs(line: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let mut spans = scan(line);
    spans.sort_by_key(|s| s.range.start);
    // The scanner stamps each construct's opener + closer with a shared
    // `pair_id`, so pairing survives NESTED constructs between them (the old
    // adjacency walk assumed at most one body span).
    let mut open: Vec<(u16, Range<usize>)> = Vec::new();
    let mut pairs = Vec::new();
    for sp in spans.into_iter().filter(|s| s.style.always_hide) {
        let id = sp.style.pair_id;
        if id == 0 {
            continue;
        }
        match open.iter().position(|(oid, _)| *oid == id) {
            Some(k) => {
                let (_, o) = open.remove(k);
                pairs.push((o, sp.range));
            }
            None => open.push((id, sp.range)),
        }
    }
    pairs.sort_by_key(|(o, _)| o.start);
    pairs
}

/// The alert kind if `body` — a blockquote line's text after its `>` prefix —
/// starts with an alert marker (see [`mdoc_markdown::syntax::alert_prefix`]).
pub fn alert_kind(body: &str) -> Option<AlertKind> {
    alert_prefix(body).map(|(kind, ..)| kind)
}

/// `(number, nesting level)` of every ordered list item: the number as
/// written in the source (ADR 0110), the level 0-based and STRUCTURAL (how
/// many lists are open above), so it survives any spaces-per-level setting.
/// Non-items are (0, 0). See [`mdoc_markdown::lines::ordered_scan`].
pub fn ordered_numbers(lines: &[&str]) -> Vec<(u32, usize)> {
    ordered_scan(lines).0
}

/// Toggle a GFM task item's checkbox in `line` — flip the char between the
/// brackets (`[ ]`↔`[x]`). Returns the rewritten line, or `None` if `line` isn't
/// a task item. The length is unchanged (one ASCII byte swapped).
pub fn toggle_task_checkbox(line: &str) -> Option<String> {
    let (prefix_len, _indent, checked) = task_prefix(line)?;
    let box_byte = prefix_len - 3; // the char between `[` and `]`
    let mut out = line.to_string();
    out.replace_range(box_byte..box_byte + 1, if checked { " " } else { "x" });
    Some(out)
}

/// If `line` is a standalone image — `![alt](src)`, optionally followed by a
/// `{width=N}` / `{width=Npx}` attribute, with only whitespace around it —
/// return `(src, explicit_width)`. The editor renders such a line as the image
/// (W4) when the caret is elsewhere; a line with any other trailing text stays
/// plain source.
pub fn image_line(line: &str) -> Option<(&str, Option<f32>)> {
    let rest = line.trim().strip_prefix("![")?;
    let close_alt = rest.find("](")?;
    let after_alt = &rest[close_alt + 2..];
    let close_src = after_alt.find(')')?;
    let src = after_alt[..close_src].trim();
    let tail = after_alt[close_src + 1..].trim();
    let width = if tail.is_empty() {
        None
    } else {
        let w = tail.strip_prefix("{width=")?.strip_suffix('}')?;
        Some(
            w.strip_suffix("px")
                .unwrap_or(w)
                .trim()
                .parse::<f32>()
                .ok()?,
        )
    };
    (!src.is_empty()).then_some((src, width))
}

/// Like [`image_line`], but also matches an image that is the sole body of a
/// list item — `- ![](src)` / `1. ![](src){width=N}`. Returns `(src, width,
/// marker_len)`, where `marker_len` is the byte length of the leading list
/// marker (0 for a plain standalone image). The editor renders the image inset
/// past the marker so a bulleted image keeps its bullet, instead of the row
/// collapsing to the image or falling back to raw source.
pub fn image_row(line: &str) -> Option<(&str, Option<f32>, usize)> {
    if let Some((src, width)) = image_line(line) {
        return Some((src, width, 0));
    }
    let (plen, ..) = list_prefix(line)?;
    let (src, width) = image_line(&line[plen..])?;
    Some((src, width, plen))
}

/// Whether `line` is a `<!-- math:left|center|right -->` alignment marker.
/// The editor hides it like a table style marker; standard Markdown viewers
/// ignore the comment.
pub fn is_math_align_marker(line: &str) -> bool {
    math_align_name(line).is_some_and(|n| matches!(n, "left" | "center" | "right"))
}

fn math_align_name(line: &str) -> Option<&str> {
    let inner = line
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    Some(inner.strip_prefix("math:")?.trim())
}

/// A detected `$$…$$` block: its line range (both fences) and the optional
/// `<!-- math:ALIGN -->` marker line directly above it.
#[derive(Clone)]
pub struct MathRegion {
    pub range: Range<usize>,
    pub marker_line: Option<usize>,
}

pub fn math_regions(content: &str) -> Vec<MathRegion> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut in_code = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            // `$$` inside a fenced code block (shell PIDs!) is never math.
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            i += 1;
            continue;
        }
        if lines[i].trim() == "$$" {
            let start = i;
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim() != "$$" {
                j += 1;
            }
            let end = (j + 1).min(lines.len()); // include the closing fence
            // An alignment marker on the line directly above the opening fence.
            let marker_line = start
                .checked_sub(1)
                .filter(|&m| is_math_align_marker(lines[m]));
            out.push(MathRegion {
                range: start..end,
                marker_line,
            });
            i = end;
        } else if one_line_math(lines[i]).is_some() {
            // `$$x^2$$` ALONE on a line is display math (issue #54): block
            // treatment, including its alignment marker.
            let marker_line = i.checked_sub(1).filter(|&m| is_math_align_marker(lines[m]));
            out.push(MathRegion {
                range: i..i + 1,
                marker_line,
            });
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The inner LaTeX when the whole (trimmed) line is one `$$…$$` pair with
/// non-empty content — the one-line display-math form.
fn one_line_math(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix("$$")?.strip_suffix("$$")?;
    let inner = inner.trim();
    // `$$$$`, `$$ $$`, and interior `$$`s (two pairs jammed together) are
    // not the one-line form.
    (!inner.is_empty() && !inner.contains("$$")).then_some(inner)
}

/// Inline `$…$` math spans within a single text line (NOT block `$$` fences) — byte ranges
/// covering the whole span, both `$` delimiters included. Follows the common (pandoc) rule so
/// prose like "it cost $5 and $10" isn't mistaken for math: the opening `$` is followed by a
/// non-space, the closing `$` is preceded by a non-space and not followed by a digit, the
/// content is non-empty, `$$` is skipped (block-fence-ish / empty), and `\$` is a literal.
///
/// `$` and `\` are ASCII bytes that can't occur inside a multi-byte UTF-8 sequence, so the
/// byte scan is char-safe and the returned ranges fall on char boundaries.
pub fn inline_math_spans(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = inline_code_end(line, i) {
            i = end;
            continue;
        }

        if bytes[i] != b'$' || is_backslash_escaped(bytes, i) {
            i += 1;
            continue;
        }
        // `$$…$$` on ONE line is display math (issue #54; block `$$` fences
        // live on their own lines and never form a same-line pair). Unlike
        // `$…$` there are no space rules — pandoc display math allows them —
        // but the pair must close on this line around non-space content.
        if bytes.get(i + 1) == Some(&b'$') {
            let mut j = i + 2;
            let mut close = None;
            while j + 1 < bytes.len() {
                if bytes[j] == b'$' && bytes[j + 1] == b'$' && !is_backslash_escaped(bytes, j) {
                    close = Some(j);
                    break;
                }
                j += 1;
            }
            match close {
                Some(c) if line[i + 2..c].trim().is_empty() => {}
                Some(c) => {
                    out.push(i..c + 2);
                    i = c + 2;
                    continue;
                }
                None => {}
            }
            i += 2;
            continue;
        }
        // Opening `$`: reject a space right after.
        match bytes.get(i + 1) {
            None => {
                i += 1;
                continue;
            }
            Some(c) if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            _ => {}
        }
        // Scan for a valid closing `$`: unescaped, non-space before, not a digit after. A `$`
        // that can't close (space before, e.g. the second `$` in "$5 and $10") is skipped and
        // the scan continues for a later valid one, as pandoc does.
        let mut j = i + 1;
        let mut close = None;
        while j < bytes.len() {
            if bytes[j] == b'$' && !is_backslash_escaped(bytes, j) {
                let space_before = bytes[j - 1].is_ascii_whitespace();
                let digit_after = bytes.get(j + 1).is_some_and(|c| c.is_ascii_digit());
                if !space_before && !digit_after {
                    close = Some(j);
                    break;
                }
            }
            j += 1;
        }
        match close {
            Some(c) => {
                out.push(i..c + 1);
                i = c + 1;
            }
            None => i += 1,
        }
    }
    out
}
