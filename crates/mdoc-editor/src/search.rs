//! A semantic, source-mapped Markdown find index.
//!
//! The editor stores Markdown source, while the user searches the text that is
//! visible after WYSIWYG syntax is projected away. This module keeps those two
//! coordinate systems separate: each segment is one searchable block and each
//! output character remembers the source range that produced it.

use std::{cell::OnceCell, ops::Range};

use caseless::{Caseless, default_case_fold_str};

use crate::markdown_syntax;

/// One logical search occurrence. A match can cover several source ranges when
/// hidden Markdown syntax lies between its visible characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub source: Vec<Range<usize>>,
}

/// Searchable visible-text segments for one Markdown document.
#[derive(Debug, Default)]
pub struct SearchIndex {
    segments: Vec<SearchSegment>,
}

#[derive(Debug)]
struct SearchSegment {
    text: String,
    units: Vec<MappedUnit>,
    folded: OnceCell<FoldedText>,
}

#[derive(Clone, Debug)]
struct MappedUnit {
    visible: Range<usize>,
    source: Range<usize>,
}

#[derive(Debug)]
struct FoldedText {
    text: String,
    units: Vec<MappedUnit>,
}

#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    next: usize,
}

impl Line {
    fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    fn has_newline(&self) -> bool {
        self.next > self.end
    }

    fn separator(&self) -> Range<usize> {
        self.end..self.next
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Paragraph,
    Heading,
    List,
    Quote,
}

#[derive(Default)]
struct SegmentBuilder {
    text: String,
    units: Vec<MappedUnit>,
}

impl SegmentBuilder {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn push_literal_range(&mut self, source: &str, range: Range<usize>) {
        let mut at = range.start;
        while at < range.end {
            let Some(ch) = source[at..range.end].chars().next() else {
                break;
            };
            let end = at + ch.len_utf8();
            self.push_char(ch, at..end);
            at = end;
        }
    }

    fn push_str(&mut self, decoded: &str, source: Range<usize>) {
        for ch in decoded.chars() {
            self.push_char(ch, source.clone());
        }
    }

    fn push_char(&mut self, ch: char, source: Range<usize>) {
        let visible_start = self.text.len();
        self.text.push(ch);
        self.units.push(MappedUnit {
            visible: visible_start..self.text.len(),
            source,
        });
    }

    fn push_separator(&mut self, source: Range<usize>) {
        self.push_char(' ', source);
    }

    fn push_code_separator(&mut self, source: Range<usize>) {
        self.push_char('\n', source);
    }

    fn finish(self) -> Option<SearchSegment> {
        (!self.text.is_empty()).then_some(SearchSegment {
            text: self.text,
            units: self.units,
            folded: OnceCell::new(),
        })
    }
}

impl SearchIndex {
    /// Build a semantic index from Markdown source without changing the source
    /// offsets used by the editor.
    pub fn from_markdown(source: &str) -> Self {
        let lines = split_lines(source);
        let syntax_style = markdown_syntax::search_style();
        let table_regions = markdown_syntax::table_regions(source);
        let math_regions = markdown_syntax::math_regions(source);
        let mut table_rows = vec![false; lines.len()];
        for region in &table_regions {
            for row in region.lines.clone() {
                if let Some(slot) = table_rows.get_mut(row) {
                    *slot = true;
                }
            }
        }
        let mut math_rows = vec![false; lines.len()];
        for region in &math_regions {
            for row in region.range.clone() {
                if let Some(slot) = math_rows.get_mut(row) {
                    *slot = true;
                }
            }
            if let Some(row) = region.marker_line
                && let Some(slot) = math_rows.get_mut(row)
            {
                *slot = true;
            }
        }
        let mut structural_rows = vec![false; lines.len()];
        for region in &table_regions {
            if let Some(row) = region.marker_line
                && let Some(slot) = structural_rows.get_mut(row)
            {
                *slot = true;
            }
        }

        let mut index = Self::default();
        let mut row = 0;
        while row < lines.len() {
            let line = lines[row];
            let text = line.text(source);

            if text.trim().is_empty()
                || math_rows[row]
                || structural_rows[row]
                || markdown_syntax::thematic_break(text)
            {
                row += 1;
                continue;
            }

            if text.trim_start().starts_with("```") {
                row = index.add_fenced_block(source, &lines, row);
                continue;
            }

            if table_rows[row] {
                if !markdown_syntax::is_table_separator(text) {
                    for cell in markdown_syntax::table_cell_ranges(text) {
                        let start = line.start + cell.start;
                        let end = line.start + cell.end;
                        index.add_projected_line(source, line, start..end, &syntax_style);
                    }
                }
                row += 1;
                continue;
            }

            if markdown_syntax::image_line(text).is_some()
                || markdown_syntax::image_row(text).is_some()
            {
                row += 1;
                continue;
            }

            let kind = block_kind(text);
            let end_row = contiguous_block_end(
                source,
                &lines,
                &table_rows,
                &math_rows,
                &structural_rows,
                row,
                kind,
            );
            index.add_text_block(source, &lines, row..end_row, kind, &syntax_style);
            row = end_row;
        }
        index
    }

