import XCTest
@testable import mdoc

@MainActor
final class LongJobRunnerTests: XCTestCase {
    func testOneJobAtATime() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let gate = TestObserver()
        let first = Task {
            await jobs.run(.scan, documentID: DocumentID(rawValue: "letter"), title: "first") { observer in
                while !gate.isCancelled() && !observer.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                return 1
            }
        }
        while jobs.current == nil { await Task.yield() }
        let second = await jobs.run(.scan, documentID: DocumentID(rawValue: "quotation"), title: "second") { _ in 2 }
        guard case .busy = second else { return XCTFail("second job should be refused") }
        gate.cancel()
        guard case .finished(1) = await first.value else { return XCTFail("first job should finish") }
        XCTAssertNil(jobs.current)
    }

    func testCancelEndsTheJobAsCancelled() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let run = Task {
            await jobs.run(.recognize, documentID: DocumentID(rawValue: "tenancy"), title: "job") { observer -> Int in
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
            await jobs.run(.recognize, documentID: DocumentID(rawValue: "tenancy"), title: "job") { observer -> Int in
                while !observer.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                throw CoreError.cancelled
            }
        }
        while jobs.current == nil { await Task.yield() }
        background.expire()
        guard case .cancelled(.background) = await run.value else { return XCTFail("expected a background cancel") }
        XCTAssertEqual(jobs.interrupted, InterruptedJob(kind: .recognize, documentID: DocumentID(rawValue: "tenancy")))
        XCTAssertTrue(background.ended)
    }

    func testProgressReachesTheRunner() async {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let gate = TestObserver()
        let run = Task {
            await jobs.run(.scan, documentID: nil, title: "job") { observer in
                observer.progress(JobUpdate(stage: .scanning, fraction: 0.5))
                while !gate.isCancelled() {
                    Thread.sleep(forTimeInterval: 0.01)
                }
                return 0
            }
        }
        while jobs.current?.progress?.fraction != 0.5 { await Task.yield() }
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
        network: FakeNetwork = FakeNetwork(.wifi),
        memory: UInt64 = 8 * Fixture.gigabyte,
        downloadSeconds: Double = 0,
        downloader: (any ModelDownloader)? = nil
    ) -> (SetupViewModel, Preferences) {
        let preferences = Fixture.preferences()
        let model = SetupViewModel(
            setup: FakeSetup(memory: memory),
            downloader: downloader ?? FakeModelDownloader(seconds: downloadSeconds),
            network: network,
            jobs: LongJobRunner(background: NoBackgroundContinuation()),
            preferences: preferences)
        return (model, preferences)
    }

    func testWiFiDownloadsAndInstalls() async {
        let (model, _) = model()
        XCTAssertTrue(model.showsCard)
        model.download()
        await model.waitForDownload()
        XCTAssertFalse(model.asksCellular)
        XCTAssertFalse(model.snapshot.needsSetup)
        XCTAssertFalse(model.showsCard)
    }

    /// Any cellular interface asks, whatever iOS's expensive flag says
    /// (ADR 0088): `NetworkMonitor` reports it as `.cellular`.
    func testCellularAsksFirst() async {
        let (model, _) = model(network: FakeNetwork(.cellular))
        model.download()
        XCTAssertTrue(model.asksCellular)
        XCTAssertTrue(model.snapshot.needsSetup)
        model.answerCellular(useCellular: true)
        await model.waitForDownload()
        XCTAssertFalse(model.asksCellular)
        XCTAssertFalse(model.snapshot.needsSetup)
    }

    func testWaitForWiFiKeepsTheWiFiSession() async {
        let downloader = HeldDownloader()
        let (model, _) = model(network: FakeNetwork(.cellular), downloader: downloader)
        model.download()
        model.answerCellular(useCellular: false)
        while downloader.cellular.isEmpty { await Task.yield() }
        XCTAssertEqual(downloader.cellular, [false])
        XCTAssertTrue(model.waitsForWiFi)
        model.cancel()
        await model.waitForDownload()
    }

    /// A download held for Wi-Fi says so, and Use cellular asks the
    /// question; a yes starts it again on the cellular session (ADR 0088).
    func testWaitingForWiFiOffersCellular() async throws {
        let network = FakeNetwork(.wifi)
        let downloader = HeldDownloader()
        let (model, _) = model(network: network, downloader: downloader)
        var continued = false
        model.download { continued = true }
        while downloader.cellular.isEmpty { await Task.yield() }
        XCTAssertFalse(model.waitsForWiFi)
        let detector = try XCTUnwrap(model.snapshot.offers.first { $0.purpose == .detector })
        XCTAssertEqual(model.state(of: detector), .downloading(fraction: 0))

        network.change(to: .cellular)
        XCTAssertTrue(model.waitsForWiFi)
        XCTAssertEqual(model.state(of: detector), .waitingForWiFi)
        model.askCellular()
        XCTAssertTrue(model.asksCellular)
        model.answerCellular(useCellular: true)
        XCTAssertFalse(model.waitsForWiFi)
        await model.waitForDownload()

        XCTAssertEqual(downloader.cellular.first, false)
        XCTAssertTrue(downloader.cellular.dropFirst().allSatisfy { $0 })
        XCTAssertFalse(model.isWorking)
        XCTAssertFalse(model.snapshot.needsSetup)
        XCTAssertNil(model.errorMessage)
        XCTAssertTrue(continued)
    }

    func testOfflineWaitsForWiFi() async {
        let downloader = HeldDownloader()
        let (model, _) = model(network: FakeNetwork(.offline), downloader: downloader)
        model.download()
        XCTAssertFalse(model.asksCellular)
        while downloader.cellular.isEmpty { await Task.yield() }
        XCTAssertTrue(model.waitsForWiFi)
        model.cancel()
        await model.waitForDownload()
        XCTAssertFalse(model.isWorking)
        XCTAssertFalse(model.waitsForWiFi)
    }

    func testDownloadAndContinueRunsTheWaitingScan() async {
        let (model, _) = model()
        var continued = false
        model.download { continued = true }
        await model.waitForDownload()
        XCTAssertTrue(continued)
    }

    /// One model downloads on its own, and its continuation runs once that
    /// model is in, with the other still missing (ADR 0090).
    func testOneModelDownloadsAlone() async throws {
        let (model, _) = model()
        let ocr = try XCTUnwrap(model.missing(.ocr).first)
        var continued = false
        model.download([ocr]) { continued = true }
        XCTAssertEqual(model.requested, [ocr.id])
        await model.waitForDownload()
        XCTAssertTrue(continued)
        XCTAssertTrue(model.missing(.ocr).isEmpty)
        XCTAssertEqual(model.missing(.detector).count, 1)
        XCTAssertEqual(model.snapshot.missingBytes, 1_190_000_000)
        // Download all then takes only what is left.
        model.download()
        XCTAssertEqual(model.requested, model.missing(.detector).map(\.id))
        await model.waitForDownload()
        XCTAssertFalse(model.snapshot.needsSetup)
    }

    /// The cellular question names the size of what was asked for.
    func testCellularQuestionNamesTheRequestedSize() async throws {
        let (model, _) = model(network: FakeNetwork(.cellular))
        let ocr = try XCTUnwrap(model.missing(.ocr).first)
        model.download([ocr])
        XCTAssertTrue(model.asksCellular)
        XCTAssertEqual(model.requestedBytes, 31_000_000)
        model.answerCellular(useCellular: true)
        await model.waitForDownload()
        XCTAssertTrue(model.missing(.ocr).isEmpty)
        XCTAssertFalse(model.missing(.detector).isEmpty)
    }

    func testCancelReturnsToARetryableState() async {
        let (model, _) = model(downloadSeconds: 5)
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

    /// Below the floor only text recognition is offered, and Home shows no
    /// setup card: it waits in the document that needs it (ADR 0088).
    func testCardNamesTheSizeOfWhatFits() {
        let (large, _) = model()
        XCTAssertEqual(large.snapshot.missingBytes, 1_190_000_000 + 31_000_000)
        XCTAssertTrue(large.showsCard)
        let (small, _) = model(memory: Fixture.belowFloor)
        XCTAssertEqual(small.snapshot.tier, .belowFloor)
        XCTAssertEqual(small.snapshot.missingBytes, 31_000_000)
        XCTAssertFalse(small.showsCard)
        XCTAssertTrue(small.snapshot.offers.allSatisfy { $0.purpose == .ocr })
    }

    func testNotNowIsRemembered() {
        let (model, preferences) = model()
        model.dismissCard()
        XCTAssertFalse(model.showsCard)
        XCTAssertTrue(preferences.setupCardDismissed)
    }
}

