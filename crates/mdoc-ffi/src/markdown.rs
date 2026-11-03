//! The Markdown spans the mobile apps style their text with (ADR 0085's Text
//! display rule), from `mdoc-markdown`, the recognition the desktop editor
//! uses.
//!
//! The apps draw the Markdown source rendered, markers hidden, as desktop
//! does (ADR 0096), or with its markers dimmed (Show Markdown). Either way
//! the text is the source, so every range is a UTF-8 byte range into the
//! text the review returned, the same offsets candidates use. The bindings say what a piece of text is, never how it
//! looks: no fonts and no theme colours, only the colours the document itself
//! names.

use std::ops::Range;

use mdoc_markdown::{
    lines::{
        blockquote_prefix, footnote_def, heading_level, html_block, list_levels, list_prefix,
        ordered_scan, task_prefix, thematic_break,
    },
    spans::{Background, Color, Span, inline_image_spans, is_backslash_escaped, scan_line},
    syntax::{AlertKind, TableStyle, alert_prefix, ordered_marker},
    tables::{Align, table_cell_ranges, table_regions},
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

/// A list item's line: where its bullet, number or box goes and how deep it
/// sits (ADR 0096). Its source marker is a hidden marker span.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownListItem {
    /// The line without its line break.
    pub range: TextRange,
    /// How many lists are open above the item (0 at the top level), whatever
    /// the source's spaces per level.
    pub depth: u32,
    /// What desktop paints in the marker's place: `•`, or the item's
    /// renumbered position as `1.`, `a.` or `i.` by depth. Empty for a task.
    pub label: String,
    /// A task item's box: checked or not. `None` for a plain item.
    pub task: Option<bool>,
    /// Siblings of one ordered list share an id, so their bodies can share
    /// one marker width. `None` for a bullet or task item.
    pub list: Option<u32>,
}

/// One of the five GitHub alert kinds (`> [!NOTE]` …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownAlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// What a line is beyond its [`MarkdownLineKind`]: roles the rendered view
/// draws (ADR 0096).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownBlockKind {
    /// A code block's opening or closing fence, hidden in the rendered view.
    CodeFence,
    /// A line of an alert (`> [!NOTE]` and the quote lines under it); the
    /// title line holds the hidden marker, where the alert's name is drawn.
    Alert {
        kind: MarkdownAlertKind,
        title: bool,
    },
    /// A footnote definition or a raw HTML line, drawn greyed as desktop does.
    Muted,
}

/// A line with a rendered-view role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownBlock {
    /// The line without its line break.
    pub range: TextRange,
    pub kind: MarkdownBlockKind,
}

/// A table column's alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownAlign {
    Left,
    Center,
    Right,
}

/// A table's look, from its `<!-- table:STYLE -->` marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownTableStyle {
    /// An outer box and every gridline (the default).
    Grid,
    /// Alternate body rows shaded, a rule under the header.
    Striped,
    /// Only the header row shaded.
    Header,
    /// Just a rule under the header.
    Minimal,
}

/// One table row: its line and each cell's trimmed content.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownTableRow {
    /// The line without its line break.
    pub range: TextRange,
    /// Each cell's content without its spaces; an empty cell is an empty
    /// range where its content would be.
    pub cells: Vec<TextRange>,
}

/// A GFM table the rendered view draws as a grid (ADR 0096).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MarkdownTable {
    /// Every line of the table, from its style marker (if any) to its last
    /// row, without the last line break.
    pub range: TextRange,
    /// The header row, then the body rows; the delimiter row is left out.
    pub rows: Vec<MarkdownTableRow>,
    pub aligns: Vec<MarkdownAlign>,
    pub style: MarkdownTableStyle,
    /// Column widths in desktop points from the marker's `cols=`; the app
    /// keeps their proportions. `None` when the table sets none.
    pub widths: Option<Vec<f32>>,
    /// The table reads right to left.
    pub rtl: bool,
}

