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
//! - A label equal to one issued before it gets " (2)", " (3)" and on,
//!   until no two labels are the same.
use std::collections::HashSet;

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

/// Labels handed out so far, so that no two are the same.
#[derive(Default)]
pub(crate) struct Labels {
    issued: HashSet<String>,
}

impl Labels {
    /// The label of the `n`th listed document, with `heading` if it is Ready
    /// and has one: " (2)", " (3)" and on until it is one not issued yet.
    pub(crate) fn next(&mut self, n: usize, heading: Option<String>) -> String {
        let base = heading.unwrap_or_else(|| format!("Document {n}"));
        let mut label = base.clone();
        let mut repeat = 1;
        while self.issued.contains(&label) {
            repeat += 1;
            label = format!("{base} ({repeat})");
        }
        self.issued.insert(label.clone());
        label
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
        // A heading that reads like a numbered repeat is numbered past it.
        assert_eq!(labels.next(5, Some("Lease (2)".into())), "Lease (2) (2)");
        assert_eq!(labels.next(6, Some("Document 2".into())), "Document 2 (2)");
    }
}
