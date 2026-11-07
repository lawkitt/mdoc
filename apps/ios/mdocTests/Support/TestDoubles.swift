import Foundation
@testable import mdoc

/// A real session whose long jobs (recognize, scan) first take `seconds`,
/// for the timing tests: a job the test can see running and cancel. A
/// cancel during the wait ends the job as the core's cancel does; every
/// other call goes straight to the real session.
final class SlowJobs: DocumentSession, @unchecked Sendable {
    let base: any DocumentSession
    let seconds: Double

    init(_ base: any DocumentSession, seconds: Double) {
        self.base = base
        self.seconds = seconds
    }

    private func wait(_ observer: any JobObserver, stage: CoreStage) throws {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if observer.isCancelled() { throw CoreError.cancelled }
            observer.progress(JobUpdate(stage: stage, fraction: 0))
            Thread.sleep(forTimeInterval: 0.01)
        }
    }

    var id: DocumentID { base.id }
    func snapshot() -> DocumentSnapshot { base.snapshot() }

    func recognize(observer: any JobObserver) throws -> DocumentSnapshot {
        try wait(observer, stage: .recognizing)
        return try base.recognize(observer: observer)
    }

    func scan(observer: any JobObserver) throws -> DocumentSnapshot {
        try wait(observer, stage: .scanning)
        return try base.scan(observer: observer)
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot {
        try base.decide(candidate, decision, scope: scope)
    }
    func replaceAllPending() throws -> DocumentSnapshot { try base.replaceAllPending() }
    func addReplacement(_ range: SourceRange, category: mdoc.Category) throws -> DocumentSnapshot {
        try base.addReplacement(range, category: category)
    }
    func copyForAI() throws -> HandOff { try base.copyForAI() }
    func restoreReply(_ reply: String) throws -> DocumentSnapshot { try base.restoreReply(reply) }
    func clearReply() throws -> DocumentSnapshot { try base.clearReply() }
    func undo() throws -> DocumentSnapshot { try base.undo() }
    func redo() throws -> DocumentSnapshot { try base.redo() }
    func markSelection(_ range: SourceRange) throws -> DocumentSnapshot { try base.markSelection(range) }
    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot { try base.propose(names) }
    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot {
        try base.rename(candidate, scope: scope, to: alias)
    }
    func categoryChoices(for candidate: CandidateID, scope: Scope) throws -> [CategoryChoice] {
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
    func dismissHint(_ hint: MergeHint) throws -> DocumentSnapshot { try base.dismissHint(hint) }
}

/// Model setup without the real files: the setup tests' downloads are
/// empty files (`FakeModelDownloader`), which the real core's checksum
/// check refuses. It offers what the real core offers for the memory,
/// with fixed sizes, and installs whatever it is given.
final class FakeSetup: Setup, @unchecked Sendable {
    static let detectorID = "gliner2-fp16"
    static let ocrID = "pp-ocr-v5"
    static let detectorBytes: Int64 = 1_190_000_000
    static let ocrBytes: Int64 = 31_000_000
    /// The real core's floor (`DETECTOR_FLOOR_BYTES`, 7 GiB).
    static let detectorFloor: UInt64 = 7 * Fixture.gigabyte

    private let lock = NSLock()
    private let physicalMemory: UInt64
    private var installed: Set<String> = []

    init(memory: UInt64) {
        physicalMemory = memory
    }

    private var tier: DeviceTier { physicalMemory >= Self.detectorFloor ? .detector : .belowFloor }

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
            return SetupSnapshot(physicalMemory: physicalMemory, tier: tier, offers: offers)
        }
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
