//! Markdown-construct **recognition**: what counts as a construct and what its
//! payload is, kept separate from the editor's rendering. Engine-neutral and
//! gpui-free.

/// The five GitHub alert kinds (`> [!NOTE]` …).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// `(kind, marker text)` for each alert, in matching order.
pub const ALERT_MARKERS: [(AlertKind, &str); 5] = [
    (AlertKind::Note, "[!NOTE]"),
    (AlertKind::Tip, "[!TIP]"),
    (AlertKind::Important, "[!IMPORTANT]"),
    (AlertKind::Warning, "[!WARNING]"),
    (AlertKind::Caution, "[!CAUTION]"),
];

impl AlertKind {
    /// The title rendered in place of the marker ("Note", "Tip", …).
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

/// [`alert_marker`] for a single line's body (text after a blockquote's `>`
/// prefix): tolerates leading spaces and returns the kind, the byte length
/// consumed within `body` (spaces, marker, fold char, one separator space) —
/// what a line-oriented editor hides before painting the label — and the fold
/// state (`Some(true)` = folded).
pub(crate) fn alert_prefix(body: &str) -> Option<(AlertKind, usize, Option<bool>)> {
    let trimmed = body.trim_start();
    let ws = body.len() - trimmed.len();
    for (kind, m) in ALERT_MARKERS {
        if let Some(rest) = trimmed.strip_prefix(m) {
            let (fold, flen) = match rest.as_bytes().first() {
                Some(b'-') => (Some(true), 1),
                Some(b'+') => (Some(false), 1),
                _ => (None, 0),
            };
            let rest = &rest[flen..];
            if rest.is_empty() {
                return Some((kind, ws + m.len() + flen, fold));
            }
            if rest.starts_with(' ') {
                return Some((kind, ws + m.len() + flen + 1, fold));
            }
        }
    }
    None
}

/// The fold char of the alert marker on `line` (a full source line, `>` prefix
/// included): its byte offset within the line and the current state
/// (`true` = `-`/folded). `None` when the line isn't a foldable alert marker.
pub(crate) fn alert_fold_char(line: &str) -> Option<(usize, bool)> {
    let b = line.as_bytes();
    let mut p = 0;
    while p < b.len() && (b[p] == b'>' || b[p] == b' ') {
        p += 1;
    }
    let (_, _, fold) = alert_prefix(&line[p..])?;
    let folded = fold?;
    // The fold char sits right after the marker's closing `]`.
    let close = line[p..].find(']')? + p;
    Some((close + 1, folded))
}

/// Visual style of a GFM table, chosen per-table via a `<!-- table:STYLE -->`
/// marker comment on the line directly above it. The renderers honor it;
/// standard Markdown viewers ignore the comment and show a plain table.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TableStyle {
    /// Full outer box + all row/column gridlines.
    #[default]
    Grid,
    /// Alternate body rows shaded; no gridlines; a rule under the header.
    Striped,
    /// Only the header row shaded; no gridlines.
    Header,
    /// No box or gridlines — just a rule under the header.
    Minimal,
}

impl TableStyle {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "grid" => Some(Self::Grid),
            "striped" => Some(Self::Striped),
            "header" => Some(Self::Header),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }
}

/// Parse a `<!-- table:STYLE -->` marker (a whole line or an HTML comment's
/// value) into its [`TableStyle`]. `None` for anything unrecognized, so an
/// unknown marker stays a plain HTML comment.
pub(crate) fn table_style_marker(text: &str) -> Option<TableStyle> {
    let body = table_marker_body(text)?;
    // The style name is the first token; later tokens are attributes
    // (`cols=…` column widths).
    TableStyle::from_name(body.split_whitespace().next().unwrap_or(""))
}

/// The inner body of a `<!-- table:… -->` marker (style name + attributes),
/// or `None` for any other text.
fn table_marker_body(text: &str) -> Option<&str> {
    let inner = text
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    Some(inner.strip_prefix("table:")?.trim())
}

/// Explicit column widths (logical px) from a table marker's `cols=` attribute
/// — `<!-- table:grid cols=120,80,200 -->` — written by the editor's
/// drag-to-resize. `None` when absent or malformed (the table stays
/// content-measured).
pub(crate) fn table_col_widths(text: &str) -> Option<Vec<f32>> {
    let body = table_marker_body(text)?;
    let attr = body
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("cols="))?;
    let widths: Vec<f32> = attr
        .split(',')
        .map(|w| w.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    (!widths.is_empty() && widths.iter().all(|w| w.is_finite() && *w > 0.)).then_some(widths)
}