final class StorageTests: XCTestCase {
    func testFoldersAreOutOfBackupsAndProtected() throws {
        let storage = try Fixture.temporaryStorage()
        for area in MdocStorage.Area.allCases {
            let values = try storage.url(area).resourceValues(forKeys: [.isExcludedFromBackupKey])
            XCTAssertEqual(values.isExcludedFromBackup, true, "\(area) is in backups")
        }
        for folder in [storage.vault, storage.intake] {
            XCTAssertEqual(try folder.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup, true)
        }
        let file = try storage.write(Data("text".utf8), named: "a.md", in: .handOff)
        let values = try file.resourceValues(forKeys: [.isExcludedFromBackupKey, .fileProtectionKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
        #if !targetEnvironment(simulator)
        // The Simulator reports every file as complete until first unlock.
        XCTAssertEqual(values.fileProtection, .complete)
        #endif
    }

    /// The Vault and the intake folder sit beside the old library's folder,
    /// never inside it (#1091 Decision 2), and preparing the storage makes
    /// none of the old library's folders.
    func testTheVaultIsASiblingOfTheOldLibrary() throws {
        let storage = try MdocStorage.shared(folder: "mdoc-sibling-test", vaultFolder: "vault-sibling-test")
        defer {
            for folder in [storage.root, storage.vault, storage.intake] { try? FileManager.default.removeItem(at: folder) }
        }
        let base = storage.root.deletingLastPathComponent()
        XCTAssertEqual(storage.vault, base.appendingPathComponent("vault-sibling-test", isDirectory: true))
        XCTAssertEqual(storage.intake, base.appendingPathComponent("vault-sibling-test-intake", isDirectory: true))
        try storage.prepare()
        for old in ["documents", "cases", "imports", "originals"] {
            XCTAssertFalse(FileManager.default.fileExists(atPath: storage.root.appendingPathComponent(old).path), old)
        }
        XCTAssertFalse(storage.url(.intake).path.hasPrefix(storage.root.path + "/"))
        XCTAssertFalse(storage.url(.ocr).path.hasPrefix(storage.root.path + "/"))
    }

    /// The restore place's file sits beside the Vault, outside the old
    /// library's folder, named for the Vault: the app and the Restore
    /// extension open the same storage, so they name the same file. Written
    /// by the app's path and read by the extension's, it names the place.
    func testTheRestorePlaceFileIsBesideTheVault() throws {
        let release = try MdocStorage.shared()
        let base = release.vault.deletingLastPathComponent()
        XCTAssertEqual(release.restorePlace, base.appendingPathComponent("restore-place.json"))
        XCTAssertFalse(release.restorePlace.path.hasPrefix(release.root.path + "/"))
        let dev = try MdocStorage.shared(folder: "mdoc-ui-testing", vaultFolder: "vault-dev")
        XCTAssertEqual(dev.restorePlace, base.appendingPathComponent("restore-place-dev.json"))

        let storage = try MdocStorage.shared(folder: "mdoc-place-test", vaultFolder: "vault-place-test")
        defer { try? FileManager.default.removeItem(at: storage.restorePlace) }
        XCTAssertEqual(storage.restorePlace.lastPathComponent, "restore-place-place-test.json")
        let place = RestorePlace.document(DocumentID(rawValue: String(repeating: "a", count: 32)))
        try RestorePlaceFile(storage).write(place)
        let again = try MdocStorage.shared(folder: "mdoc-place-test", vaultFolder: "vault-place-test")
        XCTAssertEqual(RestorePlaceFile(again).read(), place)
        let values = try storage.restorePlace.resourceValues(forKeys: [.isExcludedFromBackupKey, .fileProtectionKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
        #if !targetEnvironment(simulator)
        XCTAssertEqual(values.fileProtection, .complete)
        #endif
    }

    func testModelsKeepTheDefaultProtection() {
        XCTAssertNil(MdocStorage.Area.models.protection)
        XCTAssertEqual(MdocStorage.Area.intake.protection, .complete)
        XCTAssertEqual(MdocStorage.Area.ocr.protection, .complete)
    }

    func testImportCopyLeavesTheOriginal() throws {
        let storage = try Fixture.temporaryStorage()
        let source = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).pdf")
        try Data("%PDF".utf8).write(to: source)
        let copy = try storage.importCopy(of: source, in: .intake, named: "Contract")
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))
        XCTAssertTrue(copy.path.hasPrefix(storage.url(.intake).path))
        XCTAssertEqual(copy.lastPathComponent, "Contract.pdf")
    }

    func testClearEmptiesTheHandOffFolder() throws {
        let storage = try Fixture.temporaryStorage()
        try storage.write(Data("x".utf8), named: "x.md", in: .handOff)
        storage.clear(.handOff)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.url(.handOff).path), [])
    }
}