    /// Find non-overlapping literal occurrences. With `match_case = false`,
    /// matching uses Unicode full default case folding without normalization.
    pub fn find(&self, query: &str, match_case: bool) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let folded_query = (!match_case).then(|| default_case_fold_str(query));
        if folded_query.as_deref().is_some_and(str::is_empty) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for segment in &self.segments {
            if match_case {
                for (start, _) in segment.text.match_indices(query) {
                    let end = start + query.len();
                    if let Some(source) = map_units(&segment.units, start..end) {
                        push_match(&mut out, source);
                    }
                }
            } else {
                let folded = segment.folded.get_or_init(|| fold_segment(segment));
                for (start, _) in folded
                    .text
                    .match_indices(folded_query.as_deref().unwrap_or_default())
                {
                    let end = start + folded_query.as_ref().map_or(0, String::len);
                    if let Some(source) = map_units(&folded.units, start..end) {
                        push_match(&mut out, source);
                    }
                }
            }
        }
        out
    }

    fn add_fenced_block(&mut self, source: &str, lines: &[Line], start: usize) -> usize {
        let opening = lines[start];
        let opening_text = opening.text(source);
        let trimmed_start = opening_text.len() - opening_text.trim_start().len();
        let info_start = trimmed_start + 3;
        let info = opening_text[info_start..].trim();
        if !info.is_empty()
            && let Some(info_offset) = opening_text[info_start..].find(info)
        {
            let mut builder = SegmentBuilder::default();
            builder.push_literal_range(
                source,
                opening.start + info_start + info_offset
                    ..opening.start + info_start + info_offset + info.len(),
            );
            self.push_builder(builder);
        }

        let mut end = start + 1;
        while end < lines.len() && !lines[end].text(source).trim_start().starts_with("```") {
            end += 1;
        }
        let mut builder = SegmentBuilder::default();
        for (offset, row) in (start + 1..end).enumerate() {
            let line = lines[row];
            builder.push_literal_range(source, line.start..line.end);
            if offset + 1 < end - (start + 1) {
                builder.push_code_separator(line.separator());
            }
        }
        self.push_builder(builder);
        (end + 1).min(lines.len())
    }

