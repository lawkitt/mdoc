//! Source grammar protection shared by discovery and manual selection.
use std::ops::Range;
/// Protect letter/number-bearing grammar: list prefixes, HTML tag and attribute
/// names, and attribute quotes. Identifying-information detection stays in GLiNER2.
/// Only real Markdown and HTML syntax is protected, as CommonMark reads it:
/// an autolink (`<name@domain.invalid>`) or other text in angle brackets
/// that is not a whole tag is text, and so is a number that cannot start a
/// list where it stands.
pub(super) fn protected_syntax(source: &str) -> Vec<Range<usize>> {
    let mut protected = list_markers(source);
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
/// "Паспорт:\n46. 18 731904" the number is text.
pub(super) fn list_markers(source: &str) -> Vec<Range<usize>> {
    let mut markers = Vec::new();
    let mut base = 0;
    // Whether the line before is paragraph text, outside any list item since
    // the last blank line, which an ordered marker other than 1 continues.
    let mut paragraph = false;
    let mut listed = false;
    for line in source.split_inclusive('\n') {
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let mut start = indent;
        while line[start..].starts_with('>') {
            start += 1;
            start += line[start..].len() - line[start..].trim_start_matches([' ', '\t']).len();
        }
        let body = &line[start..];
        let mut item = false;
        if body.starts_with("- ") || body.starts_with("+ ") || body.starts_with("* ") {
            markers.push(base + start..base + start + 2);
            item = true;
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
            item = true;
        }
        let blank = body.trim().is_empty();
        listed = !blank && (listed || item);
        paragraph = !blank && !listed && indent < 2 && !body.starts_with(['#', '|', '`', '~', '<']);
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
/// Characters that only matter as Markdown structure: escapes, emphasis
/// delimiters and heading marks. A span containing them is checked
/// structurally.
const DELIMITERS: &[char] = &['\\', '*', '_', '~', '#'];
/// Whether replacing `range` with a plain token leaves the Markdown structure
/// of its block unchanged: escaped (`\_`) or literal (`____`) delimiters are
/// text, while a span that opens or closes emphasis, or splits an escape, is not.
pub(super) fn plain_text_span(source: &str, range: std::ops::Range<usize>) -> bool {
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
    let block = &source[start..end];
    let local = range.start - start..range.end - start;
    let mut replaced = String::with_capacity(block.len());
    replaced.push_str(&block[..local.start]);
    replaced.push('X');
    replaced.push_str(&block[local.end..]);
    let options = markdown::ParseOptions::gfm();
    match (
        markdown::to_mdast(block, &options),
        markdown::to_mdast(&replaced, &options),
    ) {
        (Ok(before), Ok(after)) => shape(&before) == shape(&after),
        _ => false,
    }
}
/// Node kinds and child counts in document order, ignoring text values.
fn shape(
    node: &markdown::mdast::Node,
) -> Vec<(std::mem::Discriminant<markdown::mdast::Node>, usize)> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        let children = node.children().map_or(&[][..], Vec::as_slice);
        out.push((std::mem::discriminant(node), children.len()));
        stack.extend(children.iter().rev());
    }
    out
}
