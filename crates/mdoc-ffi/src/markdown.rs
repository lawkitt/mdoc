//! The Markdown spans the mobile apps style their text with (ADR 0085's Text
//! display rule), from `mdoc-markdown`, the recognition the desktop editor
//! uses.
//!
//! The apps show the Markdown source with its markers dimmed, so every range
//! is a UTF-8 byte range into the text the review returned, the same offsets
//! candidates use. The bindings say what a piece of text is, never how it
//! looks: no fonts and no theme colours, only the colours the document itself
//! names.

use std::ops::Range;

use mdoc_markdown::{
    lines::{blockquote_prefix, heading_level, thematic_break},
    spans::{Background, Color, Span, scan_line},
    tables::{table_cell_ranges, table_regions},
};

use crate::library::TextRange;

/// An inline piece of Markdown. A span can be several kinds at once (a bold
/// link inside a highlight), so the kinds are flags.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownSpan {
    pub range: TextRange,
    /// Syntax the app draws dimmed: `**`, `#`, `[`, `](…)`, list bullets,
    /// table pipes, code fences.
    pub marker: bool,
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    pub underline: bool,
    /// Inline code.
    pub code: bool,
    /// Link text, bare URLs and footnote references.
    pub link: bool,
    /// `==highlight==` or a plain `<mark>`.
    pub highlight: bool,
    /// A text colour the document names (`<span style="color:…">`), as
    /// `0xRRGGBBAA`.
    pub color: Option<u32>,
    /// A background the document names (`<mark style="background:…">`), as
    /// `0xRRGGBBAA`.
    pub background: Option<u32>,
}

/// What a whole line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownLineKind {
    Heading {
        level: u8,
    },
    Quote,
    /// A fenced code block's line, the fences included.
    CodeBlock,
    TableHeader,
    /// A table's body row or its `| --- |` delimiter row.
    TableRow,
    /// A thematic break (`---`).
    Rule,
}

/// A line with a block role; lines of plain text have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownLine {
    /// The line without its line break.
    pub range: TextRange,
    pub kind: MarkdownLineKind,
}

/// A text's spans and lines, each in source order. Spans never overlap.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownStyling {
    pub spans: Vec<MarkdownSpan>,
    pub lines: Vec<MarkdownLine>,
}

/// The spans and line roles of a whole Markdown text: one call per text, so
/// the app styles one attributed string.
#[uniffi::export]
pub fn markdown_styling(text: String) -> MarkdownStyling {
    styling(&text)
}

/// A table line's part in its table.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TablePart {
    StyleMarker,
    Header,
    Delimiter,
    Body,
}

pub(crate) fn styling(text: &str) -> MarkdownStyling {
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        // A CRLF text keeps its `\r` out of the line.
        let end = offset + line.strip_suffix('\r').unwrap_or(line).len();
        lines.push(offset..end);
        offset += line.len() + 1;
    }

    let mut table = vec![None; lines.len()];
    for region in table_regions(text) {
        for index in region.lines.clone() {
            table[index] = Some(if index == region.lines.start {
                TablePart::Header
            } else if index == region.lines.start + 1 {
                TablePart::Delimiter
            } else {
                TablePart::Body
            });
        }
        if let Some(index) = region.marker_line {
            table[index] = Some(TablePart::StyleMarker);
        }
    }

    let mut out = Styler::new(text);
    let mut fence: Option<(u8, usize)> = None;
    for (index, range) in lines.iter().enumerate() {
        let line = &text[range.clone()];
        if let Some(open) = fence {
            // Code is literal: its line is a code block, with no inline spans.
            out.line(range, MarkdownLineKind::CodeBlock);
            if closes_fence(line, open) {
                out.marker(trimmed(text, range));
                fence = None;
            }
            continue;
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
            out.line(range, MarkdownLineKind::CodeBlock);
            out.marker(trimmed(text, range));
            continue;
        }
        match table[index] {
            Some(TablePart::StyleMarker) => {
                out.marker(trimmed(text, range));
                continue;
            }
            Some(TablePart::Delimiter) => {
                out.line(range, MarkdownLineKind::TableRow);
                out.marker(trimmed(text, range));
                continue;
            }
            Some(part @ (TablePart::Header | TablePart::Body)) => {
                let kind = if part == TablePart::Header {
                    MarkdownLineKind::TableHeader
                } else {
                    MarkdownLineKind::TableRow
                };
                out.line(range, kind);
                out.table_row(range);
                continue;
            }
            None => {}
        }
        if thematic_break(line) {
            out.line(range, MarkdownLineKind::Rule);
            out.marker(trimmed(text, range));
            continue;
        }
        if let Some(level) = heading_level(line) {
            out.line(range, MarkdownLineKind::Heading { level });
        } else if blockquote_prefix(line).is_some() {
            out.line(range, MarkdownLineKind::Quote);
        }
        out.scan(range.clone());
    }
    out.finish()
}

