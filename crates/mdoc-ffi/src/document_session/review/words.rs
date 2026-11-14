//! A name's words (ADR 0114): what Hide name hides for a selection, and Fix
//! the words, which moves a found name's span and puts the new wording in
//! the old one's place everywhere it fits, keeping the identity and its
//! alias, as one undo step. `mdoc_pii::fix_words` decides; this drives the
//! text as desktop's document view would (put back, mark, replace again).
use super::{At, Focus, ReviewText, Step, StepKind, history::moved};
use crate::CoreError;
use mdoc_history::SourceEdit;
use mdoc_pii::{
    Category,
    fix_words::{self, WordsChange},
};
use std::{collections::HashMap, ops::Range};

/// The text as it shows while a name's words are fixed: a replaced name's
/// original is put back in place of its alias.
pub(crate) struct Draft {
    pub text: String,
    /// The name's wording in `text`.
    pub wording: Range<usize>,
    /// Other names' aliases in `text`: the handles stop before them.
    pub blocked: Vec<Range<usize>>,
    /// For a replaced name, the alias range its original took in the text.
    pub replaced: Option<Range<usize>>,
}

impl ReviewText {
    /// What Hide name hides for `selection`, snapped to whole words and
    /// trimmed, and the categories Hide as offers for it: a proposed
    /// wording's own alone, else the core's guess first, then the others in
    /// picker order.
    pub fn hide_target(
        &self,
        selection: Range<usize>,
    ) -> Result<(Range<usize>, Vec<Category>), CoreError> {
        let range = self
            .review
            .selection_target(&self.text, selection)
            .map_err(CoreError::review)?;
        let categories = match self
            .review
            .proposed_wording_category(&self.text[range.clone()])
        {
            Some(category) => vec![category],
            None => {
                let guess = mdoc_detect::guess_category(&self.text, range.clone());
                std::iter::once(guess)
                    .chain(Category::ALL.into_iter().filter(|c| *c != guess))
                    .collect()
            }
        };
        Ok((range, categories))
    }

    /// The draft Fix the words shows for the mention at `range`.
    pub(crate) fn draft(&self, range: &Range<usize>) -> Result<Draft, CoreError> {
        let (text, wording, replaced) = match self.mention(range)? {
            At::Replaced { id, .. } => {
                let original = self
                    .review
                    .applied_occurrence(id)
                    .map(|a| a.step.original().to_owned())
                    .ok_or_else(|| CoreError::review("Document changed. Review it again."))?;
                let mut text = self.text.clone();
                text.replace_range(range.clone(), &original);
                let wording = range.start..range.start + original.len();
                (text, wording, Some(range.clone()))
            }
            _ => (self.text.clone(), range.clone(), None),
        };
        let shift = wording.len() as isize - range.len() as isize;
        let blocked = self
            .review
            .applied()
            .iter()
            .filter(|a| a.range != *range)
            .map(|a| {
                if a.range.start >= range.end {
                    a.range.start.saturating_add_signed(shift)
                        ..a.range.end.saturating_add_signed(shift)
                } else {
                    a.range.clone()
                }
            })
            .collect();
        Ok(Draft {
            text,
            wording,
            blocked,
            replaced,
        })
    }

    /// How far the handles of the mention at `range` reach in its draft.
    pub(crate) fn words_bounds(draft: &Draft) -> Range<usize> {
        fix_words::words_bounds(&draft.text, draft.wording.clone(), &draft.blocked)
    }

    /// The words the handles select, `words` in the draft of the mention at
    /// `range`, as Fix the words takes them; refused with the reason.
    pub fn words_target(
        &self,
        range: &Range<usize>,
        words: Range<usize>,
    ) -> Result<(Draft, Range<usize>), CoreError> {
        let draft = self.draft(range)?;
        if words.start > words.end || words.end > draft.text.len() {
            return Err(CoreError::review("Select the words in the text."));
        }
        let target = fix_words::words_target(&draft.text, words, &draft.blocked)
            .map_err(CoreError::review)?;
        Ok((draft, target))
    }

