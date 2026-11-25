package io.github.lawkitt.mdoc.core

/**
 * Whether a saved document may leave for an AI now, as the Vault checks it
 * against its Project's names (#1091 Decision 19), in the app's terms.
 */
sealed interface SavedReadiness {
    data object Ready : SavedReadiness

    /** Never saved while Ready: not scanned, or proposals left. */
    data object NotReviewed : SavedReadiness

    /** Its text is not the text it saved while Ready. */
    data object ChangedSinceReview : SavedReadiness

    /** Its text still holds this many names its Project knows, written out. */
    data class NamesNotReplaced(val count: Int) : SavedReadiness

    /** Marked "exclude from AI" (on a computer): mdoc's own exits refuse it. */
    data object ExcludedFromAi : SavedReadiness
}

/**
 * Why Copy for AI and Share refuse, in plain words, with the one step that
 * resolves it (#1209 decisions 13 and 14): the twin of iOS's
 * `HandOffRefusal`. Nothing of the core's own wording reaches the screen:
 * each refusal has its own sentence (`document/HandOffWords.kt`).
 */
sealed interface HandOffRefusal {
    /** Proposals wait, of this many names: Replace N (proposals) and copy. */
    data class NamesToDecide(val names: Int, val proposals: Int) : HandOffRefusal

    /** Names were never looked for: Find names. */
    data object NotScanned : HandOffRefusal

    /** The text is not read yet: recognition or the import comes first. */
    data object NotRead : HandOffRefusal

    /** The Vault has not seen this text Ready while its review is done. */
    data object NotChecked : HandOffRefusal

    /** The text changed after its review: Find names again. */
    data object ChangedSinceReview : HandOffRefusal

    /** Names the Project knows are written out in the text: Review. */
    data class NamesWrittenOut(val count: Int) : HandOffRefusal

    /** Excluded from AI, which only a computer changes. */
    data object ExcludedFromAi : HandOffRefusal

    /** Anything else: the text could not be prepared. */
    data object Failed : HandOffRefusal

    /** The step that resolves a refusal. */
    sealed interface Step {
        /** Accepts the pending proposals as the review's Replace all does, then copies. */
        data class ReplaceAndCopy(val count: Int) : Step

        /** The review, on the names left. */
        data object Review : Step

        /** Find names, or Find names again. */
        data object FindNames : Step
    }

    /**
     * The one step that resolves it, where this phone can take it.
     * `findsNames`: the phone runs the name detector. Below the floor no
     * step looks for names again, so a text not checked since it changed
     * has none there.
     */
    fun step(findsNames: Boolean): Step? = when (this) {
        is NamesToDecide -> if (proposals > 0) Step.ReplaceAndCopy(proposals) else Step.Review
        NotScanned, NotChecked, ChangedSinceReview -> if (findsNames) Step.FindNames else null
        is NamesWrittenOut -> Step.Review
        NotRead, ExcludedFromAi, Failed -> null
    }

    companion object {
        /** The refusal of a saved document the review has no objection to; null when it is Ready. */
        fun of(saved: SavedReadiness): HandOffRefusal? = when (saved) {
            SavedReadiness.Ready -> null
            SavedReadiness.NotReviewed -> NotChecked
            SavedReadiness.ChangedSinceReview -> ChangedSinceReview
            is SavedReadiness.NamesNotReplaced -> NamesWrittenOut(saved.count)
            SavedReadiness.ExcludedFromAi -> ExcludedFromAi
        }

        /**
         * Why Copy for AI and Share wait before a tap; null when they go.
         * The review's state first, then what the Vault says of the
         * document as saved (`saved`, null while unknown): changed since
         * its review, a known name written out, excluded from AI. A saved
         * "not reviewed" while the review is done is, at the detector
         * tier, names marked by hand in a document never scanned: Find
         * names. Below the floor it is no refusal here: the tap saves again
         * first (a phone fallen below the floor since), and says why where
         * that does not help.
         */
        fun before(snapshot: DocumentSnapshot, findsNames: Boolean, saved: SavedReadiness?): HandOffRefusal? {
            when (snapshot.phase) {
                DocumentPhase.Recognizing -> return NotRead
                DocumentPhase.Reviewing -> return toDecide(snapshot)
                DocumentPhase.Unscanned -> {
                    if (findsNames) return NotScanned
                    // Below the floor names are decided by hand: with none
                    // left the document goes, as the bar's Share says (#1091 Q4).
                    if (snapshot.pendingCount > 0) return toDecide(snapshot)
                }
                DocumentPhase.Ready -> Unit
            }
            return when (saved) {
                null -> null
                SavedReadiness.NotReviewed -> if (findsNames) NotScanned else null
                else -> of(saved)
            }
        }

        /** Proposals left: of how many names, and how many proposals. */
        fun toDecide(snapshot: DocumentSnapshot) = NamesToDecide(
            names = snapshot.identities.count { it.state == MentionState.Proposed },
            proposals = snapshot.pendingCount,
        )
    }
}
