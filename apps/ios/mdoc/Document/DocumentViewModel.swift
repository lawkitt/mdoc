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
        /// An import of a file that needs no recognition stopped before
        /// its text was read (ADR 0088).
        case finishImport
        case findNames
        case running(JobKind, JobUpdate?)
        /// Another document's job holds the one slot.
        case waiting
        case replaceAll(Int)
        case handOff
    }

    let session: any DocumentSession
    /// The device runs the name detector. Below the floor the document
    /// offers no Find names and says why (ADR 0088).
    let findsNames: Bool
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
    /// Below the floor, where Find names would be: the device cannot find
    /// names, and selecting text marks one (ADR 0088).
    var showsBelowFloorNote: Bool { !findsNames && snapshot.phase == .unscanned }

    /// An import that stopped before its text was read, of a file that needs
    /// no recognition: the document says "Import not finished".
    var importNotFinished: Bool {
        snapshot.phase == .recognizing && !snapshot.kind.needsRecognition
            && snapshot.pagesDone == 0 && !snapshot.resumesOCR
    }

    /// Recognition or the scan needs a model the device has not downloaded.
    var needsSetup: Bool { waitingForSetup != nil }
    /// The job that asked for setup, run again once the models are in.
    private(set) var waitingForSetup: JobKind?

    init(
        session: any DocumentSession, jobs: LongJobRunner, storage: MdocStorage, findsNames: Bool = true,
        onChange: @escaping () -> Void = {}
    ) {
        self.session = session
        self.findsNames = findsNames
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
        case .unscanned where !findsNames:
            // Nothing to find here: marking by selection, then the hand-off.
            return .handOff
        case .recognizing where jobs.current != nil, .unscanned where jobs.current != nil:
            return .waiting
        case .recognizing where importNotFinished:
            return .finishImport
        case .recognizing:
            return .recognize(resume: snapshot.pagesDone > 0 || snapshot.resumesOCR)
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
        waitingForSetup = nil
        let session = session
        let outcome = await jobs.run(.recognize, documentID: id, title: String(localized: "Recognizing \(snapshot.title)")) { observer in
            try session.recognize(observer: observer)
        }
        if case .failed(CoreError.modelMissing) = outcome {
            // Scans, photos and text-less PDFs need OCR.
            waitingForSetup = .recognize
            return
        }
        finish(outcome)
    }

    func findNames() async {
        waitingForSetup = nil
        let session = session
        let outcome = await jobs.run(.scan, documentID: id, title: String(localized: "Finding names in \(snapshot.title)")) { observer in
            try session.scan(observer: observer)
        }
        if case .failed(CoreError.modelMissing) = outcome {
            waitingForSetup = .scan
            return
        }
        finish(outcome)
        // A scan that finds nothing says so, rather than going quietly to
        // Share (ADR 0087).
        if case .finished(let scanned) = outcome, scanned.candidates.isEmpty {
            notice = String(localized: "No names found. Select text to mark one.")
        }
    }

    /// Download & continue: the job that asked for setup runs again.
    /// Not now on the document's setup card: Find names (or Recognize)
    /// returns as the primary action.
    func skipSetup() {
        waitingForSetup = nil
    }

    func continueAfterSetup() async {
        switch waitingForSetup {
        case .recognize: await recognize()
        case .scan: await findNames()
        case .install, nil: break
        }
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

    /// Whether a proposal is left once `candidate` is decided for `scope`:
    /// All occurrences decides the identity's other mentions too.
    func hasProposal(after candidate: CandidateID, scope: Scope) -> Bool {
        guard let decided = snapshot.candidate(candidate) else { return false }
        return snapshot.candidates.contains { other in
            other.state == .proposed && other.id != decided.id
                && (scope == .justThisOne || other.identityID != decided.identityID)
        }
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
                share = ShareItems(items: [TitledText(text: handOff.text, title: snapshot.title)])
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

    /// The restored reply, as the hand-off bar copies it on the Reply tab.
    func copyReply(to pasteboard: (String) -> Void) {
        guard let reply = snapshot.reply else { return }
        pasteboard(reply.restored.text)
        notice = String(localized: "Copied reply")
    }

    func shareReply() {
        guard let reply = snapshot.reply else { return }
        share = ShareItems(items: [TitledText(text: reply.restored.text, title: snapshot.title)])
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
