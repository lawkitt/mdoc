//! Word splitting and the word-level rules: scripts, ALL-CAPS, digits, bare
//! links, and the Latin/Cyrillic look-alike letters OCR confuses.

use std::ops::Range;

/// A candidate word: a run of letters, digits, `_`, apostrophes and hyphens,
/// trimmed of edge punctuation. Byte range within its line.
pub(crate) struct Token {
    pub range: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Script {
    Latin,
    Cyrillic,
    /// Both Latin and Cyrillic letters: almost always an OCR look-alike swap.
    Mixed,
    /// Neither (Greek, CJK, …) or no letters; never flagged.
    Other,
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic()
        || (c.is_alphabetic() && matches!(c, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}'))
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{052F}') && c.is_alphabetic()
}

fn is_combining(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}')
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{2011}')
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\'' || c == '’' || is_hyphen(c) || is_combining(c)
}

pub(crate) fn script(word: &str) -> Script {
    let (mut latin, mut cyrillic) = (false, false);
    for c in word.chars() {
        latin |= is_latin(c);
        cyrillic |= is_cyrillic(c);
    }
    match (latin, cyrillic) {
        (true, true) => Script::Mixed,
        (true, false) => Script::Latin,
        (false, true) => Script::Cyrillic,
        (false, false) => Script::Other,
    }
}

pub(crate) fn tokens(line: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if !is_word_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(i, c)) = chars.peek() {
            if !is_word_char(c) {
                break;
            }
            end = i + c.len_utf8();
            chars.next();
        }
        // Trim edge punctuation: `'quoted'`, `-dash`, `_italic_`.
        let raw = &line[start..end];
        let lead = raw.len() - raw.trim_start_matches(|c: char| !c.is_alphanumeric()).len();
        let trimmed =
            raw[lead..].trim_end_matches(|c: char| !c.is_alphanumeric() && !is_combining(c));
        if !trimmed.is_empty() {
            out.push(Token {
                range: start + lead..start + lead + trimmed.len(),
            });
        }
    }
    out
}

/// Absolute ranges of the hyphen-separated parts of `word` (at `offset`).
pub(crate) fn hyphen_parts(word: &str, offset: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in word.char_indices() {
        if is_hyphen(c) {
            if i > start {
                out.push(offset + start..offset + i);
            }
            start = i + c.len_utf8();
        }
    }
    if start < word.len() {
        out.push(offset + start..offset + word.len());
    }
    out
}

pub(crate) fn letter_count(word: &str) -> usize {
    word.chars().filter(|c| c.is_alphabetic()).count()
}

pub(crate) fn is_all_caps(word: &str) -> bool {
    let mut cased = word
        .chars()
        .filter(|c| c.is_uppercase() || c.is_lowercase())
        .peekable();
    cased.peek().is_some() && cased.all(char::is_uppercase) && letter_count(word) >= 2
}

/// Digits, optionally with an English ordinal or plural: `12`, `21st`, `1990s`.
pub(crate) fn is_number_like(part: &str) -> bool {
    let digits = part.len() - part.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return false;
    }
    matches!(
        part[digits..].to_ascii_lowercase().as_str(),
        "" | "st" | "nd" | "rd" | "th" | "s"
    )
}

/// The form looked up in a dictionary: stress marks dropped, curly
/// apostrophes made straight.
pub(crate) fn lookup_form(word: &str) -> String {
    word.chars()
        .filter(|&c| !is_combining(c))
        .map(|c| if c == '’' { '\'' } else { c })
        .collect()
}

pub(crate) fn yo_to_ye(word: &str) -> String {
    word.replace('ё', "е").replace('Ё', "Е")
}

/// Bare URLs and e-mail addresses in `line`; always skipped.
pub(crate) fn bare_links(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for chunk in line
        .split_inclusive(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '"'))
    {
        let start = offset;
        offset += chunk.len();
        let body = chunk.trim_end_matches(|c: char| {
            c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '"')
        });
        let lower = body.to_ascii_lowercase();
        let email = body
            .split_once('@')
            .is_some_and(|(user, domain)| !user.is_empty() && domain.contains('.'));
        if lower.contains("://")
            || lower.starts_with("www.")
            || lower.starts_with("mailto:")
            || email
        {
            out.push(start..start + body.len());
        }
    }
    out
}

/// Latin/Cyrillic letters that look alike: (Latin, Cyrillic).
const LOOK_ALIKES: [(char, char); 21] = [
    ('a', 'а'),
    ('c', 'с'),
    ('e', 'е'),
    ('i', 'і'),
    ('k', 'к'),
    ('o', 'о'),
    ('p', 'р'),
    ('x', 'х'),
    ('y', 'у'),
    ('A', 'А'),
    ('B', 'В'),
    ('C', 'С'),
    ('E', 'Е'),
    ('H', 'Н'),
    ('K', 'К'),
    ('M', 'М'),
    ('O', 'О'),
    ('P', 'Р'),
    ('T', 'Т'),
    ('X', 'Х'),
    ('Y', 'У'),
];

/// For a mixed-script word, the spellings that swap look-alike letters into
/// one script, majority script first (Latin on a tie). Empty when the word isn't mixed or no
/// complete swap exists.
pub(crate) fn single_script_forms(word: &str) -> Vec<String> {
    if script(word) != Script::Mixed {
        return Vec::new();
    }
    let latin = word.chars().filter(|&c| is_latin(c)).count();
    let cyrillic = word.chars().filter(|&c| is_cyrillic(c)).count();
    let to_cyrillic = |c: char| LOOK_ALIKES.iter().find(|p| p.0 == c).map_or(c, |p| p.1);
    let to_latin = |c: char| LOOK_ALIKES.iter().find(|p| p.1 == c).map_or(c, |p| p.0);
    let as_cyrillic: String = word.chars().map(to_cyrillic).collect();
    let as_latin: String = word.chars().map(to_latin).collect();
    let mut out = Vec::new();
    let ordered = if cyrillic > latin {
        [as_cyrillic, as_latin]
    } else {
        [as_latin, as_cyrillic]
    };
    for form in ordered {
        if script(&form) != Script::Mixed && !out.contains(&form) {
            out.push(form);
        }
    }
    out
}
