//! Source ranges a spell checker should skip, from the editor's own Markdown
//! recognition (ADR 0036): code, math, link/image targets and other hidden
//! syntax, and raw HTML tags. Using the same scanner as rendering keeps the
//! checker from disagreeing with what the user sees as prose.

use std::ops::Range;

use crate::markdown_syntax;

/// Byte ranges of `source` that are not prose: fenced code blocks (with their
/// fences), math, table style markers, image lines, inline code, inline images
/// and math, hidden Markdown syntax (link targets, markers, entities) and raw
/// HTML tags. Sorted by start; may touch but carry no prose.
pub fn spell_exclusions(source: &str) -> Vec<Range<usize>> {
    let lines = line_ranges(source);
    let mut whole = vec![false; lines.len()];
    for region in markdown_syntax::math_regions(source) {
        for row in region.range.chain(region.marker_line) {
            if let Some(slot) = whole.get_mut(row) {
                *slot = true;
            }
        }
    }
    for region in markdown_syntax::table_regions(source) {
        if let Some(slot) = region.marker_line.and_then(|row| whole.get_mut(row)) {
            *slot = true;
        }
    }

    let style = markdown_syntax::search_style();
    let mut out = Vec::new();
    let mut row = 0;
    while row < lines.len() {
        let line = lines[row].clone();
        let text = &source[line.clone()];
        if text.trim_start().starts_with("```") {
            // The fence, its body and the closing fence (or the rest of the file).
            let mut end = row + 1;
            while end < lines.len() && !source[lines[end].clone()].trim_start().starts_with("```") {
                end += 1;
            }
            let last = lines[end.min(lines.len() - 1)].end;
            out.push(line.start..last);
            row = end + 1;
            continue;
        }
        if whole[row]
            || markdown_syntax::image_line(text).is_some()
            || markdown_syntax::image_row(text).is_some()
        {
            out.push(line);
            row += 1;
            continue;
        }
        for span in markdown_syntax::semantic_spans_with(source, line.start, line.end, &style) {
            if span.code || span.hidden || span.replacement {
                out.push(span.range);
            }
        }
        let local = markdown_syntax::inline_image_spans(text)
            .into_iter()
            .map(|(full, _)| full)
            .chain(markdown_syntax::inline_math_spans(text))
            .chain(html_tags(text));
        out.extend(local.map(|r| line.start + r.start..line.start + r.end));
        row += 1;
    }
    out.sort_by_key(|r| (r.start, r.end));
    out
}

/// Line ranges without their `\n` (or `\r\n`).
fn line_ranges(source: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        let body = line.trim_end_matches('\n').trim_end_matches('\r');
        out.push(start..start + body.len());
        start += line.len();
    }
    if source.is_empty() || source.ends_with('\n') {
        out.push(start..start);
    }
    out
}

/// `<tag …>`, `</tag>` and `<!-- … -->` on one line.
fn html_tags(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/' || *b == b'!')
        {
            let close = if line[i..].starts_with("<!--") {
                line[i..].find("-->").map(|e| i + e + 3)
            } else {
                line[i..].find('>').map(|e| i + e + 1)
            };
            if let Some(end) = close {
                out.push(i..end);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text left after removing exclusions, for readable assertions.
    fn prose(source: &str) -> String {
        let mut keep = vec![true; source.len()];
        for range in spell_exclusions(source) {
            keep[range].fill(false);
        }
        source
            .char_indices()
            .map(|(i, c)| if keep[i] { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn code_blocks_and_inline_code_are_excluded() {
        assert_eq!(
            prose("Intro `fooo` text\n```rust\nlet zzz = 1;\n```\nAfter"),
            "Intro text After"
        );
    }

    #[test]
    fn link_targets_are_excluded_but_labels_stay() {
        assert_eq!(
            prose("See [the clause](https://exampel.com/pathh) now"),
            "See the clause now"
        );
    }

    #[test]
    fn images_math_and_html_are_excluded() {
        assert_eq!(
            prose("Text ![alt](imgg.png) and $xyzz$ <span class=\"qq\">word</span> <!-- notee -->"),
            "Text and word"
        );
        assert_eq!(prose("![](scan.png)\nNext"), "Next");
        assert_eq!(prose("$$\nabcd\n$$\nNext"), "Next");
    }

    #[test]
    fn unclosed_fence_excludes_the_rest() {
        assert_eq!(prose("Before\n```\nfooo\nbarr"), "Before");
    }

    #[test]
    fn utf8_ranges_are_char_boundaries() {
        let source = "Договор `код` и [ссылка](адрес) — конец";
        for range in spell_exclusions(source) {
            assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        }
        assert_eq!(prose(source), "Договор и ссылка — конец");
    }
}
