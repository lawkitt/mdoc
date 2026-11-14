//! Fix the words (ADR 0114): a found name's span corrected in the text. The
//! new wording takes the old one's place everywhere it fits, keeping the
//! identity and its alias; a selection outside any name snaps to whole
//! words the same way before it is hidden.
use super::{Category, Review, manual::trim_selection, overlaps, syntax, view};
use std::{collections::HashSet, ops::Range, sync::Arc};

fn word(c: char) -> bool {
    c.is_alphanumeric()
}

/// The word `at` falls inside, when it is inside one rather than at an edge.
fn word_around(source: &str, at: usize) -> Option<Range<usize>> {
    let before = source[..at].chars().next_back().filter(|c| word(*c))?;
    source[at..].chars().next().filter(|c| word(*c))?;
    let start = source[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(at - before.len_utf8(), |(i, _)| i);
    let end = source[at..]
        .char_indices()
        .find(|(_, c)| !word(*c))
        .map_or(source.len(), |(i, _)| at + i);
    Some(start..end)
}

/// `range` with each end that falls inside a word moved out to that
/// word's edge: a word the selection reaches into counts whole, so a handle
/// lands between words, never inside one.
pub fn snap_to_words(source: &str, range: Range<usize>) -> Range<usize> {
    let (mut start, mut end) = (range.start.min(source.len()), range.end.min(source.len()));
    while !source.is_char_boundary(start) {
        start -= 1;
    }
    while !source.is_char_boundary(end) {
        end += 1;
    }
    let end = end.max(start);
    let start = word_around(source, start).map_or(start, |w| w.start);
    let end = word_around(source, end).map_or(end, |w| w.end);
    start..end
}

/// Whether `range` of `source` reads as an alias the review made: a
/// category token, `_`, digits.
fn holds_alias(source: &str, range: &Range<usize>) -> bool {
    source[range.clone()]
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .any(super::generated_alias)
}

/// The words a name's handles select in `source`, `selection` snapped to
/// whole words and trimmed as a manual addition is; refused when nothing
/// but spaces and punctuation is left, when it crosses Markdown formatting,
/// a paragraph or a cell (`Review::validate_manual`), or when it takes in
/// one of `blocked`, another name's alias.
pub fn words_target(
    source: &str,
    selection: Range<usize>,
    blocked: &[Range<usize>],
) -> Result<Range<usize>, String> {
    let range = trim_selection(source, snap_to_words(source, selection))
        .filter(|r| source[r.clone()].chars().any(char::is_alphanumeric))
        .ok_or("Select identifying text.")?;
    Review::validate_manual(source, range.clone())?;
    if blocked.iter().any(|b| overlaps(b, &range)) || holds_alias(source, &range) {
        return Err("Already replaced.".into());
    }
    Ok(range)
}

/// How far a name's handles reach around `wording` in `source`: whole
/// steps (a word, or one character between words) for as long as the span
/// stays in its paragraph or cell, crosses no Markdown formatting and
/// takes in none of `blocked`. A handle stops where a drag would break it.
pub fn words_bounds(source: &str, wording: Range<usize>, blocked: &[Range<usize>]) -> Range<usize> {
    let protected = syntax::protected_syntax(source);
    let markup = view::inline_markup(source);
    let fits = |range: &Range<usize>| {
        super::manual_span(&source[range.clone()])
            && !protected.iter().any(|p| overlaps(p, range))
            && !blocked.iter().any(|b| overlaps(b, range))
            && syntax::plain_text_span(source, &markup, range.clone())
    };
    let mut bounds = wording;
    while let Some(c) = source[..bounds.start].chars().next_back() {
        let mut start = bounds.start - c.len_utf8();
        if word(c) {
            start = source[..bounds.start]
                .char_indices()
                .rev()
                .take_while(|(_, c)| word(*c))
                .last()
                .map_or(start, |(i, _)| i);
        }
        if !fits(&(start..bounds.end)) {
            break;
        }
        bounds.start = start;
    }
    while let Some(c) = source[bounds.end..].chars().next() {
        let mut end = bounds.end + c.len_utf8();
        if word(c) {
            end = source[bounds.end..]
                .char_indices()
                .find(|(_, c)| !word(*c))
                .map_or(source.len(), |(i, _)| bounds.end + i);
        }
        if !fits(&(bounds.start..end)) {
            break;
        }
        bounds.end = end;
    }
    bounds
}

/// A name's new words against its old ones, both read where the old
/// wording shows (for a replaced name, its original put back).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordsChange {
    pub old: Arc<str>,
    pub new: Arc<str>,
    /// The new words start this many bytes after the old wording's start
    /// (before it, when negative)…
    pub left: isize,
    /// …and end this many bytes after its end.
    pub right: isize,
}

impl WordsChange {
    /// The change from `old` at `wording` of `source` to `new` at `words`.
    pub fn new(source: &str, wording: &Range<usize>, words: &Range<usize>) -> Self {
        Self {
            old: source[wording.clone()].into(),
            new: source[words.clone()].into(),
            left: words.start as isize - wording.start as isize,
            right: words.end as isize - wording.end as isize,
        }
    }

