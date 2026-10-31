//! Inline-markdown styling for live-preview ("WYSIWYG") editing — W1.
//!
//! Recognition lives in the GUI-free `mdoc-markdown` crate, which classifies
//! the source into spans **without changing the byte buffer**: syntax markers
//! (`**`, `` ` ``, `[` …) are kept in the text and merely dimmed, so the
//! displayed characters still match the buffer one-to-one and the
//! caret/offset model is untouched. This module maps those spans to GPUI text
//! runs in the host's [`SyntaxStyle`], and keeps the editor-only Markdown
//! helpers (folding, list numbering, images, math).
//!
//! Scope (W1): inline constructs at the editor's normal text size — bold,
//! italic, strikethrough, inline code, links, wiki-links, and tags. Heading
//! sizes need per-line font sizes (variable line heights) and are deferred to
//! W2; block widgets (images, fenced code, tables, …) to W4.

use std::ops::Range;

use gpui::{
    Font, FontStyle, FontWeight, Hsla, StrikethroughStyle, TextRun, UnderlineStyle, hsla, px,
};
use mdoc_markdown::spans::{Background, Color, Span, SpanStyle, scan, scan_line};

use crate::Diagnostic;

// Recognition shared with the mobile apps (ADR 0085), re-exported under the
// names the editor's modules already use.
pub(crate) use mdoc_markdown::lines::{
    blockquote_prefix, heading_level, list_prefix, task_prefix, thematic_break,
};
pub(crate) use mdoc_markdown::spans::is_backslash_escaped;
pub(crate) use mdoc_markdown::syntax::{TableStyle, table_style_marker};
pub(crate) use mdoc_markdown::tables::{
    Align, TableRegion, is_table_separator, table_cell_ranges, table_cells, table_regions,
};

/// Colors + monospace font for inline markdown styling, supplied by the host so
/// the editor stays theme-agnostic. Install via
/// [`crate::EditorState::set_markdown_style`]; absent it, the editor renders
/// plain text (only spell-check underlines).
#[derive(Clone)]
pub struct SyntaxStyle {
    /// Dimmed color for the syntax markers themselves (`**`, `*`, `~~`, `[`,
    /// `]`, `[[`, `]]`, `]( … )`).
    pub marker: Hsla,
    /// Inline `code` text color.
    pub code: Hsla,
    /// Inline `code` background.
    pub code_bg: Hsla,
    /// `[text](url)` links.
    pub link: Hsla,
    /// Blockquote text + left-border color (a muted tone).
    pub quote: Hsla,
    /// GitHub-style alert (`> [!NOTE]` …) border + marker colors, per kind.
    pub alert_note: Hsla,
    pub alert_tip: Hsla,
    pub alert_important: Hsla,
    pub alert_warning: Hsla,
    pub alert_caution: Hsla,
    /// Thematic break (`---`) divider color.
    pub rule: Hsla,
    /// `<mark>` highlight background.
    pub mark_bg: Hsla,
    /// Popover/menu surface background (e.g. the right-click table menu).
    pub popover_bg: Hsla,
    /// Popover/menu border.
    pub popover_border: Hsla,
    /// Popover/menu foreground text.
    pub popover_fg: Hsla,
    /// Popover/menu hovered-row background (a soft accent tint).
    pub popover_hover: Hsla,
    /// Popover/menu group divider.
    pub popover_divider: Hsla,
    /// Popover/menu destructive rows ("Delete …" — label + glyph in red).
    pub popover_danger: Hsla,
    /// Monospace font for inline code.
    pub mono: Font,
}

impl SyntaxStyle {
    /// The themed color for a span's [`Color`].
    fn color(&self, color: Color) -> Hsla {
        match color {
            Color::Marker => self.marker,
            Color::Code => self.code,
            Color::Link => self.link,
            Color::Rgba(rgba) => hsla_of(rgba),
        }
    }

    /// The themed background for a span's [`Background`].
    fn background(&self, bg: Background) -> Hsla {
        match bg {
            Background::Code => self.code_bg,
            Background::Mark => self.mark_bg,
            Background::Rgba(rgba) => hsla_of(rgba),
        }
    }
}

/// `0xRRGGBBAA` (from `syntax::css_color`) as a gpui color.
fn hsla_of(rgba: u32) -> Hsla {
    gpui::rgba(rgba).into()
}

/// Build the editor's text runs: the base style, plus inline-markdown styling
/// when `md` is `Some`, plus a red wavy underline on each diagnostic span.
///
/// With `md = None` and no diagnostics this is a single plain run, so it
/// subsumes the former diagnostics-only builder.
pub(crate) fn styled_runs(
    text: &str,
    base_font: &Font,
    base_color: Hsla,
    diagnostics: &[Diagnostic],
    md: Option<&SyntaxStyle>,
) -> Vec<TextRun> {
    let spans = md.map(|_| scan(text)).unwrap_or_default();
    let squiggle = UnderlineStyle {
        color: Some(hsla(0., 0.8, 0.55, 1.)),
        thickness: px(1.5),
        wavy: true,
    };

    // Every point where styling can change: span + diagnostic edges (clamped).
    let mut bounds: Vec<usize> = vec![0, text.len()];
    for s in &spans {
        bounds.push(s.range.start);
        bounds.push(s.range.end);
    }
    for d in diagnostics {
        if d.range.start < d.range.end && d.range.end <= text.len() {
            bounds.push(d.range.start);
            bounds.push(d.range.end);
        }
    }
    bounds.retain(|&b| b <= text.len());
    bounds.sort_unstable();
    bounds.dedup();

    let mut runs = Vec::new();
    for win in bounds.windows(2) {
        let (a, b) = (win[0], win[1]);
        if a >= b {
            continue;
        }
        // Spans don't overlap, so the first covering one is THE one.
        let style = spans
            .iter()
            .find(|s| s.range.start <= a && a < s.range.end)
            .map(|s| &s.style);
        let underline = diagnostics
            .iter()
            .any(|d| d.range.start <= a && a < d.range.end && d.range.end <= text.len());
        runs.push(run_for(
            b - a,
            style.zip(md),
            base_font,
            base_color,
            underline.then_some(squiggle),
        ));
    }
    runs
}

/// Build a `TextRun` of byte length `len` from a scanned span's style in the
/// host's theme (or the base style when `None`); `underline` (a diagnostic's
/// squiggle) wins over the span's own. Shared with the hidden-line renderer.
fn run_for(
    len: usize,
    styled: Option<(&SpanStyle, &SyntaxStyle)>,
    base_font: &Font,
    base_color: Hsla,
    underline: Option<UnderlineStyle>,
) -> TextRun {
    let style = styled.map(|(s, _)| s);
    let mut font = base_font.clone();
    if let Some(s) = style {
        if s.bold {
            font.weight = FontWeight::BOLD;
        }
        if s.italic {
            font.style = FontStyle::Italic;
        }
    }
    TextRun {
        len,
        font,
        color: styled
            .and_then(|(s, md)| s.color.map(|c| md.color(c)))
            .unwrap_or(base_color),
        background_color: styled.and_then(|(s, md)| s.bg.map(|b| md.background(b))),
        underline: underline.or_else(|| {
            style.filter(|s| s.underline).map(|_| UnderlineStyle {
                thickness: px(1.0),
                color: None,
                wavy: false,
            })
        }),
        strikethrough: style.filter(|s| s.strike).map(|_| StrikethroughStyle {
            thickness: px(1.5),
            color: None,
        }),
    }
}

/// The maximal run of adjacent spans (a markdown construct: its markers + body)
/// containing source byte `c`, or an empty range when `c` is in plain text.
/// `spans` must be sorted by `range.start` and non-overlapping. Used to reveal
/// only the construct the caret sits in (#5).
fn construct_at(spans: &[Span], c: usize) -> Range<usize> {
    let mut i = 0;
    while i < spans.len() {
        let start = spans[i].range.start;
        let mut end = spans[i].range.end;
        let mut j = i + 1;
        while j < spans.len() && spans[j].range.start == end {
            end = spans[j].range.end;
            j += 1;
        }
        if start <= c && c <= end {
            return start..end;
        }
        i = j;
    }
    0..0
}