    fn add_text_block(
        &mut self,
        source: &str,
        lines: &[Line],
        rows: Range<usize>,
        _kind: BlockKind,
        syntax_style: &markdown_syntax::SyntaxStyle,
    ) {
        let mut current = SegmentBuilder::default();
        let mut previous_hard_break = false;
        let mut separator_before = None;
        for row in rows {
            let line = lines[row];
            let text = line.text(source);
            let hard_break = has_hard_break(text);
            let pieces = self.projected_line(source, line, line.start..line.end, syntax_style);
            if pieces.is_empty() {
                self.push_builder(std::mem::take(&mut current));
                previous_hard_break = true;
                continue;
            }
            for (piece_index, (piece, starts_after_object)) in pieces.into_iter().enumerate() {
                if starts_after_object {
                    self.push_builder(std::mem::take(&mut current));
                }
                if piece_index == 0
                    && !current.is_empty()
                    && !previous_hard_break
                    && separator_before.is_some()
                {
                    current.push_separator(separator_before.take().unwrap());
                }
                append_builder(&mut current, piece);
            }
            if hard_break {
                self.push_builder(std::mem::take(&mut current));
            }
            previous_hard_break = hard_break;
            separator_before = line.has_newline().then(|| line.separator());
        }
        self.push_builder(current);
    }

    fn add_projected_line(
        &mut self,
        source: &str,
        line: Line,
        range: Range<usize>,
        syntax_style: &markdown_syntax::SyntaxStyle,
    ) {
        let pieces = self.projected_line(source, line, range, syntax_style);
        let mut current = SegmentBuilder::default();
        for (piece, boundary) in pieces {
            if boundary {
                self.push_builder(std::mem::take(&mut current));
            }
            append_builder(&mut current, piece);
        }
        self.push_builder(current);
    }

    fn projected_line(
        &self,
        source: &str,
        line: Line,
        range: Range<usize>,
        syntax_style: &markdown_syntax::SyntaxStyle,
    ) -> Vec<(SegmentBuilder, bool)> {
        if range.start >= range.end {
            return Vec::new();
        }
        let line_text = line.text(source);
        let mut visible_end = range.end;
        if range.end == line.end {
            visible_end = visible_content_end(line_text, line.start);
        }
        if range.start >= visible_end {
            return Vec::new();
        }

        let mut visible = Vec::new();
        let mut cursor = line.start;
        let mut spans =
            markdown_syntax::semantic_spans_with(source, line.start, line.end, syntax_style);
        spans.sort_by_key(|span| span.range.start);
        for span in spans {
            if span.range.end <= line.start || span.range.start >= line.end {
                continue;
            }
            let span_start = span.range.start.max(line.start);
            let span_end = span.range.end.min(line.end);
            if span_start > cursor {
                visible.push((cursor..span_start, None));
            }
            if span_start < span_end
                && (span.decoded.is_some() || (!span.hidden && !span.replacement))
            {
                visible.push((span_start..span_end, span.decoded));
            }
            cursor = cursor.max(span_end);
        }
        if cursor < line.end {
            visible.push((cursor..line.end, None));
        }
        if visible.is_empty() {
            return Vec::new();
        }

        let mut objects = Vec::new();
        for (full, _) in markdown_syntax::inline_image_spans(line_text) {
            objects.push((line.start + full.start)..(line.start + full.end));
        }
        for full in markdown_syntax::inline_math_spans(line_text) {
            objects.push((line.start + full.start)..(line.start + full.end));
        }
        objects.sort_by_key(|range| range.start);

        let mut out = Vec::new();
        let mut after_object = false;
        let mut previous_end = range.start;
        for (interval, decoded) in visible {
            let start = interval.start.max(range.start);
            let end = interval.end.min(visible_end).min(range.end);
            if start >= end {
                continue;
            }
            if objects
                .iter()
                .any(|object| object.start >= previous_end && object.end <= start)
            {
                after_object = true;
            }
            previous_end = end;
            if let Some(decoded) = decoded {
                let mut builder = SegmentBuilder::default();
                builder.push_str(&decoded, start..end);
                out.push((builder, after_object));
                after_object = false;
                continue;
            }
            let mut at = start;
            for object in &objects {
                if object.end <= at {
                    continue;
                }
                if object.start >= end {
                    break;
                }
                if object.start > at {
                    let mut builder = SegmentBuilder::default();
                    builder.push_literal_range(source, at..object.start.min(end));
                    if !builder.is_empty() {
                        out.push((builder, after_object));
                    }
                }
                at = object.end.min(end);
                after_object = true;
            }
            if at < end {
                let mut builder = SegmentBuilder::default();
                builder.push_literal_range(source, at..end);
                if !builder.is_empty() {
                    out.push((builder, after_object));
                    after_object = false;
                }
            }
        }
        if after_object
            || objects
                .iter()
                .any(|object| object.start >= previous_end && object.start < range.end)
        {
            // Preserve an object boundary even when the object is the final
            // thing on a line; the following line must not join to the text
            // before that image/formula.
            out.push((SegmentBuilder::default(), true));
        }
        out
    }

