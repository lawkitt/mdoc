//! Source grammar protection shared by discovery and manual selection.
use crate::view::InlineMarkup;
use std::ops::Range;
/// Protect letter/number-bearing grammar: list prefixes, ATX heading marks,
/// HTML tag and attribute names, and attribute quotes. Identifying-information detection stays in GLiNER2.
/// Only real Markdown and HTML syntax is protected, as CommonMark reads it:
/// an autolink (`<name@domain.invalid>`) or other text in angle brackets
/// that is not a whole tag is text, and so is a number that cannot start a
/// list where it stands.
pub(super) fn protected_syntax(source: &str) -> Vec<Range<usize>> {
    let mut protected = list_markers(source);
    protected.extend(heading_markers(source));
    let bytes = source.as_bytes();
    let mut at = 0;
    while let Some(offset) = source[at..].find('<') {
        let open = at + offset;
        let mut parts = Vec::new();
        match html_tag(bytes, open, &mut parts) {
            Some(end) => {
                protected.extend(parts);
                at = end;
            }
            None => at = open + 1,
        }
    }
    // Link outer delimiters remain intact, while ordinary parenthesized
    // phone/address text and balanced parentheses inside destinations are valid.
    for (open, _) in source.match_indices("](") {
        let mut depth = 1;
        let mut i = open + 2;
        let mut quote = None;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if let Some(current) = quote {
                if bytes[i] == current {
                    protected.push(i..i + 1);
                    quote = None;
                }
            } else if matches!(bytes[i], b'\'' | b'"')
                && bytes.get(i - 1).is_some_and(u8::is_ascii_whitespace)
            {
                quote = Some(bytes[i]);
                protected.push(i..i + 1);
            } else if bytes[i] == b'(' {
                depth += 1;
            } else if bytes[i] == b')' {
                depth -= 1;
                if depth == 0 {
                    protected.push(i..i + 1);
                    break;
                }
            }
            i += 1;
        }
    }
    protected.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in protected {
        if let Some(last) = merged.last_mut().filter(|last| last.end >= range.start) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}