/// A line's range without its leading and trailing whitespace.
fn trimmed(text: &str, range: &Range<usize>) -> Range<usize> {
    let line = &text[range.clone()];
    let start = range.start + (line.len() - line.trim_start().len());
    let end = range.end - (line.len() - line.trim_end().len());
    start..end.max(start)
}

/// A code fence that opens a block (up to three spaces, then three or more
/// backticks or tildes): its character and length. A backtick fence's info
/// string has no backticks.
fn opens_fence(line: &str) -> Option<(u8, usize)> {
    let body = line.trim_start_matches(' ');
    if line.len() - body.len() > 3 {
        return None;
    }
    let mark = *body.as_bytes().first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let length = body.bytes().take_while(|&b| b == mark).count();
    if length < 3 || (mark == b'`' && body[length..].contains('`')) {
        return None;
    }
    Some((mark, length))
}

/// Whether a line closes the open fence: the same character, at least as
/// many, and nothing after but spaces.
fn closes_fence(line: &str, (mark, length): (u8, usize)) -> bool {
    let body = line.trim_start_matches(' ');
    if line.len() - body.len() > 3 {
        return false;
    }
    let run = body.bytes().take_while(|&b| b == mark).count();
    run >= length && body[run..].trim().is_empty()
}

/// Collects spans and lines as the scan goes.
struct Styler<'a> {
    text: &'a str,
    spans: Vec<MarkdownSpan>,
    lines: Vec<MarkdownLine>,
}

impl<'a> Styler<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            spans: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn line(&mut self, range: &Range<usize>, kind: MarkdownLineKind) {
        self.lines.push(MarkdownLine {
            range: TextRange::of(range),
            kind,
        });
    }

    fn marker(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            self.spans.push(MarkdownSpan {
                marker: true,
                ..plain(TextRange::of(&range))
            });
        }
    }

    /// The inline spans of `text[range]`, as the desktop editor scans a line.
    fn scan(&mut self, range: Range<usize>) {
        let mut found = Vec::new();
        scan_line(self.text, range.start, range.end, &mut found);
        for span in found {
            self.add(span);
        }
    }

    /// A table row: each cell scans on its own, as the desktop's table does,
    /// and the pipes between cells are markers.
    fn table_row(&mut self, range: &Range<usize>) {
        let line = &self.text[range.clone()];
        let cells = table_cell_ranges(line);
        let bytes = line.as_bytes();
        let mut pipes = Vec::new();
        for (index, &byte) in bytes.iter().enumerate() {
            let in_cell = cells.iter().any(|cell| cell.contains(&index));
            if byte == b'|' && !in_cell {
                pipes.push(range.start + index..range.start + index + 1);
            }
        }
        for pipe in pipes {
            self.marker(pipe);
        }
        for cell in cells {
            self.scan(range.start + cell.start..range.start + cell.end);
        }
    }

    /// One span from `mdoc-markdown`, in the bindings' terms.
    fn add(&mut self, span: Span) {
        let style = &span.style;
        let range = span.range.clone();
        if range.is_empty() {
            return;
        }
        let mut out = MarkdownSpan {
            range: TextRange::of(&range),
            marker: style.color == Some(Color::Marker),
            strong: style.bold,
            emphasis: style.italic,
            strikethrough: style.strike,
            underline: style.underline,
            code: style.code || style.color == Some(Color::Code),
            link: style.color == Some(Color::Link),
            highlight: style.bg == Some(Background::Mark),
            color: match style.color {
                Some(Color::Rgba(rgba)) => Some(rgba),
                _ => None,
            },
            background: match style.bg {
                Some(Background::Rgba(rgba)) => Some(rgba),
                _ => None,
            },
        };
        if style.hide && !out.marker {
            if style.replace.is_none() {
                // A hard line break's backslash.
                out.marker = true;
            } else if self.text.as_bytes()[range.start] == b'\\' && range.len() > 1 {
                // An escape (`\*`): the backslash is syntax, the character text.
                self.marker(range.start..range.start + 1);
                out.range = TextRange::of(&(range.start + 1..range.end));
            }
            // An entity (`&amp;`) shows as written.
        }
        if out.marker || out != plain(out.range) {
            self.spans.push(out);
        }
    }

    fn finish(mut self) -> MarkdownStyling {
        self.spans
            .sort_by_key(|span| (span.range.start, span.range.end));
        MarkdownStyling {
            spans: self.spans,
            lines: self.lines,
        }
    }
}

