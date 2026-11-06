import Foundation

// The three coarse objects `crates/mdoc-ffi` exports (ADR 0085). Rust owns
// document, review, case and restore state; these calls return snapshots.
// Long calls block their thread and report through a `JobObserver`, the one
// thing the core calls back into the platform for.

/// Progress and cooperative cancellation for a long core call: the real
/// core reports stages through its `JobProgress` callback and stops when its
/// session's `cancel()` is called.
protocol JobObserver: AnyObject, Sendable {
    func progress(_ update: JobUpdate)
    /// Polled between units of work; true ends the call with `CoreError.cancelled`.
    func isCancelled() -> Bool
    /// Runs `handler` once, when the job is cancelled (at once if it is
    /// already), so a core that stops on its own call can be told.
    func onCancel(_ handler: @escaping @Sendable () -> Void)
}

/// Cases and stored documents.
protocol Library: AnyObject, Sendable {
    func snapshot() -> LibrarySnapshot
    func session(for document: DocumentID) throws -> any DocumentSession
    /// Stores the document and returns its session, ready for `recognize`.
    func importDocument(_ source: ImportSource, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession
    func deleteDocument(_ document: DocumentID) throws
    /// Restores a reply in place with one case's mapping. Needs no model,
    /// so the share extension can call it.
    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult
    /// Keeps `reply`, in alias form, as the document's reply (ADR 0078).
    func saveReply(_ reply: String, to document: DocumentID) throws
    /// Saves the jobs that finished while the phone was locked, and returns
    /// their documents.
    func saveDeferred() throws -> [DocumentID]
}

extension Library {
    func saveDeferred() throws -> [DocumentID] { [] }
}

/// One document's import, OCR, review and hand-off.
protocol DocumentSession: AnyObject, Sendable {
    var id: DocumentID { get }
    func snapshot() -> DocumentSnapshot
    /// Import and OCR. Resumes after the pages already finished.
    func recognize(observer: any JobObserver) throws -> DocumentSnapshot
    /// Finds candidates; a cancelled scan reruns from the start.
    func scan(observer: any JobObserver) throws -> DocumentSnapshot
    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot
    /// Replaces every proposed mention with its proposed alias.
    func replaceAllPending() throws -> DocumentSnapshot
    /// Replace…: a manual identity for the selected source text, replaced
    /// in every exact repeat.
    func addReplacement(_ range: SourceRange, category: Category) throws -> DocumentSnapshot
    /// Copy for AI's text, through the same export path as desktop. Fails
    /// while proposals are pending. Records the document as shared last.
    func copyForAI() throws -> HandOff
    /// Restores `reply` with this document's case and keeps it as the
    /// document's reply.
    func restoreReply(_ reply: String) throws -> DocumentSnapshot
    func clearReply() throws -> DocumentSnapshot

    // MARK: Review bar (ADR 0098)
    //
    // Each change below, and `decide`, `replaceAllPending` and the marks, is
    // one undo step, held in memory while the session lives (256 steps).
    // Copy for AI, Restore reply and Clear reply are not steps.

    /// Undoes the last step. The snapshot's `redoStep` names what it undid
    /// and where, for the toast and to move the bar back.
    func undo() throws -> DocumentSnapshot
    /// Makes the last undone step again; the snapshot's `undoStep` names it.
    func redo() throws -> DocumentSnapshot
    /// Hide name: the selection and its exact repeats become proposals in
    /// the core's guessed category, replacing nothing yet.
    func markSelection(_ range: SourceRange) throws -> DocumentSnapshot
    /// Stored proposals, as a scan's: every occurrence of each wording, in
    /// its category. Try a sample uses it, so the sample reviews before the
    /// name finder is downloaded (ADR 0099).
    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot
    /// Rename…: the alias of this mention (`.justThisOne` splits it off
    /// under the alias) or of its identity. Throws `.aliasTaken` with the
    /// identity that has the alias already: Same as is the way to it.
    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot
    /// The category menu: each category with the alias choosing it gives.
    func categoryChoices(for candidate: CandidateID, scope: Scope) throws -> [CategoryChoice]
    /// The category of this mention (split off in it) or of its identity.
    func setCategory(_ candidate: CandidateID, scope: Scope, to category: Category) throws -> DocumentSnapshot
    /// Same as…: `.allOccurrences` links the mention's identity into
    /// `target`, which keeps its alias and restore wording; `.justThisOne`
    /// moves only this mention. Replaces nothing by itself.
    func sameAs(_ candidate: CandidateID, scope: Scope, target: IdentityID) throws -> DocumentSnapshot
    /// Same as… for a whole identity: an Identities row, or a merge hint.
    func link(_ identity: IdentityID, into target: IdentityID) throws -> DocumentSnapshot
    /// New alias: this mention split off as an identity of its own.
    func newAlias(_ candidate: CandidateID) throws -> DocumentSnapshot
    /// Belongs to…: a contact's or identifier's owner, a person or an
    /// organization; nil clears it.
    func setOwner(_ identity: IdentityID, to owner: IdentityID?) throws -> DocumentSnapshot
    /// × on a merge hint: hidden from now on, in the case too. Not a step.
    func dismissHint(_ hint: MergeHint) throws -> DocumentSnapshot
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
}

/// The core objects one process uses.
struct Core: Sendable {
    let library: any Library
    let setup: any Setup
}
