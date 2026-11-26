package io.github.lawkitt.mdoc.document

import android.content.res.Resources
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.HandOffRefusal

// A refusal's words (#1209 decisions 13 and 14), in iOS's
// (`HandOffRefusal.swift`): each its own sentence, none of the core's.

/** Short: under Copy for AI and Share in the document's menu, and a dialog's title. */
fun HandOffRefusal.reason(resources: Resources): String = when (this) {
    is HandOffRefusal.NamesToDecide -> if (names > 0) {
        resources.getQuantityString(R.plurals.refusal_names_to_decide, names, names)
    } else {
        resources.getString(R.string.refusal_some_names_to_decide)
    }
    HandOffRefusal.NotScanned -> resources.getString(R.string.refusal_not_scanned)
    HandOffRefusal.NotRead -> resources.getString(R.string.refusal_not_read)
    HandOffRefusal.NotChecked -> resources.getString(R.string.refusal_not_checked)
    HandOffRefusal.NamesNotChecked -> resources.getString(R.string.refusal_names_not_checked)
    HandOffRefusal.ChangedSinceReview -> resources.getString(R.string.refusal_changed_since_review)
    is HandOffRefusal.NamesWrittenOut -> resources.getQuantityString(R.plurals.refusal_names_written_out, count, count)
    HandOffRefusal.ExcludedFromAi -> resources.getString(R.string.refusal_excluded_from_ai)
    HandOffRefusal.Failed -> resources.getString(R.string.refusal_failed)
}

/** A sentence more, where a refusal shows on its own (the bar, a dialog). */
fun HandOffRefusal.detail(resources: Resources): String = resources.getString(
    when (this) {
        is HandOffRefusal.NamesToDecide -> R.string.refusal_names_to_decide_detail
        HandOffRefusal.NotScanned -> R.string.refusal_not_scanned_detail
        HandOffRefusal.NotRead -> R.string.refusal_not_read_detail
        HandOffRefusal.NotChecked -> R.string.refusal_not_checked_detail
        HandOffRefusal.NamesNotChecked -> R.string.refusal_names_not_checked_detail
        HandOffRefusal.ChangedSinceReview -> R.string.refusal_changed_since_review_detail
        is HandOffRefusal.NamesWrittenOut -> R.string.refusal_names_written_out_detail
        HandOffRefusal.ExcludedFromAi -> R.string.refusal_excluded_from_ai_detail
        HandOffRefusal.Failed -> R.string.refusal_failed_detail
    },
)

/** The bar's note in Share's place: the reason, then the sentence more. */
fun HandOffRefusal.note(resources: Resources): String = resources.getString(R.string.refusal_note, reason(resources), detail(resources))

/** The step's button and menu item. */
fun HandOffRefusal.Step.title(resources: Resources): String = when (this) {
    is HandOffRefusal.Step.ReplaceAndCopy -> resources.getString(R.string.step_replace_and_copy, count)
    HandOffRefusal.Step.Review -> resources.getString(R.string.step_review)
    HandOffRefusal.Step.FindNames -> resources.getString(R.string.find_names)
    HandOffRefusal.Step.CheckedNamesMyself -> resources.getString(R.string.step_checked_names_myself)
}
