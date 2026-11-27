//! A refused Copy for AI (#1209): the "Not copied" notice with each step
//! ("Replace 2 and copy", "Review", "Find names") and with none (excluded
//! from AI, still checking), and the Share tab's button showing the step
//! before the click. The shelf's rows are in `shelf`.
use super::{Driver, Result};
use crate::{document::Inspector, review::refusal::Refusal, shelf::Readiness};
use mdoc_pii::{Category, Detection, Recognizer};

const TEXT: &str = "Anna Petrova wrote to Boris Ivanov about the lease.";

/// A new tab holding [`TEXT`], its two names proposed and not applied, or
/// with `applied` the first applied; the Share tab showing.
fn document(d: &mut Driver, propose: bool, applied: bool) -> Result<()> {
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view().unwrap().update(cx, |v, cx| {
            v.editor.update(cx, |e, cx| e.set_text(TEXT, cx));
        });
    })?;
    d.settle()?;
    d.change(|w, window, cx| {
        w.active_view().unwrap().update(cx, |v, cx| {
            if propose {
                let detections = ["Anna Petrova", "Boris Ivanov"]
                    .iter()
                    .filter(|name| !applied || **name == "Anna Petrova")
                    .map(|name| {
                        let start = TEXT.find(name).unwrap();
                        Detection {
                            range: start..start + name.len(),
                            category: Category::Person,
                            score: 1.,
                            recognizer: Recognizer::Model,
                        }
                    })
                    .collect();
                v.pii
                    .review_mut()
                    .ingest(TEXT, detections)
                    .expect("refusal fixture");
                v.review_changed(cx);
            }
            if !v.replacements_open() {
                v.toggle_replacements_panel(window, cx);
            }
            if applied {
                v.apply_all_pii(&crate::review::PiiApplyAll, window, cx);
            }
            v.inspector = Inspector::Share;
            cx.notify();
        });
    })?;
    d.settle()
}

/// Show `refusal` on the active tab and capture it as `name`.
fn refused(d: &mut Driver, name: &str, refusal: Refusal) -> Result<()> {
    d.change(|w, _, cx| {
        w.active_view()
            .unwrap()
            .update(cx, |v, cx| v.copy_for_ai_refused(refusal, cx));
    })?;
    d.capture(name)
}

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    // Names proposed: the button and the notice say "Replace 2 and copy".
    document(d, true, false)?;
    refused(
        d,
        "copy-refused-replace",
        Refusal::of(Readiness::LeftToReview(2), true, false),
    )?;
    // Never scanned: "Find names".
    document(d, false, false)?;
    refused(
        d,
        "copy-refused-find-names",
        Refusal::of(Readiness::NotReviewed, false, false),
    )?;
    // Applied, refused by the Vault's check: "Review", or no step.
    document(d, true, true)?;
    refused(
        d,
        "copy-refused-known-name",
        Refusal::of(Readiness::NamesNotReplaced(1), true, false),
    )?;
    refused(
        d,
        "copy-refused-changed",
        Refusal::of(Readiness::ChangedSinceReview, true, false),
    )?;
    refused(
        d,
        "copy-refused-not-reviewed",
        Refusal::of(Readiness::NotReviewed, true, false),
    )?;
    refused(d, "copy-refused-excluded", Refusal::excluded())?;
    refused(
        d,
        "copy-refused-checking",
        Refusal::of(Readiness::Checking, true, false),
    )
}
