//! Inline Markdown spans over source byte ranges, for live-preview styling.
//!
//! A fast, line-scoped scanner that classifies Markdown source into spans
//! **without changing the byte buffer**: syntax markers (`**`, `` ` ``, `[` …)
//! stay in the text as marker spans, so a host can dim or hide them and the
//! displayed characters still map to source offsets one-to-one.
//!
//! Scope: inline constructs at the normal text size — bold, italic,
//! strikethrough, highlight, inline code, links, footnote references, styled
//! inline HTML and bare URLs — plus the line prefixes that hide (headings,
//! blockquotes and alerts, list and task markers). Spans name what a host
//! colours ([`Color`], [`Background`]); the host maps them to its own theme.

use std::ops::Range;

use crate::lines::{heading_level, list_prefix, task_prefix};
use crate::syntax::{
    StyledKind, alert_prefix, highlight_close, inline_tag_len, matching_styled_close, styled_tag,
    url_end,
};

/// A span's text colour: a role the host's theme colours, or a colour the
/// document itself names (`<span style="color:…">`) as `0xRRGGBBAA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// The syntax markers themselves (`**`, `*`, `~~`, `[`, `]`, `]( … )`),
    /// dimmed when shown.
    Marker,
    /// Inline `code` text.
    Code,
    /// Links, bare URLs and footnote references.
    Link,
    /// A colour named in the document.
    Rgba(u32),
}

/// A span's background: a role the host's theme colours, or a colour the
/// document itself names (`<mark style="background:…">`) as `0xRRGGBBAA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    /// Inline `code` tint.
    Code,
    /// `==highlight==` and a plain `<mark>`.
    Mark,
    /// A colour named in the document.
    Rgba(u32),
}

/// What a scanned span adds on top of the host's base text style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    /// Inline code body; read by semantic consumers (spell exclusions,
    /// search) beside its [`Color::Code`] and [`Background::Code`].
    pub code: bool,
    pub color: Option<Color>,
    pub bg: Option<Background>,
    /// A syntax marker (`**`, `#`, `[`, …) — dimmed when shown, and removed
    /// entirely when the line's markers are hidden (W6, reveal-on-caret).
    pub hide: bool,
    /// A formatting marker (`**`, `*`, `~~`, `` ` ``, `<mark>`, `<u>`) stays
    /// hidden even with the caret inside its construct — Cditor-style: the
    /// styling IS the feedback, and the toggles (⌘B/I/U/…, the selection
    /// format bar) edit it. Structural markers (links, headings, math) keep
    /// the reveal, since their raw text is the only way to edit them.
    pub always_hide: bool,
    /// Non-zero on a formatting marker: the opener and closer of one construct
    /// share an id, so a host can pair them even with NESTED constructs
    /// between (adjacency no longer identifies the pair).
    pub pair_id: u16,
    /// What a hidden marker paints in its place (an escape's character, an
    /// entity's decoded text): every replacement byte maps back to the span's
    /// start, so the display↔source maps stay consistent. `None` = plain
    /// removal.
    pub replace: Option<String>,
    /// Decoded source text, unlike generated replacement labels.
    pub decoded: bool,
}

impl SpanStyle {
    /// `self` layered over `base` (the enclosing construct's style): the
    /// booleans OR, the options prefer `self`'s — so `code` inside bold keeps
    /// its color/tint while inheriting the bold weight.
    fn over(mut self, base: &SpanStyle) -> SpanStyle {
        self.bold |= base.bold;
        self.italic |= base.italic;
        self.strike |= base.strike;
        self.underline |= base.underline;
        self.code |= base.code;
        if self.color.is_none() {
            self.color = base.color;
        }
        if self.bg.is_none() {
            self.bg = base.bg;
        }
        self
    }
}

/// One classified piece of a line: a source byte range and its style. Spans
/// from one scan don't overlap; see [`scan_line`] for their order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: SpanStyle,
}

