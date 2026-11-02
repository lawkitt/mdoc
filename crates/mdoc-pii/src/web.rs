//! Where a web address starts and ends (ADR 0094). The detector
//! (`mdoc_detect`) and discovery share these rules, so a repeat of a known
//! address counts exactly where a scan would have found that address, and
//! nowhere inside a longer one.

/// Characters a web address never holds: spaces, quotes, angle brackets, a
/// table's pipe and Markdown code.
pub fn stop(c: char) -> bool {
    c.is_whitespace() || "<>\"'«»“”‘’|`".contains(c)
}

/// Punctuation that ends a sentence, not an address, at an address's end.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', '*', '…'];

/// Whether `text` starts with a scheme (`http://`, `https://`, `ftp://`) or
/// `www.`, in any case.
pub fn explicit(text: &str) -> bool {
    scheme_len(text) > 0
        || text
            .get(..4)
            .is_some_and(|w| w.eq_ignore_ascii_case("www."))
}

/// The length of `text`'s scheme, `http://` and the like, or 0.
pub fn scheme_len(text: &str) -> usize {
    ["http://", "https://", "ftp://"]
        .iter()
        .find(|scheme| {
            text.get(..scheme.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
        })
        .map_or(0, |scheme| scheme.len())
}

/// Invisible characters the view drops ([`crate::view`]): a zero-width
/// space or a soft hyphen glues what is on either side.
pub fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
    )
}

/// Whether what starts at `start` follows an email's "@": the domain of
/// "alice@site.invalid", also wrapped after its "@" onto the next line,
/// split by spaces or invisible characters, or with the "@" written as a
/// reference or percent-encoded ("&#64;", "%40").
pub fn email_domain(source: &str, start: usize) -> bool {
    let lower =
        source[source.floor_char_boundary(start.saturating_sub(8))..start].to_ascii_lowercase();
    if ["&#64;", "&#x40;", "&commat;", "%40"]
        .iter()
        .any(|at| lower.ends_with(at))
    {
        return true;
    }
    let mut breaks = 0;
    for c in source[..start].chars().rev() {
        match c {
            '@' | '＠' => return true,
            '\n' => {
                breaks += 1;
                if breaks > 1 {
                    return false;
                }
            }
            ' ' | '\t' | '\r' => {}
            c if invisible(c) => {}
            _ => return false,
        }
    }
    false
}

/// The Markdown emphasis right before `start`: a run of up to three `_`,
/// `*` or `~` ("_https://…_", "**www.…**", "~~https://…~~"), or "".
fn emphasis_before(source: &str, start: usize) -> &str {
    let before = &source[..start];
    for mark in ['_', '*', '~'] {
        let run = before.len() - before.trim_end_matches(mark).len();
        if run > 0 {
            return &before[before.len() - run.min(3)..];
        }
    }
    ""
}

/// The length of a Markdown link destination in angle brackets that starts
/// `text`, up to its ">" ("[x](<https://…/My File.pdf>)"): it may hold
/// spaces. `None` when the address is not in one.
fn angle_destination(source: &str, start: usize) -> Option<usize> {
    let before = source[..start].strip_suffix('<')?;
    let link = before.ends_with("](") || before.trim_end_matches([' ', '\t']).ends_with("]:");
    if !link {
        return None;
    }
    let text = &source[start..];
    let close = text.find(['>', '<', '\n', '\r'])?;
    text[close..].starts_with('>').then_some(close)
}

/// The length up to the first character an address never holds, from
/// `text`'s start. An apostrophe between letters in the path is the
/// address's own ("…/it's"); `path` is where the path starts.
fn extent(text: &str, path: usize) -> usize {
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let inner_apostrophe = c == '\''
            && i > path
            && text[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
            && chars.peek().is_some_and(|(_, next)| next.is_alphanumeric());
        if stop(c) && !inner_apostrophe {
            return i;
        }
    }
    text.len()
}

/// Whether an address at `start` is glued to what is before it: a word, an
/// email's "@", and for a bare domain a path, a subdomain or a field's
/// separator too. An address with a scheme or `www.` may follow a field's
/// colon or an attribute's "=" ("Website:https://…", "href=https://…").
/// Emphasis marks before it are syntax.
pub fn glued_before(source: &str, start: usize) -> bool {
    let explicit = explicit(&source[start..]);
    let start = start - emphasis_before(source, start).len();
    let before = source[..start].chars().rev().find(|c| !invisible(*c));
    let glued = match before {
        None => false,
        Some(c) if c.is_alphanumeric() => true,
        Some(c) => !explicit && "./\\-_:%#=&+~".contains(c),
    };
    glued || email_domain(source, start)
}

/// Where an address that starts at `start` ends, as a scan reads it: with a
/// scheme or `www.`, up to a character it never holds; a bare domain, its
/// labels, a port and, after `/`, `?` or `#`, its path. Then without an
/// unbalanced closing bracket (a Markdown link's or a sentence's), trailing
/// punctuation, and closing emphasis that matches the emphasis before it.
pub fn end(source: &str, start: usize) -> usize {
    if let Some(len) = angle_destination(source, start) {
        return start + len;
    }
    let text = &source[start..];
    let raw = if explicit(text) {
        let scheme = scheme_len(text);
        let host = text[scheme..]
            .find(['/', '?', '#'])
            .map_or(text.len(), |at| scheme + at);
        extent(text, host)
    } else {
        bare_extent(text)
    };
    let mut len = trimmed_len(&text[..raw]);
    let run = emphasis_before(source, start);
    if !run.is_empty() && text[..len].ends_with(run) && len > run.len() {
        len -= run.len();
        len = trimmed_len(&text[..len]);
    }
    start + len
}

