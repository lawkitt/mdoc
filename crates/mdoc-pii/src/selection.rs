//! A selection the user copies by hand (#1209, ADR 0109 as amended): what
//! it holds, and its proposals accepted as the review accepts them. A
//! selection is clean when the document was scanned and its text has not
//! changed since (the paragraphs of `edited`), no proposal is pending in
//! it, and no name the Project knows is written out in it. The hosts make
//! the accept one undo step and copy the range it returns.
use super::{Review, overlaps, tracking::ReplacementPlan};
use crate::restore::RestoreMap;
use std::ops::Range;

/// What a selection holds, for its copy menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionCheck {
    /// Scanned, unchanged since, nothing pending, no known name: Copy.
    Clean,
    /// Not clean. With `proposals`, "Replace N and copy" takes them; it
    /// copies clean only when nothing else is set.
    NotClean(NotClean),
    /// The document was never scanned: "Copy with names" alone.
    NeverScanned,
}

/// Why a scanned selection is not clean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotClean {
    /// Pending proposals in it, or across its edges: the N of "Replace N
    /// and copy".
    pub proposals: usize,
    /// A paragraph it touches changed since it was scanned.
    pub changed_since_scan: bool,
    /// Names its Project knows still written out in it once its proposals
    /// are replaced (a Keep, a name another document added): replacing
    /// them leaves these.
    pub known_names: usize,
}

/// A selection's proposals, planned as Replace all plans them.
#[derive(Clone, Debug)]
pub struct SelectionPlans {
    /// Sorted, not overlapping: its pending proposals and the applied
    /// aliases its identities renamed since, in the text's offsets.
    pub plans: Vec<ReplacementPlan>,
    /// How many of `plans` are proposals.
    pub proposals: usize,
    /// The selection once `plans` are made: widened to whole words and over
    /// a proposal across its edge, so the copy holds that alias whole.
    pub after: Range<usize>,
}

/// `range` widened to whole words as detection reads them: a selection
/// that cuts into a word is checked as that word, a character reference
/// ("Marin&#97;") or a soft hyphen inside it included, while a note marker
/// after a word ("Durnell¹") is a word of its own. It never shrinks, and
/// never crosses a space or a line break.
pub fn whole_words(source: &str, range: Range<usize>) -> Range<usize> {
    let word = |c: char| {
        (c.is_alphanumeric() && !crate::view::note_marker(c))
            || unicode_normalization::char::is_combining_mark(c)
            || matches!(c, '&' | '#' | ';' | '\u{ad}')
    };
    let mut start = range.start;
    let mut end = range.end;
    if source[start..].chars().next().is_some_and(word) {
        while let Some(c) = source[..start].chars().next_back().filter(|&c| word(c)) {
            start -= c.len_utf8();
        }
    }
    if source[..end].chars().next_back().is_some_and(word) {
        while let Some(c) = source[end..].chars().next().filter(|&c| word(c)) {
            end += c.len_utf8();
        }
    }
    start..end
}

impl Review {
    /// What `range` of `source` (this review's text) holds. `scanned`: the
    /// document was scanned, or below the phones' floor its names were
    /// decided by hand. `known` holds the names its Project knows. Refuses
    /// a range outside the text or inside a character.
    pub fn check_selection(
        &self,
        source: &str,
        range: Range<usize>,
        scanned: bool,
        known: &RestoreMap,
    ) -> Result<SelectionCheck, String> {
        checked(source, &range)?;
        if !scanned {
            return Ok(SelectionCheck::NeverScanned);
        }
        let planned = self.selection_plans(source, range.clone())?;
        // The words as "Replace N and copy" would leave them.
        let reach = planned.reach(whole_words(source, range));
        let mut after = source[reach.clone()].to_owned();
        for plan in planned.plans.iter().rev() {
            let at = plan.range.start - reach.start..plan.range.end - reach.start;
            after.replace_range(at, &plan.after);
        }
        let not = NotClean {
            proposals: planned.proposals,
            // Text the review does not hold counts as changed, as does an
            // edit whose proposals wait for discovery: a pasted paragraph
            // can drop a proposal its scanned hash still covers.
            changed_since_scan: *self.source != *source
                || self.awaiting_discovery()
                || self.unscanned_in(&reach),
            known_names: known.known_names(&after),
        };
        Ok(if not == NotClean::default() {
            SelectionCheck::Clean
        } else {
            SelectionCheck::NotClean(not)
        })
    }

    /// The plans that accept `range`'s proposals as Replace all would:
    /// every pending proposal in its whole words ([`whole_words`], as
    /// [`Self::check_selection`] counts them) or across their edges, with
    /// the applied aliases there whose identity was renamed since. Refuses
    /// a range outside the text, inside a character, or plans that overlap.
    pub fn selection_plans(
        &self,
        source: &str,
        range: Range<usize>,
    ) -> Result<SelectionPlans, String> {
        checked(source, &range)?;
        let range = whole_words(source, range);
        let touches = |plan: &ReplacementPlan| {
            overlaps(&plan.range, &range) || (range.is_empty() && plan.range.contains(&range.start))
        };
        let mut plans: Vec<_> = (self.pending_plans(source, &Default::default())?)
            .into_iter()
            .filter(touches)
            .collect();
        let proposals = plans.len();
        plans.extend(
            (self.alias_corrections(source)?)
                .into_iter()
                .filter(touches),
        );
        plans.sort_by_key(|p| p.range.start);
        if plans.windows(2).any(|p| p[0].range.end > p[1].range.start) {
            return Err("Candidates overlap. Review them again.".into());
        }
        let mut planned = SelectionPlans {
            plans,
            proposals,
            after: range.clone(),
        };
        let reach = planned.reach(range);
        let grown: isize = (planned.plans.iter())
            .map(|p| p.after.len() as isize - p.range.len() as isize)
            .sum();
        planned.after = reach.start..reach.end.saturating_add_signed(grown);
        Ok(planned)
    }
}

impl SelectionPlans {
    /// `range` widened over the plans across its edges.
    fn reach(&self, range: Range<usize>) -> Range<usize> {
        (self.plans.iter()).fold(range, |r, p| {
            r.start.min(p.range.start)..r.end.max(p.range.end)
        })
    }
}

fn checked(source: &str, range: &Range<usize>) -> Result<(), String> {
    if source.get(range.clone()).is_none() {
        return Err("The selection is not in the text.".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