    /// Fix the words: the mention at `range` takes `words` of its draft;
    /// the new wording replaces the old one everywhere it fits, in the
    /// mention's identity (or the one whose wording it is already), a
    /// replaced mention staying replaced and a kept one kept. One step;
    /// the same words change nothing.
    pub fn fix_words(&mut self, range: Range<usize>, words: Range<usize>) -> Result<(), CoreError> {
        let (draft, target) = self.words_target(&range, words)?;
        if target == draft.wording {
            return Ok(());
        }
        let change = WordsChange::new(&draft.text, &draft.wording, &target);
        let identity = self.entity(&self.mention(&range)?);
        let category = identity
            .and_then(|id| self.review.identity(id))
            .map(|i| i.category)
            .or_else(|| self.review.proposed_wording_category(&change.old))
            .unwrap_or(Category::Other);
        let step = Step::new(
            StepKind::FixWords,
            change.new.to_string(),
            Focus {
                range: Some(range.clone()),
                identity,
            },
        );
        self.step(step, |document, step| {
            let reach = document.review.words_reach(&document.text, &change);
            // The name's old wording, followed through each change.
            let mut focus = range;
            // Replaced mentions put back, and the identity each had.
            let mut restored: Vec<(Range<usize>, u64)> = Vec::new();
            if !reach.applied.is_empty() {
                let owners: HashMap<(usize, usize), u64> = document
                    .review
                    .applied()
                    .iter()
                    .filter(|a| reach.applied.contains(&a.id))
                    .map(|a| ((a.range.start, a.range.end), a.step.identity))
                    .collect();
                let ids = reach.applied.iter().copied().collect();
                let edits = document.revert_applied(&ids, false)?;
                let mut shift = 0isize;
                for (r, text) in &edits {
                    let start = r.start.saturating_add_signed(shift);
                    if let Some(owner) = owners.get(&(r.start, r.end)) {
                        restored.push((start..start + text.len(), *owner));
                    }
                    shift += text.len() as isize - r.len() as isize;
                }
                focus = moved(focus, &source_edits(&edits));
            }
            let source = document.text.clone();
            document
                .checkpoint(|review| {
                    review.fix_words(&source, &change, identity, category, reach.everywhere)
                })?
                .map_err(CoreError::review)?;
            let mut focus = change.range_at(&focus);
            // Replaced again, each under the identity it had.
            if !restored.is_empty() {
                let assigned: HashMap<(usize, usize), u64> = restored
                    .iter()
                    .map(|(r, owner)| {
                        let words = change.range_at(r);
                        ((words.start, words.end), *owner)
                    })
                    .collect();
                let plans: Vec<_> = document
                    .review
                    .pending_plans(&document.text, &assigned)
                    .map_err(CoreError::review)?
                    .into_iter()
                    .filter(|p| assigned.contains_key(&(p.range.start, p.range.end)))
                    .collect();
                if !plans.is_empty() {
                    let edits: Vec<_> = plans
                        .iter()
                        .map(|p| SourceEdit {
                            range: p.range.clone(),
                            new_len: p.after.len(),
                        })
                        .collect();
                    document.commit_plans(&plans, None)?;
                    document.refresh();
                    focus = moved(focus, &edits);
                }
            }
            step.after = Focus {
                identity: document.at(&focus).and_then(|at| document.entity(&at)),
                range: Some(focus),
            };
            Ok(())
        })
    }
}

