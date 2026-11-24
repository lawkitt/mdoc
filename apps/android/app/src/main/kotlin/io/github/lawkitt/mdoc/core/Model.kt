package io.github.lawkitt.mdoc.core

import androidx.annotation.StringRes
import io.github.lawkitt.mdoc.R
import uniffi.mdoc_ffi.MarkdownStyling

// View types built from the core's snapshots: the app keeps only
// these and its own view state; Rust owns documents, review, Projects and
// restore, and the Vault keeps them (ADR 0125). The twins of the iOS app's
// CoreTypes.

@JvmInline value class ProjectId(val raw: String)

@JvmInline value class DocumentId(val raw: String)

@JvmInline value class CandidateId(val raw: String)

@JvmInline value class IdentityId(val raw: String)

enum class Category(@StringRes val label: Int, val token: String) {
    Person(R.string.category_person, "PERSON"),
    Organization(R.string.category_organization, "ORG"),
    Email(R.string.category_email, "EMAIL"),
    Url(R.string.category_url, "URL"),
    Phone(R.string.category_phone, "PHONE"),
    Address(R.string.category_address, "ADDRESS"),
    Date(R.string.category_date, "DATE"),
    CaseNumber(R.string.category_case_number, "CASE"),
    Identity(R.string.category_identity, "IDENTITY"),
    Tax(R.string.category_tax, "TAX"),
    Bank(R.string.category_bank, "BANK"),
    Other(R.string.category_other, "REDACTED"),
}

/** A Project's colour: it marks the Project, never a state. */
enum class ProjectColour { Red, Orange, Amber, Green, Teal, Blue, Violet, Pink }

data class ProjectSummary(val id: ProjectId, val name: String, val colour: ProjectColour)

/**
 * Where text was shared from, and so whose names a reply to it restores
 * with: a Project, or an Unfiled document by its Vault id (#1091
 * Decision 4).
 */
sealed interface Place {
    data class Project(val id: ProjectId) : Place

    data class Unfiled(val document: String) : Place
}

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
    /** `null`: Unfiled. */
    val projectId: ProjectId?,
    val kind: SourceKind,
    val phase: DocumentPhase,
    val pendingCount: Int,
    /** Milliseconds since the epoch. */
    val updated: Long,
    val candidateCount: Int = 0,
    /** Its id in the Vault, once saved; an import not yet read has none. */
    val vaultId: String? = null,
)

/** One of a Project's names on its page: its alias, its category and the wordings it stands for, the one restore puts back first. */
data class ProjectNameRow(val alias: String, val category: Category, val originals: List<String>)

/** A wording on a Project's Always list, in the category scans propose it in. */
data class AlwaysWording(val wording: String, val category: Category)

/** What a Project's page shows beside its documents and replies: its names and its Always and Never lists. */
data class ProjectDetails(
    val names: List<ProjectNameRow> = emptyList(),
    val always: List<AlwaysWording> = emptyList(),
    val never: List<String> = emptyList(),
)

/** A saved AI reply as a Project's Replies lists it; no text. */
data class SavedReplyRow(val id: String, val title: String, /** Milliseconds since the epoch. */ val created: Long)

/** What deleting a Project that has documents does with them (#1091 Decision 9): the app asks. */
enum class ProjectDeletion {
    /** Each goes to Unfiled with its names and its replies; the Project's own AI replies go with the Project. */
    MoveToUnfiled,

    /** They are deleted with it, and their replies. */
    DeleteDocuments,
}

/** Where a move takes a saved document (#1091 Decision 9): a Project, a new one named so, or Unfiled. */
sealed interface MoveTarget {
    data class Existing(val project: ProjectId) : MoveTarget

    data class NewProject(val name: String) : MoveTarget

    data object Unfiled : MoveTarget
}

/** Where a move took a document, for "Moved to <Project>" and its Undo (iOS `ProjectMove`). */
data class ProjectMove(
    /** The id this run lists it by. */
    val document: DocumentId,
    /** The Project it is in now; `null`: Unfiled. */
    val to: ProjectSummary?,
    /** The Project it left, which keeps its names; `null`: it was Unfiled. */
    val from: ProjectId?,
    /** The Vault's handle for Undo, good while this core runs. */
    val undo: ULong,
)

data class LibrarySnapshot(
    val projects: List<ProjectSummary> = emptyList(),
    val documents: List<DocumentSummary> = emptyList(),
) {
    fun project(id: ProjectId?): ProjectSummary? = projects.firstOrNull { it.id == id }
}

