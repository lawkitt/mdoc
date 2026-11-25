package io.github.lawkitt.mdoc.core

import uniffi.mdoc_ffi.SelectionCheck
import uniffi.mdoc_ffi.TextRange

/**
 * A selection the user copies by hand (#1209, ADR 0109 as amended): their
 * own copy of their own text, never blocked, so not an exit. Its menu says
 * what it holds, as `docs/specs/mobile.md` sets out, and the core decides
 * which (`DocumentSession::check_selection`). Share, Assist and the like
 * stay off a selection: they send text on without saying so.
 */
sealed interface SelectionAction {
    /** A clean selection: copied as written, unmarked. */
    data object Copy : SelectionAction

    /** Its `count` proposals replaced as the review replaces them, one undo step, then copied. */
    data class ReplaceAndCopy(val count: Int) : SelectionAction

    /** Copied as written, names and all: marked unless a fresh check finds it clean. */
    data object CopyWithNames : SelectionAction

    companion object {
        /**
         * The menu for what a selection holds: Copy when clean; Replace N
         * and copy, then Copy with names, when proposals wait in it; Copy
         * with names alone otherwise (a name its Project knows written out,
         * text changed since its scan, a document never scanned). `null`,
         * a check that failed: Copy with names alone, since a hand copy is
         * never blocked.
         */
        fun menu(check: SelectionCheck?): List<SelectionAction> = when (check) {
            SelectionCheck.Clean -> listOf(Copy)
            is SelectionCheck.NotClean ->
                if (check.proposals > 0uL) {
                    listOf(ReplaceAndCopy(check.proposals.coerceAtMost(Int.MAX_VALUE.toULong()).toInt()), CopyWithNames)
                } else {
                    listOf(CopyWithNames)
                }
            SelectionCheck.NeverScanned, null -> listOf(CopyWithNames)
        }

        /** Whether a copy is marked on the clipboard: unless the core found it clean (#1209 decision 10). */
        fun marked(check: SelectionCheck?): Boolean = check != SelectionCheck.Clean
    }
}

/** The core's range for a source range. */
internal val SourceRange.ffi: TextRange get() = TextRange(start.toULong(), end.toULong())

/** What a hand copy puts on the clipboard: `text`, `marked` sensitive unless clean. */
data class SelectionCopy(
    val text: String,
    val marked: Boolean,
    /** The document after Replace N and copy; `null` when nothing changed. */
    val snapshot: DocumentSnapshot? = null,
)