/// Scan the whole document line by line for inline markdown constructs; lines
/// come in order, spans within a line as [`scan_line`] leaves them.
pub fn scan(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut line_start = 0;
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b'\n' {
            scan_line(text, line_start, i, &mut out);
            line_start = i + 1;
        }
    }
    out
}

fn marker(out: &mut Vec<Span>, range: Range<usize>) {
    out.push(Span {
        range,
        style: SpanStyle {
            color: Some(Color::Marker),
            hide: true,
            ..Default::default()
        },
    });
}

/// A formatting marker that NEVER reveals (not even with the caret inside its
/// construct) — see [`SpanStyle::always_hide`]. The construct's opener and
/// closer share `pair_id`.
fn fmt_marker(out: &mut Vec<Span>, range: Range<usize>, pair_id: u16) {
    out.push(Span {
        range,
        style: SpanStyle {
            color: Some(Color::Marker),
            hide: true,
            always_hide: true,
            pair_id,
            ..Default::default()
        },
    });
}

/// Recursively scan a formatting construct's body: inner constructs nest
/// (bold inside italic, code inside bold, …) with `style` layered under them,
/// and the gaps — the construct's own plain text — carry `style` itself.
/// Depth-capped; at the cap the body is one flat styled span.
#[allow(clippy::too_many_arguments)]
fn scan_styled_body(
    text: &str,
    s: usize,
    e: usize,
    out: &mut Vec<Span>,
    style: SpanStyle,
    depth: u8,
    next_id: &mut u16,
) {
    if s >= e {
        return;
    }
    if depth >= 3 {
        push(out, s..e, style);
        return;
    }
    let before = out.len();
    scan_inline(text, s, e, out, &style, depth + 1, next_id);
    // Fill the gaps between the inner constructs with the body style.
    let mut covered: Vec<Range<usize>> = out[before..].iter().map(|sp| sp.range.clone()).collect();
    covered.sort_by_key(|r| r.start);
    let mut pos = s;
    let mut fills: Vec<Range<usize>> = Vec::new();
    for r in covered {
        if r.start > pos {
            fills.push(pos..r.start);
        }
        pos = pos.max(r.end);
    }
    if pos < e {
        fills.push(pos..e);
    }
    for f in fills {
        push(out, f, style.clone());
    }
}

/// Heading styling (`#`..`######` + a space) for `text[from..end]`: dim the
/// marker, bold the rest. The larger heading SIZE is applied per line at
/// layout time (variable line heights), not here — so the rest of the line
/// isn't scanned for inline constructs in W2. (W2) Returns whether it matched,
/// so callers can `return` and skip inline scanning the same as a bare
/// heading line. Shared by the line-start check and the post-list-marker
/// check (`- ### Heading` nests like the reading view's AST does).
fn apply_heading(text: &str, from: usize, end: usize, out: &mut Vec<Span>) -> bool {
    let Some(level) = heading_level(&text[from..end]) else {
        return false;
    };
    let b = text.as_bytes();
    let mut marker_end = from + level as usize;
    if marker_end < end && b[marker_end] == b' ' {
        marker_end += 1;
    }
    marker(out, from..marker_end);
    let mut next_id = 1;
    scan_styled_body(
        text,
        marker_end,
        end,
        out,
        SpanStyle {
            bold: true,
            ..Default::default()
        },
        0,
        &mut next_id,
    );
    true
}

