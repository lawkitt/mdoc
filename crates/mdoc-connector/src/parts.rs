//! Long documents in parts (#1164 Decision 24): above [`LONG`] characters a
//! read returns an outline with numbered parts, and the model fetches the
//! parts it needs. Every part is cut again from the text as it is now, so a
//! part is never older than the check it just passed.
//!
//! The text is cut into sections at its ATX headings (outside fenced code),
//! and consecutive sections are packed into parts of at most [`PART_MAX`]
//! characters. A section longer than that is cut at blank lines, then at
//! line ends, then at a character. The parts, joined, give the text back.
use crate::label;
use std::ops::Range;

/// Above this many characters (after the note) a read is an outline.
pub const LONG: usize = 60_000;
/// The most characters in one part, the note aside.
pub const PART_MAX: usize = 60_000;
/// The most headings an outline lists, across its parts.
pub const MAX_OUTLINE_HEADINGS: usize = 500;

/// A heading of the text: its level (1-6) and its text, markers trimmed,
/// at most 80 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

/// A part, as the outline lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartInfo {
    /// From 1.
    pub part: usize,
    pub chars: usize,
    /// The headings that start in it.
    pub headings: Vec<Heading>,
}

/// A part of the text.
pub(crate) struct Part<'a> {
    pub(crate) text: &'a str,
    pub(crate) chars: usize,
    pub(crate) headings: Vec<Heading>,
}

/// The ATX headings of `text` outside fenced code, with the byte offset
/// where each line starts.
pub(crate) fn headings(text: &str) -> impl Iterator<Item = (usize, Heading)> + '_ {
    let mut fence: Option<(u8, usize)> = None;
    lines(text).filter_map(move |(at, line)| {
        let line = line.trim_end_matches(['\n', '\r']);
        if let Some(run) = fence_run(line) {
            match fence {
                None => fence = Some(run),
                Some((mark, len)) if run.0 == mark && run.1 >= len => fence = None,
                Some(_) => {}
            }
            return None;
        }
        if fence.is_some() {
            return None;
        }
        let level = mdoc_markdown::lines::heading_level(line)?;
        let text = line[level as usize..].trim();
        // A closing sequence: `#`s after a space, or a line of `#`s alone.
        let stripped = text.trim_end_matches('#');
        let text = if stripped.is_empty() || stripped.ends_with(' ') {
            stripped.trim_end()
        } else {
            text
        };
        Some((
            at,
            Heading {
                level,
                text: label::cut(text, label::MAX_CHARS),
            },
        ))
    })
}

/// A code fence's mark and length, if `line` opens or closes one.
fn fence_run(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let mark = *trimmed.as_bytes().first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|&b| b == mark).count();
    (len >= 3).then_some((mark, len))
}

/// Each line of `text` with its end of line, and where it starts.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n').scan(0, |at, line| {
        let start = *at;
        *at += line.len();
        Some((start, line))
    })
}

/// `text` in parts of at most [`PART_MAX`] characters, cut at headings
/// where they allow.
pub(crate) fn parts(text: &str) -> Vec<Part<'_>> {
    let mut parts: Vec<(Range<usize>, usize, Vec<Heading>)> = Vec::new();
    for (section, heading) in sections(text) {
        let mut heading = heading;
        for piece in pieces(text, section) {
            let chars = text[piece.clone()].chars().count();
            let headings: Vec<Heading> = heading.take().into_iter().collect();
            match parts.last_mut() {
                // Pieces are consecutive, so the part grows in place.
                Some((range, total, held)) if *total + chars <= PART_MAX => {
                    range.end = piece.end;
                    *total += chars;
                    held.extend(headings);
                }
                _ => parts.push((piece, chars, headings)),
            }
        }
    }
    (parts.into_iter())
        .map(|(range, chars, headings)| Part {
            text: &text[range],
            chars,
            headings,
        })
        .collect()
}