    /// Where the new words sit around the old wording at `range`, once the
    /// old wording is in the text there.
    pub fn range_at(&self, range: &Range<usize>) -> Range<usize> {
        range.start.saturating_add_signed(self.left)..range.end.saturating_add_signed(self.right)
    }

    /// Whether the new words fit around the occurrence of the old wording
    /// at `range` of `source` (a replaced one's alias range: outside it the
    /// text is the same): the words they add on either side are there, and
    /// end at a word's edge. Inside the old wording they fit wherever it is.
    pub fn fits(&self, source: &str, range: &Range<usize>) -> bool {
        let head = (-self.left).max(0) as usize;
        let tail = self.right.max(0) as usize;
        if head > self.new.len() || tail > self.new.len() {
            // Words beside the old wording, not over it: no place to fit.
            return false;
        }
        if head > 0 {
            let Some(start) = range.start.checked_sub(head) else {
                return false;
            };
            let words = self.new.get(..head);
            if words.is_none() || source.get(start..range.start) != words {
                return false;
            }
            if self.new.starts_with(word) && source[..start].ends_with(word) {
                return false;
            }
        }
        if tail > 0 {
            let end = range.end + tail;
            let words = self
                .new
                .len()
                .checked_sub(tail)
                .and_then(|from| self.new.get(from..));
            if words.is_none() || source.get(range.end..end) != words {
                return false;
            }
            if self.new.ends_with(word) && source[end..].starts_with(word) {
                return false;
            }
        }
        true
    }
}

/// What Fix the words reaches before the text changes: the replaced
/// mentions whose new words fit, to put back and replace again, and
/// whether they fit around every occurrence of the old wording.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordsReach {
    /// Applied occurrences (`Applied::id`) of the old wording that fit.
    pub applied: Vec<u64>,
    /// Every proposed, kept and replaced occurrence fits: the old wording
    /// stops being proposed anywhere.
    pub everywhere: bool,
}

impl Review {
    /// The span Hide name hides for `selection`: its ends snapped to whole
    /// words, then as `manual_target` takes it.
    pub fn selection_target(
        &self,
        source: &str,
        selection: Range<usize>,
    ) -> Result<Range<usize>, String> {
        self.manual_target(source, snap_to_words(source, selection))
    }

    /// The occurrences of `change.old` in `source` and whether the new
    /// words fit around each (`WordsReach`).
    pub fn words_reach(&self, source: &str, change: &WordsChange) -> WordsReach {
        let old = change.old.as_ref();
        let mut everywhere = true;
        let mut fit = |range: &Range<usize>| {
            let fits = change.fits(source, range);
            everywhere &= fits;
            fits
        };
        for candidate in self.candidates() {
            if self
                .variant(candidate.variant)
                .is_some_and(|v| v.original.as_ref() == old)
            {
                fit(&candidate.range);
            }
        }
        for (range, original, _) in self.kept_mentions() {
            if original.as_ref() == old {
                fit(&range);
            }
        }
        let applied = self
            .applied()
            .iter()
            .filter(|a| a.step.original() == old)
            .filter(|a| fit(&a.range))
            .map(|a| a.id)
            .collect();
        WordsReach {
            applied,
            everywhere,
        }
    }