/// List item markers ("- ", "+ ", "* ", "1. ", "2) ") at the start of a
/// line, after any block quote prefixes. A bullet can always interrupt a
/// paragraph, but an ordered marker other than 1 cannot (CommonMark), so in
/// "Паспорт:\n46. 18 731904" the number is text. A "+" before a digit that
/// would interrupt a paragraph is the value's own sign, not a list of one:
/// in "Телефон:\n+ 7 (900) 000-84-31" the phone keeps its plus (#428).
pub(super) fn list_markers(source: &str) -> Vec<Range<usize>> {
    let mut markers = Vec::new();
    let mut base = 0;
    // Whether the line before is paragraph text, outside any list item,
    // which an ordered marker other than 1 continues.
    let mut paragraph = false;
    // Whether the line before is in a list item: the item's line, its lazy
    // lines and its lines indented to its content after a blank one.
    let mut listed = false;
    // The content column of the outermost open list item, if any.
    let mut list: Option<usize> = None;
    let mut quotes = 0;
    for line in source.split_inclusive('\n') {
        let (indent, depth, start) = block_start(line);
        let body = &line[start..];
        let blank = body.trim_matches(MARKDOWN_SPACE).is_empty();
        // A new block quote starts a new container: nothing it holds
        // continues the paragraph before it.
        if depth > quotes {
            paragraph = false;
        }
        quotes = depth;
        let rule = rule_line(body, paragraph);
        let mut item = None;
        let bullet = matches!(body.as_bytes().first(), Some(b'-' | b'+' | b'*'))
            && matches!(body.as_bytes().get(1), Some(b' ' | b'\t'));
        let sign = paragraph
            && body.starts_with('+')
            && body[1..]
                .trim_start_matches(MARKDOWN_SPACE)
                .starts_with(|c: char| c.is_ascii_digit());
        if bullet && !sign && !rule {
            markers.push(base + start..base + start + 2);
            item = Some(1);
        }
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=9).contains(&digits)
            && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
            && body
                .as_bytes()
                .get(digits + 1)
                .is_some_and(u8::is_ascii_whitespace)
            && !(paragraph && body[..digits].trim_start_matches('0') != "1")
        {
            markers.push(base + start..base + start + digits + 1);
            item = Some(digits + 1);
        }
        let nested = list.is_some_and(|content| !blank && indent >= content);
        // A heading or a fence outside the item's content ends the list:
        // only paragraph text continues an item lazily.
        let ends = rule || (!nested && (atx_heading(body).is_some() || fence(body).is_some()));
        list = if let Some(width) = item {
            // The item's content starts after its marker and 1–4 columns.
            let marker = indent + width;
            let mut column = marker;
            for byte in body[width..].bytes() {
                match byte {
                    b' ' => column += 1,
                    b'\t' => column += 4 - column % 4,
                    _ => break,
                }
            }
            let content = if (1..=4).contains(&(column - marker)) {
                column
            } else {
                marker + 1
            };
            Some(list.map_or(content, |open| open.min(content)))
        } else if ends {
            None
        } else if nested || blank || listed {
            list
        } else {
            None
        };
        listed = !blank && !ends && (item.is_some() || nested || listed);
        // Up to three spaces of indent start a paragraph; more continue one.
        paragraph = !blank
            && !listed
            && !rule
            && (indent < 4 || paragraph)
            && atx_heading(body).is_none()
            && fence(body).is_none()
            && !body.starts_with(['|', '<']);
        base += line.len();
    }
    markers
}
/// The whitespace Markdown's block structure reads; a no-break space is text.
const MARKDOWN_SPACE: &[char] = &[' ', '\t', '\n', '\r'];
/// A line's indent in columns (a tab to the next multiple of 4), its block
/// quote depth and the byte where its block starts, after the quote prefixes.
fn block_start(line: &str) -> (usize, usize, usize) {
    let mut indent = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => indent += 1,
            b'\t' => indent += 4 - indent % 4,
            _ => break,
        }
    }
    let mut depth = 0;
    let mut start = line.len() - line.trim_start_matches([' ', '\t']).len();
    while line[start..].starts_with('>') {
        depth += 1;
        start += 1;
        start += line[start..].len() - line[start..].trim_start_matches([' ', '\t']).len();
    }
    (indent, depth, start)
}
/// Whether `body` is a thematic break ("***", "- - -") or, after a
/// paragraph line, a setext heading's underline ("===", "---"): neither is
/// a list item, and both end the paragraph.
fn rule_line(body: &str, paragraph: bool) -> bool {
    let line = body.trim_matches(MARKDOWN_SPACE);
    let Some(mark) = line.chars().next() else {
        return false;
    };
    let count = line.chars().filter(|c| *c == mark).count();
    let only = |spaced: bool| {
        line.chars()
            .all(|c| c == mark || (spaced && matches!(c, ' ' | '\t')))
    };
    (matches!(mark, '-' | '*' | '_') && count >= 3 && only(true))
        || (paragraph && matches!(mark, '=' | '-') && only(false))
}
/// The code fence that `body` opens or closes ("```", "~~~~"): its
/// character and length.
fn fence(body: &str) -> Option<(u8, usize)> {
    let mark = *body
        .as_bytes()
        .first()
        .filter(|b| matches!(b, b'`' | b'~'))?;
    let len = body.bytes().take_while(|b| *b == mark).count();
    (len >= 3).then_some((mark, len))
}
/// The opening and closing `#` runs of an ATX heading line's `body`, as
/// offsets into it: "## Title ##" has both, "#5 is the ID" is no heading.
fn atx_heading(body: &str) -> Option<(Range<usize>, Option<Range<usize>>)> {
    let level = body.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level)
        || !matches!(
            body.as_bytes().get(level),
            None | Some(b' ' | b'\t' | b'\n' | b'\r')
        )
    {
        return None;
    }
    let text = body.trim_end_matches(MARKDOWN_SPACE);
    let trailing = text.len() - text.trim_end_matches('#').len();
    let closing = (text.len() > level
        && trailing > 0
        && text[..text.len() - trailing].ends_with([' ', '\t']))
    .then(|| text.len() - trailing..text.len());
    Some((0..level, closing))
}
/// The `#` runs of ATX headings ("## Title ##", "- # Title #" in a list
/// item): a literal `#` inside a value ("LC#713624", "Item #5") is text.
fn heading_markers(source: &str) -> Vec<Range<usize>> {
    let mut markers = Vec::new();
    let mut base = 0;
    let mut code: Option<(u8, usize)> = None;
    for line in source.split_inclusive('\n') {
        let (_, _, mut start) = block_start(line);
        // A heading may open a list item: read past its marker.
        loop {
            let body = &line[start..];
            let digits = body.bytes().take_while(u8::is_ascii_digit).count();
            let width = if matches!(body.as_bytes().first(), Some(b'-' | b'+' | b'*')) {
                1
            } else if (1..=9).contains(&digits)
                && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
            {
                digits + 1
            } else {
                break;
            };
            if !matches!(body.as_bytes().get(width), Some(b' ' | b'\t')) {
                break;
            }
            start +=
                width + body[width..].len() - body[width..].trim_start_matches([' ', '\t']).len();
        }
        let body = &line[start..];
        match (code, fence(body)) {
            (None, Some(open)) => code = Some(open),
            (Some((mark, len)), Some((close, closing)))
                if close == mark
                    && closing >= len
                    && body[closing..].trim_matches(MARKDOWN_SPACE).is_empty() =>
            {
                code = None;
            }
            (Some(_), _) => {}
            // An indented heading may be in a list item's content; an
            // indented code line that reads like one is protected too.
            (None, None) => {
                if let Some((open, close)) = atx_heading(body) {
                    let at = base + start;
                    markers.push(at + open.start..at + open.end);
                    markers.extend(close.map(|c| at + c.start..at + c.end));
                }
            }
        }
        base += line.len();
    }
    markers
}
/// The end of the HTML tag that starts at `open`, if a whole one does, as
/// CommonMark reads raw HTML: an open tag (`<name attr="v" flag />`) or a
/// closing tag (`</name>`). Its name, attribute names and value quotes go
/// to `parts`; values are text.
fn html_tag(bytes: &[u8], open: usize, parts: &mut Vec<Range<usize>>) -> Option<usize> {
    let space = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
    let skip = |mut i: usize| {
        while bytes.get(i).is_some_and(space) {
            i += 1;
        }
        i
    };
    let mut i = open + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let name = i;
    if !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        i += 1;
    }
    parts.push(name..i);
    if closing {
        i = skip(i);
        return (bytes.get(i) == Some(&b'>')).then_some(i + 1);
    }
    loop {
        let after = skip(i);
        match bytes.get(after) {
            Some(b'>') => return Some(after + 1),
            Some(b'/') => return (bytes.get(after + 1) == Some(&b'>')).then_some(after + 2),
            _ if after == i => return None,
            _ => i = after,
        }
        // An attribute: its name, then an optional value.
        let name = i;
        if !bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'_' | b':'))
        {
            return None;
        }
        while bytes
            .get(i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
        {
            i += 1;
        }
        parts.push(name..i);
        let equals = skip(i);
        if bytes.get(equals) != Some(&b'=') {
            continue;
        }
        i = skip(equals + 1);
        match *bytes.get(i)? {
            quote @ (b'\'' | b'"') => {
                let close = i + 1 + bytes[i + 1..].iter().position(|b| *b == quote)?;
                parts.push(i..i + 1);
                parts.push(close..close + 1);
                i = close + 1;
            }
            _ => {
                let value = i;
                while bytes
                    .get(i)
                    .is_some_and(|b| !space(b) && !b"\"'=<>`".contains(b))
                {
                    i += 1;
                }
                if i == value {
                    return None;
                }
            }
        }
    }
}
/// The HTML line breaks (`<br>`) inside `range` of `source`, with value
/// text on both sides: a value over the lines of a table cell ("27 Wisp
/// Lane<br>Norwich NR2 8ZZ").
pub(super) fn line_breaks_in(source: &str, range: &Range<usize>) -> Vec<Range<usize>> {
    let mut breaks = Vec::new();
    let mut at = range.start;
    while let Some(offset) = source[at..range.end].find('<') {
        let open = at + offset;
        match crate::view::line_break_tag(&source[open..range.end]) {
            Some(len) => {
                if open > range.start && open + len < range.end {
                    breaks.push(open..open + len);
                }
                at = open + len;
            }
            None => at = open + 1,
        }
    }
    breaks
}
/// Rewrites allowed only wholly inside a detected value. A tag pair is
/// indivisible: a partial superscript/subscript stays protected syntax.
fn markup_in(
    source: &str,
    markup: &[InlineMarkup],
    range: &Range<usize>,
) -> Vec<(Range<usize>, String)> {
    if !source[range.clone()].contains(['*', '_', '<']) {
        return Vec::new();
    }
    let first = markup.partition_point(|m| m.range.start < range.start);
    let mut edits: Vec<_> = markup[first..]
        .iter()
        .take_while(|m| m.range.start < range.end)
        .filter(|m| m.range.end <= range.end)
        // A script pair may be the value's edge. An emphasis join needs
        // value text on both sides, so it cannot consume a neighbour's run.
        .filter(|m| m.inner.is_some() || (range.start < m.range.start && m.range.end < range.end))
        .flat_map(|m| m.edits.iter().cloned())
        .collect();
    edits.sort_by_key(|(r, _)| r.start);
    edits
}