/// A table marker line for `style` (+ optional explicit column widths) — the
/// inverse of the parsers above. `None` when the marker would say nothing
/// (Grid, no widths): the default needs no marker.
pub(crate) fn table_marker_text(style: TableStyle, widths: Option<&[f32]>) -> Option<String> {
    let name = match style {
        TableStyle::Grid => "grid",
        TableStyle::Striped => "striped",
        TableStyle::Header => "header",
        TableStyle::Minimal => "minimal",
    };
    match widths {
        Some(w) if !w.is_empty() => {
            let list = w
                .iter()
                .map(|w| (w.round() as i64).to_string())
                .collect::<Vec<_>>()
                .join(",");
            Some(format!("<!-- table:{name} cols={list} -->"))
        }
        _ if style != TableStyle::Grid => Some(format!("<!-- table:{name} -->")),
        _ => None,
    }
}

/// The marker for ordered item `n` (1-based) at nesting `depth`, Word-style:
/// `1.` -> `a.` -> `i.`, cycling for deeper levels. Both views paint ordered
/// lists with this scheme (a deliberate divergence from CommonMark's
/// digits-everywhere), so nesting is readable at a glance.
pub(crate) fn ordered_marker(depth: usize, n: u32) -> String {
    match depth % 3 {
        0 => format!("{n}."),
        1 => format!("{}.", letters(n)),
        _ => format!("{}.", roman(n)),
    }
}

/// 1 → `a`, 26 → `z`, 27 → `aa` (bijective base 26).
fn letters(mut n: u32) -> String {
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

/// Lowercase roman numerals (`0` has none; empty string).
fn roman(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

// --- Linkables ---

/// What a click on a link targets: an inline or bare URL (hosts open http(s)
/// externally, resolve files themselves).
#[derive(Debug, PartialEq, Clone)]
pub enum LinkHit {
    Url(String),
}

/// A block's base writing direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub(crate) fn is_rtl(self) -> bool {
        self == Direction::Rtl
    }
}

