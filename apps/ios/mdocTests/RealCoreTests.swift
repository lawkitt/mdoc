import Foundation
import UIKit
import XCTest
@testable import mdoc

/// The app's protocols over the Rust core: import, scan with the structured
/// rules (no model), review, hand-off and restore, through `RealCore`.
final class RealCoreTests: XCTestCase {
    private var storage: MdocStorage!

    override func setUpWithError() throws {
        storage = try Fixture.temporaryStorage()
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: storage.root)
    }

    private func core() throws -> Core {
        try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: 4 * Fixture.gigabyte)
    }

    private static let letter = """
        # Letter

        Please reply to maya.ellison@example.com or call 01632 960 418.
        Ms Ellison's account: GB82 WEST 1234 5698 7654 32.
        """

    /// Import, rules-only scan and a review, saved into the case it started.
    private func reviewed(_ core: Core) throws -> any DocumentSession {
        let session = try core.library.importDocument(.text(Self.letter), title: "Letter", into: .newCase(name: "Ellison"))
        XCTAssertEqual(session.snapshot().phase, .recognizing)
        let imported = try session.recognize(observer: TestObserver())
        XCTAssertEqual(imported.phase, .unscanned)
        XCTAssertEqual(imported.caseName, "Ellison")
        let scanned = try session.scan(observer: TestObserver())
        XCTAssertEqual(scanned.phase, .reviewing)
        XCTAssertTrue(scanned.candidates.contains { $0.original == "maya.ellison@example.com" && $0.category == .email })
        return session
    }

    func testImportScanAndReviewThroughTheCore() throws {
        let core = try core()
        let session = try reviewed(core)
        let email = try XCTUnwrap(session.snapshot().candidates.first { $0.category == .email })
        let after = try session.decide(email.id, .replace, scope: .allOccurrences)
        XCTAssertFalse(after.markdown.contains("maya.ellison@example.com"))
        XCTAssertTrue(after.markdown.contains(email.alias))
        // Offsets are source bytes and land on the alias.
        let replaced = try XCTUnwrap(after.candidates.first { $0.identityID == email.identityID })
        let bytes = Array(after.markdown.utf8)
        XCTAssertEqual(String(decoding: bytes[replaced.range.start..<replaced.range.end], as: UTF8.self), email.alias)
    }

    func testHomeListsTheSavedDocumentInItsCase() throws {
        let core = try core()
        let session = try reviewed(core)
        let snapshot = core.library.snapshot()
        let document = try XCTUnwrap(snapshot.documents.first { $0.id == session.id })
        XCTAssertEqual(document.title, "Letter")
        XCTAssertEqual(snapshot.cases.first { $0.id == document.caseID }?.name, "Ellison")
        XCTAssertGreaterThan(document.pendingCount, 0)
    }

    func testShareWaitsForReviewThenRestoresWithTheCase() throws {
        let core = try core()
        let session = try reviewed(core)
        XCTAssertThrowsError(try session.copyForAI())
        let ready = try session.replaceAllPending()
        XCTAssertEqual(ready.phase, .ready)
        let handOff = try session.copyForAI()
        XCTAssertTrue(handOff.text.hasPrefix("Note: names and other identifying details"))
        XCTAssertFalse(handOff.text.contains("maya.ellison@example.com"))
        XCTAssertEqual(handOff.fileName, "Letter.md")
        XCTAssertEqual(core.library.snapshot().currentCase, ready.caseID)

        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)
        let restored = try session.restoreReply("Write to \(alias) by Friday. \(alias.dropLast())9 is new.")
        XCTAssertEqual(restored.reply?.restored.text.hasPrefix("Write to maya.ellison@example.com by Friday."), true)
        XCTAssertEqual(restored.reply?.restored.unresolvedCount, 1)
        XCTAssertEqual(restored.reply?.aliasText.hasPrefix("Write to \(alias)"), true)
    }

    @MainActor
    func testTheExtensionRestoresWithTheCurrentCase() throws {
        let core = try core()
        let session = try reviewed(core)
        let ready = try session.replaceAllPending()
        _ = try session.copyForAI()
        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)

        // The extension opens its own library on the same folder and key.
        let library = try RealCore.library(storage: storage, keys: FixedCaseKeyStore())
        let model = RestoreViewModel(library: library, reply: "Reply to \(alias).")
        XCTAssertEqual(model.selectedCase, ready.caseID)
        XCTAssertEqual(model.result?.text, "Reply to maya.ellison@example.com.")
        let target = try XCTUnwrap(model.defaultSaveTarget)
        model.save(to: target.id)
        XCTAssertEqual(model.savedTo, target.id)
        XCTAssertEqual(session.snapshot().reply?.aliasText, "Reply to \(alias).")
    }

    func testReopenedLibraryKeepsTheDocumentAndCase() throws {
        let session = try reviewed(core())
        _ = try session.replaceAllPending()
        let reopened = try core().library.snapshot()
        XCTAssertEqual(reopened.documents.count, 1)
        XCTAssertEqual(reopened.documents.first?.phase, .ready)
        XCTAssertEqual(reopened.cases.map(\.name), ["Ellison"])
    }

    func testManualReplacementReplacesEveryRepeat() throws {
        let session = try reviewed(core())
        let text = session.snapshot().markdown
        let range = try XCTUnwrap(text.range(of: "Ellison"))
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let after = try session.addReplacement(SourceRange(start: start, end: start + "Ellison".utf8.count), category: .person)
        XCTAssertFalse(after.markdown.contains("Ms Ellison's"))
    }

    func testAScanNeedsOCRToReadAPhoto() throws {
        let core = try core()
        let image = UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }
        let folder = storage.url(.originals).appendingPathComponent("photo", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent("page-1.jpg")
        try image.write(to: url)
        let session = try core.library.importDocument(.photos([url]), title: "Receipt", into: .newCase(name: "Receipt"))
        XCTAssertThrowsError(try session.recognize(observer: TestObserver())) { error in
            XCTAssertEqual(error as? CoreError, .modelMissing)
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: folder.appendingPathComponent("Receipt.jpg").path))
    }

    func testSetupOffersOCRBelowTheFloorAndTheDetectorAbove() throws {
        let small = RealSetup(storage: storage, physicalMemory: 4 * Fixture.gigabyte).snapshot()
        XCTAssertEqual(small.tier, .rulesOnly)
        XCTAssertEqual(small.offers.map(\.purpose), [.ocr])
        let large = RealSetup(storage: storage, physicalMemory: 8 * Fixture.gigabyte).snapshot()
        XCTAssertEqual(large.tier, .detector)
        XCTAssertTrue(large.offers.contains { $0.purpose == .detector })
        XCTAssertGreaterThan(large.missingBytes, 100_000_000)
        let requests = try RealSetup(storage: storage, physicalMemory: 8 * Fixture.gigabyte).downloadRequests(for: "gliner2-fp16")
        XCTAssertFalse(requests.isEmpty)
        XCTAssertTrue(requests.allSatisfy { $0.url.scheme == "https" })
    }

    func testAWrongDownloadIsRefused() throws {
        let setup = RealSetup(storage: storage, physicalMemory: 4 * Fixture.gigabyte)
        let request = try XCTUnwrap(setup.downloadRequests(for: "ocr-english-russian").first)
        let file = storage.url(.downloads).appendingPathComponent("wrong")
        try Data("not a model".utf8).write(to: file)
        XCTAssertThrowsError(try setup.install(request, from: file))
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
    }
}