fn source_edits(edits: &[(Range<usize>, String)]) -> Vec<SourceEdit> {
    edits
        .iter()
        .map(|(range, text)| SourceEdit {
            range: range.clone(),
            new_len: text.len(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::Scope;
    use super::*;
    use mdoc_pii::{Detection, Recognizer};

    fn at(text: &str, words: &str, nth: usize) -> Range<usize> {
        let start = text.match_indices(words).nth(nth).unwrap().0;
        start..start + words.len()
    }

    /// `text` with these spans proposed, as a scan proposes them.
    fn document(text: &str, spans: &[(Range<usize>, Category)]) -> ReviewText {
        let mut document = ReviewText::new(text.into());
        let detections = spans
            .iter()
            .map(|(range, category)| Detection {
                range: range.clone(),
                category: *category,
                score: 0.9,
                recognizer: Recognizer::Model,
            })
            .collect();
        document.ingest(detections).unwrap();
        document
    }

    /// The mentions' words in the text, in order, with their aliases.
    fn mentions(document: &ReviewText) -> Vec<(String, String)> {
        let mut mentions: Vec<_> = document
            .review
            .candidates()
            .iter()
            .map(|c| {
                let identity = document.review.occurrence_identity(c.variant, &c.range);
                (c.range.clone(), identity)
            })
            .chain(
                document
                    .review
                    .applied()
                    .iter()
                    .map(|a| (a.range.clone(), Some(a.step.identity))),
            )
            .chain(
                document
                    .review
                    .kept_mentions()
                    .into_iter()
                    .map(|(range, _, identity)| (range, identity)),
            )
            .collect();
        mentions.sort_by_key(|(r, _)| r.start);
        mentions
            .into_iter()
            .map(|(range, identity)| {
                let alias = identity
                    .and_then(|id| document.review.identity(id))
                    .map(|i| i.alias.clone())
                    .unwrap_or_default();
                (document.text[range].to_owned(), alias)
            })
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    const LONG: &str = "ФИО Тагиев Тебриз Фарадж оглы";
    const NAME: &str = "Тагиев Тебриз Фарадж оглы";

    fn two_mentions() -> (String, ReviewText) {
        let text = format!("{LONG} подписал. Позже {LONG} ушёл, а Ann Lee остался.");
        let document = document(
            &text,
            &[
                (at(&text, LONG, 0), Category::Person),
                (at(&text, "Ann Lee", 0), Category::Person),
            ],
        );
        (text, document)
    }

    #[test]
    fn narrowed_words_replace_the_old_wording_at_every_repeat() {
        let (text, mut document) = two_mentions();
        assert_eq!(
            mentions(&document),
            pairs(&[
                (LONG, "PERSON_1"),
                (LONG, "PERSON_1"),
                ("Ann Lee", "PERSON_2")
            ])
        );
        let first = at(&text, LONG, 0);
        document
            .fix_words(first.clone(), at(&text, NAME, 0))
            .unwrap();
        assert_eq!(document.text, text, "proposals change no text");
        assert_eq!(
            mentions(&document),
            pairs(&[
                (NAME, "PERSON_1"),
                (NAME, "PERSON_1"),
                ("Ann Lee", "PERSON_2")
            ])
        );
        // The identity lists the new wording, not the old one.
        let rows = document.review.export_identities();
        let person = rows.iter().find(|r| r.alias == "PERSON_1").unwrap();
        assert_eq!(
            person
                .originals
                .iter()
                .map(|o| o.as_ref())
                .collect::<Vec<_>>(),
            [NAME]
        );
        // One step, named for the new words, focused on them.
        let (step, focus) = document.undo_step().unwrap();
        assert_eq!(step.kind, StepKind::FixWords);
        assert_eq!(step.label(), format!("Fix the words · {NAME}"));
        assert_eq!(focus.range, Some(at(&text, NAME, 0)));
        assert!(document.undo());
        assert_eq!(
            mentions(&document),
            pairs(&[
                (LONG, "PERSON_1"),
                (LONG, "PERSON_1"),
                ("Ann Lee", "PERSON_2")
            ])
        );
        assert!(!document.can_undo());
        assert!(document.redo());
        assert_eq!(
            mentions(&document),
            pairs(&[
                (NAME, "PERSON_1"),
                (NAME, "PERSON_1"),
                ("Ann Lee", "PERSON_2")
            ])
        );
    }

    #[test]
    fn a_replaced_name_is_fixed_from_its_original_and_stays_replaced() {
        let (_, mut document) = two_mentions();
        document.replace_all().unwrap();
        assert_eq!(
            document.text,
            "PERSON_1 подписал. Позже PERSON_1 ушёл, а PERSON_2 остался."
        );
        let alias = at(&document.text.clone(), "PERSON_1", 1);
        // The draft shows the original back where the alias was.
        let draft = document.draft(&alias).unwrap();
        assert_eq!(&draft.text[draft.wording.clone()], LONG);
        assert_eq!(draft.replaced, Some(alias.clone()));
        let words = at(&draft.text, NAME, 0);
        document.fix_words(alias, words).unwrap();
        assert_eq!(
            document.text,
            "ФИО PERSON_1 подписал. Позже ФИО PERSON_1 ушёл, а PERSON_2 остался."
        );
        assert_eq!(
            mentions(&document),
            pairs(&[
                ("PERSON_1", "PERSON_1"),
                ("PERSON_1", "PERSON_1"),
                ("PERSON_2", "PERSON_2")
            ])
        );
        assert!(document.review.candidates().is_empty());
        let restored: Vec<_> = document
            .review
            .applied()
            .iter()
            .map(|a| a.step.original().to_owned())
            .collect();
        assert_eq!(restored, [NAME, NAME, "Ann Lee"]);
        // Undo takes the text back in one step.
        assert!(document.undo());
        assert_eq!(
            document.text,
            "PERSON_1 подписал. Позже PERSON_1 ушёл, а PERSON_2 остался."
        );
        assert_eq!(document.review.applied()[0].step.original(), LONG);
    }

    #[test]
    fn widened_words_leave_the_old_wording_where_they_do_not_fit() {
        let text = "Ann signed. Ann Lee paid. Ann left.";
        let ann: Vec<_> = (0..3)
            .map(|n| (at(text, "Ann", n), Category::Person))
            .collect();
        let mut document = document(text, &ann);
        document
            .fix_words(at(text, "Ann", 1), at(text, "Ann Lee", 0))
            .unwrap();
        assert_eq!(
            mentions(&document),
            pairs(&[
                ("Ann", "PERSON_1"),
                ("Ann Lee", "PERSON_1"),
                ("Ann", "PERSON_1")
            ])
        );
    }

    #[test]
    fn a_kept_name_stays_kept_as_its_new_words() {
        let (text, mut document) = two_mentions();
        let first = at(&text, LONG, 0);
        document.keep(first.clone(), Scope::This).unwrap();
        document.fix_words(first, at(&text, NAME, 0)).unwrap();
        let kept: Vec<_> = document
            .review
            .kept_mentions()
            .into_iter()
            .map(|(range, original, _)| (document.text[range].to_owned(), original.to_string()))
            .collect();
        assert_eq!(kept, [(NAME.to_owned(), NAME.to_owned())]);
        assert_eq!(
            mentions(&document),
            pairs(&[
                (NAME, "PERSON_1"),
                (NAME, "PERSON_1"),
                ("Ann Lee", "PERSON_2")
            ])
        );
    }

    #[test]
    fn words_that_are_another_identitys_wording_join_it() {
        let text = "Acme Ltd sued. The Acme Ltd board met.";
        let mut document = document(
            text,
            &[
                (at(text, "Acme Ltd", 0), Category::Organization),
                (at(text, "The Acme Ltd board", 0), Category::Organization),
            ],
        );
        assert_eq!(
            mentions(&document),
            pairs(&[("Acme Ltd", "ORG_1"), ("The Acme Ltd board", "ORG_2")])
        );
        document
            .fix_words(at(text, "The Acme Ltd board", 0), at(text, "Acme Ltd", 1))
            .unwrap();
        assert_eq!(
            mentions(&document),
            pairs(&[("Acme Ltd", "ORG_1"), ("Acme Ltd", "ORG_1")])
        );
        assert_eq!(document.review.active_identities().len(), 1);
        // Undo parts them again.
        assert!(document.undo());
        assert_eq!(
            mentions(&document),
            pairs(&[("Acme Ltd", "ORG_1"), ("The Acme Ltd board", "ORG_2")])
        );
    }

    #[test]
    fn nothing_to_hide_or_the_same_words_make_no_step() {
        let (text, mut document) = two_mentions();
        let first = at(&text, LONG, 0);
        let space = first.end..first.end + 1;
        assert!(document.fix_words(first.clone(), space).is_err());
        // Across the sentence's formatting-free text into the next is fine,
        // but the same words change nothing.
        document.fix_words(first.clone(), first.clone()).unwrap();
        assert!(!document.can_undo());
        let mut formatted = document_of("Signed **by** Ann Lee.", "Ann Lee");
        let name = at("Signed **by** Ann Lee.", "Ann Lee", 0);
        assert!(
            formatted
                .fix_words(name, at("Signed **by** Ann Lee.", "by** Ann Lee", 0))
                .is_err()
        );
        assert!(!formatted.can_undo());
    }

    fn document_of(text: &str, name: &str) -> ReviewText {
        document(text, &[(at(text, name, 0), Category::Person)])
    }

    #[test]
    fn hide_name_snaps_trims_and_offers_the_guess_first() {
        let text = "Signed: (Ann Lee), clerk.";
        let document = ReviewText::new(text.into());
        let selection = at(text, "(Ann Le", 0);
        let (range, categories) = document.hide_target(selection).unwrap();
        assert_eq!(&text[range], "Ann Lee");
        assert_eq!(categories.len(), Category::ALL.len());
        let mut sorted = categories.clone();
        sorted.sort_by_key(|c| Category::ALL.iter().position(|a| a == c));
        assert_eq!(sorted, Category::ALL);
        assert!(document.hide_target(at(text, "), ", 0)).is_err());
        // A proposed wording keeps its own category, alone.
        let (text, document) = two_mentions();
        let (_, categories) = document.hide_target(at(&text, "Ann Lee", 0)).unwrap();
        assert_eq!(categories, [Category::Person]);
    }

    #[test]
    fn a_marked_selection_is_trimmed_to_its_words() {
        let text = "Call (Mia), then Mia.";
        let mut document = ReviewText::new(text.into());
        let marked = document
            .add_selection(at(text, "(Mia), ", 0), Some(Category::Person))
            .unwrap();
        assert_eq!(&text[marked], "Mia");
        assert_eq!(document.review.remaining(), 2);
    }
}