/// The base direction of `text`, by the first *strong* directional character
/// (Unicode UAX #9 rules P2/P3 — the same rule Logseq and browsers' `dir=auto`
/// use). Neutral characters (digits, punctuation, whitespace, markdown
/// markers) are skipped, so `- سلام` and `## سلام` detect as RTL; text with no
/// strong character at all is LTR.
///
/// Ranges rather than a full Unicode table: the strong-RTL blocks are
/// contiguous and stable (Hebrew, Arabic and its supplements, Syriac, Thaana,
/// N'Ko, Samaritan, Mandaic, plus the Arabic presentation forms), and pulling
/// a bidi-class table in for one predicate isn't worth the dependency.
pub(crate) fn base_direction(text: &str) -> Direction {
    for c in text.chars() {
        if is_strong_rtl(c) {
            return Direction::Rtl;
        }
        if is_strong_ltr(c) {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

/// [`content_direction`], but `None` when the line has no strong character at
/// all — it is blank, or nothing but markers.
///
/// `> [!NOTE]` is the case that matters: strip the quote arrow and the alert
/// marker and nothing is left, so the line has no direction of its own and must
/// take the surrounding text's. Answering `Ltr` there put a Persian callout's
/// title on one side and its body on the other.
pub(crate) fn content_direction_opt(line: &str) -> Option<Direction> {
    let mut rest = line.trim_start();
    loop {
        let before = rest;
        // Blockquote markers, however deep.
        rest = rest.strip_prefix('>').unwrap_or(rest).trim_start();
        // A bullet, or an ordered marker like `12.` / `3)`.
        if let Some(r) = rest
            .strip_prefix("- ")
            .or_else(|| rest.strip_prefix("* "))
            .or_else(|| rest.strip_prefix("+ "))
        {
            rest = r.trim_start();
        } else {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits > 0
                && let Some(r) = rest[digits..]
                    .strip_prefix(". ")
                    .or_else(|| rest[digits..].strip_prefix(") "))
            {
                rest = r.trim_start();
            }
        }
        // A GitHub alert marker (`[!NOTE]`, `[!TIP]` …), plus the Obsidian
        // fold char after it. Its LABEL is Latin, so it decided the direction
        // of every Persian callout — the same trap the task box set.
        if let Some(r) = rest.strip_prefix("[!")
            && let Some(close) = r.find(']')
        {
            let after = &r[close + 1..];
            rest = after
                .strip_prefix('-')
                .or_else(|| after.strip_prefix('+'))
                .unwrap_or(after)
                .trim_start();
        }
        // A task box — the one that started this.
        if let Some(r) = rest
            .strip_prefix("[ ] ")
            .or_else(|| rest.strip_prefix("[x] "))
            .or_else(|| rest.strip_prefix("[X] "))
        {
            rest = r.trim_start();
        }
        // Heading hashes.
        if rest.starts_with('#') {
            let hashes = rest.chars().take_while(|c| *c == '#').count();
            if let Some(r) = rest[hashes..].strip_prefix(' ') {
                rest = r.trim_start();
            }
        }
        if rest == before {
            break;
        }
    }
    rest.chars().find_map(|c| {
        if is_strong_rtl(c) {
            Some(Direction::Rtl)
        } else if is_strong_ltr(c) {
            Some(Direction::Ltr)
        } else {
            None
        }
    })
}

/// Does `text` contain ANY right-to-left character?
///
/// Distinct from [`base_direction`], which answers which side a line starts on.
/// A line can read left-to-right and still hold a Persian name in the middle,
/// and that run needs the same logical↔visual mapping an RTL line does — the
/// caret misplaces inside it otherwise.
pub(crate) fn contains_rtl(text: &str) -> bool {
    text.chars().any(is_strong_rtl)
}

/// Strong right-to-left: Hebrew through Arabic-script languages.
fn is_strong_rtl(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{05FF}'   // Hebrew
        | '\u{0600}'..='\u{06FF}' // Arabic (incl. Persian, Urdu)
        | '\u{0700}'..='\u{074F}' // Syriac
        | '\u{0750}'..='\u{077F}' // Arabic Supplement
        | '\u{0780}'..='\u{07BF}' // Thaana
        | '\u{07C0}'..='\u{07FF}' // N'Ko
        | '\u{0800}'..='\u{083F}' // Samaritan
        | '\u{0840}'..='\u{085F}' // Mandaic
        | '\u{08A0}'..='\u{08FF}' // Arabic Extended-A
        | '\u{FB1D}'..='\u{FDFF}' // Hebrew/Arabic presentation forms
        | '\u{FE70}'..='\u{FEFF}' // Arabic presentation forms-B
    )
}

/// Strong left-to-right — Latin/Greek/Cyrillic and the CJK/Indic scripts.
/// Deliberately approximate in the same direction as [`is_strong_rtl`]: what
/// matters is that a strong LTR character stops the scan, and every alphabetic
/// character that isn't strong-RTL does.
fn is_strong_ltr(c: char) -> bool {
    c.is_alphabetic() && !is_strong_rtl(c)
}

/// A word character for boundary checks (a URL glued to a word isn't a link).
pub(crate) fn is_word_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Where a bare URL starting at `start` ends: consumes to whitespace or a
/// wrapping delimiter, then backs off trailing punctuation (GFM-ish).
pub(crate) fn url_end(line: &str, start: usize) -> usize {
    let b = line.as_bytes();
    let mut j = start;
    while j < line.len()
        && !b[j].is_ascii_whitespace()
        && !matches!(b[j], b'<' | b'>' | b'"' | b'`')
    {
        j += 1;
    }
    while j > start
        && matches!(
            b[j - 1],
            b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']'
        )
    {
        j -= 1;
    }
    j
}

/// Every clickable link in `line`, as `(source byte range, target)`:
/// inline `[text](url)` links and bare `http(s)://` URLs. Images
/// (`![](src)`), footnote refs, and anything inside inline code are opaque —
/// not links. One grammar for every renderer's click hit-tests, hover
/// cursors, and styling.
pub(crate) fn links(line: &str) -> Vec<(std::ops::Range<usize>, LinkHit)> {
    let b = line.as_bytes();
    let end = line.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < end {
        let c = b[i];
        // Inline code: the span is opaque (a URL inside backticks is verbatim).
        if c == b'`'
            && let Some(close) = find1(b, i + 1, end, b'`')
        {
            i = close + 1;
            continue;
        }
        // Footnote reference [^label]: styled like a link but not one.
        if c == b'['
            && i + 1 < end
            && b[i + 1] == b'^'
            && let Some(rb) = find1(b, i + 2, end, b']')
            && rb > i + 2
        {
            i = rb + 1;
            continue;
        }
        // Inline link [text](url) — or an image ![alt](src), which is NOT a
        // link click (images render as widgets / have their own machinery).
        if c == b'['
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'('
            && let Some(rp) = find1(b, rb + 2, end, b')')
        {
            let is_image = i > 0 && b[i - 1] == b'!';
            let url = line[rb + 2..rp].trim();
            if !is_image && !url.is_empty() {
                out.push((i..rp + 1, LinkHit::Url(url.to_string())));
            }
            i = rp + 1;
            continue;
        }
        // Bare URL: http(s)://… at a word boundary (GFM autolink literal).
        // Compare BYTES: `i` walks bytes, so a str slice here would panic
        // mid-char on any non-ASCII text.
        if (b[i..].starts_with(b"http://") || b[i..].starts_with(b"https://"))
            && (i == 0 || !is_word_char(b[i - 1]))
        {
            let j = url_end(line, i);
            if j > i + 8 {
                out.push((i..j, LinkHit::Url(line[i..j].to_string())));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The link under byte `col` of `line`, if any (see [`links`]).
pub(crate) fn link_at(line: &str, col: usize) -> Option<LinkHit> {
    links(line)
        .into_iter()
        .find(|(r, _)| r.contains(&col))
        .map(|(_, hit)| hit)
}

fn find1(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&i| b[i] == c)
}

// --- Forgiving `$$` fences (words attached) ----------------------------------

/// Classify a words-attached `$$` fence line: `Some(true)` for an opener
/// (`words $$` — the `$$` trails), `Some(false)` for a closer (`$$ words` —
/// the `$$` leads). A bare `$$` (the strict form) and lines whose `$$` pairs
/// up on the same line classify as neither.
pub(crate) fn math_fence_words(line: &str) -> Option<bool> {
    let t = line.trim();
    if t == "$$" || t.len() <= 2 {
        return None;
    }
    if t.ends_with("$$") && !t[..t.len() - 2].contains("$$") {
        return Some(true);
    }
    if t.starts_with("$$") && !t[2..].contains("$$") {
        return Some(false);
    }
    None
}

/// A single complete `$$…$$` pair on a line that ALSO carries other text —
/// strict inner rules (non-empty, no interior `$$`, no delimiter-adjacent
/// whitespace), so prose about prices (`$$5 and $$10`) never matches. Table
/// rows keep their cells.
pub(crate) fn embedded_math(line: &str) -> Option<(usize, usize)> {
    let t = line.trim_start();
    if t.starts_with('|') {
        return None;
    }
    let s = line.find("$$")?;
    let e = line.rfind("$$")?;
    if e < s + 3 {
        return None;
    }
    let inner = &line[s + 2..e];
    (!inner.is_empty()
        && !inner.contains("$$")
        && !inner.starts_with(char::is_whitespace)
        && !inner.ends_with(char::is_whitespace))
    .then_some((s, e + 2))
}

/// Split words-attached `$$` fences onto their own lines when they pair up
/// (an opener needs a closer before the next blank line) — `wer $$` → `wer` +
/// `$$`, `$$ wer` → `$$` + `wer` — and split a words-mixed complete pair onto
/// its own line (`text $$x$$ text` → three lines), so a math parser sees
/// well-formed display blocks. Code fences are left alone; unpaired `$$`s
/// (prose, prices) pass through untouched. Borrowed when nothing changes.
pub(crate) fn normalize_math_fences(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains("$$") {
        return std::borrow::Cow::Borrowed(source);
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut in_code = false;
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("```") {
            in_code = !in_code;
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        if !in_code && let Some((s, e)) = embedded_math(lines[i]) {
            let (before, after) = (lines[i][..s].trim_end(), lines[i][e..].trim_start());
            if !before.is_empty() {
                out.push(before.to_string());
            }
            out.push(lines[i][s..e].to_string());
            if !after.is_empty() {
                out.push(after.to_string());
            }
            changed = changed || !before.is_empty() || !after.is_empty();
            i += 1;
            continue;
        }
        if !in_code && math_fence_words(lines[i]) == Some(true) {
            // A closer before the next blank line pairs the fences; a blank
            // first means this was prose, not math.
            let closer = (i + 1..lines.len())
                .take_while(|&j| !lines[j].trim().is_empty())
                .find(|&j| lines[j].trim() == "$$" || math_fence_words(lines[j]) == Some(false));
            if let Some(j) = closer {
                let open = lines[i].trim_end();
                out.push(open[..open.len() - 2].trim_end().to_string());
                out.push("$$".to_string());
                for inner in &lines[i + 1..j] {
                    out.push(inner.to_string());
                }
                out.push("$$".to_string());
                let close = lines[j].trim_start();
                let rest = close[2..].trim_start();
                if !rest.is_empty() {
                    out.push(rest.to_string());
                }
                changed = true;
                i = j + 1;
                continue;
            }
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    if changed {
        std::borrow::Cow::Owned(out.join("\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

// --- Highlights and colors ------------------------------------------------
//
// `==text==` is the highlight most markdown apps agree on (Obsidian, Typora,
// Bear); `<mark>` is its inline-HTML twin. A *chosen* color has no markdown
// spelling at all, so it rides the HTML Obsidian (and its Highlightr plugin)
// writes: `<mark style="background:#ffd54f">` and `<span style="color:#e11">`.
// Both views recognize all of these through the functions below.

/// The closing `==` for the highlight opener at byte `open` (`line[open..]`
/// starts with `==`), under the emphasis-like rules markdown-it-mark uses so
/// `a == b == c` and `====` stay literal: the opener is followed by a
/// non-space that isn't `=`, the closer is preceded by a non-space and is
/// exactly two `=`, and the body is non-empty.
pub(crate) fn highlight_close(line: &str, open: usize) -> Option<usize> {
    let b = line.as_bytes();
    let body = open.checked_add(2)?;
    if !line.is_char_boundary(open)
        || b.get(open..body) != Some(b"==")
        || (open > 0 && b[open - 1] == b'=')
        || line
            .get(body..)?
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '=')
    {
        return None;
    }
    let mut at = body;
    while at < b.len() {
        match b[at] {
            // The editor scans one line at a time; reader pairing must agree.
            b'\r' | b'\n' => return None,
            b'\\' => at += 1 + line[at + 1..].chars().next().map_or(0, char::len_utf8),
            b'`' => {
                let end = code_span_end(line, at);
                if line[at..end].contains(['\r', '\n']) {
                    return None;
                }
                at = end;
            }
            b'=' => {
                let run = b[at..].iter().take_while(|c| **c == b'=').count();
                if run == 2
                    && at > body
                    && line[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_whitespace())
                {
                    return Some(at);
                }
                at += run;
            }
            _ => at += line[at..].chars().next()?.len_utf8(),
        }
    }
    None
}

// CommonMark code spans close with the same number of backticks. An unmatched
// opener is literal; only a matched span shields its contents from highlighting.
fn code_span_end(line: &str, open: usize) -> usize {
    let b = line.as_bytes();
    let run = b[open..].iter().take_while(|c| **c == b'`').count();
    let mut at = open + run;
    while let Some(rel) = line[at..].find('`') {
        at += rel;
        let close_run = b[at..].iter().take_while(|c| **c == b'`').count();
        if close_run == run {
            return at + run;
        }
        at += close_run;
    }
    open + run
}

/// Which inline-HTML tag a [`StyledTag`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyledKind {
    /// `<mark>` — a highlight, the theme's tint unless `background` says otherwise.
    Mark,
    /// `<span style="color:…">` — colored text (and/or a colored background).
    Span,
    /// `<u>` — underline (markdown has none).
    Underline,
}

/// An inline-HTML opening tag both views style rather than print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyledTag {
    pub kind: StyledKind,
    /// `color:` from the `style` attribute, as `0xRRGGBBAA`.
    pub color: Option<u32>,
    /// `background:` / `background-color:` from the `style` attribute, as
    /// `0xRRGGBBAA`. A bare `<mark>` has none: the theme's highlight.
    pub background: Option<u32>,
}

/// Parse an opening tag (`<mark>`, `<mark style="background:#ffd54f">`,
/// `<span style="color:#e11">`, `<u>`), the tag text including its angle
/// brackets. A `<span>` that sets neither color is not styled — `None`, and
/// it prints literally like any other HTML.
pub(crate) fn styled_tag(tag: &str) -> Option<StyledTag> {
    let inner = tag.trim().strip_prefix('<')?.strip_suffix('>')?;
    let (name, attrs) = match inner.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&inner[..i], inner[i..].trim()),
        None => (inner, ""),
    };
    let kind = match name.to_ascii_lowercase().as_str() {
        "mark" => StyledKind::Mark,
        "span" => StyledKind::Span,
        "u" => StyledKind::Underline,
        _ => return None,
    };
    let (mut color, mut background) = (None, None);
    if let Some(style) = attr_value(attrs, "style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "color" => color = css_color(v),
                    "background" | "background-color" => background = css_color(v),
                    _ => {}
                }
            }
        }
    }
    if kind == StyledKind::Span && color.is_none() && background.is_none() {
        return None;
    }
    Some(StyledTag {
        kind,
        color,
        background,
    })
}

/// The kind a closing tag (`</mark>`, `</span>`, `</u>`) closes.
pub(crate) fn styled_close(tag: &str) -> Option<StyledKind> {
    match tag.trim().to_ascii_lowercase().as_str() {
        "</mark>" => Some(StyledKind::Mark),
        "</span>" => Some(StyledKind::Span),
        "</u>" => Some(StyledKind::Underline),
        _ => None,
    }
}

/// Find the matching closing tag, retaining nested styles of the same kind.
/// Offsets are relative to the source after the opening tag.
pub(crate) fn matching_styled_close(body: &str, kind: StyledKind) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut at = 0;
    while at < body.len() {
        match body.as_bytes()[at] {
            b'`' => at = code_span_end(body, at),
            b'\\' => at += 1 + body[at + 1..].chars().next().map_or(0, char::len_utf8),
            b'<' => {
                let Some(len) = inline_tag_len(&body[at..]) else {
                    at += 1;
                    continue;
                };
                let tag = &body[at..at + len];
                if styled_kind(tag) == Some(kind) {
                    depth += 1;
                } else if styled_close(tag) == Some(kind) {
                    if depth == 0 {
                        return Some((at, len));
                    }
                    depth -= 1;
                }
                at += len;
            }
            _ => at += body[at..].chars().next()?.len_utf8(),
        }
    }
    None
}

/// An inline tag ends at an unquoted `>`; quoted attribute contents are literal.
pub(crate) fn inline_tag_len(source: &str) -> Option<usize> {
    if !source.starts_with('<') {
        return None;
    }
    let mut quote = None;
    for (at, c) in source.char_indices().skip(1) {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '<') => return None,
            (None, '>') => return Some(at + 1),
            _ => {}
        }
    }
    None
}

/// Tag identity even when its styling is unsupported. Such a tag stays literal,
/// but its closer must not consume a supported enclosing tag's style.
pub(crate) fn styled_kind(tag: &str) -> Option<StyledKind> {
    let inner = tag.trim().strip_prefix('<')?.strip_suffix('>')?;
    match inner
        .split_ascii_whitespace()
        .next()?
        .to_ascii_lowercase()
        .as_str()
    {
        "mark" => Some(StyledKind::Mark),
        "span" => Some(StyledKind::Span),
        "u" => Some(StyledKind::Underline),
        _ => None,
    }
}

/// `name="value"` / `name='value'` / `name=value` out of an attribute list.
fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs.trim_start();
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let key = &rest[..end];
        rest = rest[end..].trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            if end == 0 {
                return None;
            }
            continue;
        };
        rest = value.trim_start();
        let (value, consumed) = match rest.chars().next()? {
            q @ ('"' | '\'') => {
                let end = rest[1..].find(q)?;
                (&rest[1..end + 1], end + 2)
            }
            _ => {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], end)
            }
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
        rest = rest[consumed..].trim_start();
    }
    None
}