    fn push_builder(&mut self, builder: SegmentBuilder) {
        if let Some(segment) = builder.finish() {
            self.segments.push(segment);
        }
    }
}

fn split_lines(source: &str) -> Vec<Line> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let raw_end = source[start..]
            .find('\n')
            .map_or(source.len(), |offset| start + offset);
        let end = if raw_end > start && bytes[raw_end - 1] == b'\r' {
            raw_end - 1
        } else {
            raw_end
        };
        let next = if raw_end < source.len() {
            raw_end + 1
        } else {
            raw_end
        };
        out.push(Line { start, end, next });
        if raw_end == source.len() {
            break;
        }
        start = raw_end + 1;
    }
    out
}

fn block_kind(line: &str) -> BlockKind {
    if markdown_syntax::list_prefix(line).is_some() || markdown_syntax::task_prefix(line).is_some()
    {
        BlockKind::List
    } else if markdown_syntax::heading_level(line).is_some() {
        BlockKind::Heading
    } else if markdown_syntax::blockquote_prefix(line).is_some() {
        BlockKind::Quote
    } else {
        BlockKind::Paragraph
    }
}

fn contiguous_block_end(
    source: &str,
    lines: &[Line],
    table_rows: &[bool],
    math_rows: &[bool],
    structural_rows: &[bool],
    start: usize,
    kind: BlockKind,
) -> usize {
    if kind == BlockKind::Heading {
        return start + 1;
    }
    let mut end = start + 1;
    while end < lines.len() {
        let text = lines[end].text(source);
        if text.trim().is_empty()
            || markdown_syntax::thematic_break(text)
            || table_rows[end]
            || math_rows[end]
            || structural_rows[end]
            || text.trim_start().starts_with("```")
            || markdown_syntax::image_line(text).is_some()
            || markdown_syntax::image_row(text).is_some()
        {
            break;
        }
        let next_kind = block_kind(text);
        let joins = match kind {
            BlockKind::Paragraph => next_kind == BlockKind::Paragraph,
            BlockKind::Quote => {
                next_kind == BlockKind::Quote
                    && text
                        .chars()
                        .take_while(|ch| *ch == '>' || *ch == ' ')
                        .filter(|ch| *ch == '>')
                        .count()
                        == lines[start]
                            .text(source)
                            .chars()
                            .take_while(|ch| *ch == '>' || *ch == ' ')
                            .filter(|ch| *ch == '>')
                            .count()
            }
            BlockKind::List => {
                next_kind != BlockKind::List
                    && text
                        .as_bytes()
                        .first()
                        .is_some_and(|byte| *byte == b' ' || *byte == b'\t' || *byte == b'>')
            }
            BlockKind::Heading => false,
        };
        if !joins {
            break;
        }
        end += 1;
    }
    end
}

fn append_builder(target: &mut SegmentBuilder, source: SegmentBuilder) {
    let base = target.text.len();
    target.text.push_str(&source.text);
    target
        .units
        .extend(source.units.into_iter().map(|mut unit| {
            unit.visible.start += base;
            unit.visible.end += base;
            unit
        }));
}

