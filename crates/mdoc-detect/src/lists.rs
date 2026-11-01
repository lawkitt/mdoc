//! The user's lists (ADR 0092), the last scan-time stage: a proposal whose
//! wording is on Never goes, and every whole-word mention of an Always
//! wording becomes one, ignoring case and whitespace. A person's Always
//! entry is a full name like any proposal, so the review's propagation
//! (`mdoc_pii::propagation`) proposes its forms when the scan is ingested.
use mdoc_pii::{Detection, Recognizer};
use mdoc_store::lists::{Lists, key};
use std::ops::Range;

/// The detections a scan returns after `lists`: the case's lists over the
/// user-wide ones, or the user-wide ones alone ([`Lists::effective`]).
/// Pure, so the same source, detections and lists give the same result.
pub fn apply(source: &str, mut detections: Vec<Detection>, lists: &Lists) -> Vec<Detection> {
    if lists.is_empty() {
        return detections;
    }
    detections.retain(|d| {
        source
            .get(d.range.clone())
            .is_none_or(|text| !lists.never_has(text))
    });
    for entry in &lists.always {
        let category = entry.category();
        detections.extend(
            mentions(source, &entry.wording)
                .into_iter()
                .map(|range| Detection {
                    range,
                    category,
                    score: 1.0,
                    recognizer: Recognizer::UserList,
                }),
        );
    }
    detections
}

/// Whole-word mentions of `wording` in `source`, ignoring case, with any
/// run of whitespace matching any other.
pub fn mentions(source: &str, wording: &str) -> Vec<Range<usize>> {
    let words: Vec<&str> = wording.split_whitespace().collect();
    if key(wording).is_empty() {
        return Vec::new();
    }
    let edge = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let first = words.first().and_then(|w| w.chars().next());
    let last = words.last().and_then(|w| w.chars().next_back());
    let pattern = format!(
        "(?i){}{}{}",
        if edge(first) { r"\b" } else { "" },
        words
            .iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join(r"\s+"),
        if edge(last) { r"\b" } else { "" },
    );
    let Ok(regex) = regex::Regex::new(&pattern) else {
        return Vec::new();
    };
    regex.find_iter(source).map(|m| m.range()).collect()
}

#[cfg(test)]
mod tests {
    use super::{apply, mentions};
    use crate::{RawEntity, correct};
    use mdoc_pii::{Category, Recognizer, Review};
    use mdoc_store::lists::Lists;

    fn raw(source: &str, text: &str, label: &str) -> RawEntity {
        let start = source.find(text).unwrap();
        RawEntity {
            start,
            end: start + text.len(),
            label: label.into(),
            score: 0.8,
            text: text.into(),
        }
    }
    fn texts<'a>(source: &'a str, review: &Review) -> Vec<&'a str> {
        review
            .candidates()
            .iter()
            .map(|c| &source[c.range.clone()])
            .collect()
    }

    #[test]
    fn mentions_are_whole_words_ignoring_case_and_whitespace() {
        let source = "ACME  Ltd signed; acme\nltd paid. Acmeltd and AcmeLtd Co did not.";
        let found: Vec<_> = mentions(source, "Acme Ltd")
            .into_iter()
            .map(|r| &source[r])
            .collect();
        assert_eq!(found, ["ACME  Ltd", "acme\nltd"]);
        assert!(mentions(source, "cme").is_empty());
        assert!(mentions(source, "  ").is_empty());
        // Cyrillic folds too, and punctuation inside a wording is literal.
        let ru = "Иск ООО «Ромашка» и ооо «ромашка».";
        assert_eq!(mentions(ru, "ООО «Ромашка»").len(), 2);
    }

    #[test]
    fn a_never_entry_suppresses_a_proposal() {
        let source = "The Supreme Court heard Maya Bennett.";
        let detections = correct(
            source,
            &[
                raw(source, "Supreme Court", "organization"),
                raw(source, "Maya Bennett", "person"),
            ],
        );
        let mut lists = Lists::default();
        lists.add_never("supreme   COURT");
        let mut review = Review::default();
        review
            .ingest(source, apply(source, detections, &lists))
            .unwrap();
        assert_eq!(texts(source, &review), ["Maya Bennett"]);
    }

    #[test]
    fn an_always_entry_adds_a_proposal_and_a_persons_yields_its_forms() {
        let source = "Northwind paid. Maya Bennett signed; later Ms Bennett and NORTHWIND met.";
        let mut lists = Lists::default();
        lists.add_always("Northwind", Category::Organization);
        lists.add_always("maya bennett", Category::Person);
        let detections = apply(source, correct(source, &[]), &lists);
        assert!(
            detections
                .iter()
                .all(|d| d.recognizer == Recognizer::UserList)
        );
        let mut review = Review::default();
        review.ingest(source, detections).unwrap();
        let found = texts(source, &review);
        assert_eq!(
            found,
            ["Northwind", "Maya Bennett", "Ms Bennett", "NORTHWIND"]
        );
        let ms = review
            .candidates()
            .iter()
            .find(|c| &source[c.range.clone()] == "Ms Bennett")
            .unwrap();
        assert_eq!(
            review.variant(ms.variant).unwrap().recognizer(),
            Some(Recognizer::Propagation)
        );
    }

    #[test]
    fn no_lists_leave_the_scan_alone() {
        let source = "Maya Bennett";
        let detections = correct(source, &[raw(source, "Maya Bennett", "person")]);
        assert_eq!(
            apply(source, detections.clone(), &Lists::default()).len(),
            detections.len()
        );
    }
}