/// A text's spans and lines, each in source order. Spans never overlap.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct MarkdownStyling {
    pub spans: Vec<MarkdownSpan>,
    pub lines: Vec<MarkdownLine>,
    /// List items, in source order (ADR 0096).
    pub list_items: Vec<MarkdownListItem>,
    /// Code fences, alerts and greyed lines, in source order (ADR 0096).
    pub blocks: Vec<MarkdownBlock>,
    /// Tables, in source order (ADR 0096).
    pub tables: Vec<MarkdownTable>,
    /// Inline images, in source order, outside code and tables (ADR 0096).
    pub images: Vec<MarkdownImage>,
}

/// An inline `![alt](src)` image: the rendered view draws an Image chip in
/// its place, since the app shows no pictures (ADR 0096).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MarkdownImage {
    /// The whole `![alt](src)`.
    pub range: TextRange,
    /// The alt text; empty where it would be (`![](src)`).
    pub alt: TextRange,
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

    let sources: Vec<&str> = lines.iter().map(|range| &text[range.clone()]).collect();
    let levels = list_levels(&sources);
    let (numbers, lists) = ordered_scan(&sources);

    // Lines inside fenced code: a table written there is code, not a table.
    let mut fenced = vec![false; lines.len()];
    let mut open: Option<(u8, usize)> = None;
    for (index, line) in sources.iter().enumerate() {
        if let Some(fence) = open {
            fenced[index] = true;
            if closes_fence(line, fence) {
                open = None;
            }
        } else if let Some(fence) = opens_fence(line) {
            fenced[index] = true;
            open = Some(fence);
        }
    }

    let mut table = vec![None; lines.len()];
    let mut tables = Vec::new();
    for region in table_regions(text) {
        let first = region.marker_line.unwrap_or(region.lines.start);
        if (first..region.lines.end).any(|index| fenced[index]) {
            continue;
        }
        tables.push(table_of(text, &lines, &region));
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
    out.tables = tables;
    let mut fence: Option<(u8, usize)> = None;
    // The alert the quote lines under it belong to.
    let mut alert: Option<MarkdownAlertKind> = None;
    for (index, range) in lines.iter().enumerate() {
        let line = &text[range.clone()];
        if let Some(open) = fence {
            // Code is literal: its line is a code block, with no inline spans.
            out.line(range, MarkdownLineKind::CodeBlock);
            if closes_fence(line, open) {
                out.marker(trimmed(text, range));
                out.block(range, MarkdownBlockKind::CodeFence);
                fence = None;
            }
            continue;
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
            alert = None;
            out.line(range, MarkdownLineKind::CodeBlock);
            out.marker(trimmed(text, range));
            out.block(range, MarkdownBlockKind::CodeFence);
            continue;
        }
        if table[index].is_none() {
            // An alert runs from its marker line over the quote lines under it.
            match blockquote_prefix(line) {
                Some(prefix) => {
                    if let Some((kind, ..)) = alert_prefix(&line[prefix..]) {
                        alert = Some(alert_kind(kind));
                        out.block(
                            range,
                            MarkdownBlockKind::Alert {
                                kind: alert_kind(kind),
                                title: true,
                            },
                        );
                    } else if let Some(kind) = alert {
                        out.block(range, MarkdownBlockKind::Alert { kind, title: false });
                    }
                }
                None => alert = None,
            }
            if footnote_def(line).is_some() || html_block(line) {
                out.block(range, MarkdownBlockKind::Muted);
            }
            if let Some(depth) = levels[index] {
                let task = task_prefix(line).map(|(_, _, checked)| checked);
                let (number, level) = numbers[index];
                let ordered = list_prefix(line).is_some_and(|(_, _, ordered, _)| ordered);
                let label = match (task, ordered) {
                    (Some(_), _) => String::new(),
                    (None, true) => ordered_marker(level, number),
                    (None, false) => "•".to_owned(),
                };
                out.list_items.push(MarkdownListItem {
                    range: TextRange::of(range),
                    depth: depth as u32,
                    label,
                    task,
                    list: if task.is_none() && ordered {
                        lists[index]
                    } else {
                        None
                    },
                });
            }
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
        if let Some(level) = line_heading(line) {
            out.line(range, MarkdownLineKind::Heading { level });
        } else if blockquote_prefix(line).is_some() {
            out.line(range, MarkdownLineKind::Quote);
        }
        out.scan(range.clone());
    }
    out.finish()
}

fn alert_kind(kind: AlertKind) -> MarkdownAlertKind {
    match kind {
        AlertKind::Note => MarkdownAlertKind::Note,
        AlertKind::Tip => MarkdownAlertKind::Tip,
        AlertKind::Important => MarkdownAlertKind::Important,
        AlertKind::Warning => MarkdownAlertKind::Warning,
        AlertKind::Caution => MarkdownAlertKind::Caution,
    }
}

/// A table region as the bindings carry it: rows without the delimiter row,
/// each with its cells' trimmed content ranges.
fn table_of(
    text: &str,
    lines: &[Range<usize>],
    region: &mdoc_markdown::tables::TableRegion,
) -> MarkdownTable {
    let first = region.marker_line.unwrap_or(region.lines.start);
    let rows = region
        .lines
        .clone()
        .filter(|&index| index != region.lines.start + 1)
        .map(|index| {
            let range = &lines[index];
            MarkdownTableRow {
                range: TextRange::of(range),
                cells: row_cells(&text[range.clone()])
                    .into_iter()
                    .map(|cell| TextRange::of(&(range.start + cell.start..range.start + cell.end)))
                    .collect(),
            }
        })
        .collect();
    MarkdownTable {
        range: TextRange::of(&(lines[first].start..lines[region.lines.end - 1].end)),
        rows,
        aligns: region
            .aligns
            .iter()
            .map(|align| match align {
                Align::Left => MarkdownAlign::Left,
                Align::Center => MarkdownAlign::Center,
                Align::Right => MarkdownAlign::Right,
            })
            .collect(),
        style: match region.style {
            TableStyle::Grid => MarkdownTableStyle::Grid,
            TableStyle::Striped => MarkdownTableStyle::Striped,
            TableStyle::Header => MarkdownTableStyle::Header,
            TableStyle::Minimal => MarkdownTableStyle::Minimal,
        },
        widths: region.col_widths_attr.clone(),
        rtl: region.rtl,
    }
}

/// A row's cells, line-local: between the pipes, plus the text after the
/// last pipe when the closing pipe is left out (`| a | b`), as the desktop's
/// `table_cells` reads it.
fn row_cells(line: &str) -> Vec<Range<usize>> {
    let mut cells = table_cell_ranges(line);
    if let Some(last) = line.rfind('|') {
        let after = &line[last + 1..];
        let content = after.trim();
        if !content.is_empty() {
            let start = last + 1 + (after.len() - after.trim_start().len());
            cells.push(start..start + content.len());
        }
    }
    cells
}

/// A line's heading level: an ATX heading, or one right after a list or task
/// marker (`- ## Notes`), which `scan_line` styles as a heading too.
fn line_heading(line: &str) -> Option<u8> {
    heading_level(line).or_else(|| {
        let prefix = task_prefix(line)
            .map(|(prefix, ..)| prefix)
            .or_else(|| list_prefix(line).map(|(prefix, ..)| prefix))?;
        heading_level(&line[prefix..])
    })
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
    list_items: Vec<MarkdownListItem>,
    blocks: Vec<MarkdownBlock>,
    tables: Vec<MarkdownTable>,
    images: Vec<MarkdownImage>,
}

impl<'a> Styler<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            spans: Vec::new(),
            lines: Vec::new(),
            list_items: Vec::new(),
            blocks: Vec::new(),
            tables: Vec::new(),
            images: Vec::new(),
        }
    }

    fn block(&mut self, range: &Range<usize>, kind: MarkdownBlockKind) {
        self.blocks.push(MarkdownBlock {
            range: TextRange::of(range),
            kind,
        });
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
        // An image counts only where the scan hid its `![` as syntax: one
        // inside raw HTML or code stays literal text.
        let mut openers = Vec::new();
        for span in found {
            if span.style.hide && self.text[span.range.clone()].starts_with("![") {
                openers.push(span.range.start);
            }
            self.add(span);
        }
        let piece = &self.text[range.clone()];
        for (full, _) in inline_image_spans(piece) {
            let start = range.start + full.start;
            if !openers.contains(&start) {
                continue;
            }
            let alt_end = full.start + piece[full.clone()].find("](").unwrap_or(2);
            self.images.push(MarkdownImage {
                range: TextRange::of(&(start..range.start + full.end)),
                alt: TextRange::of(&(start + 2..range.start + alt_end)),
            });
        }
    }

    /// A table row: each cell scans on its own, as the desktop's table does,
    /// and the pipes between cells are markers.
    fn table_row(&mut self, range: &Range<usize>) {
        let line = &self.text[range.clone()];
        let cells = row_cells(line);
        let bytes = line.as_bytes();
        let mut pipes = Vec::new();
        for (index, &byte) in bytes.iter().enumerate() {
            let in_cell = cells.iter().any(|cell| cell.contains(&index));
            if byte == b'|' && !in_cell && !is_backslash_escaped(bytes, index) {
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
        // An escape's backslash (`1\.`) comes as a marker, its character as
        // text. A hard line break's backslash is hidden syntax too; an entity
        // (`&amp;`) shows as written.
        if style.hide && !out.marker && style.replace.is_none() {
            out.marker = true;
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
            list_items: self.list_items,
            blocks: self.blocks,
            tables: self.tables,
            images: self.images,
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
    fn a_row_without_its_closing_pipe_styles_its_last_cell() {
        let text = "| A | B |\n| --- | --- |\n| left | **right**\n| `one`";
        assert_eq!(
            described(text),
            pairs(&[
                ("|", "marker"),
                ("|", "marker"),
                ("|", "marker"),
                ("| --- | --- |", "marker"),
                ("|", "marker"),
                ("|", "marker"),
                ("**", "marker"),
                ("right", "strong"),
                ("**", "marker"),
                ("|", "marker"),
                ("`", "marker"),
                ("one", "code"),
                ("`", "marker"),
            ])
        );
    }

    #[test]
    fn a_heading_in_a_list_item_is_a_heading_line() {
        for (text, level) in [("- ## Notes", 2), ("1. ### Steps", 3), ("- [x] # Done", 1)] {
            assert_eq!(
                styling(text).lines,
                vec![MarkdownLine {
                    range: TextRange {
                        start: 0,
                        end: text.len() as u64,
                    },
                    kind: MarkdownLineKind::Heading { level },
                }],
                "{text}"
            );
        }
        assert!(styling("- #hashtag").lines.is_empty());
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
        // A converter's numbered paragraph: `1\.` is no list, and only the
        // backslash dims.
        let text = "1\\. **Harrow & Finch Ltd**, of\n2\\. Ms Maya Bennett";
        assert_eq!(
            described(text),
            pairs(&[
                (r"\", "marker"),
                ("**", "marker"),
                ("Harrow & Finch Ltd", "strong"),
                ("**", "marker"),
                (r"\", "marker"),
            ])
        );
        // Escaped emphasis and a heading sign stay literal.
        assert_eq!(
            described(r"\*not\* \# no"),
            pairs(&[(r"\", "marker"), (r"\", "marker"), (r"\", "marker")])
        );
        assert!(styling(r"\# no").lines.is_empty());
        // A character the escape sits in keeps the construct's style.
        assert_eq!(
            described(r"**a\*b**"),
            pairs(&[
                ("**", "marker"),
                ("a", "strong"),
                (r"\", "marker"),
                ("*", "strong"),
                ("b", "strong"),
                ("**", "marker"),
            ])
        );
    }

    #[test]
    fn an_escape_in_a_link_label_dims_its_backslash() {
        assert_eq!(
            described(r"[a\*](u)"),
            pairs(&[
                ("[", "marker"),
                ("a", "link"),
                (r"\", "marker"),
                ("*", "link"),
                ("](u)", "marker"),
            ])
        );
    }

    #[test]
    fn a_backslash_in_code_or_before_a_letter_is_text() {
        assert_eq!(
            described(r"`a\*b` C:\Users"),
            pairs(&[("`", "marker"), (r"a\*b", "code"), ("`", "marker")])
        );
        let text = "```\n1\\. code\n```";
        assert_eq!(
            described(text),
            pairs(&[("```", "marker"), ("```", "marker")])
        );
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
            (r"\", "marker"),
        ] {
            assert!(
                all.contains(&(text.to_owned(), kind.to_owned())),
                "no {kind} span on {text}"
            );
        }
    }

    /// What the rendered view draws (ADR 0096): list items with desktop's
    /// labels and levels, fences, alerts, greyed lines and table rows.
    #[test]
    fn rendered_view_roles() {
        let text = "1. One\n3. Two\n    - Sub\n    1. Deep\n- [x] Done\n\n> [!WARNING]\n> Body\n\n[^1]: note\n<div>\n\n```\ncode\n```\n\n<!-- table:striped cols=100,200 -->\n| A | B |\n| --- | :-: |\n| a | b |\n| c";
        let styling = styling(text);
        let items: Vec<_> = styling
            .list_items
            .iter()
            .map(|item| {
                (
                    slice(text, item.range),
                    item.depth,
                    item.label.as_str(),
                    item.task,
                )
            })
            .collect();
        assert_eq!(
            items,
            [
                ("1. One", 0, "1.", None),
                ("3. Two", 0, "2.", None),
                ("    - Sub", 1, "•", None),
                ("    1. Deep", 1, "a.", None),
                ("- [x] Done", 0, "", Some(true)),
            ]
        );
        assert_eq!(styling.list_items[0].list, styling.list_items[1].list);
        assert!(styling.list_items[0].list.is_some());
        assert_eq!(styling.list_items[2].list, None);
        let blocks: Vec<_> = styling
            .blocks
            .iter()
            .map(|block| (slice(text, block.range), block.kind))
            .collect();
        assert_eq!(
            blocks,
            [
                (
                    "> [!WARNING]",
                    MarkdownBlockKind::Alert {
                        kind: MarkdownAlertKind::Warning,
                        title: true
                    }
                ),
                (
                    "> Body",
                    MarkdownBlockKind::Alert {
                        kind: MarkdownAlertKind::Warning,
                        title: false
                    }
                ),
                ("[^1]: note", MarkdownBlockKind::Muted),
                ("<div>", MarkdownBlockKind::Muted),
                ("```", MarkdownBlockKind::CodeFence),
                ("```", MarkdownBlockKind::CodeFence),
            ]
        );
        let [table] = styling.tables.as_slice() else {
            panic!("one table expected");
        };
        assert!(slice(text, table.range).starts_with("<!-- table:striped"));
        assert!(slice(text, table.range).ends_with("| c"));
        assert_eq!(table.style, MarkdownTableStyle::Striped);
        assert_eq!(table.widths, Some(vec![100., 200.]));
        assert_eq!(table.aligns, [MarkdownAlign::Left, MarkdownAlign::Center]);
        let rows: Vec<Vec<&str>> = table
            .rows
            .iter()
            .map(|row| row.cells.iter().map(|cell| slice(text, *cell)).collect())
            .collect();
        assert_eq!(rows, [vec!["A", "B"], vec!["a", "b"], vec!["c"]]);
    }

    /// Images carry their whole range and their alt text, empty or not; code
    /// keeps its images literal.
    #[test]
    fn images_and_their_alt_text() {
        let text = "See ![Floor plan](plan.png) and ![](scan.png).\n`![no](x.png)`";
        let styling = styling(text);
        let images: Vec<_> = styling
            .images
            .iter()
            .map(|image| (slice(text, image.range), slice(text, image.alt)))
            .collect();
        assert_eq!(
            images,
            [
                ("![Floor plan](plan.png)", "Floor plan"),
                ("![](scan.png)", "")
            ]
        );
        // Raw HTML keeps its image syntax literal; a table cell's image counts.
        let text = "<!-- ![](scan.png) -->\n\n| A | B |\n| --- | --- |\n| ![](stamp.png) | ![Seal](seal.png) |";
        let images: Vec<_> = super::styling(text)
            .images
            .iter()
            .map(|image| (slice(text, image.range), slice(text, image.alt)))
            .collect();
        assert_eq!(
            images,
            [("![](stamp.png)", ""), ("![Seal](seal.png)", "Seal")]
        );
    }

    /// A table written inside fenced code is code: no table record, and its
    /// pipes stay code text.
    #[test]
    fn a_fenced_table_is_code() {
        let text = "```\n| A | B |\n| --- | --- |\n| a | b |\n```";
        let styling = styling(text);
        assert!(styling.tables.is_empty());
        assert!(
            styling
                .lines
                .iter()
                .all(|line| line.kind == MarkdownLineKind::CodeBlock)
        );
    }
}
