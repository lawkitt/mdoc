//! AI labels: how a document is named to an AI app (#1091, "Decided for
//! Milestone 2"; #1164 Decision 22). The AI sees the first heading of the
//! reviewed text, else "Document N". A stored title is never read: titles
//! come from file names and are never reviewed, so they can hold real names.
//!
//! - The heading is the first non-empty ATX heading of the text that just
//!   passed the check, after its note, outside fenced code; its markers
//!   trimmed and at most [`MAX_CHARS`] characters.
//! - A document that is not Ready now is "Document N": its text is not
//!   reviewed, so nothing of it names it.
//! - N is its place among the AI project's listed documents, oldest first,
//!   from 1.
//! - A label equal to an earlier one gets " (2)", " (3)" in the same order.
use std::collections::HashMap;

/// The longest label heading, in characters.
pub(crate) const MAX_CHARS: usize = 80;

/// The label heading of `body`, the text after the note: its first
/// non-empty heading, cut to [`MAX_CHARS`].
pub(crate) fn heading(body: &str) -> Option<String> {
    crate::parts::headings(body)
        .map(|(_, heading)| heading.text)
        .find(|text| !text.is_empty())
}

/// `text` at most `max` characters: cut at a space where one is near the
/// end, with "…".
pub(crate) fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max - 1).collect();
    let at_space = kept
        .rfind(' ')
        .filter(|&at| kept[..at].chars().count() >= max / 2);
    let kept = at_space.map_or(kept.as_str(), |at| &kept[..at]).trim_end();
    format!("{kept}…")
}

/// Labels handed out so far, for numbering their repeats.
#[derive(Default)]
pub(crate) struct Labels {
    seen: HashMap<String, usize>,
}

impl Labels {
    /// The label of the `n`th listed document, with `heading` if it is Ready
    /// and has one.
    pub(crate) fn next(&mut self, n: usize, heading: Option<String>) -> String {
        let label = heading.unwrap_or_else(|| format!("Document {n}"));
        let seen = self.seen.entry(label.clone()).or_default();
        *seen += 1;
        match *seen {
            1 => label,
            repeat => format!("{label} ({repeat})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_non_empty_heading_outside_code_names_it() {
        let body =
            "Intro.\n\n```\n# not a heading\n```\n\n#\n\n## Lease of PERSON_1's flat ##\n\n# Later";
        assert_eq!(heading(body).as_deref(), Some("Lease of PERSON_1's flat"));
        assert_eq!(heading("No heading at all.\n"), None);
    }

    #[test]
    fn a_long_heading_is_cut_at_a_space() {
        let long = "word ".repeat(40);
        let cut = cut(long.trim(), MAX_CHARS);
        assert!(cut.chars().count() <= MAX_CHARS, "{cut}");
        assert!(cut.ends_with("word…"), "{cut}");
        let unbroken = "x".repeat(200);
        assert_eq!(cut_len(&unbroken), MAX_CHARS);
    }

    fn cut_len(text: &str) -> usize {
        cut(text, MAX_CHARS).chars().count()
    }

    #[test]
    fn repeats_are_numbered_in_order() {
        let mut labels = Labels::default();
        assert_eq!(labels.next(1, Some("Lease".into())), "Lease");
        assert_eq!(labels.next(2, None), "Document 2");
        assert_eq!(labels.next(3, Some("Lease".into())), "Lease (2)");
        assert_eq!(labels.next(4, Some("Lease".into())), "Lease (3)");
    }
}
