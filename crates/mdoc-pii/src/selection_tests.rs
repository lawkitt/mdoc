use super::*;
use crate::restore::{ExportedIdentity, tests::apply_pending};
use crate::{Category, Detection, Recognizer};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};
use std::sync::Arc;

const LETTER: &str =
    "Ann Lee signed the lease.\n\nBob Hart paid the rent.\n\nThe term ends in May.";

fn person(source: &str, name: &str) -> Detection {
    let start = source.find(name).unwrap();
    Detection {
        range: start..start + name.len(),
        category: Category::Person,
        score: 0.9,
        recognizer: Recognizer::Model,
    }
}

/// `LETTER` scanned, Ann Lee and Bob Hart proposed.
fn proposed() -> Review {
    let mut review = Review::default();
    let found = vec![person(LETTER, "Ann Lee"), person(LETTER, "Bob Hart")];
    review.ingest(LETTER, found).unwrap();
    review
}

/// `LETTER` scanned with both names replaced: history state 1.
fn replaced() -> (Review, String) {
    let mut review = proposed();
    let text = apply_pending(&mut review, LETTER, 1);
    assert_eq!(
        text,
        "PERSON_1 signed the lease.\n\nPERSON_2 paid the rent.\n\nThe term ends in May."
    );
    (review, text)
}

fn at(text: &str, words: &str) -> Range<usize> {
    let start = text.find(words).unwrap();
    start..start + words.len()
}

fn known(names: &[(&str, &str)]) -> RestoreMap {
    RestoreMap::new(
        (names.iter())
            .map(|(alias, original)| ExportedIdentity {
                alias: (*alias).into(),
                previous: Vec::new(),
                category: Category::Person,
                custom: false,
                originals: vec![Arc::from(*original)],
            })
            .collect(),
    )
}

fn not_clean(proposals: usize, changed_since_scan: bool, known_names: usize) -> SelectionCheck {
    SelectionCheck::NotClean(NotClean {
        proposals,
        changed_since_scan,
        known_names,
    })
}

/// A document never scanned is "Copy with names" alone, whatever it holds.
#[test]
fn a_document_never_scanned_is_never_clean() {
    let review = proposed();
    let none = RestoreMap::default();
    for range in [at(LETTER, "Ann Lee signed"), at(LETTER, "The term")] {
        let check = review.check_selection(LETTER, range, false, &none);
        assert_eq!(check, Ok(SelectionCheck::NeverScanned));
    }
}

/// Scanned, every name replaced, nothing edited: every selection is clean.
#[test]
fn a_scanned_replaced_selection_is_clean() {
    let (review, text) = replaced();
    let map = known(&[("PERSON_1", "Ann Lee"), ("PERSON_2", "Bob Hart")]);
    for range in [
        0..text.len(),
        at(&text, "PERSON_1 signed"),
        at(&text, "May"),
    ] {
        let check = review.check_selection(&text, range, true, &map);
        assert_eq!(check, Ok(SelectionCheck::Clean));
    }
}

/// Pending proposals in a selection, or across its edges, are its N; a
/// selection without them is clean.
#[test]
fn proposals_in_a_selection_are_its_names_not_replaced() {
    let review = proposed();
    let map = known(&[("PERSON_1", "Ann Lee"), ("PERSON_2", "Bob Hart")]);
    let check = |range| review.check_selection(LETTER, range, true, &map);
    assert_eq!(check(0..LETTER.len()), Ok(not_clean(2, false, 0)));
    assert_eq!(check(at(LETTER, "Lee signed")), Ok(not_clean(1, false, 0)));
    assert_eq!(
        check(at(LETTER, "signed the lease")),
        Ok(SelectionCheck::Clean)
    );
    assert_eq!(check(at(LETTER, "The term")), Ok(SelectionCheck::Clean));
}

/// Accepting a selection's proposals replaces those alone, and its range
/// widens over a proposal across its edge, so the copy holds the alias
/// whole. Once accepted, the selection is clean.
#[test]
fn accepting_a_selection_replaces_its_proposals_alone() {
    let mut review = proposed();
    let planned = review
        .selection_plans(LETTER, at(LETTER, "Lee signed"))
        .unwrap();
    assert_eq!(planned.proposals, 1);
    let edited: Vec<_> = planned.plans.iter().map(|p| p.before.as_ref()).collect();
    assert_eq!(edited, ["Ann Lee"]);
    let added = review.prepare_replacements(&planned.plans);
    let mut text = LETTER.to_owned();
    for plan in planned.plans.iter().rev() {
        text.replace_range(plan.range.clone(), &plan.after);
    }
    review.on_transaction(
        &EditorTransaction {
            revision: 1,
            changes: vec![HistoryChange {
                before: 0,
                after: 1,
                edits: (planned.plans.iter())
                    .map(|p| SourceEdit {
                        range: p.range.clone(),
                        new_len: p.after.len(),
                    })
                    .collect(),
            }],
            retained: vec![0, 1],
        },
        &text,
    );
    review.commit_replacements(1, added);
    review.refresh(&text);
    assert_eq!(&text[planned.after.clone()], "PERSON_1 signed");
    assert_eq!(review.remaining(), 1, "Bob Hart is still proposed");
    let map = known(&[("PERSON_1", "Ann Lee")]);
    let check = review.check_selection(&text, planned.after, true, &map);
    assert_eq!(check, Ok(SelectionCheck::Clean));
}

