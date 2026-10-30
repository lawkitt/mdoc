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
struct FakeNetwork: NetworkConditions {
    var isExpensive: Bool
}

/// Records a job's progress and cancels on request.
final class TestObserver: JobObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private(set) var fractions: [Double] = []

    func cancel() {
        lock.lock()
        cancelled = true
        lock.unlock()
    }

    func progress(fraction: Double, page: Int?, pageCount: Int?) {
        lock.lock()
        fractions.append(fraction)
        lock.unlock()
    }

    func isCancelled() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }
}
