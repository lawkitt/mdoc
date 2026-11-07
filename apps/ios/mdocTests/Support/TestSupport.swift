import Foundation
import XCTest
@testable import mdoc

enum Fixture {
    static let gigabyte: UInt64 = 1_073_741_824
    /// Below the detector floor: the core scans with its structured rules
    /// and no model, so the tests need none.
    static let belowFloor: UInt64 = 4 * gigabyte
    /// At the detector tier: a scan needs the name detector, which the
    /// tests never download, so it asks for setup.
    static let detectorTier: UInt64 = 8 * gigabyte

    /// Where every test's library goes. Most tests leave theirs; the first
    /// use in a run clears what earlier runs left.
    private static let temporaryFolder: URL = {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-tests", isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        return folder
    }()

    static func temporaryStorage() throws -> MdocStorage {
        let root = temporaryFolder.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let storage = MdocStorage(root: root)
        try storage.prepare()
        return storage
    }

    /// The repository, for its test fixtures: the Simulator reads the Mac's files.
    static let repository = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()

    /// A copy of the repository's `path` in a folder of its own under the
    /// storage's originals, as the app's intake leaves a file: an import
    /// renames its file to the title.
    static func copy(_ path: String, into storage: MdocStorage) throws -> URL {
        let source = repository.appendingPathComponent(path)
        let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let copy = folder.appendingPathComponent(source.lastPathComponent)
        try FileManager.default.copyItem(at: source, to: copy)
        return copy
    }

    /// Preferences of a test's own, in a fresh store.
    static func preferences() -> Preferences {
        Preferences(store: UserDefaults(suiteName: "mdoc-tests-\(UUID().uuidString)")!)
    }

    /// The app's model over `seeded`, with preferences of its own.
    @MainActor
    static func app(_ seeded: Seeded) -> AppModel {
        AppModel(
            core: seeded.core, storage: seeded.storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), preferences: preferences())
    }

    /// The real core over fresh temporary storage, with a fixed case key.
    static func core(memory: UInt64 = belowFloor) throws -> (Core, MdocStorage) {
        let storage = try temporaryStorage()
        return (try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: memory), storage)
    }

    /// The fictional library (`Letters`) on the real core: the quotation,
    /// replaced and shared, and the letter waiting for review in "Ellison v
    /// Brightwater"; the tenancy scan, not yet recognized, in "Harbour
    /// Street lease". The letter's and quotation's names are proposed as
    /// Try a sample proposes its own, so no model is needed.
    static func seeded(memory: UInt64 = belowFloor) throws -> Seeded {
        let (core, storage) = try core(memory: memory)
        let library = core.library
        let pages = try Letters.pageImages(
            Letters.tenancyPages, named: "tenancy", in: storage.url(.originals).appendingPathComponent(UUID().uuidString))
        let tenancy = try library.importDocument(.scan(pages), title: Letters.tenancyTitle, into: .newCase(name: Letters.harbourCase))
        let quotation = try library.importDocument(.text(Letters.quotation), title: Letters.quotationTitle, into: .newCase(name: Letters.ellisonCase))
        _ = try quotation.recognize(observer: TestObserver())
        _ = try quotation.propose(Letters.names)
        _ = try quotation.replaceAllPending()
        _ = try quotation.copyForAI()
        let ellison = try XCTUnwrap(quotation.snapshot().caseID)
        let letter = try library.importDocument(.text(Letters.letter), title: Letters.letterTitle, into: .existing(ellison))
        _ = try letter.recognize(observer: TestObserver())
        _ = try letter.propose(Letters.names)
        // The core dates a saved document by its file, to the second: the
        // letter an hour ago and the quotation two, so the order is the
        // fixture's (the tenancy, an import not yet read, dates from now).
        for (session, hours) in [(letter, 1.0), (quotation, 2.0)] {
            let path = try XCTUnwrap((session as? RealSession)?.path)
            try FileManager.default.setAttributes(
                [.modificationDate: Date().addingTimeInterval(-hours * 3600)], ofItemAtPath: path)
        }
        return Seeded(
            core: core, storage: storage, ellison: ellison, harbour: try XCTUnwrap(tenancy.snapshot().caseID),
            letter: letter.id, quotation: quotation.id, tenancy: tenancy.id)
    }
}

/// `Fixture.seeded()`: the real core and the ids of what it holds.
struct Seeded {
    let core: Core
    let storage: MdocStorage
    let ellison: CaseID
    let harbour: CaseID
    let letter: DocumentID
    let quotation: DocumentID
    let tenancy: DocumentID

    var library: any Library { core.library }

    func session(_ id: DocumentID) throws -> any DocumentSession {
        try core.library.session(for: id)
    }

    /// A document's view model over its real session; `slow` delays its
    /// long jobs (`SlowJobs`).
    @MainActor
    func model(
        _ id: DocumentID, jobs: LongJobRunner = LongJobRunner(background: NoBackgroundContinuation()),
        findsNames: Bool = true, slow seconds: Double? = nil
    ) throws -> DocumentViewModel {
        let session = try session(id)
        return DocumentViewModel(
            session: seconds.map { SlowJobs(session, seconds: $0) } ?? session, jobs: jobs, storage: storage,
            findsNames: findsNames)
    }
}

/// A connection the test controls.
@MainActor
final class FakeNetwork: NetworkConditions {
    private(set) var route: NetworkRoute
    var onChange: ((NetworkRoute) -> Void)?

    init(_ route: NetworkRoute) {
        self.route = route
    }

    func change(to route: NetworkRoute) {
        self.route = route
        onChange?(route)
    }
}

/// Holds a Wi-Fi-only download until it is cancelled, as the background
/// session does on cellular; a cellular one finishes at once.
final class HeldDownloader: ModelDownloader, @unchecked Sendable {
    private let lock = NSLock()
    private var calls: [Bool] = []

    /// `allowCellular` of each download started, in order.
    var cellular: [Bool] { lock.withLock { calls } }

    func download(
        _ request: ModelDownloadRequest,
        allowCellular: Bool,
        progress: @escaping @Sendable (Double) -> Void
    ) async throws -> URL {
        lock.withLock { calls.append(allowCellular) }
        if !allowCellular {
            try await Task.sleep(for: .seconds(600))
        }
        progress(1)
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(request.modelID)-\(UUID().uuidString).download")
        try Data().write(to: file)
        return file
    }
}

/// Records a job's progress and cancels on request.
final class TestObserver: JobObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private var handlers: [@Sendable () -> Void] = []
    private(set) var updates: [JobUpdate] = []

    func cancel() {
        lock.lock()
        cancelled = true
        let handlers = self.handlers
        self.handlers = []
        lock.unlock()
        handlers.forEach { $0() }
    }

    func progress(_ update: JobUpdate) {
        lock.lock()
        updates.append(update)
        lock.unlock()
    }

    func onCancel(_ handler: @escaping @Sendable () -> Void) {
        lock.lock()
        if cancelled {
            lock.unlock()
            handler()
        } else {
            handlers.append(handler)
            lock.unlock()
        }
    }

    func isCancelled() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }
}
