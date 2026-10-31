import Foundation
@testable import mdoc

enum Fixture {
    static let letter = DocumentID(rawValue: "doc-letter")
    static let quotation = DocumentID(rawValue: "doc-quotation")
    static let tenancy = DocumentID(rawValue: "doc-tenancy")
    static let ellison = CaseID(rawValue: "case-ellison")
    static let harbour = CaseID(rawValue: "case-harbour")

    static let gigabyte: UInt64 = 1_073_741_824

    static func core(modelsInstalled: Bool = true, timing: FakeCore.Timing = .instant, memory: UInt64 = 8 * gigabyte) -> Core {
        FakeCore.make(FakeCore.Options(
            timing: timing, modelsInstalled: modelsInstalled, originalsFolder: nil, physicalMemory: memory))
    }

    static func temporaryStorage() throws -> MdocStorage {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("mdoc-tests-\(UUID().uuidString)", isDirectory: true)
        let storage = MdocStorage(root: root)
        try storage.prepare()
        return storage
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
