import Foundation
import Observation

/// One document screen. It holds the core's latest snapshot and view state
/// only; every change goes through the session and comes back as a new
/// snapshot.
@MainActor
@Observable
final class DocumentViewModel {
    enum Pane: Hashable {
        case text, original, reply
    }

    /// The header's one primary action, following the document's state.
    enum PrimaryAction: Equatable {
        case recognize(resume: Bool)
        case findNames
        case running(JobKind, JobProgress)
        /// Another document's job holds the one slot.
        case waiting
        case replaceAll(Int)
        case handOff
    }

    let session: any DocumentSession
    private let jobs: LongJobRunner
    private let storage: MdocStorage
    private let onChange: () -> Void

    private(set) var snapshot: DocumentSnapshot
    /// The candidate whose sheet is open.
    var reviewing: CandidateID?
    /// The selection Replace… was chosen for.
    var manualSelection: SourceRange?
    var showsIdentities = false
    var pane: Pane = .text
    var showsReplySheet = false
    var share: ShareItems?
    var errorMessage: String?
    /// A short confirmation such as "Copied for AI".
    private(set) var notice: String?
    /// The scan needs a model the device has not downloaded.
    private(set) var needsSetup = false

    init(session: any DocumentSession, jobs: LongJobRunner, storage: MdocStorage, onChange: @escaping () -> Void = {}) {
        self.session = session
        self.jobs = jobs
        self.storage = storage
        self.onChange = onChange
        snapshot = session.snapshot()
    }

    var id: DocumentID { snapshot.id }

    var hasOriginal: Bool { snapshot.original != .none }

    var runningJob: RunningJob? {
        guard let job = jobs.current, job.documentID == id else { return nil }
        return job
    }

    /// A job of this document stopped when mdoc left the screen.
    var interruption: InterruptedJob? {
        guard let job = jobs.interrupted, job.documentID == id else { return nil }
        return job
    }

    var primaryAction: PrimaryAction {
        if let job = runningJob {
            return .running(job.kind, job.progress)
        }
        switch snapshot.phase {
        case .recognizing where jobs.current != nil, .unscanned where jobs.current != nil:
            return .waiting
        case .recognizing:
            return .recognize(resume: snapshot.pagesDone > 0)
        case .unscanned:
            return .findNames
        case .reviewing:
            return .replaceAll(snapshot.pendingCount)
        case .ready:
            return .handOff
        }
    }

    // MARK: Jobs

    func recognize() async {
        let session = session
        let outcome = await jobs.run(.recognize, documentID: id, title: String(localized: "Recognizing \(snapshot.title)")) { observer in
            try session.recognize(observer: observer)
        }
        finish(outcome)
    }

    func findNames() async {
        needsSetup = false
        let session = session
        let outcome = await jobs.run(.scan, documentID: id, title: String(localized: "Finding names in \(snapshot.title)")) { observer in
            try session.scan(observer: observer)
        }
        if case .failed(CoreError.modelMissing) = outcome {
            needsSetup = true
            return
        }
        finish(outcome)
    }

    /// Resume after mdoc left the screen: OCR keeps its finished pages, the
    /// scan reruns.
    func resume() async {
        guard let interruption else { return }
        jobs.dismissInterruption()
        switch interruption.kind {
        case .recognize: await recognize()
        case .scan: await findNames()
        case .install: break
        }
    }

    func cancelJob() {
        jobs.cancel()
    }

    private func finish(_ outcome: JobOutcome<DocumentSnapshot>) {
        switch outcome {
        case .finished(let snapshot):
            apply(snapshot)
        case .cancelled:
            apply(session.snapshot())
        case .failed(let error):
            errorMessage = error.localizedDescription
            apply(session.snapshot())
        case .busy:
            errorMessage = String(localized: "Another job is running. Try again when it finishes.")
        }
    }

    // MARK: Review

    func open(_ candidate: CandidateID) {
        reviewing = candidate
    }

    /// Opens the first proposal after the one under review, wrapping around.
    func reviewNext() {
        let pending = snapshot.candidates.filter { $0.state == .proposed }
        guard !pending.isEmpty else { return }
        let start = reviewing.flatMap { id in snapshot.candidate(id)?.range.start } ?? -1
        reviewing = (pending.first { $0.range.start > start } ?? pending[0]).id
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) {
        perform { try session.decide(candidate, decision, scope: scope) }
    }

    func decide(identity: IdentityID, _ decision: Decision) {
        guard let first = snapshot.mentions(of: identity).first else { return }
        decide(first.id, decision, scope: .allOccurrences)
    }

    func replaceAllPending() {
        perform { try session.replaceAllPending() }
    }

    func beginReplace(_ selection: SourceRange) {
        manualSelection = selection
    }

    func addReplacement(category: Category) {
        guard let selection = manualSelection else { return }
        manualSelection = nil
        perform { try session.addReplacement(selection, category: category) }
    }

    /// The source text of the selection Replace… was chosen for.
    var manualSelectionText: String {
        guard let selection = manualSelection else { return "" }
        let bytes = Array(snapshot.markdown.utf8)
        guard selection.start >= 0, selection.end <= bytes.count, selection.start < selection.end else { return "" }
        return String(decoding: bytes[selection.start..<selection.end], as: UTF8.self)
    }

    // MARK: Hand-off

    /// Copy for AI to the clipboard.
    func copyForAI(to pasteboard: (String) -> Void) {
        do {
            let handOff = try session.copyForAI()
            pasteboard(handOff.text)
            notice = String(localized: "Copied for AI")
            onChange()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Share Copy for AI's text, as text or as a `.md` file.
    func share(asFile: Bool) {
        do {
            let handOff = try session.copyForAI()
            if asFile {
                storage.clear(.handOff)
                let url = try storage.write(Data(handOff.text.utf8), named: Self.safeFileName(handOff.fileName), in: .handOff)
                share = ShareItems(items: [url], temporaryFile: url)
            } else {
                share = ShareItems(items: [handOff.text])
            }
            onChange()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func shareFinished(_ items: ShareItems) {
        if let file = items.temporaryFile {
            try? FileManager.default.removeItem(at: file)
        }
    }

    func clearNotice() {
        notice = nil
    }

    static func safeFileName(_ name: String) -> String {
        let cleaned = name.components(separatedBy: CharacterSet(charactersIn: "/:\\")).joined(separator: "-")
        return cleaned.isEmpty ? "document.md" : cleaned
    }

    // MARK: Restore

    /// Paste reply: restores it with this document's case and opens it.
    func restore(reply: String) {
        let trimmed = reply.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        perform { try session.restoreReply(reply) }
        if snapshot.reply != nil {
            showsReplySheet = false
            pane = .reply
        }
    }

    func clearReply() {
        perform { try session.clearReply() }
        if pane == .reply {
            pane = .text
        }
    }

    // MARK: Helpers

    func refresh() {
        apply(session.snapshot())
    }

    private func perform(_ change: () throws -> DocumentSnapshot) {
        do {
            apply(try change())
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func apply(_ new: DocumentSnapshot) {
        snapshot = new
        if let reviewing, new.candidate(reviewing) == nil {
            self.reviewing = nil
        }
        if new.reply == nil && pane == .reply {
            pane = .text
        }
        onChange()
    }
}