fn visible_content_end(line: &str, global_start: usize) -> usize {
    let trimmed = line.trim_end_matches([' ', '\t']);
    let mut end = global_start + trimmed.len();
    if trimmed.ends_with('\\') && !is_escaped(trimmed.as_bytes(), trimmed.len() - 1) {
        end -= 1;
    }
    end
}

fn has_hard_break(line: &str) -> bool {
    let trimmed = line.trim_end_matches([' ', '\t']);
    line.len() - trimmed.len() >= 2
        || (trimmed.ends_with('\\') && !is_escaped(trimmed.as_bytes(), trimmed.len() - 1))
}

fn is_escaped(bytes: &[u8], at: usize) -> bool {
    let mut slashes = 0;
    let mut cursor = at;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

fn fold_segment(segment: &SearchSegment) -> FoldedText {
    let mut text = String::new();
    let mut units = Vec::with_capacity(segment.units.len());
    for unit in &segment.units {
        let visible = &segment.text[unit.visible.clone()];
        let start = text.len();
        text.extend(visible.chars().default_case_fold());
        units.push(MappedUnit {
            visible: start..text.len(),
            source: unit.source.clone(),
        });
    }
    FoldedText { text, units }
}

fn map_units(units: &[MappedUnit], range: Range<usize>) -> Option<Vec<Range<usize>>> {
    if range.start >= range.end {
        return None;
    }
    let mut out: Vec<Range<usize>> = Vec::new();
    let first = units.partition_point(|unit| unit.visible.end <= range.start);
    for unit in &units[first..] {
        if unit.visible.start >= range.end {
            break;
        }
        if let Some(previous) = out.last_mut() {
            if previous.end == unit.source.start {
                previous.end = unit.source.end;
                continue;
            }
            if previous.start == unit.source.start && previous.end == unit.source.end {
                continue;
            }
        }
        out.push(unit.source.clone());
    }
    (!out.is_empty()).then_some(out)
}

fn push_match(matches: &mut Vec<SearchMatch>, source: Vec<Range<usize>>) {
    if matches
        .last()
        .is_none_or(|previous| previous.source != source)
    {
        matches.push(SearchMatch { source });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(index: &SearchIndex, query: &str, case: bool) -> Vec<Vec<Range<usize>>> {
        index
            .find(query, case)
            .into_iter()
            .map(|m| m.source)
            .collect()
    }

    #[test]
    fn hides_formatting_but_matches_across_it() {
        let source = "hello **world**";
        assert_eq!(
            texts(&SearchIndex::from_markdown(source), "hello world", true),
            vec![vec![0..6, 8..13]]
        );
    }

    #[test]
    fn link_destination_and_image_metadata_are_not_searchable() {
        let source = "[visible](hidden-url) ![alt](image.png)";
        let index = SearchIndex::from_markdown(source);
        assert_eq!(index.find("visible", true).len(), 1);
        assert!(index.find("hidden-url", true).is_empty());
        assert!(index.find("alt", true).is_empty());
        assert!(index.find("image.png", true).is_empty());
    }

    #[test]
    fn table_matches_cross_inline_formatting() {
        let index = SearchIndex::from_markdown("| hello **world** | other |\n| --- | --- |");
        assert_eq!(index.find("hello world", true).len(), 1);
    }

    #[test]
    fn code_entities_are_literal_and_objects_separate_words() {
        let index = SearchIndex::from_markdown("`&amp;` and before![alt](url)after");
        assert_eq!(index.find("&amp;", true).len(), 1);
        assert!(index.find("beforeafter", true).is_empty());
    }

    #[test]
    fn heading_formatting_and_escaped_entities() {
        let index = SearchIndex::from_markdown("# hello **world**\n\n\\&amp;");
        assert_eq!(index.find("hello world", true).len(), 1);
        assert_eq!(index.find("&amp;", true).len(), 1);
    }

    #[test]
    fn code_objects_remain_literal_and_rules_are_boundaries() {
        let index = SearchIndex::from_markdown("`![alt](url) $math$`\n\nfirst\n---\nsecond");
        assert_eq!(index.find("![alt](url) $math$", true).len(), 1);
        assert!(index.find("---", true).is_empty());
        assert!(index.find("first second", true).is_empty());
        let index = SearchIndex::from_markdown("before![alt](url)\nafter");
        assert!(index.find("before after", true).is_empty());
        assert_eq!(
            SearchIndex::from_markdown(r"\*").find("*", true)[0].source,
            vec![0..2]
        );
    }

    #[test]
    fn tables_are_cell_local() {
        let source = "| left | right |\n| --- | --- |\n| alpha | beta |";
        let index = SearchIndex::from_markdown(source);
        assert!(index.find("left right", true).is_empty());
        assert_eq!(index.find("alpha", true).len(), 1);
        assert_eq!(index.find("beta", true).len(), 1);
        assert!(index.find("alpha beta", true).is_empty());
    }

    #[test]
    fn soft_breaks_join_and_hard_breaks_do_not() {
        let soft = SearchIndex::from_markdown("one\ntwo");
        assert_eq!(soft.find("one two", true).len(), 1);
        let hard = SearchIndex::from_markdown("one  \ntwo");
        assert!(hard.find("one two", true).is_empty());
    }

    #[test]
    fn code_preserves_whitespace_and_excludes_fences() {
        let source = "```rust\nlet  x = 1;\n```";
        let index = SearchIndex::from_markdown(source);
        assert_eq!(index.find("let  x", true).len(), 1);
        assert!(index.find("```", true).is_empty());
        assert_eq!(index.find("rust", true).len(), 1);
    }

    #[test]
    fn decodes_entities_and_full_case_folds_without_normalizing() {
        let index = SearchIndex::from_markdown("&amp; Straße Σ");
        assert_eq!(index.find("& strasse σ", false).len(), 1);
        assert!(
            SearchIndex::from_markdown("café")
                .find("café", false)
                .is_empty()
        );
    }

    #[test]
    fn unicode_source_ranges_are_utf8_safe_and_non_overlapping() {
        let source = "Привет 😀 привет";
        let matches = SearchIndex::from_markdown(source).find("ПРИВЕТ", false);
        assert_eq!(matches.len(), 2);
        assert_eq!(&source[matches[0].source[0].clone()], "Привет");
        assert_eq!(&source[matches[1].source[0].clone()], "привет");
    }

    #[test]
    fn malformed_markup_remains_searchable_source_text() {
        let index = SearchIndex::from_markdown("[unfinished link");
        assert_eq!(index.find("unfinished", true).len(), 1);
    }

    #[test]
    fn block_markers_are_not_visible_and_blocks_do_not_join() {
        let source = "# Title\n\n- first\n- second\n\n> quoted\nplain";
        let index = SearchIndex::from_markdown(source);
        assert_eq!(index.find("Title", true).len(), 1);
        assert!(index.find("# Title", true).is_empty());
        assert_eq!(index.find("first", true).len(), 1);
        assert_eq!(index.find("second", true).len(), 1);
        assert_eq!(index.find("quoted", true).len(), 1);
        assert!(index.find("quoted plain", true).is_empty());
    }

    #[test]
    fn full_folding_does_not_create_duplicate_matches_for_expansions() {
        let index = SearchIndex::from_markdown("ß");
        assert_eq!(index.find("s", false).len(), 1);
        assert_eq!(index.find("ss", false).len(), 1);
        assert!(index.find("S", true).is_empty());
    }

    #[test]
    fn code_newlines_are_not_soft_spaces() {
        let index = SearchIndex::from_markdown("```\none\ntwo\n```");
        assert_eq!(index.find("one\ntwo", true).len(), 1);
        assert!(index.find("one two", true).is_empty());
    }

    #[test]
    fn escapes_entities_sigma_and_emoji_keep_their_source_mapping() {
        let source = r"\* literal &lt; ΟΣ ος 😀";
        let index = SearchIndex::from_markdown(source);
        assert_eq!(index.find("* literal <", true).len(), 1);
        assert_eq!(index.find("οσ", false).len(), 2);
        let emoji = index.find("😀", true);
        assert_eq!(emoji.len(), 1);
        assert_eq!(&source[emoji[0].source[0].clone()], "😀");
    }

    #[test]
    fn literal_matching_is_non_overlapping_and_case_toggle_is_exact() {
        let index = SearchIndex::from_markdown("aaaa World");
        assert_eq!(index.find("aa", true).len(), 2);
        assert!(index.find("world", true).is_empty());
        assert_eq!(index.find("world", false).len(), 1);
    }

    /// Run serially with `cargo test -p mdoc-editor search_performance_matrix
    /// -- --ignored --nocapture --test-threads=1`. Budgets catch large regressions
    /// even in debug builds; reported medians/p95 are the comparison evidence.
    #[test]
    #[ignore = "performance matrix; run separately on an idle machine"]
    fn search_performance_matrix() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };

        fn measure(name: &str, bytes: usize, budget_ms: u64, mut run: impl FnMut()) {
            let mut samples = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                run();
                samples.push(start.elapsed());
            }
            samples.sort();
            eprintln!(
                "{bytes:>8} bytes {name:<24} median={:?} p95={:?}",
                samples[3], samples[6]
            );
            assert!(
                samples[6] < Duration::from_millis(budget_ms),
                "{name}: {:?} exceeds {budget_ms} ms",
                samples[6]
            );
        }

        let block = "# Heading\n\nA **needle** and Straße Пример 😀 &amp;.\n\n| Label | Value |\n| --- | --- |\n| needle | `code` |\n\n```rust\nneedle value\n```\n\n";
        for repeats in [16, 512, 16_384] {
            let source = block.repeat(repeats);
            let bytes = source.len();
            let build_budget = if repeats < 100 {
                100
            } else if repeats < 1_000 {
                500
            } else {
                5_000
            };
            let query_budget = if repeats < 1_000 { 50 } else { 250 };
            measure("cold build + fold", bytes, build_budget, || {
                let index = SearchIndex::from_markdown(black_box(&source));
                assert_eq!(index.find("NEEDLE", false).len(), repeats * 3);
            });
            let index = SearchIndex::from_markdown(&source);
            assert_eq!(index.find("NEEDLE", false).len(), repeats * 3);
            for (name, query, case, expected) in [
                ("empty", "", false, 0),
                ("absent", "not-present", false, 0),
                ("case sensitive", "needle", true, repeats * 3),
                ("cached folded", "NEEDLE", false, repeats * 3),
                ("Unicode expansion", "STRASSE", false, repeats),
                ("Cyrillic", "ПРИМЕР", false, repeats),
                ("emoji", "😀", true, repeats),
                ("across formatting", "A needle", true, repeats),
            ] {
                measure(name, bytes, query_budget, || {
                    assert_eq!(index.find(black_box(query), case).len(), expected);
                });
            }
            measure("incremental typing", bytes, query_budget * 7, || {
                for query in ["n", "ne", "nee", "need", "needle", "need", "nee"] {
                    black_box(index.find(black_box(query), false));
                }
            });
            let edited = format!("inserted\n\n{source}");
            measure("edit rebuild + query", bytes, build_budget, || {
                assert_eq!(
                    SearchIndex::from_markdown(black_box(&edited))
                        .find("needle", false)
                        .len(),
                    repeats * 3
                );
            });
        }
        // A single huge segment exercises binary source mapping and dense results.
        let source = format!("```\n{}\n```", "a ".repeat(100_000));
        let index = SearchIndex::from_markdown(&source);
        measure("dense single segment", source.len(), 250, || {
            assert_eq!(index.find(black_box("a"), true).len(), 100_000);
        });
    }
}
