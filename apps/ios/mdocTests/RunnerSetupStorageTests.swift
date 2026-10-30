import XCTest
@testable import mdoc

@MainActor
final class LongJobRunnerTests: XCTestCase {
    func testOneJobAtATime() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let gate = TestObserver()
        let first = Task {
            await jobs.run(.scan, documentID: Fixture.letter, title: "first") { observer in
                while !gate.isCancelled() && !observer.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                return 1
            }
        }
        while jobs.current == nil { await Task.yield() }
        let second = await jobs.run(.scan, documentID: Fixture.quotation, title: "second") { _ in 2 }
        guard case .busy = second else { return XCTFail("second job should be refused") }
        gate.cancel()
        guard case .finished(1) = await first.value else { return XCTFail("first job should finish") }
        XCTAssertNil(jobs.current)
    }

    func testCancelEndsTheJobAsCancelled() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let run = Task {
            await jobs.run(.recognize, documentID: Fixture.tenancy, title: "job") { observer -> Int in
                while !observer.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                throw CoreError.cancelled
            }
        }
        while jobs.current == nil { await Task.yield() }
        jobs.cancel()
        guard case .cancelled(.user) = await run.value else { return XCTFail("expected a user cancel") }
        XCTAssertNil(jobs.interrupted)
    }

    func testBackgroundExpiryOffersResume() async {
        let background = ExpiringContinuation()
        let jobs = LongJobRunner(background: background)
        let run = Task {
            await jobs.run(.recognize, documentID: Fixture.tenancy, title: "job") { observer -> Int in
                while !observer.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                throw CoreError.cancelled
            }
        }
        while jobs.current == nil { await Task.yield() }
        background.expire()
        guard case .cancelled(.background) = await run.value else { return XCTFail("expected a background cancel") }
        XCTAssertEqual(jobs.interrupted, InterruptedJob(kind: .recognize, documentID: Fixture.tenancy))
        XCTAssertTrue(background.ended)
    }

    func testProgressReachesTheRunner() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let gate = TestObserver()
        let run = Task {
            await jobs.run(.scan, documentID: nil, title: "job") { observer in
                observer.progress(fraction: 0.5, page: nil, pageCount: nil)
                while !gate.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                return 0
            }
        }
        while jobs.current?.progress.fraction != 0.5 { await Task.yield() }
        gate.cancel()
        _ = await run.value
    }
}

/// A background continuation the test expires by hand.
private final class ExpiringContinuation: BackgroundContinuation, @unchecked Sendable {
    private var onExpire: (@Sendable () -> Void)?
    private(set) var ended = false

    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease {
        onExpire = expire
        return Lease(owner: self)
    }

    func expire() {
        onExpire?()
    }

    private struct Lease: BackgroundLease {
        let owner: ExpiringContinuation
        func progress(_ fraction: Double) {}
        func end(success: Bool) { owner.ended = true }
    }
}

@MainActor
final class SetupViewModelTests: XCTestCase {
    private func model(
        expensive: Bool,
        memory: UInt64 = 8 * Fixture.gigabyte,
        downloadSeconds: Double = 0
    ) -> (SetupViewModel, UserDefaults) {
        let defaults = UserDefaults(suiteName: "mdoc-tests-\(UUID().uuidString)")!
        let model = SetupViewModel(
            setup: Fixture.core(modelsInstalled: false, memory: memory).setup,
            downloader: FakeModelDownloader(seconds: downloadSeconds),
            network: FakeNetwork(isExpensive: expensive),
            jobs: LongJobRunner(background: NoBackgroundContinuation()),
            physicalMemory: memory,
            defaults: defaults)
        return (model, defaults)
    }

    func testWiFiDownloadsAndInstalls() async {
        let (model, _) = model(expensive: false)
        XCTAssertTrue(model.showsCard)
        model.download()
        await model.waitForDownload()
        XCTAssertFalse(model.asksCellular)
        XCTAssertFalse(model.snapshot.needsSetup)
        XCTAssertFalse(model.showsCard)
    }

    func testCellularAsksFirst() async {
        let (model, _) = model(expensive: true)
        model.download()
        XCTAssertTrue(model.asksCellular)
        XCTAssertTrue(model.snapshot.needsSetup)
        model.answerCellular(useCellular: true)
        await model.waitForDownload()
        XCTAssertFalse(model.asksCellular)
        XCTAssertFalse(model.snapshot.needsSetup)
    }

    func testDownloadAndContinueRunsTheWaitingScan() async {
        let (model, _) = model(expensive: false)
        var continued = false
        model.download { continued = true }
        await model.waitForDownload()
        XCTAssertTrue(continued)
    }

    func testCancelReturnsToARetryableState() async {
        let (model, _) = model(expensive: false, downloadSeconds: 5)
        var continued = false
        model.download { continued = true }
        XCTAssertTrue(model.isWorking)
        model.cancel()
        await model.waitForDownload()
        XCTAssertFalse(model.isWorking)
        XCTAssertFalse(continued)
        XCTAssertNil(model.errorMessage)
        XCTAssertTrue(model.snapshot.needsSetup)
        XCTAssertTrue(model.snapshot.offers.allSatisfy { model.state(of: $0) == .notInstalled })
    }

    func testCardNamesTheSizeOfWhatFits() {
        let (large, _) = model(expensive: false)
        XCTAssertEqual(large.snapshot.missingBytes, 1_190_000_000 + 31_000_000)
        let (small, _) = model(expensive: false, memory: 4 * Fixture.gigabyte)
        XCTAssertEqual(small.snapshot.tier, .rulesOnly)
        XCTAssertEqual(small.snapshot.missingBytes, 31_000_000)
    }

    func testNotNowIsRemembered() {
        let (model, defaults) = model(expensive: false)
        model.dismissCard()
        XCTAssertFalse(model.showsCard)
        XCTAssertTrue(defaults.bool(forKey: SetupViewModel.dismissedKey))
    }
}

final class StorageTests: XCTestCase {
    func testFoldersAreOutOfBackupsAndProtected() throws {
        let storage = try Fixture.temporaryStorage()
        for area in MdocStorage.Area.allCases {
            let values = try storage.url(area).resourceValues(forKeys: [.isExcludedFromBackupKey])
            XCTAssertEqual(values.isExcludedFromBackup, true, "\(area) is in backups")
        }
        let file = try storage.write(Data("text".utf8), named: "a.md", in: .markdown)
        let values = try file.resourceValues(forKeys: [.isExcludedFromBackupKey, .fileProtectionKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
        XCTAssertEqual(values.fileProtection, .complete)
    }

    func testModelsKeepTheDefaultProtection() {
        XCTAssertNil(MdocStorage.Area.models.protection)
        XCTAssertEqual(MdocStorage.Area.cases.protection, .complete)
    }

    func testImportCopyLeavesTheOriginal() throws {
        let storage = try Fixture.temporaryStorage()
        let source = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).pdf")
        try Data("%PDF".utf8).write(to: source)
        let copy = try storage.importCopy(of: source, in: .documents)
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))
        XCTAssertTrue(copy.path.hasPrefix(storage.url(.documents).path))
    }

    func testClearEmptiesTheHandOffFolder() throws {
        let storage = try Fixture.temporaryStorage()
        try storage.write(Data("x".utf8), named: "x.md", in: .handOff)
        storage.clear(.handOff)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.url(.handOff).path), [])
    }
}