/// Scan one line `text[start..end]`, appending its spans to `out`. The spans
/// don't overlap but are NOT in source order: a construct's plain-text gaps
/// are appended after its nested constructs, so sort by `range.start` before
/// walking them in order. Markers are ASCII, so byte scanning is UTF-8-safe
/// (an ASCII byte never appears inside a multi-byte char).
pub fn scan_line(text: &str, start: usize, end: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    if apply_heading(text, start, end, out) {
        return;
    }
    // Blockquote: leading `>` (GFM nesting) + optional spaces. Hide the markers;
    // the body keeps inline styling over a muted base color (set by the caller).
    let mut i = start;
    if b.get(start) == Some(&b'>') {
        let mut p = start;
        while p < end && b[p] == b'>' {
            p += 1;
            if p < end && b[p] == b' ' {
                p += 1;
            }
        }
        // A GitHub alert's `[!NOTE]`-style marker hides with the quote prefix —
        // the paint draws a bold colored label in its place (LineMark::Alert).
        if let Some((_, mlen, _)) = alert_prefix(&text[p..end]) {
            p += mlen;
        }
        marker(out, start..p);
        i = p;
    } else if let Some(prefix_len) = task_prefix(&text[start..end])
        .map(|(p, ..)| p)
        .or_else(|| list_prefix(&text[start..end]).map(|(p, ..)| p))
    {
        // List / task item: hide the leading whitespace + marker (+ checkbox); a
        // bullet/number/box is painted in its place, body keeps inline styling.
        marker(out, start..start + prefix_len);
        i = start + prefix_len;
        // A heading right after the marker (`- ### Notes`) — same treatment as
        // a top-level heading, skipping inline scanning past it.
        if apply_heading(text, i, end, out) {
            return;
        }
    }
    let mut next_id = 1u16;
    scan_inline(text, i, end, out, &SpanStyle::default(), 0, &mut next_id);
}