/** What the user brings in. */
sealed interface ImportSource {
    data class File(val file: java.io.File, val kind: SourceKind) : ImportSource

    /** Camera pages or photos, in order. */
    data class Pages(val pages: List<java.io.File>, val kind: SourceKind) : ImportSource

    data class Text(val text: String) : ImportSource
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

/**
 * One file of a document's original as the Original view reads it: a
 * received file not saved yet, or the Vault's sealed original, decrypted
 * the first time it is read, into memory when small, else into a viewing
 * copy (#1091 Decision 24).
 */
sealed interface OriginalData {
    data class Local(val file: java.io.File) : OriginalData

    /** The Vault's original, read by [open]; [close] drops what it decrypted. */
    class Stored(private val decrypt: () -> Decrypted, private val remove: () -> Unit) : OriginalData {
        /** One decrypt at a time; [close] never waits for it. */
        private val decrypting = Any()
        private var decrypted: Decrypted? = null

        /** Counts closes: a decrypt that a close overtook keeps nothing. */
        private var closes = 0

        fun open(): Decrypted = synchronized(decrypting) {
            synchronized(this) { decrypted }?.let { return it }
            val before = synchronized(this) { closes }
            val fresh = decrypt()
            synchronized(this) {
                if (closes == before) {
                    decrypted = fresh
                    return fresh
                }
            }
            (fresh as? Decrypted.Memory)?.bytes?.fill(0)
            remove()
            throw IllegalStateException("The document closed.")
        }

        /** The document closed: its bytes go, and its viewing copy, even while a decrypt runs. */
        fun close() {
            val dropped = synchronized(this) {
                closes++
                decrypted.also { decrypted = null }
            }
            (dropped as? Decrypted.Memory)?.bytes?.fill(0)
            remove()
        }
    }

    sealed interface Decrypted {
        class Memory(val bytes: ByteArray) : Decrypted

        data class Copy(val file: java.io.File) : Decrypted
    }
}

/** The pages a document came from, for the Original view: a PDF, or photos and camera pages; DOCX and text have none. */
sealed interface OriginalSource {
    data object None : OriginalSource

    data class Pdf(val file: OriginalData) : OriginalSource

    /** Photos or camera pages, a page each, in order, each drawn on its own. */
    data class Images(val files: List<OriginalData>) : OriginalSource

    /** Drops what was decrypted to view it. */
    fun close() {
        when (this) {
            None -> Unit
            is Pdf -> (file as? OriginalData.Stored)?.close()
            is Images -> files.forEach { (it as? OriginalData.Stored)?.close() }
        }
    }

    companion object {
        private val images = setOf("jpg", "jpeg", "png", "webp", "gif", "heic", "heif")

        /** Received files, by their names. */
        fun of(files: List<java.io.File>): OriginalSource {
            val first = files.firstOrNull { it.exists() } ?: return None
            return when {
                first.extension.lowercase() == "pdf" -> Pdf(OriginalData.Local(first))
                files.all { it.extension.lowercase() in images } -> Images(files.map(OriginalData::Local))
                else -> None
            }
        }

        /** A Vault original, by the name it was imported with. */
        fun stored(fileName: String, data: OriginalData.Stored): OriginalSource =
            when (fileName.substringAfterLast('.', "").lowercase()) {
                "pdf" -> Pdf(data)
                in images -> Images(listOf(data))
                else -> None
            }
    }
}

data class ReplySnapshot(
    val aliasText: String,
    val restored: RestoreResult,
    val styling: MarkdownStyling? = null,
    /** The saved AI Reply it shows, by its Vault id. */
    val id: String? = null,
)

data class HandOff(val text: String, val fileName: String)

data class DocumentSnapshot(
    val id: DocumentId,
    val title: String,
    /** `null`: Unfiled. */
    val projectId: ProjectId?,
    val projectName: String?,
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

    /** A locked DOCX or PDF: the screen asks for its password. */
    class NeedsPassword : AppError("This document is password-protected")

    /** The password given doesn't unlock the document: the screen asks again. */
    class WrongPassword : AppError("The password is incorrect. Check it and try again.")

    class NotFound : AppError("The document is gone.")

    class PendingReview(val count: Int) : AppError("Review the proposals first.")

    class EmptySelection : AppError("Select the words to replace.")

    class Failed(message: String) : AppError(message)
}
