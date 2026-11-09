//! Helpers the layers' tests share.
use super::*;
use mdoc_pii::Recognizer;

/// What the review proposes for `source` without a model: each span's
/// text and category token.
pub(super) fn proposed(source: &str) -> Vec<(String, String)> {
    crate::recognition::proposals(source, &[])
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect()
}

/// The case numbers the review shows for `source` without a model.
pub(super) fn cases(source: &str) -> Vec<String> {
    crate::recognition::proposals(source, &[])
        .unwrap()
        .into_iter()
        .filter(|span| span.category == "CASE")
        .map(|span| source[span.range].to_owned())
        .collect()
}

pub(super) fn fixed(source: &str, spans: &[(&str, Category)]) -> Vec<String> {
    let mut detections: Vec<Detection> = spans
        .iter()
        .map(|(text, category)| {
            let start = source.find(text).unwrap();
            Detection {
                range: start..start + text.len(),
                category: *category,
                score: 0.9,
                recognizer: Recognizer::Model,
            }
        })
        .collect();
    pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
    detections
        .into_iter()
        .map(|d| source[d.range].to_owned())
        .collect()
}

/// The organization proposals for the model's `orgs` spans in `source`,
/// each found after the one before it, as the user would be shown them.
pub(super) fn proposed_orgs(source: &str, orgs: &[&str]) -> Vec<String> {
    let mut from = 0;
    let raw: Vec<_> = orgs
        .iter()
        .map(|text| {
            let start = from + source[from..].find(text).unwrap();
            from = start + text.len();
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: "organization".into(),
                score: 0.9,
                text: (*text).into(),
            }
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .filter(|span| span.category == "ORG")
        .map(|span| source[span.range].to_owned())
        .collect()
}

/// The model's spans, each found after the one before it, as the user
/// would be shown them: `CATEGORY⟦text⟧`.
pub(super) fn proposed_marks(source: &str, model: &[(&str, &str)]) -> Vec<String> {
    let mut from = 0;
    let raw: Vec<_> = model
        .iter()
        .map(|(label, text)| {
            let start = from + source[from..].find(text).unwrap();
            from = start + text.len();
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: (*label).into(),
                score: 0.9,
                text: (*text).into(),
            }
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| format!("{}⟦{}⟧", span.category, &source[span.range]))
        .collect()
}

/// What the user is shown for the model's `raw` spans (text and label,
/// each found at its first place): the correction stage and the review.
pub(super) fn shown(source: &str, raw: &[(&str, &str)]) -> Vec<(String, String)> {
    let raw: Vec<crate::RawEntity> = raw
        .iter()
        .map(|(text, label)| {
            let start = source
                .find(text)
                .unwrap_or_else(|| panic!("{text:?} not in the source"));
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: (*label).into(),
                score: 0.8,
                text: (*text).into(),
            }
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect()
}

pub(super) fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(text, category)| ((*text).to_owned(), (*category).to_owned()))
        .collect()
}

/// [`shown`] for spans each at the `n`th place of its text (from 0).
pub(super) fn shown_at(source: &str, raw: &[(&str, usize, &str)]) -> Vec<(String, String)> {
    let raw: Vec<crate::RawEntity> = raw
        .iter()
        .map(|(text, n, label)| {
            let (start, _) = source
                .match_indices(text)
                .nth(*n)
                .unwrap_or_else(|| panic!("{text:?} #{n} not in the source"));
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: (*label).into(),
                score: 0.8,
                text: (*text).into(),
            }
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect()
}