/// The inline scanning loop, recursive so formatting constructs NEST: a
/// construct's body re-scans with the construct's style as `base` (layered by
/// [`SpanStyle::over`]), matching the reader's CommonMark nesting — `**bold
/// *italic* bold**` styles the inner run bold+italic. `start` is the segment
/// start (word-boundary and image-bang checks are segment-local).
#[allow(clippy::too_many_arguments)]
fn scan_inline(
    text: &str,
    start: usize,
    end: usize,
    out: &mut Vec<Span>,
    base: &SpanStyle,
    depth: u8,
    next_id: &mut u16,
) {
    let b = text.as_bytes();
    let mut i = start;
    while i < end {
        let c = b[i];
        // A backslash escape (`\*`, `\|`, `\\`, … — any ASCII punctuation):
        // the backslash is a hideable marker (the reader consumes it), the
        // escaped character is literal content — skip it so no construct
        // opens on it. Reveal-on-caret shows the backslash for editing.
        if c == b'\\'
            && i + 1 < end
            && b[i + 1].is_ascii_punctuation()
            && !is_backslash_escaped(b, i)
        {
            push(
                out,
                i..i + 2,
                SpanStyle {
                    hide: true,
                    replace: Some(text[i + 1..i + 2].to_owned()),
                    decoded: true,
                    ..Default::default()
                }
                .over(base),
            );
            i += 2;
            continue;
        }
        // A hard line break (`\` ending a line with text before it, as ⇧Enter
        // writes, ADR 0060): the reader consumes the backslash, so it hides
        // like any marker and the caret reveals it.
        if c == b'\\'
            && b.get(i + 1).is_none_or(|&n| n == b'\n')
            && !is_backslash_escaped(b, i)
            && text[..i]
                .rsplit('\n')
                .next()
                .is_some_and(|before| !before.trim().is_empty())
        {
            push(
                out,
                i..i + 1,
                SpanStyle {
                    hide: true,
                    ..Default::default()
                }
                .over(base),
            );
            i += 1;
            continue;
        }
        // Entities share one decoded replacement and source map with search.
        if c == b'&'
            && let Some((len, decoded)) = decode_entity(&text[i..end])
        {
            push(
                out,
                i..i + len,
                SpanStyle {
                    hide: true,
                    replace: Some(decoded),
                    decoded: true,
                    ..Default::default()
                }
                .over(base),
            );
            i += len;
            continue;
        }
        // Inline code: `code` — backticks are hideable markers, the body a
        // highlight (code color on a tint, body font) matching the reading
        // view. CommonMark run matching: an opening run of N backticks closes
        // at the next run of EXACTLY N, so ``a`b`` keeps its inner backtick
        // (backslashes are literal INSIDE a span; only the opener can be
        // escaped).
        if c == b'`' && !is_backslash_escaped(b, i) {
            let mut n = 1;
            while i + n < end && b[i + n] == b'`' {
                n += 1;
            }
            let mut j = i + n;
            let mut close = None;
            while j < end {
                if b[j] == b'`' {
                    let mut m = 1;
                    while j + m < end && b[j + m] == b'`' {
                        m += 1;
                    }
                    if m == n {
                        close = Some(j);
                        break;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            if let Some(close) = close {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + n, id);
                push(
                    out,
                    i + n..close,
                    SpanStyle {
                        color: Some(Color::Code),
                        bg: Some(Background::Code),
                        code: true,
                        ..Default::default()
                    }
                    .over(base),
                );
                fmt_marker(out, close..close + n, id);
                i = close + n;
                continue;
            }
            // No matching run: the whole backtick STRING is one token
            // (CommonMark) — skip past it so a shorter run inside it can't
            // start a span the reader wouldn't.
            i += n;
            continue;
        }
        // Bold: **text** (check before single-* italic). A `\*` (escaped)
        // opener or closer is literal text, matching the reader's CommonMark.
        if c == b'*'
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'*'
            && let Some(close) = find2_unescaped(b, i + 2, end, b'*', b'*')
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                out,
                SpanStyle {
                    bold: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, id);
            i = close + 2;
            continue;
        }
        // Italic: *text* — escaped `\*` stays literal.
        if c == b'*'
            && !is_backslash_escaped(b, i)
            && let Some(close) = find1_unescaped(b, i + 1, end, b'*')
            && close > i + 1
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 1, id);
            scan_styled_body(
                text,
                i + 1,
                close,
                out,
                SpanStyle {
                    italic: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 1, id);
            i = close + 1;
            continue;
        }
        // Underscore emphasis (__bold__ / _italic_), CommonMark's flanking
        // rules so snake_case identifiers never light up: `_` only opens
        // after a non-word character (or the segment start) and only closes
        // before one, and a backslash-escaped `\_` (the shruggie's arms)
        // stays literal. The reader's CommonMark parser applies the same
        // word-boundary rule, so the views agree.
        if c == b'_' && (i == start || (!is_word_byte(b[i - 1]) && b[i - 1] != b'\\')) {
            // __bold__ (check before single-_ italic).
            if i + 1 < end
                && b[i + 1] == b'_'
                && i + 2 < end
                && b[i + 2] != b' '
                && let Some(close) = find_underscore_close(b, i + 2, end, true)
            {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + 2, id);
                scan_styled_body(
                    text,
                    i + 2,
                    close,
                    out,
                    SpanStyle {
                        bold: true,
                        ..Default::default()
                    }
                    .over(base),
                    depth,
                    next_id,
                );
                fmt_marker(out, close..close + 2, id);
                i = close + 2;
                continue;
            }
            if i + 1 < end
                && b[i + 1] != b'_'
                && b[i + 1] != b' '
                && let Some(close) = find_underscore_close(b, i + 1, end, false)
            {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + 1, id);
                scan_styled_body(
                    text,
                    i + 1,
                    close,
                    out,
                    SpanStyle {
                        italic: true,
                        ..Default::default()
                    }
                    .over(base),
                    depth,
                    next_id,
                );
                fmt_marker(out, close..close + 1, id);
                i = close + 1;
                continue;
            }
        }
        // Highlight: ==text== — the pairing rules are the reading view's
        // (`syntax::highlight_close`), so `a == b == c` stays literal in both.
        if c == b'='
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'='
            && let Some(close) = highlight_close(&text[..end], i)
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                out,
                SpanStyle {
                    bg: Some(Background::Mark),
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, id);
            i = close + 2;
            continue;
        }
        // Strikethrough: ~~text~~
        if c == b'~'
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'~'
            && let Some(close) = find2_unescaped(b, i + 2, end, b'~', b'~')
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                out,
                SpanStyle {
                    strike: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, id);
            i = close + 2;
            continue;
        }
        // Footnote reference: [^label] — rendered `[label]` in link color (the
        // `^` hidden), matching the reading view's resolved marker.
        if c == b'['
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'^'
            && let Some(rb) = find1(b, i + 2, end, b']')
            && rb > i + 2
        {
            push(
                out,
                i..i + 1,
                SpanStyle {
                    color: Some(Color::Link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, i + 1..i + 2); // hide the `^`
            push(
                out,
                i + 2..rb + 1,
                SpanStyle {
                    color: Some(Color::Link),
                    ..Default::default()
                }
                .over(base),
            );
            i = rb + 1;
            continue;
        }
        // Link: [text](url) — or an image `![alt](src)` (the caret's own line
        // falls through to this generic scan, since the block-image widget is
        // suppressed there — W4's image_row() only handles other lines). Treated
        // the same: `text`/`alt` colored, brackets dimmed — plus the `!` for an
        // image, so an empty-alt `![](src)` hides cleanly instead of leaving a
        // bare unhidden `!`.
        if c == b'['
            && !is_backslash_escaped(b, i)
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'('
            && let Some(rp) = find1(b, rb + 2, end, b')')
        {
            let marker_start = if i > start && b[i - 1] == b'!' {
                i - 1
            } else {
                i
            };
            marker(out, marker_start..i + 1);
            push(
                out,
                i + 1..rb,
                SpanStyle {
                    color: Some(Color::Link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, rb..rp + 1);
            i = rp + 1;
            continue;
        }
        // Reference link: [text][id] — `text` colored, brackets + `[id]` dimmed.
        // (Inline `[text](url)` is handled just above; this is the `][` form.)
        if c == b'['
            && !is_backslash_escaped(b, i)
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'['
            && let Some(rb2) = find1(b, rb + 2, end, b']')
        {
            marker(out, i..i + 1);
            push(
                out,
                i + 1..rb,
                SpanStyle {
                    color: Some(Color::Link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, rb..rb2 + 1);
            i = rb2 + 1;
            continue;
        }
        // Styled inline HTML — `<mark>` / `<mark style="background:…">`,
        // `<span style="color:…">`, `<u>` — the tags hidden, the body styled.
        // Recognition is the reading view's (`syntax::styled_tag`), so the two
        // views can't disagree about which tags count.
        if c == b'<'
            && let Some(len) = inline_tag_len(&text[i..end])
            && let Some(tag) = styled_tag(&text[i..i + len])
            && let Some((rel, close_len)) = matching_styled_close(&text[i + len..end], tag.kind)
        {
            let body = i + len;
            let close = body + rel;
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..body, id);
            let style = match tag.kind {
                StyledKind::Mark => SpanStyle {
                    bg: Some(tag.background.map_or(Background::Mark, Background::Rgba)),
                    ..Default::default()
                },
                StyledKind::Span => SpanStyle {
                    color: tag.color.map(Color::Rgba),
                    bg: tag.background.map(Background::Rgba),
                    ..Default::default()
                },
                StyledKind::Underline => SpanStyle {
                    underline: true,
                    ..Default::default()
                },
            };
            scan_styled_body(text, body, close, out, style.over(base), depth, next_id);
            fmt_marker(out, close..close + close_len, id);
            i = close + close_len;
            continue;
        }
        // Bare URL: colored like a link (it clicks like one — see the shared
        // `links()` grammar; `url_end` keeps styling and hit-tests identical).
        // Compare BYTES: `i` walks bytes, so a str slice here would panic
        // mid-char on any non-ASCII text (`¯\_(ツ)_/¯` did, v0.5.0 dev).
        if (b[i..end].starts_with(b"http://") || b[i..end].starts_with(b"https://"))
            && (i == start || !is_word(b[i - 1]))
        {
            let j = i + url_end(&text[i..end], 0);
            if j > i + 8 {
                push(
                    out,
                    i..j,
                    SpanStyle {
                        color: Some(Color::Link),
                        ..Default::default()
                    }
                    .over(base),
                );
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

fn push(out: &mut Vec<Span>, range: Range<usize>, style: SpanStyle) {
    out.push(Span { range, style });
}

/// First index of byte `c` in `b[from..end]`.
fn find1(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&k| b[k] == c)
}

/// First UNESCAPED index of byte `c` — a `\`-escaped occurrence (odd
/// preceding backslash run) is literal text, not a marker (CommonMark).
fn find1_unescaped(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&k| b[k] == c && !is_backslash_escaped(b, k))
}

/// First UNESCAPED index of the pair — see [`find1_unescaped`].
fn find2_unescaped(b: &[u8], from: usize, end: usize, c1: u8, c2: u8) -> Option<usize> {
    (from..end.saturating_sub(1))
        .find(|&k| b[k] == c1 && b[k + 1] == c2 && !is_backslash_escaped(b, k))
}

/// A "word" byte for `_`-emphasis flanking: ASCII alphanumerics, plus any
/// non-ASCII byte — accented/CJK letters count as word characters, so
/// intraword underscores stay literal (CommonMark's rule for `_`).
fn is_word_byte(x: u8) -> bool {
    x.is_ascii_alphanumeric() || x >= 0x80
}

/// The closing `_` (or `__`) for an underscore-emphasis span whose content
/// starts at `from`: the first candidate with non-empty content that doesn't
/// end in a space, that isn't backslash-escaped, and whose following
/// character is not a word character (CommonMark right-flank). `None` if the
/// span never closes.
fn find_underscore_close(b: &[u8], from: usize, end: usize, double: bool) -> Option<usize> {
    let width = if double { 2 } else { 1 };
    let mut j = from + 1; // content is at least one byte
    while j + width <= end {
        if b[j] == b'_'
            && (!double || b[j + 1] == b'_')
            && b[j - 1] != b' '
            && b[j - 1] != b'\\'
            && (j + width >= end || !is_word_byte(b[j + width]))
        {
            return Some(j);
        }
        j += 1;
    }
    None
}
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

// One tag grammar with the reader (namespaced `#a/b` included).

fn decode_entity(source: &str) -> Option<(usize, String)> {
    if source.as_bytes().first() != Some(&b'&') {
        return None;
    }
    let relative_end = source
        .as_bytes()
        .iter()
        .take(33)
        .position(|byte| *byte == b';')?;
    let entity_end = relative_end + 1;
    let token = &source[..entity_end];
    if let Some(value) = decode_numeric_entity(token) {
        return Some((token.len(), value.to_string()));
    }
    entities::ENTITIES
        .iter()
        .find(|entity| entity.entity == token)
        .map(|entity| (token.len(), entity.characters.to_owned()))
}

fn decode_numeric_entity(token: &str) -> Option<char> {
    let digits = token
        .strip_prefix("&#x")
        .or_else(|| token.strip_prefix("&#X"))
        .map(|digits| digits.strip_suffix(';').unwrap_or(digits))
        .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        .or_else(|| {
            token
                .strip_prefix("&#")
                .and_then(|digits| digits.strip_suffix(';').unwrap_or(digits).parse().ok())
        })?;
    char::from_u32(digits)
}

/// Whether the byte at `i` is escaped: preceded by an ODD number of `\`s.
pub fn is_backslash_escaped(b: &[u8], i: usize) -> bool {
    let mut n = 0;
    while n < i && b[i - 1 - n] == b'\\' {
        n += 1;
    }
    n % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The styled substrings a line scans to, as `(text, bold, italic)` —
    /// markers excluded (they carry a color, not bold/italic).
    fn emphasis_spans(line: &str) -> Vec<(String, bool, bool)> {
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &mut out);
        out.iter()
            .filter(|sp| sp.style.bold || sp.style.italic)
            .map(|sp| {
                (
                    line[sp.range.clone()].to_string(),
                    sp.style.bold,
                    sp.style.italic,
                )
            })
            .collect()
    }

    #[test]
    fn underscore_emphasis_at_word_boundaries() {
        assert_eq!(
            emphasis_spans("_italic_ and __bold__"),
            vec![
                ("italic".to_string(), false, true),
                ("bold".to_string(), true, false)
            ]
        );
        assert_eq!(
            emphasis_spans("mid _span here_ end"),
            vec![("span here".to_string(), false, true)]
        );
    }

    #[test]
    fn underscores_inside_words_stay_plain() {
        // The reason `_` was originally skipped — identifiers must not light up.
        assert_eq!(emphasis_spans("snake_case_name"), vec![]);
        assert_eq!(emphasis_spans("a foo_bar_baz b"), vec![]);
        assert_eq!(emphasis_spans("CONST_MAX_VALUE = 3"), vec![]);
        // Closing candidate followed by a word character doesn't close.
        assert_eq!(emphasis_spans("_a_b"), vec![]);
        // Space-adjacent content never opens/closes.
        assert_eq!(emphasis_spans("_ nope_"), vec![]);
        assert_eq!(emphasis_spans("_nope _"), vec![]);
    }

    #[test]
    fn escaped_underscores_stay_literal() {
        // The shruggie's arms are backslash-escaped underscores.
        assert_eq!(emphasis_spans(r"¯\_(ツ)_/¯"), vec![]);
        assert_eq!(emphasis_spans(r"\_not italic\_"), vec![]);
    }

    #[test]
    fn double_equals_highlight_hides_markers_and_tints_the_body() {
        let text = "an ==important== word, a == b == c";
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &mut out);
        let open = 3;
        let body = open + 2;
        let close = text.find("== word").unwrap();
        assert!(out.iter().any(|s| s.range == (open..body) && s.style.hide));
        assert!(
            out.iter()
                .any(|s| s.range == (close..close + 2) && s.style.hide)
        );
        assert!(
            out.iter()
                .any(|s| s.range.start == body && s.style.bg == Some(Background::Mark))
        );
        // The spaced `==`s after it are prose: nothing hidden past the closer.
        assert!(
            !out.iter()
                .any(|s| s.range.start > close + 2 && s.style.hide)
        );
    }

    #[test]
    fn colored_mark_and_span_tags_style_their_body() {
        let text =
            r#"a <mark style="background:#ff0000">hi</mark> <span style="color:#00ff00">go</span>"#;
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &mut out);
        let hi = text.find("hi<").unwrap();
        let go = text.find("go<").unwrap();
        assert!(
            out.iter()
                .any(|s| s.range.start == hi && s.style.bg == Some(Background::Rgba(0xff0000ff)))
        );
        assert!(
            out.iter()
                .any(|s| s.range.start == go && s.style.color == Some(Color::Rgba(0x00ff00ff)))
        );
        // Both tag pairs are hidden markers.
        assert!(
            out.iter()
                .any(|s| s.range.start == 2 && s.range.end == hi && s.style.hide)
        );
        assert!(
            out.iter()
                .any(|s| s.range == (hi + 2..hi + 9) && s.style.hide)
        );
    }

    #[test]
    fn underline_tags_hide_and_style_the_body() {
        let text = "an <u>underlined</u> word";
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &mut out);
        // Tags are hidden markers; the body span carries the underline.
        let open = text.find("<u>").unwrap();
        let body = open + 3;
        assert!(out.iter().any(|s| s.range == (open..body) && s.style.hide));
        assert!(
            out.iter()
                .any(|s| s.range.start == body && s.style.underline)
        );
    }

    #[test]
    fn scan_line_survives_multibyte_text() {
        // Regression: byte-wise scanning once str-sliced at continuation
        // bytes and panicked on this exact line.
        let text = "- I don't know what is wrong ¯\\_(ツ)_/¯ https://a.io";
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &mut out);
        // The trailing URL still gets recognized (spans cover it).
        assert!(
            out.iter()
                .any(|s| s.range.start == text.find("https").unwrap())
        );
    }

    #[test]
    fn backtick_runs_match_commonmark() {
        // ``a`b`` — a double-backtick span keeps its inner backtick as body.
        let line = "``a`b`` x";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &mut out);
        assert!(
            out.iter()
                .any(|s| s.range == (2..5) && s.style.bg.is_some()),
            "body a`b styled as code"
        );
        // An unclosed run stays plain (no half-matched span).
        let mut out = Vec::new();
        scan_line("``a` x", 0, 6, &mut out);
        assert!(out.iter().all(|s| s.style.bg.is_none()));
    }

    #[test]
    fn backslash_escapes_stay_literal() {
        // \*text\* must NOT style — the reader shows literal asterisks.
        for line in [
            r"\*text\*",
            r"a \`x\` b",
            r"\~~s\~~",
            r"\[[Page]]",
            r"\[t](u)",
        ] {
            let mut out = Vec::new();
            scan_line(line, 0, line.len(), &mut out);
            // Escape backslashes become (hidden) marker spans; nothing may
            // carry actual STYLING.
            assert!(
                out.iter().all(|s| !s.style.bold
                    && !s.style.italic
                    && !s.style.strike
                    && s.style.bg.is_none()
                    && (s.style.hide || s.style.color.is_none())),
                "{line:?} styled: {:?}",
                out.iter().map(|s| s.range.clone()).collect::<Vec<_>>()
            );
        }
        // An escaped closer doesn't close: **a \** stays unstyled (no pair),
        // while the reader treats it the same.
        let mut out = Vec::new();
        let line = r"**a \** x";
        scan_line(line, 0, line.len(), &mut out);
        assert!(out.iter().all(|s| !s.style.bold), "escaped closer ignored");
    }

    #[test]
    fn nested_inline_styles_layer() {
        // **bold *italic* bold** — the inner run carries BOTH styles, the
        // gaps around it stay bold (the reader nests the same way).
        let line = "**bold *it* bold**";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &mut out);
        let styled = |s: usize, e: usize| out.iter().find(|sp| sp.range == (s..e)).unwrap();
        let inner = styled(8, 10); // "it"
        assert!(inner.style.bold && inner.style.italic);
        let gap = styled(2, 7); // "bold " before the italic
        assert!(gap.style.bold && !gap.style.italic);
        // Inline code inside bold keeps its tint and inherits the weight.
        let line = "**b `c` d**";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &mut out);
        let code = out
            .iter()
            .find(|sp| sp.range == (5..6))
            .expect("code body span");
        assert!(code.style.bold && code.style.bg == Some(Background::Code));
    }

    #[test]
    fn prefixes_and_links_carry_their_kinds() {
        // Every hidden prefix is a marker span coloured as a marker.
        for (line, prefix) in [
            ("## Hi", 3),
            ("> quote", 2),
            ("> [!NOTE] body", 10),
            ("  - item", 4),
            ("- [x] done", 6),
        ] {
            let mut out = Vec::new();
            scan_line(line, 0, line.len(), &mut out);
            assert_eq!(out[0].range, 0..prefix, "{line:?}");
            assert!(out[0].style.hide && out[0].style.color == Some(Color::Marker));
        }
        // A heading's body is bold; a link's text is a link between markers.
        let spans = scan("# T\n[a](u)");
        assert!(spans.iter().any(|s| s.range == (2..3) && s.style.bold));
        let link = spans.iter().find(|s| s.range == (5..6)).unwrap();
        assert_eq!(link.style.color, Some(Color::Link));
        assert!(!link.style.hide);
        assert!(spans.iter().any(|s| s.range == (6..10) && s.style.hide));
        // Inline code is code-coloured on its tint, between always-hidden
        // markers that share a pair id.
        let spans = scan("`c`");
        assert_eq!(spans[1].style.color, Some(Color::Code));
        assert!(spans[1].style.code);
        assert_eq!(spans[0].style.pair_id, spans[2].style.pair_id);
        assert!(spans[0].style.always_hide && spans[2].style.always_hide);
        // Entities and escapes decode to their replacement text.
        let spans = scan(r"&amp; \*");
        assert_eq!(spans[0].style.replace.as_deref(), Some("&"));
        assert_eq!(spans[1].style.replace.as_deref(), Some("*"));
        assert!(spans.iter().all(|s| s.style.decoded && s.style.hide));
    }
}
