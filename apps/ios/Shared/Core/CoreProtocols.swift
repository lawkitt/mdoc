import Foundation
import MdocFFI

// The three coarse objects `crates/mdoc-ffi` exports (ADR 0085). Rust owns
// document, review, case and restore state; these calls return snapshots.
// Long calls block their thread and report through a `JobObserver`, the one
// thing the core calls back into the platform for.

/// Progress and cooperative cancellation for a long core call: the real
/// core reports stages through its `JobProgress` callback, and `onCancel`
/// stops the job by its id (`cancel_job`), even one not started yet.
protocol JobObserver: AnyObject, Sendable {
    func progress(_ update: JobUpdate)
    /// Polled between units of work; true ends the call with `CoreError.cancelled`.
    func isCancelled() -> Bool
    /// Runs `handler` once, when the job is cancelled (at once if it is
    /// already), so a core that stops on its own call can be told.
    func onCancel(_ handler: @escaping @Sendable () -> Void)
}

/// Projects and stored documents, the app's over the Vault (ADR 0125). The
/// Restore extension has a `ReplyStore` instead.
protocol Library: AnyObject, Sendable {
    func snapshot() -> LibrarySnapshot
    func session(for document: DocumentID) throws -> any DocumentSession
    /// Stores the document and returns its session, ready for `recognize`:
    /// Unfiled for nil, or in the Project it was started from. No import
    /// makes a Project (#1091 Decision 35).
    func importDocument(_ source: ImportSource, title: String, into project: ProjectID?) throws -> any DocumentSession
    func deleteDocument(_ document: DocumentID) throws
    /// A new document left empty goes when it is left (ADR 0109). True
    /// when it went; any other document stays.
    func discardIfEmpty(_ document: DocumentID) throws -> Bool
    /// Takes in what another handle wrote meanwhile, the Restore
    /// extension's replies above all (#1091 Decision 20): true when
    /// anything changed.
    @discardableResult
    func takeInChanges() -> Bool
    /// Saved reply `id` restored with the people it named when saved, as
    /// they are now (#1091 Decision 18).
    func restoreSavedReply(_ id: ReplyID) throws -> RestoreResult
    /// Saves the jobs that finished while the phone was locked, and returns
    /// their documents.
    func saveDeferred() throws -> [DocumentID]
    /// Saves now what review taps left for their save (ADR 0104: the save
    /// follows the tap), in every document: mdoc leaves the foreground.
    func saveChanges() throws
    /// Tells `observer`, on the main thread, how each save that follows
    /// taps went.
    func observeSaves(_ observer: @escaping @MainActor @Sendable (SaveEvent) -> Void)
    /// Rename: the document's title, as typed, trimmed. Refused for a
    /// blank title, and for an import not read yet.
    func renameDocument(_ document: DocumentID, to title: String) throws
    /// Renames a Project; its documents keep their titles.
    func renameProject(_ projectID: ProjectID, to name: String) throws
    /// Move to Project (#1091 Decision 9): the document joins `target`, its
    /// names taking that Project's aliases (a name the Project knows takes
    /// its alias, a clashing alias the next free number) and its text
    /// rewritten with them. The Project it left keeps its names, so replies
    /// to what was sent still restore. Refused for the Project it is in
    /// already, and for an import not read yet.
    func moveDocument(_ document: DocumentID, to target: ProjectChoice) throws -> ProjectMove
    /// Undo of Move to Project: the document moves back and its names take
    /// their old aliases there. The names the move added go again where
    /// nothing uses them, unless text was shared from there since.
    func undoMove(_ move: ProjectMove) throws -> ProjectMove
    /// New Project: an empty Project named `name`, trimmed; a blank name is
    /// refused.
    func createProject(named name: String) throws -> ProjectSummary
    /// A Project page's names and its Always and Never lists.
    func projectDetails(_ projectID: ProjectID) throws -> ProjectDetails
    /// Deletes a Project with its own AI replies; its documents go to
    /// Unfiled or are deleted, as `documents` says. A document open in
    /// this run follows: moved, it opens again in Unfiled; deleted, nothing
    /// saves it again.
    func deleteProject(_ projectID: ProjectID, documents: ProjectDeletion) throws
    /// Unfiled's multi-select delete. A failure stops it, and the documents
    /// deleted before it stay deleted.
    func deleteDocuments(_ documents: [DocumentID]) throws
    /// Takes `wording` off a Project's Always list, or its Never list when
    /// `always` is false.
    func removeListed(_ wording: String, always: Bool, from projectID: ProjectID) throws
    /// The saved documents imported from a file of the same bytes, most
    /// recently updated first (#1091 Decision 7); none when it can't be
    /// read.
    func documentsFromSource(_ file: URL) -> [DocumentID]
}

/// How a save that followed review taps went (`Library.observeSaves`).
enum SaveEvent: Sendable {
    /// Saved. `rejoined`: another document changed the case meanwhile, and
    /// joining it again may have renumbered this one's aliases.
    case saved(DocumentID, rejoined: Bool)
    /// Not saved, and still to save.
    case failed(DocumentID, CoreError)
}

