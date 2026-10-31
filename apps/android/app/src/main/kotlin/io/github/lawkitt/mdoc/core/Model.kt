package io.github.lawkitt.mdoc.core

import androidx.annotation.StringRes
import io.github.lawkitt.mdoc.R
import uniffi.mdoc_ffi.MarkdownStyling

// View types built from the core's snapshots (ADR 0085): the app keeps only
// these and its own view state; Rust owns documents, review, cases and
// restore. The twins of the iOS app's CoreTypes.

@JvmInline value class CaseId(val raw: String)

@JvmInline value class DocumentId(val raw: String)

@JvmInline value class CandidateId(val raw: String)

@JvmInline value class IdentityId(val raw: String)

enum class Category(@StringRes val label: Int, val token: String) {
    Person(R.string.category_person, "PERSON"),
    Organization(R.string.category_organization, "ORG"),
    Email(R.string.category_email, "EMAIL"),
    Phone(R.string.category_phone, "PHONE"),
    Address(R.string.category_address, "ADDRESS"),
    Date(R.string.category_date, "DATE"),
    Identity(R.string.category_identity, "IDENTITY"),
    Tax(R.string.category_tax, "TAX"),
    Bank(R.string.category_bank, "BANK"),
    Other(R.string.category_other, "REDACTED"),
}

/** A case's colour: it marks the case, never a state (ADR 0077). */
enum class CaseColour { Red, Orange, Amber, Green, Teal, Blue, Violet, Pink }

data class CaseSummary(val id: CaseId, val name: String, val colour: CaseColour)

enum class SourceKind {
    Pdf, Docx, Image, Scan, Text;

    /** Only text recognition can read these. */
    val needsRecognition: Boolean get() = this == Scan || this == Image
}

enum class DocumentPhase {
    /** The import has not finished: its text is not read yet. */
    Recognizing,

    /** Read, names not searched for. */
    Unscanned,

    /** Proposals wait for Keep or Replace. */
    Reviewing,

    /** Nothing left to decide: ready to share. */
    Ready,
}

data class DocumentSummary(
    val id: DocumentId,
    val title: String,
    val caseId: CaseId?,
    val kind: SourceKind,
    val phase: DocumentPhase,
    val pendingCount: Int,
    /** Milliseconds since the epoch. */
    val updated: Long,
    val candidateCount: Int = 0,
    /** Saved into its case; an import not yet read is not. */
    val saved: Boolean = true,
)

data class LibrarySnapshot(
    val cases: List<CaseSummary> = emptyList(),
    val documents: List<DocumentSummary> = emptyList(),
    val currentCase: CaseId? = null,
) {
    fun case(id: CaseId?): CaseSummary? = cases.firstOrNull { it.id == id }
}

/** What the user brings in. */
sealed interface ImportSource {
    data class File(val file: java.io.File, val kind: SourceKind) : ImportSource

    /** Camera pages or photos, in order. */
    data class Pages(val pages: List<java.io.File>, val kind: SourceKind) : ImportSource

    data class Text(val text: String) : ImportSource
}

sealed interface CaseChoice {
    data class New(val name: String) : CaseChoice

    data class Existing(val id: CaseId) : CaseChoice
}

enum class MentionState { Proposed, Replaced, Kept }

data class Candidate(
    val id: CandidateId,
    val identityId: IdentityId,
    val range: SourceRange,
    val original: String,
    val alias: String,
    val category: Category,
    val state: MentionState,
)

data class Identity(
    val id: IdentityId,
    val alias: String,
    val category: Category,
    val primaryWording: String,
    val variants: List<String>,
    val mentionCount: Int,
    val state: MentionState,
)

enum class Decision { Replace, Keep }

enum class Scope { AllOccurrences, JustThisOne }

sealed interface RestoredSegment {
    data class Text(val text: String) : RestoredSegment

    data class Restored(val value: String) : RestoredSegment

    data class Unresolved(val token: String) : RestoredSegment
}

data class RestoreResult(
    val segments: List<RestoredSegment>,
    val restoredCount: Int,
    val unresolvedCount: Int,
) {
    val text: String
        get() = segments.joinToString("") {
            when (it) {
                is RestoredSegment.Text -> it.text
                is RestoredSegment.Restored -> it.value
                is RestoredSegment.Unresolved -> it.token
            }
        }
}

/** The page a document came from, for the Original view (ADR 0085): a PDF or a photo; DOCX and text have none. */
sealed interface OriginalSource {
    data object None : OriginalSource

    data class Pdf(val file: java.io.File) : OriginalSource

    data class Image(val file: java.io.File) : OriginalSource

    companion object {
        fun of(file: java.io.File?): OriginalSource = when (file?.extension?.lowercase()) {
            null -> None
            "pdf" -> if (file.exists()) Pdf(file) else None
            "jpg", "jpeg", "png", "webp" -> if (file.exists()) Image(file) else None
            else -> None
        }
    }
}

data class ReplySnapshot(val aliasText: String, val restored: RestoreResult, val styling: MarkdownStyling? = null)

data class HandOff(val text: String, val fileName: String)

data class DocumentSnapshot(
    val id: DocumentId,
    val title: String,
    val caseId: CaseId?,
    val caseName: String?,
    val kind: SourceKind,
    val phase: DocumentPhase,
    val markdown: String,
    val styling: MarkdownStyling?,
    val candidates: List<Candidate>,
    val identities: List<Identity>,
    val reply: ReplySnapshot?,
    val warning: String? = null,
    /** Text recognition began and kept finished pages: Recognize resumes. */
    val resumesOcr: Boolean = false,
    val original: OriginalSource = OriginalSource.None,
) {
    val pendingCount: Int get() = candidates.count { it.state == MentionState.Proposed }
    val replacedCount: Int get() = candidates.count { it.state == MentionState.Replaced }

    fun candidate(id: CandidateId): Candidate? = candidates.firstOrNull { it.id == id }

    fun mentions(of: IdentityId): List<Candidate> = candidates.filter { it.identityId == of }
}

/** Why a core call failed, in the app's terms. */
sealed class AppError(message: String) : Exception(message) {
    class Cancelled : AppError("Cancelled.")

    /** Recognition or Find names needs a model that is not downloaded. */
    class ModelMissing : AppError("Download the models first.")

    class NotFound : AppError("The document is gone.")

    class NoCase : AppError("This document has no case yet.")

    class PendingReview(val count: Int) : AppError("Review the proposals first.")

    class EmptySelection : AppError("Select the words to replace.")

    class Failed(message: String) : AppError(message)
}
