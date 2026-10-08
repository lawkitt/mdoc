//! Source grammar protection shared by discovery and manual selection.
use std::ops::Range;
/// Protect letter/number-bearing grammar: list prefixes, HTML tag and attribute
/// names, and attribute quotes. Identifying-information detection stays in GLiNER2.
pub(super) fn protected_syntax(source: &str) -> Vec<Range<usize>> {
    let mut protected = Vec::new();
    let mut base = 0;
    for line in source.split_inclusive('\n') {
        let mut start = line.len() - line.trim_start_matches([' ', '\t']).len();
        while line[start..].starts_with('>') {
            start += 1;
            start += line[start..].len() - line[start..].trim_start_matches([' ', '\t']).len();
        }
        let body = &line[start..];
        if body.starts_with("- ") || body.starts_with("+ ") || body.starts_with("* ") {
            protected.push(base + start..base + start + 2);
        }
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=9).contains(&digits)
            && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
            && body
                .as_bytes()
                .get(digits + 1)
                .is_some_and(u8::is_ascii_whitespace)
        {
            protected.push(base + start..base + start + digits + 1);
        }
        base += line.len();
    }
    let bytes = source.as_bytes();
    let name_byte = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':');
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        let mut i = at + 1;
        if bytes.get(i) == Some(&b'/') {
            i += 1;
        }
        if !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
            continue;
        }
        let tag = i;
        while bytes.get(i).is_some_and(|byte| name_byte(*byte)) {
            i += 1;
        }
        protected.push(tag..i);
        loop {
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                break;
            }
            if bytes[i] == b'/' {
                i += 1;
                continue;
            }
            let name = i;
            while bytes.get(i).is_some_and(|byte| name_byte(*byte)) {
                i += 1;
            }
            if i == name {
                break;
            }
            protected.push(name..i);
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                continue;
            }
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if let Some(quote @ (b'\'' | b'"')) = bytes.get(i).copied() {
                protected.push(i..i + 1);
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    protected.push(i..i + 1);
                    i += 1;
                }
            } else {
                while bytes
                    .get(i)
                    .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
                {
                    i += 1;
                }
            }
        }
        at = i.saturating_add(1);
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