/// Syntax that detection reads as text inside this value: line breaks,
/// emphasis joins and paired superscript/subscript tags.
pub(super) fn value_syntax_in(
    source: &str,
    markup: &[InlineMarkup],
    range: &Range<usize>,
) -> Vec<Range<usize>> {
    let mut parts = line_breaks_in(source, range);
    parts.extend(markup_in(source, markup, range).into_iter().map(|(r, _)| r));
    parts.sort_by_key(|r| r.start);
    parts
}

/// Whether `range` crosses protected syntax, allowing the formatting
/// detection reads as text wholly inside the value.
pub(super) fn crosses_syntax(
    protected: &[Range<usize>],
    markup: &[InlineMarkup],
    source: &str,
    range: &Range<usize>,
) -> bool {
    let first = protected.partition_point(|r| r.end <= range.start);
    let breaks = value_syntax_in(source, markup, range);
    protected[first..]
        .iter()
        .take_while(|r| r.start < range.end)
        .any(|r| !breaks.iter().any(|b| b.start <= r.start && r.end <= b.end))
}
/// Characters that only matter as Markdown structure: escapes, emphasis
/// delimiters, heading marks, line breaks and HTML. A span containing them
/// is checked structurally: a soft line break inside a paragraph is text.
const DELIMITERS: &[char] = &['\\', '*', '_', '~', '#', '\n', '\r', '<'];
/// Whether replacing `range` with a plain token leaves the Markdown structure
/// of its block unchanged: escaped (`\_`) or literal (`____`) delimiters are
/// text, while a span that opens or closes emphasis, or splits an escape, is not.
pub(super) fn plain_text_span(
    source: &str,
    markup: &[InlineMarkup],
    range: std::ops::Range<usize>,
) -> bool {
    let value = &source[range.clone()];
    if !value.contains(DELIMITERS) {
        return true;
    }
    // A backslash before the span would escape the token's first character.
    if source[..range.start].ends_with('\\') {
        return false;
    }
    // Parse only the surrounding block: blank-line bounded.
    let start = source[..range.start].rfind("\n\n").map_or(0, |i| i + 2);
    let end = source[range.end..]
        .find("\n\n")
        .map_or(source.len(), |i| range.end + i);
    let mut block = source[start..end].to_owned();
    let mut local = range.start - start..range.end - start;
    // Compare the grammar with the same formatting joins detection reads.
    // Outer emphasis and every other structural marker still participate.
    for (part, replacement) in markup_in(source, markup, &range).into_iter().rev() {
        local.end -= part.len() - replacement.len();
        block.replace_range(part.start - start..part.end - start, &replacement);
    }
    let block = block.as_str();
    let mut replaced = String::with_capacity(block.len());
    replaced.push_str(&block[..local.start]);
    // In a URI autolink's brackets, "<X_>" is text: "<X>" would be an HTML
    // tag, and at a block's start an HTML block ("- <https://…>").
    let autolink = uri_autolink(block, &local);
    replaced.push_str(if autolink { "X_" } else { "X" });
    replaced.push_str(&block[local.end..]);
    let options = markdown::ParseOptions::gfm();
    match (
        markdown::to_mdast(block, &options),
        markdown::to_mdast(&replaced, &options),
    ) {
        // Only a line break the replaced value held is part of it: one
        // the replacement exposes ("Ann\<br>" ends in an escape) is syntax.
        (Ok(before), Ok(after)) => shape(&before, &local) == shape(&after, &(0..0)),
        _ => false,
    }
}
/// Node kinds and child counts in document order, ignoring text values.
/// An HTML line break inside a value is text too ("27 Wisp Lane<br>Norwich",
/// ADR 0092): `<br>` nodes wholly inside `value` are skipped and the text
/// around them is one. So is a GFM autolink literal wholly inside it
/// ("https://site.invalid/u/alice_r", ADR 0094): the web address is the
/// value, and its alias is plain text.
static AS_TEXT: markdown::mdast::Node = markdown::mdast::Node::Text(markdown::mdast::Text {
    value: String::new(),
    position: None,
});
/// Whether `value` of `block` is a URI autolink's whole address
/// ("<https://…>").
fn uri_autolink(block: &str, value: &std::ops::Range<usize>) -> bool {
    block[..value.start].ends_with('<')
        && block[value.end..].starts_with('>')
        && crate::web::explicit(&block[value.clone()])
}
/// Whether `node` reads as plain text for `value`: a link written as its
/// own text ("https://…", "www.…") wholly inside it, or a URI autolink
/// around exactly it ("<https://…>").
fn autolink_literal(node: &markdown::mdast::Node, value: &std::ops::Range<usize>) -> bool {
    use markdown::mdast::Node;
    let (children, at) = match node {
        Node::Link(link) => (link.children.as_slice(), link.position.as_ref()),
        _ => return false,
    };
    let Some(at) = at else {
        return false;
    };
    let len = at.end.offset - at.start.offset;
    match children {
        [Node::Text(text)] if value.start <= at.start.offset && at.end.offset <= value.end => {
            text.value.len() == len
        }
        [Node::Text(text)] => {
            at.start.offset + 1 == value.start
                && at.end.offset == value.end + 1
                && text.value.len() + 2 == len
        }
        _ => false,
    }
}
fn shape(
    node: &markdown::mdast::Node,
    value: &std::ops::Range<usize>,
) -> Vec<(std::mem::Discriminant<markdown::mdast::Node>, usize)> {
    use markdown::mdast::Node;
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        let mut children: Vec<&Node> = Vec::new();
        for child in node.children().map_or(&[][..], Vec::as_slice) {
            let child = if autolink_literal(child, value) {
                &AS_TEXT
            } else {
                child
            };
            let line_break = matches!(child, Node::Html(html)
            if crate::view::line_break_tag(&html.value) == Some(html.value.len())
                && html.position.as_ref().is_some_and(|at| {
                    value.start <= at.start.offset && at.end.offset <= value.end
                }));
            let text = matches!(child, Node::Text(_));
            if line_break || (text && matches!(children.last(), Some(Node::Text(_)))) {
                continue;
            }
            children.push(child);
        }
        out.push((std::mem::discriminant(node), children.len()));
        stack.extend(children.into_iter().rev());
    }
    out
}
