//! Word boundaries for word movement, word deletion and double-click.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The end of the next word at/after `offset` (⌥→ on macOS).
pub fn next_word(source: &str, offset: usize) -> usize {
    source
        .unicode_word_indices()
        .map(|(i, w)| i + w.len())
        .find(|&end| end > offset)
        .unwrap_or(source.len())
}

/// The start of the next word after `offset`, or the end of its line if
/// that comes first (Ctrl+→ and Ctrl+Delete on Windows).
pub fn next_word_start(source: &str, offset: usize) -> usize {
    let word = source
        .unicode_word_indices()
        .map(|(i, _)| i)
        .find(|&start| start > offset)
        .unwrap_or(source.len());
    let line_end = source[offset.min(source.len())..]
        .char_indices()
        .skip(1)
        .find(|&(_, c)| c == '\n')
        .map_or(source.len(), |(i, _)| offset + i);
    word.min(line_end)
}

/// The start of the previous word before `offset` (⌥← on macOS).
pub fn prev_word(source: &str, offset: usize) -> usize {
    source
        .unicode_word_indices()
        .map(|(i, _)| i)
        .rfind(|&start| start < offset)
        .unwrap_or(0)
}

/// The byte range of the word at `offset` (double-click); `None` in whitespace.
pub fn word_range_at(source: &str, offset: usize) -> Option<Range<usize>> {
    let mut ends_at = None;
    for (i, w) in source.unicode_word_indices() {
        let range = i..i + w.len();
        if range.start <= offset && offset < range.end {
            return Some(range);
        }
        if range.end == offset {
            ends_at = Some(range);
        }
    }
    ends_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_move_by_unicode_words() {
        let s = "один two\nthree";
        assert_eq!(next_word(s, 0), 8);
        assert_eq!(next_word(s, 8), 12);
        assert_eq!(prev_word(s, 12), 9);
        assert_eq!(next_word_start(s, 0), 9);
        assert_eq!(next_word_start(s, 9), 12);
        assert_eq!(word_range_at(s, 9), Some(9..12));
        assert_eq!(word_range_at(s, 12), Some(9..12));
        assert_eq!(word_range_at("a  b", 2), None);
    }
}