extension Library {
    func takeInChanges() -> Bool { false }
    func restoreSavedReply(_ id: ReplyID) throws -> RestoreResult { throw CoreError.notFound }
    func saveDeferred() throws -> [DocumentID] { [] }
    func saveChanges() throws {}
    func observeSaves(_ observer: @escaping @MainActor @Sendable (SaveEvent) -> Void) {}
    func renameDocument(_ document: DocumentID, to title: String) throws { throw CoreError.notFound }
    func renameProject(_ projectID: ProjectID, to name: String) throws { throw CoreError.noProject }
    func moveDocument(_ document: DocumentID, to target: ProjectChoice) throws -> ProjectMove { throw CoreError.notFound }
    func undoMove(_ move: ProjectMove) throws -> ProjectMove { throw CoreError.noProject }
    func discardIfEmpty(_ document: DocumentID) throws -> Bool { false }
    func createProject(named name: String) throws -> ProjectSummary { throw CoreError.noProject }
    func projectDetails(_ projectID: ProjectID) throws -> ProjectDetails { throw CoreError.noProject }
    func deleteProject(_ projectID: ProjectID, documents: ProjectDeletion) throws { throw CoreError.noProject }
    func deleteDocuments(_ documents: [DocumentID]) throws {
        for document in documents { try deleteDocument(document) }
    }
    func removeListed(_ wording: String, always: Bool, from projectID: ProjectID) throws { throw CoreError.noProject }
    func documentsFromSource(_ file: URL) -> [DocumentID] { [] }
}

/// One document's import, OCR, review and hand-off.
protocol DocumentSession: AnyObject, Sendable {
    /// Moves the import's in-memory unlock to the open document.
    func takeUnlockedDocx() -> UnlockedDocx?
    /// The document closed: its decrypted original goes (#1091 Decision 24).
    func closeOriginal()
    var id: DocumentID { get }
    func snapshot() -> DocumentSnapshot
    /// Import and OCR. Resumes after the pages already finished. A locked
    /// DOCX throws `CoreError.needsPassword` until `password` unlocks it;
    /// the password is used for this call only.
    func recognize(observer: any JobObserver, password: String?) throws -> DocumentSnapshot
    /// Finds candidates; a cancelled scan reruns from the start.
    func scan(observer: any JobObserver) throws -> DocumentSnapshot
    /// Minutes a scan takes on this phone, when it is expected to take more
    /// than about a minute (ADR 0120); nil otherwise.
    func scanEstimate() -> Int?
    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot
    /// Not a name (ADR 0122): the mark, or every mark of its identity,
    /// comes off with no trace; replaced text returns to its original.
    func notAName(_ candidate: CandidateID, scope: Scope) throws -> DocumentSnapshot
    /// Replaces every proposed mention with its proposed alias.
    func replaceAllPending() throws -> DocumentSnapshot
    /// Copy for AI's text, through the same export path as desktop. The core
    /// refuses while proposals are pending. Records the document as shared
    /// last. Saves the document first, so the case the Restore extension
    /// reads has every alias the text has.
    func copyForAI() throws -> HandOff
    /// Saves now what review taps left for their save (ADR 0104): before
    /// the document is left for another, or mdoc leaves the foreground.
    func saveChanges() throws
    /// Saves `reply` in the Vault as an AI Reply to text shared from this
    /// document (its Project's, or its own while Unfiled) and shows it in
    /// the Reply pane, restored.
    func restoreReply(_ reply: String) throws -> DocumentSnapshot
    /// The Reply pane shows saved reply `id`, one of the snapshot's
    /// `replies`.
    func showReply(_ id: ReplyID) -> DocumentSnapshot
    /// Remove reply: deletes the reply the pane shows from the Vault.
    func clearReply() throws -> DocumentSnapshot

    // MARK: Review bar (ADR 0098)
    //
    // Each change below, and `decide`, `replaceAllPending` and the marks, is
    // one undo step, held in memory while the session lives (256 steps).
    // Copy for AI, Restore reply and Clear reply are not steps. Each returns
    // at once; its change is saved shortly after, on a utility queue,
    // several taps in one save (ADR 0104).

