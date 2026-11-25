use super::*;
use crate::restore::tests::apply_pending;
use crate::{Category, Recognizer};
use mdoc_history::{EditorTransaction, HistoryChange, SourceEdit};

const LETTER: &str = "Ann Lee signed the lease.\n\nThe term ends in May.";

fn person(source: &str, name: &str) -> Detection {
    let start = source.find(name).unwrap();
    Detection {
        range: start..start + name.len(),
        category: Category::Person,
        score: 0.9,
        recognizer: Recognizer::Model,
    }
}

/// `LETTER` scanned with Ann Lee found and replaced: revision 1.
fn replaced() -> (Review, String) {
    let mut review = Review::default();
    review
        .ingest(LETTER, vec![person(LETTER, "Ann Lee")])
        .unwrap();
    let text = apply_pending(&mut review, LETTER, 1);
    assert_eq!(text, "PERSON_1 signed the lease.\n\nThe term ends in May.");
    (review, text)
}

/// Types `insert` at `at` as history state `revision`.
fn type_at(review: &mut Review, text: &mut String, at: usize, insert: &str, revision: u64) {
    text.insert_str(at, insert);
    review.on_transaction(
        &EditorTransaction {
            revision,
            changes: vec![HistoryChange {
                before: revision - 1,
                after: revision,
                edits: vec![SourceEdit {
                    range: at..at,
                    new_len: insert.len(),
                }],
            }],
            retained: vec![revision - 1, revision],
        },
        text,
    );
    review.refresh(text);
}

/// Undo or redo to `text`, as history travel arrives: no edits.
fn travel(review: &mut Review, to: &str, revision: u64, after: u64) {
    review.on_transaction(
        &EditorTransaction {
            revision,
            changes: vec![HistoryChange {
                before: revision - 1,
                after,
                edits: Vec::new(),
            }],
            retained: vec![after],
        },
        to,
    );
    review.refresh(to);
}

/// What a scan of the edited paragraphs finds: `names` wherever the
/// excerpt holds them.
fn scan(edited: &EditedText, names: &[&str]) -> Vec<Detection> {
    names
        .iter()
        .filter(|name| edited.text().contains(*name))
        .map(|name| person(edited.text(), name))
        .collect()
}

#[test]
fn paragraphs_are_the_lines_between_blank_lines() {
    let source = "# Title\nFirst line\n  \n- a\n- b\n\r\n\n| x |\n| y |";
    let found: Vec<_> = (paragraphs(source).into_iter())
        .map(|range| &source[range])
        .collect();
    assert_eq!(found, ["# Title\nFirst line", "- a\n- b", "| x |\n| y |"]);
    assert!(paragraphs("\n\n").is_empty());
}

/// A name typed into a scanned document is owed a scan of its paragraph
/// alone; found, it is a proposal, and the document has one left to decide.
#[test]
fn a_typed_new_name_becomes_a_proposal() {
    let (mut review, mut text) = replaced();
    assert!(!review.edits_unscanned(), "a replacement is no edit");
    let at = text.len();
    type_at(&mut review, &mut text, at, " Bob Hart agreed.", 2);
    assert!(review.edits_unscanned());
    assert_eq!(review.remaining(), 0, "discovery knows only Ann Lee");
    let edited = review.edited_text().unwrap();
    assert_eq!(edited.text(), "The term ends in May. Bob Hart agreed.");
    let found = scan(&edited, &["Bob Hart"]);
    assert!(review.ingest_edited(&text, &edited, found).unwrap());
    assert!(!review.edits_unscanned());
    let proposed: Vec<_> = (review.candidates().iter())
        .map(|c| &text[c.range.clone()])
        .collect();
    assert_eq!(proposed, ["Bob Hart"]);
    assert_eq!(review.applied().len(), 1, "Ann Lee stays replaced");
}

/// An edit that adds no name is owed its scan too, and once that finds
/// nothing, nothing is left: the document is fully reviewed again.
#[test]
fn an_edit_with_no_new_name_leaves_nothing_to_decide() {
    let (mut review, mut text) = replaced();
    let at = text.find(" the lease").unwrap();
    type_at(&mut review, &mut text, at, " today", 2);
    let edited = review.edited_text().unwrap();
    assert_eq!(edited.text(), "PERSON_1 signed today the lease.");
    assert!(review.ingest_edited(&text, &edited, Vec::new()).unwrap());
    assert!(!review.edits_unscanned() && !review.awaiting_discovery());
    assert_eq!(review.remaining(), 0);
}

/// Undo back to the scanned text owes nothing; Redo owes the edit again.
#[test]
fn undo_to_scanned_text_owes_nothing() {
    let (mut review, mut text) = replaced();
    let scanned = text.clone();
    let at = text.len();
    type_at(&mut review, &mut text, at, " Bob Hart agreed.", 2);
    travel(&mut review, &scanned, 3, 1);
    assert!(!review.edits_unscanned());
    travel(&mut review, &text, 4, 2);
    assert!(review.edits_unscanned());
}

/// A paragraph copied or moved is text already scanned: it owes nothing.
#[test]
fn a_copied_paragraph_owes_nothing() {
    let (mut review, mut text) = replaced();
    let at = text.len();
    type_at(&mut review, &mut text, at, "\n\nThe term ends in May.", 2);
    assert!(!review.edits_unscanned());
}

/// A scan of text that changed since it was read takes nothing; the
/// paragraphs stay owed. A find across two paragraphs is dropped.
#[test]
fn a_stale_scan_takes_nothing_and_seams_drop_finds() {
    let (mut review, mut text) = replaced();
    type_at(&mut review, &mut text, 0, "Dear Bob\n\n", 2);
    let at = text.len();
    type_at(&mut review, &mut text, at, " Hart wrote.", 3);
    let edited = review.edited_text().unwrap();
    assert_eq!(
        edited.text(),
        "Dear Bob\n\nThe term ends in May. Hart wrote."
    );
    let stale = text.clone();
    type_at(&mut review, &mut text, 0, "x", 4);
    assert!(!review.ingest_edited(&text, &edited, Vec::new()).unwrap());
    assert!(!review.ingest_edited(&stale, &edited, Vec::new()).unwrap());
    assert!(review.edits_unscanned());

    let edited = review.edited_text().unwrap();
    let across = person(edited.text(), "Bob\n\nThe");
    assert!(review.ingest_edited(&text, &edited, vec![across]).unwrap());
    assert_eq!(review.remaining(), 0);
    assert!(!review.edits_unscanned());
}

/// A saved review counts its text as scanned; `owe_whole_text` takes that
/// back, for a document that may have been left mid-edit.
#[test]
fn a_restored_review_owes_nothing_until_told_otherwise() {
    let (review, text) = replaced();
    let mut restored = Review::restore(review.save(), &text, 1).unwrap();
    assert!(!restored.edits_unscanned());
    restored.owe_whole_text();
    let edited = restored.edited_text().unwrap();
    assert_eq!(edited.text(), text);
    assert!(restored.ingest_edited(&text, &edited, Vec::new()).unwrap());
    assert!(!restored.edits_unscanned());
}

/// Before any text, nothing is owed; the first text the review takes counts
/// as scanned.
#[test]
fn the_first_text_counts_as_scanned() {
    let mut review = Review::default();
    assert!(!review.edits_unscanned());
    let mut text = "Plain text.".to_string();
    review.refresh(&text);
    assert!(!review.edits_unscanned());
    type_at(&mut review, &mut text, 0, "More. ", 1);
    assert!(review.edits_unscanned());
}
