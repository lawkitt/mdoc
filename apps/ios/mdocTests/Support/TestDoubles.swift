import Foundation
@testable import mdoc
import MdocFFI

/// A real session whose long jobs (recognize, scan) first take `seconds`,
/// for the timing tests: a job the test can see running and cancel. A
/// cancel during the wait ends the job as the core's cancel does; every
/// other call goes straight to the real session.
final class SlowJobs: mdoc.DocumentSession, @unchecked Sendable {
    let base: any mdoc.DocumentSession
    let seconds: Double
    /// What `scanEstimate` answers, as a timed phone's core would.
    var estimate: Int?

    init(_ base: any mdoc.DocumentSession, seconds: Double) {
        self.base = base
        self.seconds = seconds
    }

    private func wait(_ observer: any JobObserver, stage: JobStage) throws {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if observer.isCancelled() { throw CoreError.cancelled }
            observer.progress(JobUpdate(stage: stage, fraction: 0))
            Thread.sleep(forTimeInterval: 0.01)
        }
    }

    var id: DocumentID { base.id }
    func snapshot() -> DocumentSnapshot { base.snapshot() }

    func recognize(observer: any JobObserver, password: String?) throws -> DocumentSnapshot {
        try wait(observer, stage: .recognizing)
        return try base.recognize(observer: observer, password: password)
    }

    func scan(observer: any JobObserver) throws -> DocumentSnapshot {
        try wait(observer, stage: .scanning)
        return try base.scan(observer: observer)
    }

    func scanEstimate() -> Int? { estimate }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot {
        try base.decide(candidate, decision, scope: scope)
    }
    func notAName(_ candidate: CandidateID, scope: Scope) throws -> DocumentSnapshot {
        try base.notAName(candidate, scope: scope)
    }
    func replaceAllPending() throws -> DocumentSnapshot { try base.replaceAllPending() }
    func copyForAI() throws -> HandOff { try base.copyForAI() }
    func saveChanges() throws { try base.saveChanges() }
    func restoreReply(_ reply: String) throws -> DocumentSnapshot { try base.restoreReply(reply) }
    func showReply(_ id: ReplyID) -> DocumentSnapshot { base.showReply(id) }
    func clearReply() throws -> DocumentSnapshot { try base.clearReply() }
    func type(_ range: SourceRange, _ text: String) throws -> Typed { try base.type(range, text) }
    func format(_ edits: [EditorEdit]) throws -> Typed { try base.format(edits) }
    func endTyping() { base.endTyping() }
    func discover() throws -> DocumentSnapshot { try base.discover() }
    func followFirstLine() throws -> String? { try base.followFirstLine() }
    func undo() throws -> DocumentSnapshot { try base.undo() }
    func redo() throws -> DocumentSnapshot { try base.redo() }
    func hideTarget(_ range: SourceRange) throws -> WordsTarget { try base.hideTarget(range) }
    func markSelection(_ range: SourceRange, as category: mdoc.Category?) throws -> DocumentSnapshot {
        try base.markSelection(range, as: category)
    }
    func fixWordsDraft(_ candidate: CandidateID) throws -> WordsDraft { try base.fixWordsDraft(candidate) }
    func fixWordsTarget(_ candidate: CandidateID, words: SourceRange) throws -> WordsTarget {
        try base.fixWordsTarget(candidate, words: words)
    }
    func fixWords(_ candidate: CandidateID, words: SourceRange) throws -> DocumentSnapshot {
        try base.fixWords(candidate, words: words)
    }
    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot { try base.propose(names) }
    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot {
        try base.rename(candidate, scope: scope, to: alias)
    }
    func categoryChoices(for candidate: mdoc.Candidate, scope: Scope) throws -> [CategoryChoice] {
        try base.categoryChoices(for: candidate, scope: scope)
    }
    func setCategory(_ candidate: CandidateID, scope: Scope, to category: mdoc.Category) throws -> DocumentSnapshot {
        try base.setCategory(candidate, scope: scope, to: category)
    }
    func sameAs(_ candidate: CandidateID, scope: Scope, target: IdentityID) throws -> DocumentSnapshot {
        try base.sameAs(candidate, scope: scope, target: target)
    }
    func link(_ identity: IdentityID, into target: IdentityID) throws -> DocumentSnapshot {
        try base.link(identity, into: target)
    }
    func newAlias(_ candidate: CandidateID) throws -> DocumentSnapshot { try base.newAlias(candidate) }
    func setOwner(_ identity: IdentityID, to owner: IdentityID?) throws -> DocumentSnapshot {
        try base.setOwner(identity, to: owner)
    }
    func dismissHint(_ hint: LinkHint) throws -> DocumentSnapshot { try base.dismissHint(hint) }
}

/// Model setup without the real files: the setup tests' downloads are
/// empty files (`FakeModelDownloader`), which the real core's checksum
/// check refuses. It offers what the real core offers for the memory,
/// with fixed sizes, and installs whatever it is given.
final class FakeSetup: mdoc.Setup, @unchecked Sendable {
    static let detectorID = "gliner2-fp16"
    static let ocrID = "pp-ocr-v5"
    static let detectorBytes: Int64 = 1_190_000_000
    static let ocrBytes: Int64 = 31_000_000
    /// The real core's floor (`DETECTOR_FLOOR_BYTES`, 3.25 GiB).
    static let detectorFloor: UInt64 = 13 * Fixture.gigabyte / 4

    private let lock = NSLock()
    private let physicalMemory: UInt64
    private var installed: Set<String> = []
    /// What the detector's timed run finds: too slow, or 62 s per 10 pages.
    var timesTooSlow = false
    private var timed = false

    init(memory: UInt64) {
        physicalMemory = memory
    }

    private var tooSlow: Bool { timed && timesTooSlow }
    private var tier: DeviceTier { physicalMemory >= Self.detectorFloor && !tooSlow ? .detector : .belowFloor }

    func snapshot() -> SetupSnapshot {
        lock.withLock {
            var offers = [
                ModelOffer(
                    id: Self.ocrID, name: "Text recognition", purpose: .ocr, byteSize: Self.ocrBytes,
                    state: installed.contains(Self.ocrID) ? .installed : .notInstalled),
            ]
            if tier == .detector {
                offers.insert(ModelOffer(
                    id: Self.detectorID, name: "Name detector", purpose: .detector, byteSize: Self.detectorBytes,
                    state: installed.contains(Self.detectorID) ? .installed : .notInstalled), at: 0)
            }
            return SetupSnapshot(
                physicalMemory: physicalMemory, tier: tier, offers: offers,
                belowFloor: tooSlow ? .speed : physicalMemory < Self.detectorFloor ? .memory : nil,
                tenPagesSeconds: timed ? 62 : nil)
        }
    }

    func timeDetector(observer: any JobObserver) throws -> SetupSnapshot {
        lock.withLock {
            if installed.contains(Self.detectorID) { timed = true }
        }
        return snapshot()
    }

    func downloadRequests(for model: String) throws -> [ModelDownloadRequest] {
        guard model == Self.detectorID || model == Self.ocrID else { throw CoreError.notFound }
        // A reserved example host: the fake downloader never fetches it.
        return [ModelDownloadRequest(
            modelID: model, file: "model.onnx", url: URL(string: "https://models.example.invalid/\(model)")!,
            byteSize: model == Self.detectorID ? Self.detectorBytes : Self.ocrBytes)]
    }

    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot {
        _ = lock.withLock { installed.insert(request.modelID) }
        return snapshot()
    }
}
