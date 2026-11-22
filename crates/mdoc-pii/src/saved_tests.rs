use super::*;
use crate::{Detection, restore::ExportedIdentity};
use std::collections::HashSet;

const SOURCE: &str = "PERSON_1 met Ann and Bob. Ann left. Carl stayed. Dana came.";

fn at(wording: &str, nth: usize) -> Range<usize> {
    let start = SOURCE.match_indices(wording).nth(nth).unwrap().0;
    start..start + wording.len()
}

fn person(wording: &str, nth: usize) -> Detection {
    Detection {
        range: at(wording, nth),
        category: Category::Person,
        score: 0.9,
        recognizer: Recognizer::Model,
    }
}

/// A review holding every kind of decision: an applied replacement, pending
/// proposals, a Keep, a Not a name, an occurrence override and a renamed
/// identity.
fn decided() -> Review {
    let mut review = Review::default();
    let known = [ExportedIdentity {
        alias: "PERSON_1".into(),
        previous: Vec::new(),
        category: Category::Person,
        custom: false,
        originals: vec!["Alice Moss".into()],
    }];
    let ids = review.seed(&known, SOURCE);
    assert_eq!(review.relink(SOURCE, &known, &ids, 0), 1);
    let detections = vec![
        person("Ann", 0),
        person("Bob", 0),
        person("Carl", 0),
        person("Dana", 0),
    ];
    review.ingest(SOURCE, detections).unwrap();
    let group = |review: &Review, wording: &str| {
        review
            .variants()
            .iter()
            .find(|variant| variant.original.as_ref() == wording)
            .unwrap()
            .id
    };
    let carl = group(&review, "Carl");
    review.keep(carl, Some(at("Carl", 0)));
    let dana = group(&review, "Dana");
    review.not_a_name_mentions([(dana, at("Dana", 0))], &HashSet::new());
    let other = review.new_identity(Category::Person);
    review.commit_assignment(0, at("Ann", 1), other);
    let bob = review.variant_identity(group(&review, "Bob")).unwrap();
    review.rename_identity(bob, "WITNESS").unwrap();
    review
}

/// What a reopened document shows of its review.
fn shown(review: &Review) -> impl PartialEq + std::fmt::Debug {
    (
        review
            .candidates()
            .iter()
            .map(|c| (c.id, c.variant, c.range.clone()))
            .collect::<Vec<_>>(),
        review
            .applied()
            .iter()
            .map(|a| (a.id, a.range.clone(), a.step.original().to_owned()))
            .collect::<Vec<_>>(),
        review.kept_mentions(),
        review.not_a_name_wordings(),
        review
            .assignments()
            .iter()
            .map(|a| (a.range.clone(), a.identity))
            .collect::<Vec<_>>(),
        review.export_identities_with_ids(),
        review
            .pending_plans(SOURCE, &HashMap::new())
            .unwrap()
            .into_iter()
            .map(|plan| (plan.range, plan.after.to_string(), plan.identity))
            .collect::<Vec<_>>(),
    )
}

fn round_trip(saved: &SavedReview) -> SavedReview {
    serde_json::from_str(&serde_json::to_string(saved).unwrap()).unwrap()
}

#[test]
fn a_review_with_every_kind_of_decision_comes_back_as_it_was() {
    let review = decided();
    assert_eq!(review.applied().len(), 1);
    assert_eq!(review.kept_mentions().len(), 1);
    assert_eq!(review.not_a_name_wordings().len(), 1);
    assert_eq!(review.assignments().len(), 1);
    assert!(review.remaining() >= 3, "Ann twice and Bob stay proposed");
    let saved = review.save();
    let restored = Review::restore(round_trip(&saved), SOURCE, 0).unwrap();
    assert_eq!(restored.save(), saved);
    assert_eq!(shown(&restored), shown(&review));
    // It carries on as the original does: discovery finds the same mentions.
    let (mut review, mut restored) = (review, restored);
    review.refresh(SOURCE);
    restored.refresh(SOURCE);
    assert_eq!(restored.save(), review.save());
}

#[test]
fn a_saved_review_names_each_shared_step_once() {
    let mut review = decided();
    let mut step = review.applied()[0].step.as_ref().clone();
    step.predecessor = Some(review.applied()[0].step.clone());
    let step = Arc::new(step);
    for applied in &mut review.tracking.applied {
        applied.step = step.clone();
    }
    let saved = review.save();
    assert_eq!(saved.steps.len(), 2);
    assert_eq!(saved.steps[1].predecessor, Some(0));
    let restored = Review::restore(round_trip(&saved), SOURCE, 0).unwrap();
    assert_eq!(restored.save(), saved);
}

#[test]
fn a_review_saved_by_a_newer_mdoc_or_for_other_text_is_refused() {
    let saved = decided().save();
    let mut newer = saved.clone();
    newer.format = FORMAT + 1;
    assert_eq!(
        Review::restore(newer, SOURCE, 0).err(),
        Some(RestoreError::Newer(FORMAT + 1))
    );
    assert_eq!(
        Review::restore(saved.clone(), "PERSON_1 met Ann", 0).err(),
        Some(RestoreError::Mismatch)
    );
    let other = SOURCE.replace("Carl", "Cora");
    assert_eq!(
        Review::restore(saved.clone(), &other, 0).err(),
        Some(RestoreError::Mismatch),
        "text of the same length with other words"
    );
    let mut looped = saved.clone();
    looped.steps[0].predecessor = Some(0);
    assert_eq!(
        Review::restore(looped, SOURCE, 0).err(),
        Some(RestoreError::Mismatch)
    );
    let mut lineage = saved;
    let (first, second) = (lineage.variants[0].id, lineage.variants[1].id);
    lineage.variants[0].source_variant = Some(second);
    lineage.variants[1].source_variant = Some(first);
    assert_eq!(
        Review::restore(lineage, SOURCE, 0).err(),
        Some(RestoreError::Mismatch),
        "a spelling lineage that loops"
    );
}

#[test]
fn a_reopened_replacement_reverts_at_once() {
    let saved = decided().save();
    let restored = Review::restore(saved, SOURCE, 7).unwrap();
    assert!(restored.matches_history(7));
    let id = restored.applied()[0].id;
    let edits = restored
        .reversion_edits(SOURCE, &HashSet::from([id]))
        .unwrap();
    assert_eq!(edits, [(0..8, "Alice Moss".to_owned())]);
    assert_eq!(restored.prepare_restore(&edits).len(), 1);
}

#[test]
fn a_review_saved_before_discovery_finishes_discovers_on_reopening() {
    let mut review = decided();
    let remaining = review.remaining();
    review.candidates.await_discovery();
    review.awaiting_discovery = true;
    assert_eq!(review.remaining(), 0);
    let restored = Review::restore(round_trip(&review.save()), SOURCE, 0).unwrap();
    assert!(!restored.awaiting_discovery());
    assert_eq!(restored.remaining(), remaining);
}
