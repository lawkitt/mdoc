//! Why Copy for AI or a shelf row refuses a document, and the one step that
//! resolves it (#1209, decision 13). GPUI-free, so the editor's notice and
//! Share tab and the shelf's rows say the same; a selection's "Replace N and
//! Copy" (Vault & Projects item 43) is to reuse [`Step::ReplaceAndCopy`].
//!
//! A step never hands text out by itself: "Replace N and copy" replaces the
//! pending names as Apply does and then asks Copy for AI, whose readiness
//! check (`Vault::text_for_ai`) decides what leaves.

use super::text::{self, reason};
use crate::shelf::Readiness;

/// The one step that resolves a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Replace the names still proposed, as Apply does, then copy if the
    /// result is Ready.
    ReplaceAndCopy(usize),
    /// Open the review: a name is written out, or the text changed since
    /// its review.
    Review,
    /// Scan for names: the document was never scanned.
    FindNames,
}

impl Step {
    /// The step for a document in `readiness`; `scanned`: it was scanned for
    /// names, so a document never saved Ready has a review to finish.
    /// `None` when nothing the user does resolves it now: ready, or still
    /// being checked.
    pub(crate) fn of(readiness: Readiness, scanned: bool) -> Option<Self> {
        match readiness {
            Readiness::Ready | Readiness::Checking => None,
            Readiness::LeftToReview(n) => Some(Self::ReplaceAndCopy(n)),
            Readiness::NamesNotReplaced(_) | Readiness::ChangedSinceReview => Some(Self::Review),
            Readiness::NotReviewed if scanned => Some(Self::Review),
            Readiness::NotReviewed => Some(Self::FindNames),
        }
    }
}

/// A refused Copy for AI: why, and what resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub reason: String,
    pub step: Option<Step>,
}

impl Refusal {
    /// The refusal of a document in `readiness`. `failed`: its last write
    /// failed, so the check can't see its text.
    pub(crate) fn of(readiness: Readiness, scanned: bool, failed: bool) -> Self {
        Self {
            reason: reason(readiness, scanned, failed),
            step: Step::of(readiness, scanned),
        }
    }

    /// A document kept from AI: no step on desktop changes that.
    pub(crate) fn excluded() -> Self {
        Self {
            reason: text::EXCLUDED_FROM_AI.into(),
            step: None,
        }
    }

    /// A document with no applied alias: names to replace, none found yet,
    /// or every proposal kept as written.
    pub(crate) fn nothing_applied(pending: usize, scanned: bool) -> Self {
        if pending > 0 {
            return Self::of(Readiness::LeftToReview(pending), true, false);
        }
        if !scanned {
            return Self::of(Readiness::NotReviewed, false, false);
        }
        Self {
            reason: text::NO_NAMES_REPLACED.into(),
            step: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_carries_the_step_that_resolves_it() {
        assert_eq!(
            Step::of(Readiness::LeftToReview(3), true),
            Some(Step::ReplaceAndCopy(3))
        );
        assert_eq!(Step::ReplaceAndCopy(3).label(), "Replace 3 and copy");
        for readiness in [
            Readiness::NamesNotReplaced(1),
            Readiness::ChangedSinceReview,
        ] {
            assert_eq!(
                Step::of(readiness, true),
                Some(Step::Review),
                "{readiness:?}"
            );
        }
        assert_eq!(Step::of(Readiness::NotReviewed, true), Some(Step::Review));
        assert_eq!(
            Step::of(Readiness::NotReviewed, false),
            Some(Step::FindNames)
        );
        assert_eq!(Step::FindNames.label(), "Find names");
        assert_eq!(Step::of(Readiness::Checking, true), None);
        assert_eq!(Step::of(Readiness::Ready, true), None);
    }

    #[test]
    fn reasons_read_as_one_sentence_each() {
        let reasons = [
            Refusal::of(Readiness::LeftToReview(1), true, false),
            Refusal::of(Readiness::LeftToReview(2), true, false),
            Refusal::of(Readiness::NamesNotReplaced(1), true, false),
            Refusal::of(Readiness::NamesNotReplaced(2), true, false),
            Refusal::of(Readiness::ChangedSinceReview, true, false),
            Refusal::of(Readiness::NotReviewed, true, false),
            Refusal::of(Readiness::NotReviewed, false, false),
            Refusal::of(Readiness::Checking, true, true),
            Refusal::of(Readiness::Checking, true, false),
            Refusal::excluded(),
            Refusal::nothing_applied(0, true),
        ];
        for refusal in reasons {
            let reason = refusal.reason;
            assert!(!reason.contains("  "), "{reason:?}");
            assert!(reason.ends_with('.'), "{reason:?}");
        }
        assert_eq!(
            Refusal::nothing_applied(4, true).step,
            Some(Step::ReplaceAndCopy(4))
        );
        assert_eq!(
            Refusal::nothing_applied(0, false).step,
            Some(Step::FindNames)
        );
        assert_eq!(Refusal::nothing_applied(0, true).step, None);
        assert_eq!(Refusal::excluded().step, None);
    }
}
