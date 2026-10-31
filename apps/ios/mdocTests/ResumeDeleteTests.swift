import CryptoKit
import UIKit
import XCTest
@testable import mdoc

/// Delete and drop through the core, and the state that offers Resume after
/// OCR stopped part-way (ADR 0085 Long jobs).
final class ResumeDeleteTests: XCTestCase {
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

    /// A scanned page waiting for OCR, imported but not read.
    private func pendingScan(_ core: Core) throws -> (any DocumentSession, URL) {
        let folder = storage.url(.originals).appendingPathComponent("scan", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let page = folder.appendingPathComponent("page-1.jpg")
        try UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }.write(to: page)
        let session = try core.library.importDocument(.scan([page]), title: "Scan", into: .newCase(name: "Scan"))
        return (session, folder.appendingPathComponent("Scan.jpg"))
    }

    func testDeletingASavedDocumentLeavesItsCase() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("Write to ann@example.com."), title: "Note", into: .newCase(name: "Lee"))
        let saved = try session.recognize(observer: TestObserver())
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let caseID = try XCTUnwrap(saved.caseID).rawValue
        let path = try XCTUnwrap(library.ffi.caseInfo(id: caseID).documents.first)
        XCTAssertTrue(FileManager.default.fileExists(atPath: path))

        try core.library.deleteDocument(session.id)
        XCTAssertFalse(FileManager.default.fileExists(atPath: path))
        // The case stays, without the document.
        XCTAssertEqual(try library.ffi.caseInfo(id: caseID).documents, [])
        XCTAssertTrue(core.library.snapshot().documents.isEmpty)
        XCTAssertEqual(core.library.snapshot().cases.map(\.id.rawValue), [caseID])
    }

    func testDroppingAnImportDiscardsItsKeptOCRPages() throws {
        let core = try core()
        let (session, source) = try pendingScan(core)
        // Pages the core kept for this file, and for another one.
        let digest = SHA256.hash(data: try Data(contentsOf: source)).map { String(format: "%02x", $0) }.joined()
        let kept = storage.url(.imports).appendingPathComponent("\(digest)-0123456789abcdef", isDirectory: true)
        let other = storage.url(.imports).appendingPathComponent("\(String(repeating: "0", count: 64))-0123456789abcdef", isDirectory: true)
        for folder in [kept, other] {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try Data("{}".utf8).write(to: folder.appendingPathComponent("1.json"))
        }

        try core.library.deleteDocument(session.id)
        XCTAssertFalse(FileManager.default.fileExists(atPath: kept.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: other.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
    }

    func testAnImportWhoseOCRStartedOffersResumeAfterARestart() throws {
        let (session, _) = try pendingScan(try core())
        XCTAssertFalse(session.snapshot().resumesOCR)
        // OCR began, then mdoc stopped.
        PendingImports(storage: storage).markOCRStarted(session.id)

        let reopened = try core().library.session(for: session.id)
        XCTAssertTrue(reopened.snapshot().resumesOCR)
        XCTAssertEqual(reopened.snapshot().phase, .recognizing)
    }

    @MainActor
    func testAStoppedOCRImportOffersResumeRecognition() throws {
        let (session, _) = try pendingScan(try core())
        PendingImports(storage: storage).markOCRStarted(session.id)
        let model = DocumentViewModel(
            session: try core().library.session(for: session.id),
            jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        XCTAssertEqual(model.primaryAction, .recognize(resume: true))
    }

    func testTheCoreKeepsOCRPagesInTheProtectedImportsFolder() throws {
        XCTAssertEqual(MdocStorage.Area.imports.rawValue, "imports")
        XCTAssertEqual(MdocStorage.Area.imports.protection, .complete)
        var isFolder: ObjCBool = false
        XCTAssertTrue(FileManager.default.fileExists(atPath: storage.url(.imports).path, isDirectory: &isFolder))
        XCTAssertTrue(isFolder.boolValue)
        let values = try storage.url(.imports).resourceValues(forKeys: [.isExcludedFromBackupKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
    }
}