/// The background downloader's delegate path, on an ordinary session (tests
/// cannot run a background one) and a local file.
final class BackgroundModelDownloaderTests: XCTestCase {
    func testADownloadLandsInStagingAndIsReusedNextTime() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-download-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("model.onnx")
        try Data(repeating: 7, count: 4096).write(to: source)
        let downloader = BackgroundModelDownloader(staging: root.appendingPathComponent("staging")) { _, _ in .ephemeral }
        let request = ModelDownloadRequest(modelID: "ocr", file: "det/model.onnx", url: source, byteSize: 4096)
        let file = try await downloader.download(request, allowCellular: false) { _ in }
        XCTAssertEqual(file.path, root.appendingPathComponent("staging/ocr/det/model.onnx").path)
        XCTAssertEqual(try Data(contentsOf: file).count, 4096)
        try FileManager.default.removeItem(at: source)
        let again = try await downloader.download(request, allowCellular: false) { _ in }
        XCTAssertEqual(again, file)
    }
}

/// The Keychain store, on the Simulator's keychain under a test service.
final class KeychainCaseKeyStoreTests: XCTestCase {
    func testTheKeyIsMadeOnceAndKept() throws {
        let store = KeychainCaseKeyStore(service: "io.github.lawkitt.mdoc.tests", account: UUID().uuidString)
        defer { store.remove() }
        let first = try store.key()
        XCTAssertEqual(first.count, 32)
        XCTAssertEqual(try store.key(), first)
    }
}