/// Whether `range` is a whole web address where it stands: not glued to
/// what is before it, and ending where a scan would end it, so not the
/// start of a longer address ("…/doc" of "…/doc(token)" or "…/doc;id=7").
pub fn whole(source: &str, range: &std::ops::Range<usize>) -> bool {
    !glued_before(source, range.start) && end(source, range.start) == range.end
}

/// A bare domain's extent: labels of letters, digits, `_` and `-` joined by
/// dots, an optional port, and a path after `/`, `?` or `#`.
fn bare_extent(text: &str) -> usize {
    let label = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut end = 0;
    let mut chars = text.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if label(c) {
            end = i + c.len_utf8();
            chars.next();
        } else if c == '.' && end > 0 && text[i + 1..].starts_with(label) {
            chars.next();
        } else {
            break;
        }
    }
    let rest = &text[end..];
    if let Some(port) = rest.strip_prefix(':') {
        let digits = port.len() - port.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if (1..=5).contains(&digits) {
            end += 1 + digits;
        }
    }
    if text[end..].starts_with(['/', '?', '#']) {
        end += extent(&text[end..], 0);
    }
    end
}

/// The length of the address `text` holds (up to its extent): up to an
/// unbalanced closing bracket, without trailing punctuation or a bracket it
/// opened and did not close.
fn trimmed_len(text: &str) -> usize {
    let mut depth = [0usize; 2];
    let mut end = 0;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        let pair = usize::from(matches!(c, '[' | ']'));
        // A Markdown escape makes a bracket the address's own
        // ("…/receipt\\).pdf" in a link target).
        let literal = escaped;
        escaped = c == '\\' && !literal;
        if !literal && matches!(c, '(' | '[') {
            depth[pair] += 1;
        } else if !literal && matches!(c, ')' | ']') {
            if depth[pair] == 0 {
                break;
            }
            depth[pair] -= 1;
        }
        end = i + c.len_utf8();
    }
    let mut address = &text[..end];
    loop {
        let shorter = address.trim_end_matches(TRAILING);
        let shorter = shorter.strip_suffix(['(', '[']).unwrap_or(shorter);
        if shorter.len() == address.len() {
            break;
        }
        address = shorter;
    }
    address.len()
}

#[cfg(test)]
mod tests {
    use super::{end, whole};

    fn at(source: &str, text: &str) -> std::ops::Range<usize> {
        let start = source.find(text).unwrap();
        start..start + text.len()
    }

    #[test]
    fn an_address_ends_where_a_scan_ends_it() {
        for (source, address) in [
            ("See https://a.invalid/doc.", "https://a.invalid/doc"),
            ("[x](https://a.invalid/doc)", "https://a.invalid/doc"),
            ("(https://a.invalid/Page_(x))", "https://a.invalid/Page_(x)"),
            ("_https://a.invalid/Alice_", "https://a.invalid/Alice"),
            ("__https://a.invalid/Alice__ x", "https://a.invalid/Alice"),
            ("**www.a.invalid/u**", "www.a.invalid/u"),
            ("site.invalid:8080/x, then", "site.invalid:8080/x"),
            ("сайт ivanov-law.ru—это", "ivanov-law.ru"),
            ("https://a.invalid/token_ x", "https://a.invalid/token_"),
            ("~~https://a.invalid/x~~ gone", "https://a.invalid/x"),
            ("See https://a.invalid/it's here", "https://a.invalid/it's"),
            ("'https://a.invalid/x' quoted", "https://a.invalid/x"),
            ("Visit https://a.invalid's page", "https://a.invalid"),
            (
                "[x](<https://a.invalid/My File.pdf>)",
                "https://a.invalid/My File.pdf",
            ),
            (
                "[ref]: <https://a.invalid/My File.pdf> \"T\"",
                "https://a.invalid/My File.pdf",
            ),
            ("<https://a.invalid/x y>", "https://a.invalid/x"),
        ] {
            let range = at(source, address);
            assert_eq!(&source[range.start..end(source, range.start)], address);
            assert!(whole(source, &range), "{source}");
        }
    }

    #[test]
    fn a_known_address_is_not_part_of_a_longer_one_or_an_email() {
        for (source, known) in [
            (
                "https://a.invalid/doc(SecretToken)",
                "https://a.invalid/doc",
            ),
            (
                "https://a.invalid/doc;token=Secret",
                "https://a.invalid/doc",
            ),
            ("https://a.invalid/doc/2", "https://a.invalid/doc"),
            ("alice@www.a.invalid", "www.a.invalid"),
            ("alice@a.invalid", "a.invalid"),
            ("mail.a.invalid", "a.invalid"),
            ("a.invalid:8080", "a.invalid"),
            ("https://a.invalid", "a.invalid"),
            ("xhttps://a.invalid", "https://a.invalid"),
        ] {
            let range = source.rfind(known).unwrap()..source.rfind(known).unwrap() + known.len();
            assert!(!whole(source, &range), "{source}");
        }
        for (source, known) in [
            ("Website:https://a.invalid/x", "https://a.invalid/x"),
            ("<a href=https://a.invalid/x>", "https://a.invalid/x"),
        ] {
            assert!(whole(source, &at(source, known)), "{source}");
        }
    }
}