    /// Fix the words, the review's part, as one metadata change made after
    /// the replaced mentions that fit were put back (`words_reach`): the
    /// new words join `identity` (or the identity whose wording they are
    /// already), proposed wherever they appear; a kept mention of the old
    /// wording stays kept as the new words; the old wording stops being
    /// proposed when the new words fit everywhere. Returns the new
    /// wording's variant.
    pub fn fix_words(
        &mut self,
        source: &str,
        change: &WordsChange,
        identity: Option<u64>,
        category: Category,
        everywhere: bool,
    ) -> Result<u64, String> {
        self.revision.advance();
        let new = change.new.clone();
        if !new.chars().any(char::is_alphanumeric) {
            return Err("Select identifying text.".into());
        }
        self.refresh(source);
        // Kept mentions of the old wording that fit stay kept, as the new.
        let kept: Vec<_> = self
            .kept_mentions()
            .into_iter()
            .filter(|(range, original, _)| {
                original.as_ref() == change.old.as_ref() && change.fits(source, range)
            })
            .map(|(range, _, _)| range)
            .collect();
        let lifted: HashSet<_> = kept.iter().map(|r| (r.start, r.end)).collect();
        self.tracking.unkeep(&lifted);
        let existed = self.candidates.by_original(&new).is_some();
        let id = self.add_seed_linked(&new, category, identity);
        self.manual_priority += 1;
        let priority = self.manual_priority;
        if let Some(variant) = self.candidates.variant_mut(id) {
            variant.keep_default |= !existed;
            variant.priority = priority;
            variant.mid_sentence = false;
        }
        if self.variant(id).is_some_and(|v| v.cues.is_some()) {
            self.identities.count_every_mention(id);
        }
        self.set_kept(id, false);
        self.tracking.remove_keeps_for(&new);
        if everywhere
            && let Some(old) = self
                .candidates
                .by_original(&change.old)
                .filter(|old| *old != id)
        {
            self.set_kept(old, true);
        }
        self.refresh(source);
        if !kept.is_empty() {
            let owner = self.variant_identity(id);
            self.tracking.keep_many(
                kept.iter()
                    .map(|range| (change.range_at(range), new.clone(), owner)),
            );
            self.refresh(source);
        }
        let category = self.variant(id).map_or(category, |v| v.category);
        if category == Category::Person
            && (super::identities::split_full_name(&new).is_some()
                || super::propagation::People::spreads_initialled(&new))
        {
            self.propagate(source, true);
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(source: &str, words: &str) -> Range<usize> {
        let start = source.find(words).unwrap();
        start..start + words.len()
    }

    #[test]
    fn a_word_the_selection_reaches_into_counts_whole() {
        let text = "ФИО Тагиев Тебриз Фарадж оглы.";
        let name = at(text, "Тагиев Тебриз");
        // One letter into "Тагиев", one short of "Тебриз"'s end.
        let inside = name.start + 'Т'.len_utf8()..name.end - 'з'.len_utf8();
        assert_eq!(snap_to_words(text, inside), name);
        // Inside one word, or a caret in it: the word.
        let word = at(text, "Фарадж");
        assert_eq!(snap_to_words(text, word.start + 2..word.end - 2), word);
        assert_eq!(snap_to_words(text, word.start + 2..word.start + 2), word);
        // At edges, nothing moves.
        assert_eq!(snap_to_words(text, name.clone()), name);
        let gap = at(text, " Тебриз ");
        assert_eq!(snap_to_words(text, gap.clone()), gap);
    }

    #[test]
    fn the_words_are_trimmed_and_never_only_punctuation() {
        let text = "Signed: «Ann Lee», (witness).";
        assert_eq!(
            words_target(text, at(text, "«Ann Lee», "), &[]),
            Ok(at(text, "Ann Lee"))
        );
        assert!(words_target(text, at(text, "», ("), &[]).is_err());
        let gap = text.find("», ").unwrap() + "»".len();
        assert!(words_target(text, gap..gap, &[]).is_err());
    }

    #[test]
    fn the_words_stay_in_one_paragraph_or_cell_and_out_of_formatting() {
        let text = "Ann **Lee** and Bob Hart\n\nnext | cell\n";
        assert!(words_target(text, at(text, "Ann **Lee"), &[]).is_err());
        assert!(words_target(text, at(text, "Hart\n\nnext"), &[]).is_err());
        assert!(words_target(text, at(text, "next | cell"), &[]).is_err());
        // Another name's alias is no part of these words.
        let text = "Bob Hart and PERSON_2 met.";
        assert!(words_target(text, at(text, "Hart and PERSON_2"), &[]).is_err());
    }

    #[test]
    fn the_handles_stop_where_a_drag_would_break_the_span() {
        let text = "Intro.\n\nSigned by Mr Ann Lee **today** here.\n";
        let bounds = words_bounds(text, at(text, "Ann Lee"), &[]);
        assert_eq!(&text[bounds], "Signed by Mr Ann Lee ");
        let text = "| ФИО Тагиев Тебриз | 1980 |\n| --- | --- |\n";
        let bounds = words_bounds(text, at(text, "Тагиев"), &[]);
        assert_eq!(&text[bounds], " ФИО Тагиев Тебриз ");
        // Another name's alias stops it too.
        let text = "PERSON_2 and Ann Lee met.";
        let alias = at(text, "PERSON_2");
        let bounds = words_bounds(text, at(text, "Ann Lee"), &[alias]);
        assert_eq!(&text[bounds], " and Ann Lee met.");
    }

    #[test]
    fn new_words_fit_where_their_added_words_are_there() {
        let text = "Ann met Ann Lee, then Ann Leeds.";
        let first = at(text, "Ann");
        let change = WordsChange {
            old: "Ann".into(),
            new: "Ann Lee".into(),
            left: 0,
            right: 4,
        };
        assert!(!change.fits(text, &first));
        let second = 8..11;
        assert!(change.fits(text, &second));
        assert_eq!(&text[change.range_at(&second)], "Ann Lee");
        // "Ann Lee" inside "Ann Leeds" is no whole word.
        let third = 22..25;
        assert!(!change.fits(text, &third));
        // Narrowed words fit wherever the old wording is.
        let narrowed = WordsChange {
            old: "Mr Ann Lee".into(),
            new: "Ann Lee".into(),
            left: 3,
            right: 0,
        };
        assert!(narrowed.fits("PERSON_1", &(0..8)));
        // Words beside the old wording never fit, and never underflow.
        let beside = WordsChange {
            old: "Ann".into(),
            new: "Lee".into(),
            left: 4,
            right: 4,
        };
        assert!(!beside.fits("Ann Lee.", &(0..3)));
    }
}
