import Foundation

// The three coarse objects `crates/mdoc-ffi` exports (ADR 0085). Rust owns
// document, review, case and restore state; these calls return snapshots.
// Long calls block their thread and report through a `JobObserver`, the one
// thing the core calls back into the platform for.

/// Progress and cooperative cancellation for a long core call. A UniFFI
/// callback interface on the real core.
protocol JobObserver: AnyObject, Sendable {
    /// `fraction` runs from 0 to 1; `page` counts OCR pages when known.
    func progress(fraction: Double, page: Int?, pageCount: Int?)
    /// Polled between units of work; true ends the call with `CoreError.cancelled`.
    func isCancelled() -> Bool
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
}

/// Models that fit the device, and verifying and installing downloads.
protocol Setup: AnyObject, Sendable {
    func snapshot(physicalMemory: UInt64) -> SetupSnapshot
    func downloadRequest(for model: String) throws -> ModelDownloadRequest
    /// Verifies the downloaded file's SHA-256 and installs it.
    func install(model: String, from file: URL, observer: any JobObserver) throws -> SetupSnapshot
}

/// The core objects one process uses.
struct Core: Sendable {
    let library: any Library
    let setup: any Setup
}