    /// Undoes the last step. The snapshot's `redoStep` names what it undid
    /// and where, for the toast and to move the bar back.
    func undo() throws -> DocumentSnapshot
    /// Makes the last undone step again; the snapshot's `undoStep` names it.
    func redo() throws -> DocumentSnapshot
    /// Hide name's callout for a selection outside any name (ADR 0114):
    /// what it hides, snapped to whole words and trimmed, and Hide as's
    /// categories, the core's guess first. Throws the core's reason when
    /// there is nothing to hide there.
    func hideTarget(_ range: SourceRange) throws -> WordsTarget
    /// Hide name: the selection, snapped and trimmed as `hideTarget` shows
    /// it, and its exact repeats become proposals in `category`, or the
    /// core's guess when nil; a proposed wording keeps its own. Replaces
    /// nothing yet.
    func markSelection(_ range: SourceRange, as category: Category?) throws -> DocumentSnapshot
    /// Fix the words begins on a name (ADR 0114): the document as it shows
    /// meanwhile (a replaced name's original back in place), where the
    /// handles start and how far they reach. Changes nothing.
    func fixWordsDraft(_ candidate: CandidateID) throws -> WordsDraft
    /// Hide «…» for `words` of the draft: the words Fix the words would
    /// take, or the core's reason it can't.
    func fixWordsTarget(_ candidate: CandidateID, words: SourceRange) throws -> WordsTarget
    /// Fix the words: the name takes `words` of its draft; the new wording
    /// replaces the old one everywhere it fits and keeps the identity and
    /// its alias, or joins the identity whose wording it is already. One
    /// undo step.
    func fixWords(_ candidate: CandidateID, words: SourceRange) throws -> DocumentSnapshot
    /// Stored proposals, as a scan's: every occurrence of each wording, in
    /// its category. Try a sample uses it, so the sample reviews before the
    /// name finder is downloaded (ADR 0099).
    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot
    /// Rename…: the alias of this mention (`.this` splits it off under the
    /// alias) or of its identity. Throws `.aliasTaken` with the
    /// identity that has the alias already: Same as is the way to it.
    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot
    /// The category menu: each category with the alias choosing it gives.
    func categoryChoices(for candidate: Candidate, scope: Scope) throws -> [CategoryChoice]
    /// The category of this mention (split off in it) or of its identity.
    func setCategory(_ candidate: CandidateID, scope: Scope, to category: Category) throws -> DocumentSnapshot
    /// Same as…: `.all` links the mention's identity into `target`, which
    /// keeps its alias and restore wording; `.this` moves only this mention.
    /// Replaces nothing by itself.
    func sameAs(_ candidate: CandidateID, scope: Scope, target: IdentityID) throws -> DocumentSnapshot
    /// Same as… for a whole identity: an Identities row, or a merge hint.
    func link(_ identity: IdentityID, into target: IdentityID) throws -> DocumentSnapshot
    /// New alias: this mention split off as an identity of its own.
    func newAlias(_ candidate: CandidateID) throws -> DocumentSnapshot
    /// Belongs to…: a contact's or identifier's owner, a person or an
    /// organization; nil clears it.
    func setOwner(_ identity: IdentityID, to owner: IdentityID?) throws -> DocumentSnapshot
    /// × on a merge hint: hidden from now on, in the case too. Not a step.
    func dismissHint(_ hint: LinkHint) throws -> DocumentSnapshot

    // MARK: Edit mode (ADR 0109)

    /// An edit typed in edit mode, committed (never marked IME text): the
    /// text at `range` becomes `text`. Typing joins the undo line beside
    /// review decisions, a run of keystrokes as one "Typing" step; the
    /// change saves once typing pauses. Returns at once with what the edit
    /// view needs, not a snapshot.
    func type(_ range: SourceRange, _ text: String) throws -> Typed
    /// One edit-mode command (a toolbar or formatting-keyboard button, ⌘B,
    /// the link sheet), as the view applied the editor core's edits: one
    /// "Formatting" undo step of its own. The edits are sorted, in UTF-8
    /// offsets of the text before any of them.
    func format(_ edits: [EditorEdit]) throws -> Typed
    /// The next typed edit starts a step of its own: the caret moved.
    func endTyping()
    /// After a typing pause: the names under review found again in what
    /// was typed. Safe off the main thread; typing goes on meanwhile.
    func discover() throws -> DocumentSnapshot
    /// A new document's title follows its first line until the user
    /// renames it (ADR 0109): the title it took, or nil when nothing
    /// changed. Safe off the main thread.
    func followFirstLine() throws -> String?
}

/// Models that fit the device, and verifying and installing downloads.
/// The device's physical RAM is given when the object is made.
protocol Setup: AnyObject, Sendable {
    func snapshot() -> SetupSnapshot
    /// The files of `model` still to download, with their pinned URLs.
    func downloadRequests(for model: String) throws -> [ModelDownloadRequest]
    /// Verifies one downloaded file's size and SHA-256 and moves it into the
    /// model's folder.
    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot
    /// Times one short detector run and keeps the result, right after the
    /// detector installs (ADR 0120). It runs as a scan does: it needs the
    /// memory free, pauses while the phone is too hot and stops when
    /// `observer` is cancelled. A phone past the speed ceiling is below the
    /// floor afterwards.
    func timeDetector(observer: any JobObserver) throws -> SetupSnapshot
}

extension Setup {
    func timeDetector(observer: any JobObserver) throws -> SetupSnapshot { snapshot() }
}

/// The core objects one process uses.
struct Core: Sendable {
    let library: any Library
    let setup: any Setup
}

extension DocumentSession {
    func takeUnlockedDocx() -> UnlockedDocx? { nil }

    func closeOriginal() {}

    func scanEstimate() -> Int? { nil }

    /// Hide name in the core's guessed category.
    func markSelection(_ range: SourceRange) throws -> DocumentSnapshot {
        try markSelection(range, as: nil)
    }
}

extension DocumentSession {
    func recognize(observer: any JobObserver) throws -> DocumentSnapshot {
        try recognize(observer: observer, password: nil)
    }
}