/// A CSS color as `0xRRGGBBAA`: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
/// `rgb(r, g, b)` / `rgba(r, g, b, a)`, and the basic named colors.
pub(crate) fn css_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let d: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        let (r, g, b, a) = match d.as_slice() {
            [r, g, b] => (r * 17, g * 17, b * 17, 255),
            [r, g, b, a] => (r * 17, g * 17, b * 17, a * 17),
            [r1, r2, g1, g2, b1, b2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255),
            [r1, r2, g1, g2, b1, b2, a1, a2] => {
                (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2)
            }
            _ => return None,
        };
        return Some(r << 24 | g << 16 | b << 8 | a);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|r| r.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if !matches!(parts.len(), 3 | 4) {
            return None;
        }
        let channel = |p: &str| {
            p.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v.clamp(0.0, 255.0) as u32)
        };
        let (r, g, b) = (channel(parts[0])?, channel(parts[1])?, channel(parts[2])?);
        let a = match parts.get(3) {
            Some(p) => {
                let alpha = p.parse::<f32>().ok()?;
                if !alpha.is_finite() {
                    return None;
                }
                (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
            }
            None => 255,
        };
        return Some(r << 24 | g << 16 | b << 8 | a);
    }
    let rgb = match lower.as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "pink" => 0xffc0cb,
        "gray" | "grey" => 0x808080,
        "brown" => 0xa52a2a,
        "cyan" => 0x00ffff,
        "magenta" => 0xff00ff,
        "lime" => 0x00ff00,
        "navy" => 0x000080,
        "teal" => 0x008080,
        _ => return None,
    };
    Some(rgb << 8 | 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_close_follows_the_pairing_rules() {
        assert_eq!(highlight_close("a ==b== c", 2), Some(5));
        assert_eq!(highlight_close("a ==b=== c", 2), None);
    }

    #[test]
    fn styled_tags_and_css_colors_parse() {
        let t = styled_tag(r#"<mark style="background:#ff0000">"#).unwrap();
        assert_eq!((t.kind, t.background), (StyledKind::Mark, Some(0xff0000ff)));
        let t = styled_tag("<mark>").unwrap();
        assert_eq!(
            (t.kind, t.background, t.color),
            (StyledKind::Mark, None, None)
        );
        let t = styled_tag("<span style='color: rgb(0, 255, 0); font-weight: bold'>").unwrap();
        assert_eq!((t.kind, t.color), (StyledKind::Span, Some(0x00ff00ff)));
        assert_eq!(styled_tag("<span class=\"x\">"), None);
        assert_eq!(
            styled_tag("<u>").map(|t| t.kind),
            Some(StyledKind::Underline)
        );
        assert_eq!(styled_tag("<b>"), None);
        assert_eq!(styled_close("</MARK>"), Some(StyledKind::Mark));
        assert_eq!(css_color("#abc"), Some(0xaabbccff));
        assert_eq!(css_color("#11223344"), Some(0x11223344));
        assert_eq!(css_color("rgba(255, 0, 0, 0.5)"), Some(0xff000080));
        assert_eq!(css_color("Orange"), Some(0xffa500ff));
        assert_eq!(css_color("#12"), None);
    }

    #[test]
    fn unsupported_attributes_and_colors_are_not_styling() {
        for tag in [
            "<span data-style='color:red'>",
            "<span title='style=color:red'>",
            "<span style='color:rgb(NaN,0,0)'>",
            "<span style='color:rgba(0,0,0,inf)'>",
            "<span style='color:rgb(1,2,3,4,5)'>",
        ] {
            assert_eq!(styled_tag(tag), None, "{tag}");
        }
        assert_eq!(
            styled_tag("<span title='a>b' STYLE = 'color:blue'>")
                .unwrap()
                .color,
            Some(0x0000ffff),
        );
    }

    #[test]
    fn math_fence_normalization() {
        // Paired words-fences split onto their own lines.
        assert_eq!(
            normalize_math_fences("wer $$\nx+y\n$$ wer").as_ref(),
            "wer\n$$\nx+y\n$$\nwer"
        );
        // An opener with no closer before a blank line is prose — untouched
        // (and Borrowed).
        let prose = "cost $$\n\nlater";
        assert!(matches!(
            normalize_math_fences(prose),
            std::borrow::Cow::Borrowed(_)
        ));
        // Code fences are left alone.
        let code = "```sh\necho $$\n$$\n```";
        assert!(matches!(
            normalize_math_fences(code),
            std::borrow::Cow::Borrowed(_)
        ));
        // A words-mixed complete pair splits onto its own line.
        assert_eq!(
            normalize_math_fences("What if words $$E=mc^2$$ more").as_ref(),
            "What if words\n$$E=mc^2$$\nmore"
        );
        // Prices (delimiter-adjacent whitespace) stay prose.
        assert!(matches!(
            normalize_math_fences("fees are $$5 and $$10 total"),
            std::borrow::Cow::Borrowed(_)
        ));
        // A pair ALONE on its line is already well-formed.
        assert!(matches!(
            normalize_math_fences("$$y$$"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn markers_do_not_decide_a_line_s_direction() {
        let dir = |line| content_direction_opt(line).unwrap_or(Direction::Ltr);
        // The bug: `x` is strong left-to-right, so a COMPLETED task read LTR
        // while the same line unchecked read RTL — the two sat on opposite
        // sides of the note.
        assert!(dir("- [x] یک کار انجام‌شده").is_rtl());
        assert!(dir("- [ ] یک کار انجام‌نشده").is_rtl());
        assert!(dir("- مورد فهرست").is_rtl());
        assert!(dir("1. مورد شماره‌دار").is_rtl());
        assert!(dir("## سلام دنیا").is_rtl());
        assert!(dir("> یک نقل‌قول").is_rtl());
        // An alert's label is Latin whatever the prose is.
        assert!(dir("> [!NOTE]\n> یک هشدار فارسی").is_rtl());
        assert!(dir("> [!WARNING]- یک هشدار").is_rtl());
        assert!(!dir("> [!NOTE]\n> an english callout").is_rtl());
        // A marker-only line has NO direction of its own — the caller decides
        // whether that means the line above or the content below.
        assert_eq!(content_direction_opt("> [!NOTE]"), None);
        assert_eq!(content_direction_opt("- "), None);
        assert_eq!(content_direction_opt(""), None);
        assert!(!dir("> > [!NOTE]\n").is_rtl(), "no content");
        // Latin content still reads left-to-right, markers or not.
        assert!(!dir("- [x] a done task").is_rtl());
        assert!(!dir("## English heading").is_rtl());
        // A line that is only markers has no direction of its own.
        assert!(!dir("- [ ] ").is_rtl());
    }

    #[test]
    fn base_direction_follows_the_first_strong_character() {
        use Direction::*;
        // Plain cases.
        assert_eq!(base_direction("hello"), Ltr);
        assert_eq!(base_direction("سلام دنیا"), Rtl); // Persian
        assert_eq!(base_direction("שלום עולם"), Rtl); // Hebrew
        // Neutrals lead — markdown markers, digits, punctuation, whitespace
        // are all skipped, which is what makes list items and headings work.
        assert_eq!(base_direction("- سلام"), Rtl);
        assert_eq!(base_direction("## سلام"), Rtl);
        assert_eq!(base_direction("> «سلام»"), Rtl);
        assert_eq!(base_direction("  \t123 — سلام"), Rtl);
        assert_eq!(base_direction("- hello"), Ltr);
        // The FIRST strong character decides, not the majority: a line opening
        // in English stays LTR however much Persian follows (and vice versa).
        assert_eq!(base_direction("Rust سلام دنیا و بیشتر"), Ltr);
        assert_eq!(base_direction("سلام Rust and more English"), Rtl);
        // No strong character anywhere → LTR.
        assert_eq!(base_direction(""), Ltr);
        assert_eq!(base_direction("123 — !?"), Ltr);
        // Other scripts are LTR.
        assert_eq!(base_direction("日本語"), Ltr);
        assert_eq!(base_direction("Привет"), Ltr);
        assert!(Direction::Rtl.is_rtl() && !Direction::Ltr.is_rtl());
    }

    #[test]
    fn alert_recognition_both_forms() {
        assert!(matches!(
            alert_prefix("  [!TIP] x"),
            Some((AlertKind::Tip, 9, None))
        ));
        assert_eq!(AlertKind::Caution.label(), "Caution");
    }

    #[test]
    fn alert_fold_markers_and_toggle() {
        // `-` = folded, `+` = open; the strip consumes the fold char.
        assert!(matches!(
            alert_prefix(" [!TIP]- x"),
            Some((AlertKind::Tip, 9, Some(true)))
        ));

        // The fold char locates + flips within a full source line.
        assert_eq!(alert_fold_char("> [!NOTE]- body"), Some((9, true)));
        assert_eq!(alert_fold_char("> [!NOTE] body"), None);
    }

    #[test]
    fn links_cover_urls_only() {
        // Note-taking syntax is plain text (ADR 0030): `[[wiki]]`, `#tag`, `((id))`.
        let hits =
            links("see [[Page|alias]] and [x](https://a.io) #tag/sub ((b1)) https://b.io/p, done");
        assert_eq!(
            hits.iter().map(|(_, h)| h).collect::<Vec<_>>(),
            vec![
                &LinkHit::Url("https://a.io".into()),
                &LinkHit::Url("https://b.io/p".into()), // trailing comma trimmed
            ]
        );
        // Opaque: code spans, images, footnotes, glued #.
        assert!(links("`https://x.io` ![a](i.png) [^1] word#no").is_empty());
        // Regression: multi-byte text before a URL must not panic the
        // byte-wise walk (it once str-sliced at a continuation byte).
        let hits = links("shrug ¯\\_(ツ)_/¯ then https://a.io done");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn ordered_markers_cycle_word_style() {
        assert_eq!(ordered_marker(0, 2), "2.");
        assert_eq!(ordered_marker(1, 1), "a.");
        assert_eq!(ordered_marker(1, 27), "aa.");
        assert_eq!(ordered_marker(2, 4), "iv.");
        assert_eq!(ordered_marker(2, 9), "ix.");
        assert_eq!(ordered_marker(3, 2), "2."); // cycle restarts
    }

    #[test]
    fn table_style_markers_parse() {
        assert_eq!(
            table_style_marker("<!-- table:striped -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(table_style_marker("<!-- math:left -->"), None);
        assert_eq!(table_style_marker("plain text"), None);
    }
}