/// The outline of `parts`: at most [`MAX_OUTLINE_HEADINGS`] headings in all.
pub(crate) fn outline(parts: &[Part<'_>]) -> Vec<PartInfo> {
    let mut left = MAX_OUTLINE_HEADINGS;
    (parts.iter().enumerate())
        .map(|(index, part)| {
            let headings: Vec<Heading> = part.headings.iter().take(left).cloned().collect();
            left -= headings.len();
            PartInfo {
                part: index + 1,
                chars: part.chars,
                headings,
            }
        })
        .collect()
}

/// `text` cut before each heading: each section with its heading, the
/// first without one when the text does not start with a heading.
fn sections(text: &str) -> Vec<(Range<usize>, Option<Heading>)> {
    let mut sections = Vec::new();
    let mut start = 0;
    let mut heading = None;
    for (at, next) in headings(text) {
        if at > start {
            sections.push((start..at, heading.take()));
        }
        start = at;
        heading = Some(next);
    }
    if start < text.len() || sections.is_empty() {
        sections.push((start..text.len(), heading));
    }
    sections
}

/// The section `range` of `text` in pieces of at most [`PART_MAX`]
/// characters: cut at the last blank line that fits, else the last line
/// end, else at a character.
fn pieces(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = range.start;
    loop {
        let rest = &text[start..range.end];
        let Some((limit, _)) = rest.char_indices().nth(PART_MAX) else {
            pieces.push(start..range.end);
            return pieces;
        };
        let window = &rest[..limit];
        let cut = (window.rfind("\n\n").map(|at| at + 2))
            .or_else(|| window.rfind('\n').map(|at| at + 1))
            .filter(|&at| at > 0)
            .unwrap_or(limit);
        pieces.push(start..start + cut);
        start += cut;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(parts: &[Part<'_>]) -> String {
        parts.iter().map(|part| part.text).collect()
    }

    #[test]
    fn sections_are_packed_into_parts_cut_at_headings() {
        let section = |n: usize| {
            format!(
                "## Section {n}\n\n{}\n\n",
                "PERSON_1 signed. ".repeat(1_000)
            )
        };
        let text: String = (1..=12).map(section).collect();
        let parts = parts(&text);
        assert_eq!(joined(&parts), text);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(part.chars <= PART_MAX);
            assert_eq!(part.chars, part.text.chars().count());
            assert!(
                part.text.starts_with("## Section "),
                "{:?}",
                &part.text[..20]
            );
            assert!(!part.headings.is_empty());
        }
        let headings: usize = parts.iter().map(|part| part.headings.len()).sum();
        assert_eq!(headings, 12);
    }

    #[test]
    fn a_section_too_long_for_a_part_is_cut_at_blank_lines() {
        let paragraph = format!("{}\n\n", "Ж".repeat(999));
        let text = paragraph.repeat(150);
        let parts = parts(&text);
        assert_eq!(joined(&parts), text);
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert!(part.chars <= PART_MAX);
            assert!(part.text.ends_with("\n\n"));
        }
    }

    #[test]
    fn a_text_with_no_line_end_is_cut_at_a_character() {
        let text = "é".repeat(PART_MAX * 2 + 5);
        let parts = parts(&text);
        assert_eq!(joined(&parts), text);
        let sizes: Vec<_> = parts.iter().map(|part| part.chars).collect();
        assert_eq!(sizes, [PART_MAX, PART_MAX, 5]);
    }

    #[test]
    fn headings_in_fenced_code_do_not_cut() {
        let text = "Intro\n```\n# code\n```\n# Real\n";
        let found: Vec<_> = headings(text).map(|(at, h)| (at, h.text)).collect();
        assert_eq!(found, [(text.find("# Real").unwrap(), "Real".to_string())]);
    }

    #[test]
    fn the_outline_lists_at_most_its_headings() {
        let text: String = (0..MAX_OUTLINE_HEADINGS + 50)
            .map(|n| format!("# H{n}\n{}\n", "x".repeat(400)))
            .collect();
        let parts = parts(&text);
        let outline = outline(&parts);
        let listed: usize = outline.iter().map(|part| part.headings.len()).sum();
        assert_eq!(listed, MAX_OUTLINE_HEADINGS);
        assert_eq!(outline.len(), parts.len());
    }
}
