//! Manual additions: the span a lawyer's selection would replace.
use super::{Review, exact_boundary, overlaps};
use std::ops::Range;

/// Brackets and quotes that stay when both halves are inside the span.
const PAIRS: [(char, char); 7] = [
    ('(', ')'),
    ('«', '»'),
    ('“', '”'),
    ('„', '“'),
    ('‘', '’'),
    ('"', '"'),
    ('\'', '\''),
];
/// Abbreviations whose final period belongs to a name ("Acme Ltd.").
const ABBREVIATIONS: [&str; 8] = ["ltd", "inc", "co", "corp", "jr", "sr", "plc", "bros"];

fn edge(c: char) -> bool {
    c.is_whitespace() || (c.is_ascii_punctuation() && c != '+') || "«»“”„‘’…–—".contains(c)
}
/// Keep a final period that ends an initial ("М.С."), an abbreviation or a
/// dotted token; a sentence period after a full word is trimmed.
fn abbreviation(text: &str) -> bool {
    let Some(body) = text.strip_suffix('.') else {
        return false;
    };
    let token = body.rsplit(char::is_whitespace).next().unwrap_or(body);
    token.contains('.')
        || (token.chars().count() == 1 && token.chars().all(char::is_alphabetic))
        || ABBREVIATIONS.contains(&token.to_lowercase().as_str())
}
/// Trim whitespace, punctuation, wrapping quotes/brackets and Markdown
/// delimiters from both edges, keeping balanced pairs and a leading `+`.
pub(super) fn trim_selection(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    let (mut start, mut end) = (range.start, range.end);
    loop {
        let text = source.get(start..end)?;
        let (Some(first), Some(last)) = (text.chars().next(), text.chars().next_back()) else {
            return None;
        };
        if text.len() > first.len_utf8()
            && PAIRS
                .iter()
                .any(|&(open, close)| first == open && last == close)
        {
            start += first.len_utf8();
            end -= last.len_utf8();
            continue;
        }
        let rest = &text[first.len_utf8()..];
        let opens_pair = PAIRS
            .iter()
            .any(|&(open, close)| first == open && open != close && rest.contains(close));
        if edge(first) && !opens_pair {
            start += first.len_utf8();
            continue;
        }
        let head = &text[..text.len() - last.len_utf8()];
        let closes_pair = PAIRS
            .iter()
            .any(|&(open, close)| last == close && open != close && head.contains(open));
        if edge(last) && !closes_pair && !(last == '.' && abbreviation(text)) {
            end -= last.len_utf8();
            continue;
        }
        return Some(start..end);
    }
}
fn placeholder(word: &str) -> bool {
    word.split_once('_').is_some_and(|(prefix, suffix)| {
        !suffix.is_empty()
            && suffix.bytes().all(|b| b.is_ascii_digit())
            && super::Category::ALL.iter().any(|c| c.token() == prefix)
    })
}

impl Review {
    /// The span a manual addition replaces for `selection`, or a one-line reason
    /// it cannot. It may contain whole pending mentions, which it supersedes,
    /// but must not cut through one, an applied replacement or a placeholder.
    pub fn manual_target(
        &self,
        source: &str,
        selection: Range<usize>,
    ) -> Result<Range<usize>, String> {
        let range = trim_selection(source, selection)
            .filter(|r| source[r.clone()].chars().any(char::is_alphanumeric))
            .ok_or("Select identifying text.")?;
        Self::validate_manual(source, range.clone())?;
        let text = &source[range.clone()];
        if !exact_boundary(source, &range, text) {
            return Err("Select whole words.".into());
        }
        if self.applied().iter().any(|a| overlaps(&a.range, &range))
            || text
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .any(placeholder)
        {
            return Err("Already replaced.".into());
        }
        let partial = |c: &super::Candidate| {
            overlaps(&c.range, &range)
                && !(range.start <= c.range.start && c.range.end <= range.end)
        };
        if let Some(candidate) = self.candidates().iter().find(|c| partial(c)) {
            return Err(
                if candidate.range.start <= range.start && range.end <= candidate.range.end {
                    "Already part of a proposed replacement."
                } else {
                    "Select all of the proposed replacement, or none of it."
                }
                .into(),
            );
        }
        Ok(range)
    }
}
