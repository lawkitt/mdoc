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
        try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
    }

    /// A scanned page waiting for OCR, imported but not read.
    private func pendingScan(_ core: Core) throws -> (any mdoc.DocumentSession, URL) {
        let folder = storage.url(.intake).appendingPathComponent("scan", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let page = folder.appendingPathComponent("page-1.jpg")
        try UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }.write(to: page)
        let session = try core.library.importDocument(.scan([page]), title: "Scan", into: nil)
        return (session, folder.appendingPathComponent("Scan.jpg"))
    }

    func testDeletingASavedDocumentLeavesItsProject() throws {
        let core = try core()
        let projectID = try Fixture.createProject("Ann", in: core)
        let session = try core.library.importDocument(.text("Write to ann@example.com."), title: "Note", into: projectID)
        let saved = try session.recognize(observer: TestObserver())
        XCTAssertEqual(saved.projectID, projectID)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let vaultID = try XCTUnwrap((session as? RealSession)?.vaultID)
        XCTAssertEqual(try library.vault.projectPage(id: projectID.rawValue).documents.map(\.id), [vaultID])

        try core.library.deleteDocument(session.id)
        XCTAssertThrowsError(try library.vault.document(id: vaultID))
        // The Project stays, without the document.
        XCTAssertEqual(try library.vault.projectPage(id: projectID.rawValue).documents, [])
        XCTAssertTrue(core.library.snapshot().documents.isEmpty)
        XCTAssertEqual(core.library.snapshot().projects.map(\.id), [projectID])
    }

    func testDroppingAnImportDiscardsItsKeptOCRPages() throws {
        let core = try core()
        let (session, source) = try pendingScan(core)
        // Pages the core kept for this file, and for another one.
        let digest = SHA256.hash(data: try Data(contentsOf: source)).map { String(format: "%02x", $0) }.joined()
        let kept = storage.url(.ocr).appendingPathComponent("\(digest)-0123456789abcdef", isDirectory: true)
        let other = storage.url(.ocr).appendingPathComponent("\(String(repeating: "0", count: 64))-0123456789abcdef", isDirectory: true)
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
        XCTAssertNil(reopened.snapshot().phase, "recognizing")
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

    /// The core starts recognition on every PDF once the model is in, so a
    /// stopped PDF import reads as an import, never as missing OCR (ADR 0088).
    @MainActor
    func testAStoppedPDFImportOffersFinishImport() throws {
        let pdf = storage.url(.intake).appendingPathComponent("statement.pdf")
        try Data("%PDF-1.4".utf8).write(to: pdf)
        let session = try core().library.importDocument(.file(pdf, .pdf), title: "Statement", into: nil)
        PendingImports(storage: storage).markOCRStarted(session.id)
        let reopened = try core().library.session(for: session.id)
        XCTAssertTrue(reopened.snapshot().resumesOCR)
        let model = DocumentViewModel(
            session: reopened, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        XCTAssertTrue(model.importNotFinished)
        XCTAssertEqual(model.primaryAction, .finishImport)
    }

    /// The library's folders as a locked phone leaves them (Data Protection
    /// Complete): unwritable, and `ProtectedData` says so.
    private func lock() throws {
        try Fixture.setVault(storage, writable: false)
        ProtectedData.isAvailable = false
    }

    /// The folders writable again; `ProtectedData` still says locked until
    /// the test unlocks.
    private func makeWritable() throws {
        try Fixture.setVault(storage, writable: true)
    }

    /// A document deleted while its scan's save waits for the phone to
    /// unlock: the unlock writes nothing, so it does not come back.
    func testADeletedDocumentsSaveOnUnlockIsDropped() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("Write to ann@example.com."), title: "Note", into: nil)
        _ = try session.recognize(observer: TestObserver())
        defer {
            try? makeWritable()
            ProtectedData.isAvailable = true
        }
        try lock()
        XCTAssertEqual(try session.scan(observer: TestObserver()).phase, .reviewing, "saved on unlock")
        try makeWritable()
        try core.library.deleteDocument(session.id)
        ProtectedData.isAvailable = true
        XCTAssertEqual(try core.library.saveDeferred(), [])
        XCTAssertTrue(core.library.snapshot().documents.isEmpty)
        XCTAssertTrue(try self.core().library.snapshot().documents.isEmpty, "not saved again")
    }

    /// A new document emptied and left while its typing's save waits for
    /// the phone to unlock: discarded, it stays gone after the unlock.
    func testADiscardedDocumentsSaveOnUnlockIsDropped() throws {
        let core = try core()
        let session = try core.library.importDocument(.newDocument, title: "New document", into: nil)
        _ = try session.recognize(observer: TestObserver())
        _ = try session.type(SourceRange(start: 0, end: 0), "Hello")
        try session.saveChanges()
        defer {
            try? makeWritable()
            ProtectedData.isAvailable = true
        }
        try lock()
        _ = try session.type(SourceRange(start: 5, end: 5), " there")
        try session.saveChanges()
        try makeWritable()
        _ = try session.type(SourceRange(start: 0, end: 11), "")
        XCTAssertTrue(try core.library.discardIfEmpty(session.id))
        ProtectedData.isAvailable = true
        XCTAssertEqual(try core.library.saveDeferred(), [])
        XCTAssertTrue(try self.core().library.snapshot().documents.isEmpty, "not saved again")
    }

    /// A delete the Vault refuses (its folder cannot be written) leaves the
    /// document as it was: still listed, and its next changes saved.
    func testAFailedDeleteLeavesTheDocumentAsItWas() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("Write to ann@example.com."), title: "Note", into: nil)
        _ = try session.recognize(observer: TestObserver())
        XCTAssertEqual(try session.scan(observer: TestObserver()).phase, .reviewing)
        try Fixture.setVault(storage, writable: false)
        defer { try? Fixture.setVault(storage, writable: true) }
        XCTAssertThrowsError(try core.library.deleteDocument(session.id))
        try Fixture.setVault(storage, writable: true)
        XCTAssertEqual(try session.replaceAllPending().phase, .ready)
        try core.library.saveChanges()
        XCTAssertEqual(try self.core().library.snapshot().documents.map(\.phase), [.ready])
    }

    /// A job on a deleted document stops as it starts, and what it would
    /// have found is not saved: the document does not come back.
    func testAJobOnADeletedDocumentSavesNothing() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("Write to ann@example.com."), title: "Note", into: nil)
        _ = try session.recognize(observer: TestObserver())
        try core.library.deleteDocument(session.id)
        XCTAssertThrowsError(try session.scan(observer: TestObserver())) { error in
            XCTAssertEqual(error as? CoreError, .cancelled)
        }
        XCTAssertTrue(try self.core().library.snapshot().documents.isEmpty, "not saved again")
    }

    /// OCR's finished pages go to the intake folder, never the old
    /// library's `imports` (#1109's note for item 9).
    func testTheCoreKeepsOCRPagesInTheProtectedIntakeFolder() throws {
        XCTAssertEqual(storage.url(.ocr), storage.intake.appendingPathComponent("ocr", isDirectory: true))
        XCTAssertEqual(MdocStorage.Area.ocr.protection, .complete)
        var isFolder: ObjCBool = false
        XCTAssertTrue(FileManager.default.fileExists(atPath: storage.url(.ocr).path, isDirectory: &isFolder))
        XCTAssertTrue(isFolder.boolValue)
        let values = try storage.url(.ocr).resourceValues(forKeys: [.isExcludedFromBackupKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
    }
}