/// Render `line` with its syntax markers HIDDEN (W6): returns the display string
/// (source minus the marker chars), the styled runs over it, and a per-display-
/// byte map back to the source byte offset (length `display.len() + 1`, so the
/// end position maps too). Used for every styled line that isn't fully revealed;
/// the construct under `caret_col` (if any) keeps its markers (#5). The caller
/// maps caret/selection columns through the returned map (see `display_col`).
/// Spans don't overlap and cover the line in order, so each non-marker segment
/// contributes one run of its byte length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hidden_runs(
    line: &str,
    base_font: &Font,
    base_color: Hsla,
    diagnostics: &[Diagnostic],
    caret_col: Option<usize>,
    reveal_prefix: usize,
    // Bytes whose marker spans stay hidden even inside the caret's construct
    // — the line-level prefix while its gutter mark (bullet/number/box) is
    // painted, else the glyph and the raw marker would BOTH show.
    hide_prefix: usize,
    // Show every inline marker (a selection touches this line, so the
    // highlighted glyphs must be the copied bytes) while the prefix stays
    // hidden behind its painted mark.
    reveal_inline: bool,
    md: &SyntaxStyle,
) -> (String, Vec<TextRun>, Vec<usize>) {
    let mut spans = scan(line);
    spans.sort_by_key(|s| s.range.start);

    // The construct (a maximal run of adjacent spans: its markers + body) the
    // caret sits in keeps its markers visible — everything else hides them (#5,
    // per-construct reveal). Empty range when the caret is in plain text / absent.
    let reveal = caret_col.map_or(0..0, |c| construct_at(&spans, c));

    // The visible segments (markers dropped), each a source byte range + style
    // (+ a replacement string when a hidden marker paints something in its
    // place). A visible segment is copied verbatim, so source↔display is 1:1
    // within it — which lets a diagnostic (source coords) map straight onto
    // the display.
    let mut segs: Vec<(Range<usize>, Option<&SpanStyle>, Option<&str>)> = Vec::new();
    let mut pos = 0;
    let mut k = 0;
    while k < spans.len() {
        let span = &spans[k];
        k += 1;
        if span.range.start > pos {
            segs.push((pos..span.range.start, None, None));
        }
        // A marker is shown when it's inside the caret's construct OR within the
        // revealed line-level prefix (e.g. a blockquote's `>` while the caret is
        // anywhere on the line).
        let in_construct = reveal.start <= span.range.start && span.range.end <= reveal.end;
        let in_prefix = span.range.end <= reveal_prefix;
        let force = span.range.end <= hide_prefix;
        let hidden = span.style.hide
            && (span.style.always_hide || force || (!reveal_inline && !in_construct && !in_prefix));
        if !hidden {
            segs.push((span.range.clone(), Some(&span.style), None));
        } else if let Some(rep) = span.style.replace.as_deref() {
            segs.push((span.range.clone(), Some(&span.style), Some(rep)));
        } else if let Some(ch) = spans
            .get(k)
            .filter(|ch| span.style.escape && ch.range.start == span.range.end)
        {
            // A hidden escape paints its character in the backslash's place,
            // so a click before it lands before the whole escape.
            segs.push((
                span.range.start..ch.range.end,
                Some(&ch.style),
                Some(&line[ch.range.clone()]),
            ));
            pos = ch.range.end;
            k += 1;
            continue;
        }
        pos = span.range.end;
    }
    if pos < line.len() {
        segs.push((pos..line.len(), None, None));
    }

    let squiggle = UnderlineStyle {
        color: Some(hsla(0., 0.8, 0.55, 1.)),
        thickness: px(1.5),
        wavy: true,
    };
    let mut display = String::with_capacity(line.len());
    let mut runs: Vec<TextRun> = Vec::new();
    let mut map: Vec<usize> = Vec::with_capacity(line.len() + 1);
    for (src, style, replace) in &segs {
        // A replacement paints in the hidden marker's place: its bytes all map
        // back to the span's start (so caret/click math lands on the marker),
        // and it takes one run in the marker's own style.
        if let Some(rep) = replace {
            display.push_str(rep);
            map.extend(std::iter::repeat_n(src.start, rep.len()));
            runs.push(run_for(
                rep.len(),
                style.map(|s| (s, md)),
                base_font,
                base_color,
                None,
            ));
            continue;
        }
        display.push_str(&line[src.clone()]);
        map.extend(src.clone());
        // Split the segment at any diagnostic edges falling inside it, so the
        // covered pieces get a spell-check squiggle (W6 lines kept their markers
        // hidden but were dropping these underlines).
        let mut edges = vec![src.start, src.end];
        for d in diagnostics.iter().filter(|d| d.range.start < d.range.end) {
            for e in [d.range.start, d.range.end] {
                if e > src.start && e < src.end {
                    edges.push(e);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        for w in edges.windows(2) {
            let (a, b) = (w[0], w[1]);
            let under = diagnostics
                .iter()
                .any(|d| d.range.start <= a && a < d.range.end);
            runs.push(run_for(
                b - a,
                style.map(|s| (s, md)),
                base_font,
                base_color,
                under.then_some(squiggle),
            ));
        }
    }
    map.push(line.len());
    (display, runs, map)
}

/// A source span as seen by the Markdown search projection. Unlike
/// [`hidden_runs`], this deliberately has no caret-dependent reveal rules:
/// search indexes the stable rendered text, not the transient editing view.
#[derive(Clone, Debug)]
pub(crate) struct SemanticSpan {
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
pub(crate) fn semantic_spans_with(text: &str, start: usize, end: usize) -> Vec<SemanticSpan> {
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

/// A color/font-neutral style, for tests and for widgets that need a style
/// before the host installs its theme.
pub(crate) fn search_style() -> SyntaxStyle {
    SyntaxStyle {
        marker: Hsla::default(),
        code: Hsla::default(),
        code_bg: Hsla::default(),
        link: Hsla::default(),
        quote: Hsla::default(),
        alert_note: Hsla::default(),
        alert_tip: Hsla::default(),
        alert_important: Hsla::default(),
        alert_warning: Hsla::default(),
        alert_caution: Hsla::default(),
        rule: Hsla::default(),
        mark_bg: Hsla::default(),
        popover_bg: Hsla::default(),
        popover_border: Hsla::default(),
        popover_fg: Hsla::default(),
        popover_hover: Hsla::default(),
        popover_divider: Hsla::default(),
        popover_danger: Hsla::default(),
        mono: Font::default(),
    }
}

/// The always-hidden formatting-marker PAIRS on a line, as
/// `(opening range, closing range)` per construct — for the editor's
/// Cditor-style delete (skip the invisible bytes; collapse an emptied pair).
pub(crate) fn fmt_marker_pairs(line: &str) -> Vec<(Range<usize>, Range<usize>)> {
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

// Linkables (wiki/tag/url/bare-url) are shared with the reader
// (`mdoc_markdown::syntax`) — one grammar for clicks, hover cursors, and
// styling in every renderer.
pub(crate) use mdoc_markdown::syntax::{link_at, links};

/// One ATX heading of a document: its depth, its text without the `#`
/// markers, and the byte offset where its line starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub title: String,
    pub offset: usize,
}

/// The document's top-level ATX headings in order, skipping fenced code.
pub fn headings(content: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    // The open fence's character and length: a fence closes only with the
    // same character, at least as long, and nothing after it.
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in content.split('\n') {
        let start = offset;
        offset += line.len() + 1;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let indent = line.len() - line.trim_start_matches(' ').len();
        let marker = line.trim_start_matches(' ');
        let run = |c: u8| marker.bytes().take_while(|b| *b == c).count();
        let opening = (indent < 4)
            .then(|| {
                b"`~".iter().map(|&c| (c, run(c))).find(|&(c, n)| {
                    // A backtick fence's info string has no backticks, so
                    // ```code``` on one line is inline code.
                    n >= 3 && (c != b'`' || !marker[n..].contains('`'))
                })
            })
            .flatten();
        match (fence, opening) {
            (None, Some(open)) => {
                fence = Some(open);
                continue;
            }
            (Some((c, n)), Some((d, m))) if c == d && m >= n && marker[m..].trim().is_empty() => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        // Up to three spaces of indent; a space or tab after the markers.
        let level = marker.bytes().take_while(|b| *b == b'#').count();
        if indent > 3
            || !(1..=6).contains(&level)
            || !matches!(marker.as_bytes().get(level), None | Some(b' ' | b'\t'))
        {
            continue;
        }
        // A closing sequence (`## Title ##`) needs a space before it, so
        // `## C#` keeps its `#`.
        let rest = marker[level..].trim();
        let stripped = rest.trim_end_matches('#');
        let title =
            if stripped.len() < rest.len() && (stripped.is_empty() || stripped.ends_with(' ')) {
                stripped.trim_end()
            } else {
                rest
            };
        if !title.is_empty() {
            out.push(Heading {
                level: level as u8,
                title: title.to_owned(),
                offset: start,
            });
        }
    }
    out
}

/// [`heading_level`], also matching a heading behind a list marker
/// (`- ### Notes`, the outliner shape). Returns the depth plus the line's
/// indent when it's a LIST heading (`None` = a top-level heading) — list
/// headings fold by indent, not by heading boundaries.
pub(crate) fn line_heading_level(line: &str) -> Option<(u8, Option<usize>)> {
    if let Some(l) = heading_level(line) {
        return Some((l, None));
    }
    let p = list_prefix(line).map(|(p, ..)| p)?;
    let l = heading_level(&line[p..])?;
    Some((l, Some(line.len() - line.trim_start().len())))
}

/// Line ranges (heading line through section end) of collapsed heading
/// sections: `folded` holds the trimmed source lines of folded headings
/// (`## Goals`). A top-level section runs from its heading to the next
/// heading of the same or a higher level, fence-aware — mirrors
/// [`mdoc_markdown::syntax::extract_section`]. A LIST heading's section
/// (`- ### Notes`) is its indented children: it runs while lines sit deeper
/// than the heading's own indent (blank lines included). Every line matching
/// a folded key folds (duplicate headings fold together; the key is the line
/// text).
pub(crate) fn heading_fold_regions(
    content: &str,
    folded: &std::collections::HashSet<String>,
) -> Vec<Range<usize>> {
    if folded.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            in_fence = !in_fence;
            i += 1;
            continue;
        }
        let level = if in_fence {
            None
        } else {
            line_heading_level(lines[i])
        };
        match level {
            Some((l, list_indent)) if folded.contains(lines[i].trim()) => {
                let start = i;
                i += 1;
                while i < lines.len() {
                    if lines[i].trim_start().starts_with("```") {
                        in_fence = !in_fence;
                    } else if !in_fence {
                        let stop = match list_indent {
                            None => heading_level(lines[i]).is_some_and(|n| n <= l),
                            Some(ind) => !lines[i].trim().is_empty() && indent(lines[i]) <= ind,
                        };
                        if stop {
                            break;
                        }
                    }
                    i += 1;
                }
                out.push(start..i);
            }
            _ => i += 1,
        }
    }
    out
}

/// Byte length of the `[^label]:` prefix if `line` is a footnote definition, so
/// the editor can render the whole line muted (the reading view does the same).
/// `None` otherwise.
pub(crate) fn footnote_def(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find(']')?;
    // Label must be non-empty and the `]` immediately followed by `:`.
    (close > 0 && rest[close + 1..].starts_with(':')).then_some(2 + close + 2)
}

/// True if `line` looks like a block-level raw HTML line — `<tag …>`, `</tag>`,
/// or `<!-- … -->` at the start (after optional indentation). The editor renders
/// it muted, matching the reading view (raw HTML is shown literally, never run).
pub(crate) fn html_block(line: &str) -> bool {
    let t = line.trim_start().as_bytes();
    matches!(t, [b'<', rest, ..] if rest.is_ascii_alphabetic() || *rest == b'/' || *rest == b'!')
}

/// Byte length of a blockquote's leading marker — one or more `>` (GFM nesting),
/// each with an optional trailing space — if `line` is a blockquote. `None`
/// otherwise. The editor hides this marker (reveal-on-caret) and renders the line
/// with a muted color + a left border.
// Alert recognition is shared with the reader (`mdoc_markdown::syntax`) —
// what a marker IS lives in one place; this crate only decides how to paint
// it (hide the prefix, label + colored bar, reveal on caret).
pub(crate) use mdoc_markdown::syntax::{AlertKind, alert_prefix};

impl SyntaxStyle {
    /// The themed color for one alert kind.
    pub(crate) fn alert_color(&self, kind: AlertKind) -> Hsla {
        match kind {
            AlertKind::Note => self.alert_note,
            AlertKind::Tip => self.alert_tip,
            AlertKind::Important => self.alert_important,
            AlertKind::Warning => self.alert_warning,
            AlertKind::Caution => self.alert_caution,
        }
    }
}

/// The alert kind if `body` — a blockquote line's text after its `>` prefix —
/// starts with an alert marker (see [`mdoc_markdown::syntax::alert_prefix`]).
pub(crate) fn alert_kind(body: &str) -> Option<AlertKind> {
    alert_prefix(body).map(|(kind, ..)| kind)
}

/// Foldable-callout regions: for each alert whose marker carries a fold char
/// (`> [!NOTE]-` / `+`), the line range it spans (marker + its `>` continuation
/// lines, ending at a blank/non-quote line or the next alert marker) and
/// whether it's folded (`-`). Body lines of a folded region collapse in the
/// WYSIWYG view unless the caret is inside (reveal-on-caret).
pub(crate) fn alert_fold_regions(content: &str) -> Vec<(Range<usize>, bool)> {
    // `Some(fold)` when the line is an alert marker; fold = its fold char.
    fn marker_fold(line: &str) -> Option<Option<bool>> {
        let p = blockquote_prefix(line)?;
        alert_prefix(&line[p..]).map(|(_, _, fold)| fold)
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(Some(folded)) = marker_fold(lines[i]) {
            let start = i;
            i += 1;
            while i < lines.len()
                && blockquote_prefix(lines[i]).is_some()
                && marker_fold(lines[i]).is_none()
            {
                i += 1;
            }
            out.push((start..i, folded));
        } else {
            i += 1;
        }
    }
    out
}

/// `(position, nesting level)` of every ordered list item, both 0-based-
/// level / 1-based-position. Word-style, NOT CommonMark: every list counts
/// from 1 (source digits are display-irrelevant), a nested list is its own
/// list, and any break — a blank line or prose — ends the open lists, so the
/// next list starts over. The level is STRUCTURAL (how many lists are open
/// above), not an indent-width guess, so it survives any spaces-per-level
/// setting. Non-items are (0, 0).
pub(crate) fn ordered_numbers(lines: &[&str]) -> Vec<(u32, usize)> {
    ordered_scan(lines).0
}

/// [`ordered_numbers`] plus, for each ordered item, the list it belongs to:
/// siblings share an id (a nested list, or a new list after a break, gets its
/// own), so their bodies can share one marker width. Non-items are `None`.
pub(crate) fn ordered_scan(lines: &[&str]) -> (Vec<(u32, usize)>, Vec<Option<u32>>) {
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

/// A copied slice with its ordered markers rewritten to their DISPLAYED
/// positions — still digit markdown (letters/romans are display-only), so a
/// nested block pastes starting at 1 and a continuation pastes as `3.`,
/// exactly the counting the screen showed, in any markdown app. Lines whose
/// marker sits outside the slice are copied verbatim. Positions come from
/// the WHOLE document, so a mid-list copy keeps its true numbering.
pub(crate) fn renumber_copy(content: &str, range: std::ops::Range<usize>) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let nums = ordered_numbers(&lines);
    let mut out = String::with_capacity(range.len());
    let mut line_start = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        // The slice of this line (excluding its newline) inside the range.
        let seg_start = range.start.max(line_start);
        let seg_end = range.end.min(line_end);
        if seg_start <= seg_end && range.start <= line_end && range.end >= line_start {
            let rel = (seg_start - line_start)..(seg_end - line_start);
            let marker = list_prefix(line).filter(|&(_, _, ordered, _)| ordered);
            match marker {
                // Rewrite only when the whole marker is inside the slice.
                Some((plen, indent, _, _))
                    if rel.start == 0 && rel.end >= plen && nums[i].0 > 0 =>
                {
                    out.push_str(&line[..indent]);
                    out.push_str(&nums[i].0.to_string());
                    // Keep the source's `.`/`)` punctuation and the body.
                    let digits_end = indent
                        + line.as_bytes()[indent..]
                            .iter()
                            .take_while(|b| b.is_ascii_digit())
                            .count();
                    out.push_str(&line[digits_end..rel.end]);
                }
                _ => out.push_str(&line[rel]),
            }
        }
        // The newline between this line and the next, when it's in range.
        if range.start <= line_end && line_end < range.end && i + 1 < lines.len() {
            out.push('\n');
        }
        line_start = line_end + 1;
    }
    out
}

/// Toggle a GFM task item's checkbox in `line` — flip the char between the
/// brackets (`[ ]`↔`[x]`). Returns the rewritten line, or `None` if `line` isn't
/// a task item. The length is unchanged (one ASCII byte swapped).
pub(crate) fn toggle_task_checkbox(line: &str) -> Option<String> {
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
pub(crate) fn image_line(line: &str) -> Option<(&str, Option<f32>)> {
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
pub(crate) fn image_row(line: &str) -> Option<(&str, Option<f32>, usize)> {
    if let Some((src, width)) = image_line(line) {
        return Some((src, width, 0));
    }
    let (plen, ..) = list_prefix(line)?;
    let (src, width) = image_line(&line[plen..])?;
    Some((src, width, plen))
}

/// Font-size multiplier for a line — larger for headings (matching the reading
/// view's scale), 1.0 for body text. Drives the editor's variable line heights.
/// A heading right after a list/task marker (`- ### Notes`) counts too, same
/// as [`apply_heading`].
pub(crate) fn line_scale(line: &str) -> f32 {
    let body = list_prefix(line).map_or(line, |(p, ..)| &line[p..]);
    match heading_level(body) {
        Some(1) => 1.8,
        Some(2) => 1.5,
        Some(3) => 1.3,
        Some(4) => 1.15,
        Some(5) => 1.05,
        _ => 1.0,
    }
}

/// Fenced ` ```mermaid ` blocks: each entry is `(line_range, source)` — the line
/// range covering both fences (so it can collapse), and the diagram source (the
/// lines between the fences, joined). Used to render the block as a diagram.
pub(crate) fn mermaid_blocks(content: &str) -> Vec<(Range<usize>, String)> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("```") && t[3..].trim() == "mermaid" {
            let start = i;
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            let source = lines[start + 1..j].join("\n");
            let end = (j + 1).min(lines.len()); // include the closing fence
            out.push((start..end, source));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Whether `line` is a `<!-- math:left|center|right -->` alignment marker.
/// The editor hides it like a table style marker; standard Markdown viewers
/// ignore the comment.
pub(crate) fn is_math_align_marker(line: &str) -> bool {
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
pub(crate) struct MathRegion {
    pub range: Range<usize>,
    pub marker_line: Option<usize>,
}

pub(crate) fn math_regions(content: &str) -> Vec<MathRegion> {
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
/// Every inline `![alt](src)` image on `line`, as `(full span, src range)`.
/// A whole-line image is handled as a block widget (`image_row`) before a line
/// reaches inline shaping, so these are the mixed-line (text + image) ones.
/// Images inside inline code aren't matched.
// Inline objects cannot begin inside a code span. Reuse this boundary check
// in both object scanners, which also feed rendering and hit testing.
fn inline_code_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if bytes.get(start) != Some(&b'`') || is_backslash_escaped(bytes, start) {
        return None;
    }
    let width = bytes[start..].iter().take_while(|&&b| b == b'`').count();
    let mut at = start + width;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let count = bytes[at..].iter().take_while(|&&b| b == b'`').count();
            if count == width {
                return Some(at + count);
            }
            at += count;
        } else {
            at += 1;
        }
    }
    None
}

pub(crate) fn inline_image_spans(line: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if let Some(end) = inline_code_end(line, i) {
            i = end;
            continue;
        }

        if b[i] == b'!'
            && !is_backslash_escaped(b, i)
            && b[i + 1] == b'['
            && let Some(rb) = line[i + 2..].find(']')
            && line[i + 2 + rb + 1..].starts_with('(')
            && let Some(rp) = line[i + 2 + rb + 2..].find(')')
        {
            let src = (i + 2 + rb + 2)..(i + 2 + rb + 2 + rp);
            out.push((i..(src.end + 1), src.clone()));
            i = src.end + 1;
        } else {
            i += 1;
        }
    }
    out
}

pub(crate) fn inline_math_spans(line: &str) -> Vec<Range<usize>> {
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

/// Where one inline formula sits in a shaped line: the byte offset of its spacer in the DISPLAY
/// string (to position the painted image) and the SOURCE byte range of the `$…$` span (to
/// hit-test a click back to the formula and to target its edit/commit).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineMathPlace {
    pub display_off: usize,
    /// Byte length of the spacer. Needed to place the raster on an RTL row,
    /// where `display_off` is the spacer's RIGHT edge, not its left.
    pub len: usize,
    pub source: Range<usize>,
}

/// The sub-slice of `runs` covering display byte range `[lo, hi)`, clipping the runs that
/// straddle an edge. `runs` must tile the display in order (as `hidden_runs` emits them).
fn clip_runs(runs: &[TextRun], lo: usize, hi: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut off = 0;
    for r in runs {
        let (s, e) = (off, off + r.len);
        off = e;
        let (cs, ce) = (s.max(lo), e.min(hi));
        if cs < ce {
            out.push(TextRun {
                len: ce - cs,
                ..r.clone()
            });
        }
    }
    out
}

/// Replace each inline formula's `$…$` glyphs with an invisible spacer the caller paints the
/// typeset image over. Takes `hidden_runs` output (`display`, `runs`, `map` — the display→source
/// byte map of length `display.len() + 1`) and the line's `formulas` as `(source range, spacer
/// width in spaces)`, sorted by source start and non-overlapping. Returns the rewritten
/// `display` / `runs` / `map` plus each formula's placement. The spacer borrows `gap`'s font/
/// color (its glyphs are spaces, so only the advance matters) and maps back to the span start,
/// so a click on it lands the caret at the formula (which then reveals raw source / opens the
/// editor).
pub(crate) fn splice_inline_math(
    display: &str,
    runs: &[TextRun],
    map: &[usize],
    formulas: &[(Range<usize>, usize)],
    gap: &TextRun,
) -> (String, Vec<TextRun>, Vec<usize>, Vec<InlineMathPlace>) {
    let mut nd = String::with_capacity(display.len());
    let mut nm: Vec<usize> = Vec::with_capacity(map.len());
    let mut nr: Vec<TextRun> = Vec::new();
    let mut places = Vec::new();
    let mut cursor = 0; // display byte offset consumed so far
    for (src, n) in formulas {
        // The display range covering this span: first display offset reaching the span's
        // start, up to the first reaching its end (`map` is monotonic non-decreasing).
        let d0 = map
            .iter()
            .position(|&s| s >= src.start)
            .unwrap_or(display.len());
        let d1 = map
            .iter()
            .position(|&s| s >= src.end)
            .unwrap_or(display.len());
        if d0 < cursor {
            continue; // overlapping / out-of-order formula — skip defensively
        }
        // Verbatim text before the formula.
        nd.push_str(&display[cursor..d0]);
        nm.extend_from_slice(&map[cursor..d0]);
        nr.extend(clip_runs(runs, cursor, d0));
        // The spacer, mapped back to the span start.
        places.push(InlineMathPlace {
            display_off: nd.len(),
            len: *n,
            source: src.clone(),
        });
        let src_at = map.get(d0).copied().unwrap_or(src.start);
        for _ in 0..*n {
            nd.push(' ');
        }
        nm.extend(std::iter::repeat_n(src_at, *n));
        nr.push(TextRun {
            len: *n,
            ..gap.clone()
        });
        cursor = d1;
    }
    // Tail after the last formula.
    nd.push_str(&display[cursor..]);
    nm.extend_from_slice(&map[cursor..display.len()]);
    nr.extend(clip_runs(runs, cursor, display.len()));
    nm.push(*map.last().unwrap_or(&0)); // final source-end sentinel
    (nd, nr, nm, places)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_renumber_to_displayed_positions() {
        // Source digits are stale (7., 9.) and the nested list starts at 3 —
        // the copy carries what the SCREEN showed: 1,2 nested from 1.
        let doc = "1. a\n7. b\n  3. c\n  9. d\n5. e";
        assert_eq!(
            renumber_copy(doc, 0..doc.len()),
            "1. a\n2. b\n  1. c\n  2. d\n3. e"
        );
        // A mid-list copy keeps its true positions (paste renders 2,3).
        let from = doc.find("7.").unwrap();
        assert_eq!(
            renumber_copy(doc, from..doc.len()),
            "2. b\n  1. c\n  2. d\n3. e"
        );
        // A partial first line (marker outside the slice) is verbatim.
        let from = doc.find("b").unwrap();
        assert_eq!(
            renumber_copy(doc, from..doc.find('c').unwrap() + 1),
            "b\n  1. c"
        );
        // Non-list text passes through untouched.
        assert_eq!(renumber_copy("plain\ntext", 0..10), "plain\ntext");
    }

    #[test]
    fn selection_reveal_shows_inline_but_not_prefix() {
        let (font, c, st) = (Font::default(), Hsla::default(), test_style());
        // A selected list line: the `- ` prefix stays hidden behind its
        // painted bullet — and formatting markers stay hidden too now
        // (always_hide wins over the selection reveal; structural markers
        // like links still come back).
        let (disp, _, _) = hidden_runs("- a **b** c", &font, c, &[], None, 0, 2, true, &st);
        assert_eq!(disp, "a b c");
        let (disp, _, _) = hidden_runs("- [t](u) c", &font, c, &[], None, 0, 2, true, &st);
        assert_eq!(disp, "[t](u) c");
        // Same line unselected: everything hidden.
        let (disp, _, _) = hidden_runs("- a **b** c", &font, c, &[], None, 0, 2, false, &st);
        assert_eq!(disp, "a b c");
    }

    /// Helper: the substrings the spans cover, for readable assertions.
    fn spans(line: &str) -> Vec<&str> {
        inline_math_spans(line)
            .into_iter()
            .map(|r| &line[r])
            .collect()
    }

    #[test]
    fn inline_math_basic() {
        assert_eq!(spans("the area $\\pi r^2$ of a circle"), vec!["$\\pi r^2$"]);
        assert_eq!(spans("$x$"), vec!["$x$"]);
        assert_eq!(spans("a $x$ and $y$ b"), vec!["$x$", "$y$"]);
        // Spaces are fine *inside* a span; only the immediate inner edges must be non-space.
        assert_eq!(spans("$x = 5$ done"), vec!["$x = 5$"]);
    }

    #[test]
    fn inline_math_rejects_money_and_empties() {
        assert!(spans("it cost $5 and $10 total").is_empty());
        assert!(spans("$ x $").is_empty()); // space right after opener / before closer
        assert!(spans("a $$ b").is_empty()); // `$$` is not an inline span
        assert!(spans("lone $ dollar").is_empty());
        // Closer immediately followed by a digit isn't a closer (pandoc rule).
        assert!(spans("$x$5").is_empty());
    }

    #[test]
    fn inline_math_escapes_and_later_close() {
        // `\$` is a literal dollar, not a delimiter.
        assert!(spans("price is 5\\$ even").is_empty());
        assert_eq!(spans("\\$ and $x$"), vec!["$x$"]);
        // A `$` that can't close is skipped; a later valid one closes the span.
        assert_eq!(spans("$a $5 b$ end"), vec!["$a $5 b$"]);
    }

    /// A plain text run of `len` bytes (style is irrelevant to the splice; only `len` matters).
    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: gpui::font("Helvetica"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    /// Total bytes the runs cover — must always equal the display length.
    fn runs_len(runs: &[TextRun]) -> usize {
        runs.iter().map(|r| r.len).sum()
    }

    #[test]
    fn splice_one_formula() {
        // "a $x$ b" with the `$x$` span (bytes 2..5) → a 3-space spacer. display == source here,
        // so the map is the identity (length 8).
        let display = "a $x$ b";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) =
            splice_inline_math(display, &[run(display.len())], &map, &[(2..5, 3)], &run(0));
        assert_eq!(nd, "a     b"); // "a " + 3 spaces + " b" = a + 5 spaces + b
        assert_eq!(
            places,
            vec![InlineMathPlace {
                display_off: 2,
                len: 3,
                source: 2..5
            }]
        );
        assert_eq!(nm, vec![0, 1, 2, 2, 2, 5, 6, 7]); // spacer bytes map to the span start (2)
        assert_eq!(nm.len(), nd.len() + 1);
        assert_eq!(runs_len(&nr), nd.len()); // runs still tile the display
    }

    #[test]
    fn splice_two_formulas_and_leading() {
        // Two spans, the first at the very start: "$a$ and $b$".
        let display = "$a$ and $b$";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) = splice_inline_math(
            display,
            &[run(display.len())],
            &map,
            &[(0..3, 2), (8..11, 4)],
            &run(0),
        );
        assert_eq!(nd, "   and     "); // 2 spaces + " and " + 4 spaces
        assert_eq!(
            places,
            vec![
                InlineMathPlace {
                    display_off: 0,
                    len: 2,
                    source: 0..3
                },
                InlineMathPlace {
                    display_off: 7,
                    len: 4,
                    source: 8..11
                },
            ]
        );
        assert_eq!(nm.len(), nd.len() + 1);
        assert_eq!(runs_len(&nr), nd.len());
        assert_eq!(*nm.last().unwrap(), display.len()); // sentinel preserved
    }

    #[test]
    fn splice_no_formulas_is_identity() {
        let display = "plain text";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) =
            splice_inline_math(display, &[run(display.len())], &map, &[], &run(0));
        assert_eq!(nd, display);
        assert_eq!(nm, map);
        assert!(places.is_empty());
        assert_eq!(runs_len(&nr), nd.len());
    }

    #[test]
    fn inline_math_multibyte_safe() {
        // A multi-byte char (é, —) adjacent to a delimiter counts as non-space; ranges stay
        // on char boundaries.
        let line = "café $x$ — π";
        assert_eq!(spans(line), vec!["$x$"]);
        for r in inline_math_spans(line) {
            assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
        }
    }

    #[test]
    fn one_line_math_regions() {
        // `$$x$$` alone on a line is a display-math REGION (block treatment);
        // so is a single complete pair with words around it (see
        // `forgiving_math_forms`).
        let r = math_regions("before\n$$x^2$$\nafter");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].range, 1..2);
        // The alignment marker above applies, as with fenced blocks.
        let r = math_regions("<!-- math:right -->\n$$x$$");
        assert_eq!(r[0].marker_line, Some(0));
        // Degenerate forms are not regions.
        assert!(math_regions("$$$$").is_empty());
        assert!(math_regions("$$ $$").is_empty());
    }

    #[test]
    fn forgiving_math_forms() {
        // Words mixed with math are NORMALIZED apart (edit-time), never
        // hidden: mixed lines are not regions themselves.
        assert!(math_regions("words $$E=mc^2$$ more").is_empty());
        assert!(math_regions("fees are $$5 and $$10 total").is_empty());
        assert!(math_regions("wer $$\nu(m)=v(n,x)\n$$ wer").is_empty());
        // `$$` inside a fenced code block is never math.
        assert!(math_regions("```sh\necho $$\n$$\n```").is_empty());
    }

    #[test]
    fn inline_display_math_double_dollar() {
        // Same-line `$$…$$` is display math (issue #54): one span covering
        // both delimiter pairs, spaces inside allowed.
        assert_eq!(spans("$$E=mc^2$$"), vec!["$$E=mc^2$$"]);
        assert_eq!(spans("a $$ x + y $$ b"), vec!["$$ x + y $$"]);
        // Unclosed / empty pairs match nothing; a lone `$$` fence line is
        // the BLOCK scanner's business.
        assert_eq!(spans("$$"), Vec::<&str>::new());
        assert_eq!(spans("$$$$"), Vec::<&str>::new());
        assert_eq!(spans("$$x"), Vec::<&str>::new());
        // A `$$` pair doesn't swallow a later single-dollar span.
        assert_eq!(spans("$$a$$ then $b$"), vec!["$$a$$", "$b$"]);
    }

    #[test]
    fn mermaid_block_extraction() {
        let md = "intro\n\n```mermaid\ngraph TD\nA --> B\n```\n\nafter";
        let blocks = mermaid_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 2..6); // ```mermaid(2), graph(3), A-->B(4), ```(5)
        assert_eq!(blocks[0].1, "graph TD\nA --> B");
        // A plain ``` block is not mermaid.
        assert!(mermaid_blocks("```rust\nfn x() {}\n```").is_empty());
        // Trailing-space lang still matches.
        assert_eq!(mermaid_blocks("```mermaid \npie\n```").len(), 1);
    }

    #[test]
    fn image_line_and_row() {
        assert_eq!(image_line("![a](b.png)"), Some(("b.png", None)));
        assert_eq!(
            image_line("![a](b.png){width=320}"),
            Some(("b.png", Some(320.0)))
        );
        assert_eq!(image_line("text ![a](b.png)"), None);
        // A list item whose sole body is an image renders too (its bullet stays).
        assert_eq!(
            image_row("- ![a](b.png){width=452}"),
            Some(("b.png", Some(452.0), 2))
        );
        assert_eq!(image_row("1. ![a](b.png)"), Some(("b.png", None, 3)));
        assert_eq!(image_row("![a](b.png)"), Some(("b.png", None, 0)));
        assert_eq!(image_row("- text ![a](b.png)"), None);
    }

    #[test]
    fn link_at_hits_only_urls() {
        let line = "see [[Ops Net|the net]] and #scada plus [docs](https://x.io/d) `#not` [[]]";
        // Wiki links and tags are plain text (ADR 0030).
        assert_eq!(link_at(line, 4), None);
        assert_eq!(link_at(line, 15), None);
        assert_eq!(link_at(line, 28), None);
        // The inline link: label, brackets, or url all navigate.
        assert_eq!(link_at(line, 41), Some("https://x.io/d".into()));
        assert_eq!(link_at(line, 55), Some("https://x.io/d".into()));
        // Plain text and code are not links.
        assert_eq!(link_at(line, 0), None);
        assert_eq!(link_at(line, 66), None); // inside `#not`
        // An image is a widget, not a link — even on its url.
        assert_eq!(link_at("![alt](images/a.png)", 10), None);
        // A footnote ref is styled like a link but isn't one.
        assert_eq!(link_at("x [^1] y", 3), None);
    }

    #[test]
    fn heading_nested_in_list_item() {
        // A heading right after a list marker scales like a top-level heading
        // (`- ### Hi` nests like the reading view's AST does), not like plain text.
        assert_eq!(line_scale("### Hi"), 1.3);
        assert_eq!(line_scale("- ### Hi"), 1.3);
        assert_eq!(line_scale("  - ## Hi"), 1.5);
        assert_eq!(line_scale("1. # Hi"), 1.8);
        // Plain list text (no heading) is unaffected.
        assert_eq!(line_scale("- not a heading"), 1.0);

        // Both the list marker and the heading marker are hidden, leaving just
        // the heading text — same as a bare "## Hi" line.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, ..) = hidden_runs("- ### Notes", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Notes");
    }

    #[test]
    fn note_taking_syntax_renders_as_plain_text() {
        // ADR 0030: wiki links, tags, block ids and block refs are ordinary text.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        for src in ["[[Page|alias]]", "a #tag", "see ((b1))", "Decision ^d1"] {
            let (disp, ..) = hidden_runs(src, &font, c, &[], None, 0, 0, false, &st);
            assert_eq!(disp, src);
        }
    }

    fn test_style() -> SyntaxStyle {
        let c = hsla(0., 0., 0.5, 1.);
        SyntaxStyle {
            marker: c,
            code: c,
            code_bg: c,
            link: c,
            quote: c,
            alert_note: c,
            alert_tip: c,
            alert_important: c,
            alert_warning: c,
            alert_caution: c,
            rule: c,
            mark_bg: c,
            popover_bg: c,
            popover_border: c,
            popover_fg: c,
            popover_hover: c,
            popover_divider: c,
            popover_danger: c,
            mono: gpui::font("monospace"),
        }
    }

    #[test]
    fn footnote_def_and_html_block() {
        assert_eq!(footnote_def("[^1]: a note"), Some(5)); // `[^1]:`
        assert_eq!(footnote_def("[^note]:x"), Some(8)); // `[^note]:`
        assert_eq!(footnote_def("[^1] not a def"), None);
        assert_eq!(footnote_def("[^]: empty label"), None);
        assert_eq!(footnote_def("plain"), None);

        assert!(html_block("<div>"));
        assert!(html_block("</section>"));
        assert!(html_block("<!-- comment -->"));
        assert!(html_block("  <span>indented"));
        assert!(!html_block("< 5 items"));
        assert!(!html_block("text <b>inline</b>"));
    }

    #[test]
    fn hidden_runs_removes_markers_and_maps_back() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        // "**bold**" → "bold"; display 0 maps to source 2, end (4) to 8.
        let (disp, _, map) = hidden_runs("**bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold");
        assert_eq!(map.len(), disp.len() + 1);
        assert_eq!(map[0], 2);
        assert_eq!(map[4], 8);

        // "## Hi" → "Hi"; the `## ` prefix is gone, display 0 maps to source 3.
        let (disp, _, map) = hidden_runs("## Hi", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Hi");
        assert_eq!(map[0], 3);
        assert_eq!(map[2], 5);

        // No markers → unchanged, identity map.
        let (disp, _, map) = hidden_runs("plain text", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "plain text");
        assert_eq!(map, (0..=10).collect::<Vec<_>>());
    }

    #[test]
    fn decoded_visible_text_agrees_with_search_and_source_map() {
        let source = "# hello **world** &amp; \\* `&amp;`";
        let (display, _, map) = hidden_runs(
            source,
            &gpui::font("Helvetica"),
            Hsla::default(),
            &[],
            None,
            0,
            0,
            false,
            &test_style(),
        );
        assert_eq!(display, "hello world & * &amp;");
        assert_eq!(
            crate::SearchIndex::from_markdown(source)
                .find(&display, true)
                .len(),
            1
        );
        let amp = display.find('&').unwrap();
        assert_eq!(map[amp], source.find("&amp;").unwrap());
    }

    #[test]
    fn highlight_projection_preserves_unicode_code_tables_and_source_offsets() {
        for (source, expected) in [
            ("==Договор== конец", "Договор конец"),
            (r"\==literal==", "==literal=="),
            ("===literal==", "===literal=="),
            ("``a` ==b== c``", "a` ==b== c"),
            ("==`a==b` tail== plain", "a==b tail plain"),
            (
                "x <mark>before `</mark>` after</mark> plain",
                "x before </mark> after plain",
            ),
            (
                r#"x <span style="color:unsupported">text</span> y"#,
                r#"x <span style="color:unsupported">text</span> y"#,
            ),
        ] {
            let (display, _, map) = hidden_runs(
                source,
                &gpui::font("Helvetica"),
                Hsla::default(),
                &[],
                None,
                0,
                0,
                false,
                &test_style(),
            );
            assert_eq!(display, expected, "{source:?}");
            assert_eq!(map.len(), display.len() + 1);
            for (offset, _) in display.char_indices() {
                assert!(source.is_char_boundary(map[offset]), "{source:?}");
            }
        }
        let source = "| ==Договор== | <span style='color:red'>Сторона</span> |\n| --- | --- |";
        let index = crate::SearchIndex::from_markdown(source);
        assert!(index.find("==", true).is_empty());
        assert!(index.find("color", true).is_empty());
        let found = index.find("Договор", true);
        assert_eq!(found.len(), 1);
        let start = source.find("Договор").unwrap();
        assert_eq!(found[0].source, vec![start..start + "Договор".len()]);
    }

    #[test]
    fn nested_styled_tags_restore_parent_and_keep_unsupported_inner_tags() {
        let source = r#"x <span style='color:red'>outer <span style='color:blue'>inner</span> outer</span> plain"#;
        let (display, runs, _) = hidden_runs(
            source,
            &gpui::font("Helvetica"),
            Hsla::default(),
            &[],
            None,
            0,
            0,
            false,
            &test_style(),
        );
        assert_eq!(display, "x outer inner outer plain");
        let end = display.rfind("outer").unwrap();
        let mut at = 0;
        let run = runs
            .iter()
            .find(|r| {
                let contains = (at..at + r.len).contains(&end);
                at += r.len;
                contains
            })
            .unwrap();
        assert_eq!(run.color, hsla_of(0xff0000ff));

        let source =
            r#"x <span style='color:red'>outer <span class=x>literal</span> outer</span> plain"#;
        let (display, ..) = hidden_runs(
            source,
            &gpui::font("Helvetica"),
            Hsla::default(),
            &[],
            None,
            0,
            0,
            false,
            &test_style(),
        );
        assert_eq!(display, "x outer <span class=x>literal</span> outer plain");
        let incomplete =
            r#"x <span style='color:red'>outer <span style='color:blue'>inner</span> plain"#;
        let (display, ..) = hidden_runs(
            incomplete,
            &gpui::font("Helvetica"),
            Hsla::default(),
            &[],
            None,
            0,
            0,
            false,
            &test_style(),
        );
        assert_eq!(display, "x <span style='color:red'>outer inner plain");
    }

    #[test]
    fn hidden_runs_image_link_hides_the_bang_too() {
        // `![](src)` (empty alt — the shape a fresh drop/paste inserts) must hide
        // the leading `!` along with the rest of the markers, not leave it as a
        // bare visible character (the editor falls back to this generic inline
        // scan on the caret's own image line, where the block-image widget is
        // suppressed).
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, ..) = hidden_runs("![](images/a.png)", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "");

        // A named alt still shows it, same as a plain link.
        let (disp, ..) = hidden_runs(
            "![alt](images/a.png)",
            &font,
            c,
            &[],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "alt");
    }

    #[test]
    fn hidden_runs_squiggle_diagnostics() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        // A diagnostic on the bold body ("bold" at source 2..6) underlines the
        // whole display string — squiggles survive marker hiding (W6).
        let (disp, runs, _) = hidden_runs(
            "**bold**",
            &font,
            c,
            &[Diagnostic { range: 2..6 }],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "bold");
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), disp.len());
        assert!(runs.iter().all(|r| r.underline.is_some()));

        // A partial diagnostic splits the segment: only "text" (6..10) squiggles.
        let (disp, runs, _) = hidden_runs(
            "plain text",
            &font,
            c,
            &[Diagnostic { range: 6..10 }],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "plain text");
        let underlined: usize = runs
            .iter()
            .filter(|r| r.underline.is_some())
            .map(|r| r.len)
            .sum();
        assert_eq!(underlined, 4); // "text"
    }

    #[test]
    fn hidden_runs_reveals_only_caret_construct() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        let line = "**bold** *it*";
        // Formatting markers never reveal — not even under the caret
        // (Cditor-style; see `formatting_markers_never_reveal`).
        let (disp, _, _) = hidden_runs(line, &font, c, &[], Some(4), 0, 0, false, &st);
        assert_eq!(disp, "bold it");
        // Caret in plain text reveals nothing either.
        let (disp, _, _) = hidden_runs("a **b**", &font, c, &[], Some(0), 0, 0, false, &st);
        assert_eq!(disp, "a b");
        // No caret on the line → fully hidden (W6).
        let (disp, _, _) = hidden_runs(line, &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold it");
        // STRUCTURAL constructs keep the per-construct caret reveal: the
        // caret inside a link shows its brackets/URL; outside, hidden.
        let link = "a [t](u) b";
        let (disp, _, _) = hidden_runs(link, &font, c, &[], Some(3), 0, 0, false, &st);
        assert_eq!(disp, "a [t](u) b");
        let (disp, _, _) = hidden_runs(link, &font, c, &[], Some(0), 0, 0, false, &st);
        assert_eq!(disp, "a t b");
    }

    #[test]
    fn alert_kinds_detected() {
        assert_eq!(alert_kind("[!NOTE]"), Some(AlertKind::Note));
        assert_eq!(alert_kind(" [!CAUTION] "), Some(AlertKind::Caution));
        // Lenient form: body text on the marker line.
        assert_eq!(alert_kind("[!NOTE] text after"), Some(AlertKind::Note));
        for s in ["[!note]", "[!NOTEXT]", "note", ""] {
            assert_eq!(alert_kind(s), None, "{s:?}");
        }
    }

    #[test]
    fn hard_break_backslash_hides() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, _) = hidden_runs("Alpha\\", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Alpha");
        // Shown at the caret, and literal when escaped or alone on a line.
        let (disp, _, _) = hidden_runs("Alpha\\", &font, c, &[], Some(6), 0, 0, false, &st);
        assert_eq!(disp, "Alpha\\");
        let (disp, _, _) = hidden_runs("Alpha\\\\", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Alpha\\");
        let (disp, _, _) = hidden_runs("\\", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "\\");
    }

    #[test]
    fn blockquote_prefix_hides() {
        // The `> ` marker hides; inline styling in the body still applies.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("> a **b**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "a b"); // "> " + "**" hidden
        assert_eq!(map[0], 2); // display 0 ('a') ← source 2

        // With the prefix revealed (caret on the line) the `> ` shows even when
        // the caret isn't in it; inline markers still hide unless under the caret.
        let (disp, _, _) = hidden_runs("> a **b**", &font, c, &[], Some(3), 2, 0, false, &st);
        assert_eq!(disp, "> a b");
    }

    #[test]
    fn escape_backslash_hides_like_the_reader() {
        // The reader consumes the `\` of an escape — the editor hides it the
        // same way (a marker span), leaving the escaped char as content.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, _) = hidden_runs(r"\*text\*", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "*text*");
        // `\\` shows a single backslash.
        let (disp, _, _) = hidden_runs(r"a \\ b", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, r"a \ b");
        // CommonMark: \**bold** = literal *, italic "bold", literal *.
        let (disp, _, _) = hidden_runs(r"\**bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "*bold*");
        // A converter's `1\.`: hidden, the `.` stands in for the whole escape
        // (it maps to the backslash) in the text's own colour.
        let (disp, runs, map) = hidden_runs(r"1\. a", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "1. a");
        assert_eq!(&map[..3], &[0, 1, 3]);
        assert!(runs.iter().all(|r| r.color == c));
        // Revealed at the caret, the backslash dims like any marker and the
        // `.` keeps the text colour.
        let dim = hsla(0., 0., 0.5, 1.);
        let (disp, runs, _) = hidden_runs(r"1\. a", &font, c, &[], Some(2), 0, 0, false, &st);
        assert_eq!(disp, r"1\. a");
        let colours: Vec<_> = runs.iter().map(|r| (r.len, r.color)).collect();
        assert_eq!(colours, vec![(1, c), (1, dim), (1, c), (2, c)]);
        // The full-source line dims it the same way.
        let runs = styled_runs(r"1\. a", &font, c, &[], Some(&st));
        let colours: Vec<_> = runs.iter().map(|r| (r.len, r.color)).collect();
        assert_eq!(colours, vec![(1, c), (1, dim), (1, c), (2, c)]);
    }

    #[test]
    fn nested_inline_markers_hide_and_pair() {
        let st = test_style();
        // Display hides every marker, nested included.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let (disp, _, _) = hidden_runs("**bold *it* bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold it bold");
        // Pairing survives nesting: two pairs, outer first.
        let pairs = fmt_marker_pairs("**bold *it* bold**");
        assert_eq!(pairs, vec![(0..2, 16..18), (7..8, 10..11)]);
    }

    #[test]
    fn fmt_marker_pairs_stay_per_construct() {
        // Adjacent constructs pair up separately — a delete between them must
        // never fuse one's closer with the other's opener.
        assert_eq!(
            fmt_marker_pairs("**a**~~b~~"),
            vec![(0..2, 3..5), (5..7, 8..10)]
        );
        // An empty pair (post-delete state) is still a pair.
        assert_eq!(fmt_marker_pairs("****"), vec![(0..2, 2..4)]);
        // Structural markers (links) aren't formatting pairs.
        assert_eq!(fmt_marker_pairs("[t](u)"), vec![]);
        assert_eq!(fmt_marker_pairs("<u>x</u>"), vec![(0..3, 4..8)]);
    }

    #[test]
    fn formatting_markers_never_reveal() {
        // Cditor-style: even with the caret INSIDE the construct, formatting
        // markers stay hidden (the styling is the feedback; toggles edit it).
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        for (line, disp) in [
            ("**bold** x", "bold x"),
            ("*it* x", "it x"),
            ("~~s~~ x", "s x"),
            ("`c` x", "c x"),
            ("<u>u</u> x", "u x"),
            ("<mark>m</mark> x", "m x"),
        ] {
            let caret_inside = Some(3.min(line.len() - 1));
            let (d, _, _) = hidden_runs(line, &font, c, &[], caret_inside, 0, 0, false, &st);
            assert_eq!(d, disp, "{line:?}");
        }
        // Structural constructs keep the caret reveal: a wiki link's brackets
        // show while the caret is inside it.
        let (d, _, _) = hidden_runs("[[Page]] x", &font, c, &[], Some(4), 0, 0, false, &st);
        assert_eq!(d, "[[Page]] x");
    }

    #[test]
    fn ordered_items_know_their_list() {
        let (_, lists) = ordered_scan(&[
            "1. a", "2. b", "   1. c", "   2. d", "3. e", "", "1. f", "- g", "1. h",
        ]);
        assert_eq!(
            lists,
            vec![
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                Some(0),
                None,
                Some(2),
                None,
                Some(4)
            ]
        );
    }

    #[test]
    fn ordered_numbers_word_style() {
        // A nested list counts on its own from 1 (whatever the digits say)
        // one level deeper, and the outer list resumes after it. The level
        // is structural — ANY deeper indent nests, whatever the tab setting.
        assert_eq!(
            ordered_numbers(&["1. a", "2. b", "  3. c", "  9. d", "6. e"]),
            vec![(1, 0), (2, 0), (1, 1), (2, 1), (3, 0)]
        );
        // Source digits are display-irrelevant; every list starts at 1.
        assert_eq!(ordered_numbers(&["5. a", "9. b"]), vec![(1, 0), (2, 0)]);
        // A bullet at the same indent ends the ordered run (bullets still
        // occupy a level for anything nested under them).
        assert_eq!(
            ordered_numbers(&["1. a", "- b", "7. c"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        // Any break — blank line or prose — starts numbering over.
        assert_eq!(
            ordered_numbers(&["1. a", "", "2. b"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        assert_eq!(
            ordered_numbers(&["1. a", "prose", "7. b"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        // A hanging indent (wrapped text under an item) keeps the list open.
        assert_eq!(
            ordered_numbers(&["1. a", "   wrapped", "7. b"]),
            vec![(1, 0), (0, 0), (2, 0)]
        );
    }

    #[test]
    fn list_prefix_hides() {
        // The marker hides; a nested item maps the body back past the prefix.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("  - hi", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "hi");
        assert_eq!(map[0], 4); // display 0 ('h') ← source 4
    }

    #[test]
    fn task_prefix_hides() {
        // The `- [ ] ` prefix hides entirely; body maps back past it.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("- [x] go", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "go");
        assert_eq!(map[0], 6); // display 0 ('g') ← source 6
    }

    #[test]
    fn toggle_task_checkbox_flips() {
        assert_eq!(
            toggle_task_checkbox("- [ ] todo").as_deref(),
            Some("- [x] todo")
        );
        assert_eq!(
            toggle_task_checkbox("- [x] done").as_deref(),
            Some("- [ ] done")
        );
        assert_eq!(
            toggle_task_checkbox("- [X] done").as_deref(),
            Some("- [ ] done")
        );
        assert_eq!(
            toggle_task_checkbox("  - [ ] nested").as_deref(),
            Some("  - [x] nested")
        );
        assert_eq!(toggle_task_checkbox("- plain"), None);
    }

    #[test]
    fn alert_fold_regions_span_the_quote_block() {
        let src = "> [!NOTE]- hidden\n> body\n> more\nprose\n> [!TIP]+ open\n> b\n> [!NOTE] plain";
        let r = alert_fold_regions(src);
        // Folded NOTE spans its quote lines; open TIP's region ends at the
        // plain (non-foldable) alert marker, which starts no region itself.
        assert_eq!(r, vec![(0..3, true), (4..6, false)]);
    }

    #[test]
    fn heading_fold_regions_span_their_section() {
        let src = "# Top\nbody\n## Sub\nsub body\n```\n# not a heading\n```\n## Next\nx";
        let folded: std::collections::HashSet<String> = ["## Sub".to_string()].into();
        // `## Sub` folds through the fence (the `#` inside is code) up to
        // `## Next`; nothing else folds.
        assert_eq!(heading_fold_regions(src, &folded), vec![2..7]);
        // Folding `# Top` swallows everything (no later h1).
        let folded: std::collections::HashSet<String> = ["# Top".to_string()].into();
        assert_eq!(heading_fold_regions(src, &folded), vec![0..9]);
        assert!(heading_fold_regions(src, &Default::default()).is_empty());
        // A LIST heading folds its indented children (blanks included) and
        // stops at the next sibling — not at unrelated later headings.
        let src = "* ### Notes: ^abc\n  - one\n\n  - two\n* sibling\n# After";
        let folded: std::collections::HashSet<String> = ["* ### Notes: ^abc".to_string()].into();
        assert_eq!(heading_fold_regions(src, &folded), vec![0..4]);
    }
}

#[cfg(test)]
mod heading_list_tests {
    use super::*;

    #[test]
    fn headings_list_atx_titles_outside_fences() {
        let text = "# Title\nBody\n```\n# not a heading\n```\n## Terms ##\n#hashtag\n### \n####### seven\n## C#\n~~~\n# tilde code\n~~~\n````\n```\n# still code\n````\n```inline```\n   ## Indented\n##\tTabbed\n    # four spaces is code\n";
        let found = headings(text);
        assert_eq!(
            found,
            vec![
                Heading {
                    level: 1,
                    title: "Title".into(),
                    offset: 0
                },
                Heading {
                    level: 2,
                    title: "Terms".into(),
                    offset: text.find("## Terms").unwrap()
                },
                Heading {
                    level: 2,
                    title: "C#".into(),
                    offset: text.find("## C#").unwrap()
                },
                Heading {
                    level: 2,
                    title: "Indented".into(),
                    offset: text.find("   ## Indented").unwrap()
                },
                Heading {
                    level: 2,
                    title: "Tabbed".into(),
                    offset: text.find("##\tTabbed").unwrap()
                },
            ]
        );
    }
}