/// Text typed since the scan makes its paragraph's selections not clean
/// until a scan of the edit; the other paragraphs stay clean.
#[test]
fn text_changed_since_its_scan_is_not_clean() {
    let (mut review, mut text) = replaced();
    let typed = text.find(" paid").unwrap();
    text.insert_str(typed, " gladly");
    review.on_transaction(
        &EditorTransaction {
            revision: 2,
            changes: vec![HistoryChange {
                before: 1,
                after: 2,
                edits: vec![SourceEdit {
                    range: typed..typed,
                    new_len: " gladly".len(),
                }],
            }],
            retained: vec![1, 2],
        },
        &text,
    );
    review.refresh(&text);
    let none = RestoreMap::default();
    let check = |review: &Review, range| review.check_selection(&text, range, true, &none);
    assert_eq!(
        check(&review, at(&text, "the rent")),
        Ok(not_clean(0, true, 0))
    );
    assert_eq!(check(&review, 0..text.len()), Ok(not_clean(0, true, 0)));
    assert_eq!(
        check(&review, at(&text, "the lease")),
        Ok(SelectionCheck::Clean)
    );
    let edited = review.edited_text().unwrap();
    assert!(review.ingest_edited(&text, &edited, Vec::new()).unwrap());
    assert_eq!(
        check(&review, at(&text, "the rent")),
        Ok(SelectionCheck::Clean)
    );
}

/// A name the Project knows, written out and not proposed (a Keep, or a
/// name another document added), is not clean, even cut into; replacing
/// the selection's proposals leaves it.
#[test]
fn a_known_name_written_out_is_not_clean() {
    let (review, text) = replaced();
    let text_with = text.replace("The term", "Maya Bennett's term");
    let mut review = review;
    review.refresh(&text_with);
    let map = known(&[("PERSON_1", "Ann Lee"), ("PERSON_9", "Maya Bennett")]);
    let check = |range| review.check_selection(&text_with, range, true, &map);
    assert_eq!(
        check(at(&text_with, "Maya Bennett's")).unwrap(),
        not_clean(0, true, 1)
    );
    let cut = at(&text_with, "ya Benn");
    let SelectionCheck::NotClean(not) = check(cut).unwrap() else {
        panic!("a cut name is still the name");
    };
    assert_eq!(not.known_names, 1);

    // Proposed, the name is N, and once replaced it is no known name left.
    let review = proposed();
    let check = review.check_selection(LETTER, at(LETTER, "Ann Lee"), true, &map);
    assert_eq!(check, Ok(not_clean(1, false, 0)));
    let kept = "Ann Lee signed.";
    let mut plain = Review::default();
    plain.refresh(kept);
    let check = plain.check_selection(kept, 0..kept.len(), true, &map);
    assert_eq!(
        check,
        Ok(not_clean(0, false, 1)),
        "written out, not proposed"
    );
}

/// Widening to whole words never shrinks a selection nor adds a line break.
#[test]
fn whole_words_never_shrink_nor_cross_a_line() {
    let source = "Ann Lee\n\nBob-Hart  Jos\u{0065}\u{0301} paid.\nМарина";
    let bounds: Vec<usize> = (0..=source.len())
        .filter(|&i| source.is_char_boundary(i))
        .collect();
    for (i, &start) in bounds.iter().enumerate() {
        for &end in &bounds[i..] {
            let words = whole_words(source, start..end);
            assert!(words.start <= start && end <= words.end, "{start}..{end}");
            let added = [&source[words.start..start], &source[end..words.end]];
            assert!(!added.iter().any(|a| a.contains('\n')), "{start}..{end}");
            assert!(!added.iter().any(|a| a.contains(' ')), "{start}..{end}");
        }
    }
    let cut = at(source, "ee\n\nBo");
    assert_eq!(&source[whole_words(source, cut)], "Lee\n\nBob");
    let mark = source.find('\u{0301}').unwrap();
    assert_eq!(&source[whole_words(source, mark..mark)], "Jose\u{0301}");
}

/// A range outside the text or inside a character is refused.
#[test]
fn a_range_outside_the_text_is_refused() {
    let review = proposed();
    let none = RestoreMap::default();
    let len = LETTER.len();
    assert!(
        review
            .check_selection(LETTER, len..len + 1, true, &none)
            .is_err()
    );
    let reversed = Range { start: 4, end: 2 };
    assert!(review.selection_plans(LETTER, reversed).is_err());
    let cyrillic = "Марина";
    assert!(Review::default().selection_plans(cyrillic, 1..2).is_err());
}