/// A span with no kind: nothing to style.
fn plain(range: TextRange) -> MarkdownSpan {
    MarkdownSpan {
        range,
        marker: false,
        strong: false,
        emphasis: false,
        strikethrough: false,
        underline: false,
        code: false,
        link: false,
        highlight: false,
        color: None,
        background: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared fixture the Swift tests and the screenshots use too.
    const FIXTURE: &str = include_str!("../../../tests/fixtures/mobile-markdown/review.md");

    fn slice(text: &str, range: TextRange) -> &str {
        &text[range.start as usize..range.end as usize]
    }

    /// Spans as (text, what it is) pairs, in order.
    fn described(text: &str) -> Vec<(String, String)> {
        styling(text)
            .spans
            .into_iter()
            .map(|span| {
                let mut kinds = Vec::new();
                for (on, name) in [
                    (span.marker, "marker"),
                    (span.strong, "strong"),
                    (span.emphasis, "emphasis"),
                    (span.strikethrough, "strike"),
                    (span.underline, "underline"),
                    (span.code, "code"),
                    (span.link, "link"),
                    (span.highlight, "highlight"),
                ] {
                    if on {
                        kinds.push(name.to_owned());
                    }
                }
                if let Some(rgba) = span.color {
                    kinds.push(format!("color {rgba:08x}"));
                }
                if let Some(rgba) = span.background {
                    kinds.push(format!("background {rgba:08x}"));
                }
                (slice(text, span.range).to_owned(), kinds.join(" "))
            })
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn a_heading_dims_its_marker_and_names_its_level() {
        let styling = styling("## Стороны");
        assert_eq!(
            described("## Стороны"),
            pairs(&[("## ", "marker"), ("Стороны", "strong")])
        );
        assert_eq!(
            styling.lines,
            vec![MarkdownLine {
                range: TextRange { start: 0, end: 17 },
                kind: MarkdownLineKind::Heading { level: 2 },
            }]
        );
    }

    #[test]
    fn emphasis_nests_and_markers_pair() {
        assert_eq!(
            described("**bold *both* bold**"),
            pairs(&[
                ("**", "marker"),
                ("bold ", "strong"),
                ("*", "marker"),
                ("both", "strong emphasis"),
                ("*", "marker"),
                (" bold", "strong"),
                ("**", "marker"),
            ])
        );
    }

    #[test]
    fn links_code_strike_and_highlight() {
        assert_eq!(
            described("[реестр](https://example.com) ~~old~~ `a*b` ==check=="),
            pairs(&[
                ("[", "marker"),
                ("реестр", "link"),
                ("](https://example.com)", "marker"),
                ("~~", "marker"),
                ("old", "strike"),
                ("~~", "marker"),
                ("`", "marker"),
                ("a*b", "code"),
                ("`", "marker"),
                ("==", "marker"),
                ("check", "highlight"),
                ("==", "marker"),
            ])
        );
    }

    #[test]
    fn the_documents_own_colours_come_through() {
        let text = r#"<span style="color:#c0392b">red</span> <mark style="background:#ffd54f">tint</mark>"#;
        assert_eq!(
            described(text),
            pairs(&[
                (r#"<span style="color:#c0392b">"#, "marker"),
                ("red", "color c0392bff"),
                ("</span>", "marker"),
                (r#"<mark style="background:#ffd54f">"#, "marker"),
                ("tint", "background ffd54fff"),
                ("</mark>", "marker"),
            ])
        );
    }

    #[test]
    fn a_table_dims_its_pipes_and_delimiter_and_scans_each_cell() {
        let text = "| Item | Сумма |\n| --- | ---: |\n| **Rent** | £1,150 |";
        assert_eq!(
            described(text),
            pairs(&[
                ("|", "marker"),
                ("|", "marker"),
                ("|", "marker"),
                ("| --- | ---: |", "marker"),
                ("|", "marker"),
                ("**", "marker"),
                ("Rent", "strong"),
                ("**", "marker"),
                ("|", "marker"),
                ("|", "marker"),
            ])
        );
        let kinds: Vec<_> = styling(text).lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MarkdownLineKind::TableHeader,
                MarkdownLineKind::TableRow,
                MarkdownLineKind::TableRow
            ]
        );
    }

    #[test]
    fn fenced_code_is_literal() {
        let text = "```text\n**not bold** `x`\n```\nafter **bold**";
        assert_eq!(
            described(text),
            pairs(&[
                ("```text", "marker"),
                ("```", "marker"),
                ("**", "marker"),
                ("bold", "strong"),
                ("**", "marker"),
            ])
        );
        let kinds: Vec<_> = styling(text).lines.iter().map(|line| line.kind).collect();
        assert_eq!(kinds, vec![MarkdownLineKind::CodeBlock; 3]);
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        let lines = styling("~~~~\ncode\n~~~\nstill code").lines;
        assert_eq!(lines.len(), 4);
        assert!(
            lines
                .iter()
                .all(|line| line.kind == MarkdownLineKind::CodeBlock)
        );
    }

    #[test]
    fn quotes_lists_and_rules() {
        let text = "> Note\n- item\n---";
        assert_eq!(
            described(text),
            pairs(&[("> ", "marker"), ("- ", "marker"), ("---", "marker")])
        );
        let kinds: Vec<_> = styling(text).lines.iter().map(|line| line.kind).collect();
        assert_eq!(kinds, vec![MarkdownLineKind::Quote, MarkdownLineKind::Rule]);
    }

    #[test]
    fn an_escape_dims_only_its_backslash() {
        assert_eq!(described(r"a \* b"), pairs(&[(r"\", "marker")]));
    }

    #[test]
    fn crlf_lines_leave_the_return_out() {
        let styling = styling("# Title\r\nbody");
        assert_eq!(styling.lines[0].range, TextRange { start: 0, end: 7 });
    }

    #[test]
    fn plain_text_has_no_spans() {
        assert_eq!(
            styling("Анна Смирнова 🏠 pays rent."),
            MarkdownStyling::default()
        );
    }

    #[test]
    fn the_fixture_spans_are_ordered_on_character_boundaries_and_never_overlap() {
        let styling = styling(FIXTURE);
        assert!(!styling.spans.is_empty());
        let mut end = 0;
        for span in &styling.spans {
            let (start, stop) = (span.range.start as usize, span.range.end as usize);
            assert!(start >= end, "overlap at {start}");
            assert!(start < stop);
            assert!(FIXTURE.is_char_boundary(start) && FIXTURE.is_char_boundary(stop));
            end = stop;
        }
        let kinds: Vec<_> = styling.lines.iter().map(|line| line.kind).collect();
        for kind in [
            MarkdownLineKind::Heading { level: 1 },
            MarkdownLineKind::Heading { level: 2 },
            MarkdownLineKind::Quote,
            MarkdownLineKind::TableHeader,
            MarkdownLineKind::TableRow,
            MarkdownLineKind::CodeBlock,
            MarkdownLineKind::Rule,
        ] {
            assert!(kinds.contains(&kind), "no {kind:?} line");
        }
        let all = described(FIXTURE);
        for (text, kind) in [
            ("Анна Смирнова", "emphasis"),
            ("реестр", "link"),
            ("пункт 4.2", "code"),
            ("monthly", "strong"),
        ] {
            assert!(
                all.contains(&(text.to_owned(), kind.to_owned())),
                "no {kind} span on {text}"
            );
        }
    }
}
