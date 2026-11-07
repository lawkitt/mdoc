import Foundation
import struct MdocFFI.ModelFile
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

    /// A name kept everywhere stays in Identities, under Kept (ADR 0098).
    func testAKeptNameStaysInTheList() throws {
        let core = try core()
        let session = try reviewed(core)
        let email = try XCTUnwrap(session.snapshot().candidates.first { $0.category == .email })
        let kept = try session.decide(email.id, .keep, scope: .allOccurrences)
        XCTAssertEqual(kept.identities.first { $0.id == email.identityID }?.state, .kept)
    }

    /// Try a sample: its stored names become proposals with no model.
    func testTheSampleProposesItsNamesWithoutAScan() throws {
        let core = try core()
        let sample = SampleDocument.english
        let session = try core.library.importDocument(.text(sample.text), title: sample.title, into: .newCase(name: sample.title))
        _ = try session.recognize(observer: TestObserver())
        let proposed = try session.propose(sample.names)
        XCTAssertEqual(proposed.phase, .reviewing)
        XCTAssertTrue(proposed.candidates.contains { $0.original == "Maya Bennett" && $0.category == .person })
        XCTAssertTrue(proposed.candidates.contains { $0.original == "maya.bennett@example.com" && $0.category == .email })
        XCTAssertTrue(proposed.candidates.allSatisfy { $0.state == .proposed })
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

    /// Below the floor a document added to a case can arrive with the
    /// case's names proposed: Replace N comes before the hand-off, which the
    /// core refuses while proposals wait (ADR 0088).
    @MainActor
    func testBelowTheFloorCaseProposalsComeBeforeShare() throws {
        let core = try core()
        let first = try reviewed(core)
        let email = try XCTUnwrap(first.snapshot().candidates.first { $0.category == .email })
        _ = try first.decide(email.id, .replace, scope: .allOccurrences)
        let caseID = try XCTUnwrap(first.snapshot().caseID)

        let second = try core.library.importDocument(
            .text("Write to maya.ellison@example.com today."), title: "Note", into: .existing(caseID))
        let imported = try second.recognize(observer: TestObserver())
        XCTAssertEqual(imported.phase, .unscanned)
        XCTAssertGreaterThan(imported.pendingCount, 0)
        let model = DocumentViewModel(
            session: second, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage,
            findsNames: false)
        XCTAssertEqual(model.primaryAction, .replaceAll(imported.pendingCount))
        XCTAssertTrue(model.showsBelowFloorNote)
        model.replaceAllPending()
        XCTAssertEqual(model.primaryAction, .handOff)
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

    /// A scan that found nothing is still a scan (#673): after mdoc
    /// restarts the document is ready, not offered Find names again.
    func testAScanThatFoundNothingStaysScannedAfterARestart() throws {
        let session = try core().library.importDocument(
            .text("# Note\n\nNothing to hide here."), title: "Note", into: .newCase(name: "Plain"))
        _ = try session.recognize(observer: TestObserver())
        let scanned = try session.scan(observer: TestObserver())
        XCTAssertTrue(scanned.candidates.isEmpty)
        XCTAssertEqual(scanned.phase, .ready)
        let reopened = try core().library
        XCTAssertEqual(reopened.snapshot().documents.first?.phase, .ready)
        let id = try XCTUnwrap(reopened.snapshot().documents.first?.id)
        XCTAssertEqual(try reopened.session(for: id).snapshot().phase, .ready)
    }

    /// Category menus list categories in `mdoc_pii::Category`'s order
    /// (#682): Identity, Tax and Bank before Case number and Date.
    func testCategoriesFollowTheCoresOrder() {
        XCTAssertEqual(
            Category.allCases.map(\.rawValue),
            ["person", "organization", "email", "url", "phone", "address", "identity", "tax", "bank", "caseNumber", "date", "other"])
    }

    /// File names from titles follow the core's rule (#683, `file_stem` in
    /// `crates/mdoc-ffi/src/library.rs`): a look-alike for each character a
    /// file name cannot hold, control characters as "-", no leading dot,
    /// at most 240 bytes before the extension, cut on a character.
    func testFileNamesFollowTheCoresRule() {
        XCTAssertEqual(MdocStorage.fileName("Claim: Bennett", extension: "pdf"), "Claim\u{A789} Bennett.pdf")
        XCTAssertEqual(
            MdocStorage.fileName("Why? <draft> \"A|B\" *", extension: "docx"),
            "Why\u{FF1F} \u{FF1C}draft\u{FF1E} \u{FF02}A\u{2223}B\u{FF02} \u{2217}.docx")
        XCTAssertEqual(MdocStorage.fileName(".hidden", extension: "pdf"), "hidden.pdf", "never a hidden file")
        XCTAssertEqual(MdocStorage.fileName("Tab\there", extension: ""), "Tab-here")
        XCTAssertEqual(MdocStorage.fileName("  ", extension: "pdf"), "Document.pdf")
        let long = MdocStorage.fileName(String(repeating: "ж", count: 200), extension: "pdf")
        XCTAssertEqual(long, String(repeating: "ж", count: 120) + ".pdf", "240 bytes, whole letters")
    }

    func testReopenedLibraryKeepsTheDocumentAndCase() throws {
        let session = try reviewed(core())
        _ = try session.replaceAllPending()
        let reopened = try core().library.snapshot()
        XCTAssertEqual(reopened.documents.count, 1)
        XCTAssertEqual(reopened.documents.first?.phase, .ready)
        XCTAssertEqual(reopened.cases.map(\.name), ["Ellison"])
    }

    /// Find names that ends while the phone is locked cannot write the
    /// library (Data Protection Complete): the result shows at once and is
    /// saved on unlock. Unlocked, a failed save is still an error.
    func testAScanThatEndsWhileLockedIsSavedOnUnlock() throws {
        let core = try core()
        let session = try core.library.importDocument(.text(Self.letter), title: "Letter", into: .newCase(name: "Ellison"))
        _ = try session.recognize(observer: TestObserver())
        let documents = storage.url(.documents)
        // Unwritable, as the library's folders are once the phone locks.
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: documents.path)
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: documents.path)
            ProtectedData.isAvailable = true
        }
        XCTAssertThrowsError(try session.scan(observer: TestObserver()), "unlocked, a failed save is an error")
        ProtectedData.isAvailable = false
        let scanned = try session.scan(observer: TestObserver())
        XCTAssertEqual(scanned.phase, .reviewing)
        XCTAssertEqual(try self.core().library.snapshot().documents.first?.phase, .unscanned, "not saved while locked")
        XCTAssertEqual(try core.library.saveDeferred(), [], "still locked")
        // Unlocked but still failing (a full disk): reported, and kept.
        ProtectedData.isAvailable = true
        XCTAssertThrowsError(try core.library.saveDeferred())
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: documents.path)
        XCTAssertEqual(try core.library.saveDeferred(), [session.id])
        XCTAssertEqual(try self.core().library.snapshot().documents.first?.phase, .reviewing)
        XCTAssertEqual(try core.library.saveDeferred(), [], "saved once")
    }

    /// The sample's proposals, when the phone locks before they save: in
    /// review at once, saved on unlock, as a scan's result.
    func testSampleProposalsWhileLockedAreSavedOnUnlock() throws {
        let core = try core()
        let sample = SampleDocument.english
        let session = try core.library.importDocument(.text(sample.text), title: sample.title, into: .newCase(name: sample.title))
        _ = try session.recognize(observer: TestObserver())
        let documents = storage.url(.documents)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: documents.path)
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: documents.path)
            ProtectedData.isAvailable = true
        }
        ProtectedData.isAvailable = false
        let proposed = try session.propose(sample.names)
        XCTAssertEqual(proposed.phase, .reviewing)
        ProtectedData.isAvailable = true
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: documents.path)
        XCTAssertEqual(try core.library.saveDeferred(), [session.id])
        XCTAssertEqual(try self.core().library.snapshot().documents.first?.phase, .reviewing)
    }

    /// ADR 0098 through the real core: a mark proposes in the guessed
    /// category, decisions and identity changes undo one step at a time,
    /// and each saves into the case without taking an undone step back.
    func testReviewBarCallsUndoAndSaveThroughTheCore() throws {
        let core = try core()
        let session = try reviewed(core)
        let text = session.snapshot().markdown
        let range = try XCTUnwrap(text.range(of: "Ms Ellison"))
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let marked = try session.markSelection(SourceRange(start: start, end: start + "Ms Ellison".utf8.count))
        XCTAssertTrue(marked.markdown.contains("Ms Ellison"), "a mark proposes, it does not replace")
        let ellison = try XCTUnwrap(marked.candidates.first { $0.original == "Ms Ellison" })
        XCTAssertEqual(ellison.state, .proposed)
        XCTAssertEqual(marked.undoStep?.kind, .mark)
        XCTAssertEqual(marked.undoStep?.candidateID, ellison.id)

        let email = try XCTUnwrap(marked.candidates.first { $0.category == .email })
        let replaced = try session.decide(email.id, .replace, scope: .allOccurrences)
        XCTAssertEqual(replaced.undoStep?.label, "Replace · maya.ellison@example.com")
        let alias = try XCTUnwrap(replaced.candidates.first { $0.identityID == email.identityID })
        let renamed = try session.rename(alias.id, scope: .allOccurrences, to: "CONTACT")
        XCTAssertTrue(renamed.markdown.contains("CONTACT"))
        XCTAssertEqual(renamed.undoStep?.label, "Rename · CONTACT")
        let contact = try XCTUnwrap(renamed.candidates.first { $0.alias == "CONTACT" })
        XCTAssertEqual(renamed.undoStep?.candidateID, contact.id)
        let name = try XCTUnwrap(renamed.candidates.first { $0.original == "Ms Ellison" })
        XCTAssertThrowsError(try session.rename(name.id, scope: .allOccurrences, to: "CONTACT")) { error in
            XCTAssertEqual(error as? CoreError, .aliasTaken(alias: "CONTACT", by: email.identityID))
        }
        XCTAssertThrowsError(try session.rename(name.id, scope: .allOccurrences, to: "no spaces")) { error in
            guard case .refused = error as? CoreError else { return XCTFail("\(error)") }
        }

        let undone = try session.undo()
        XCTAssertFalse(undone.markdown.contains("CONTACT"))
        XCTAssertEqual(undone.redoStep?.kind, .rename)
        // Saved after the undo, reopened: still undone.
        let reopened = try core.library.session(for: session.id).snapshot()
        XCTAssertFalse(reopened.markdown.contains("CONTACT"))
        XCTAssertTrue(try session.redo().markdown.contains("CONTACT"))

        let choices = try session.categoryChoices(for: name.id, scope: .allOccurrences)
        XCTAssertEqual(choices.count, Category.allCases.count)
        XCTAssertEqual(choices.filter(\.isCurrent).map(\.category), [name.category])
        var person = name
        if person.category != .person {
            let recategorized = try session.setCategory(person.id, scope: .allOccurrences, to: .person)
            XCTAssertEqual(recategorized.undoStep?.label, "Category · Person")
            person = try XCTUnwrap(recategorized.candidates.first { $0.original == "Ms Ellison" })
        }
        // The address belongs to Ms Ellison.
        let owned = try session.setOwner(email.identityID, to: person.identityID)
        XCTAssertEqual(owned.identities.first { $0.id == email.identityID }?.owner, person.identityID)
        XCTAssertEqual(owned.undoStep?.kind, .owner)

        let split = try session.newAlias(contact.id)
        XCTAssertEqual(split.undoStep?.kind, .newAlias)
        let moved = try XCTUnwrap(split.undoStep?.candidateID)
        XCTAssertNotEqual(split.candidate(moved)?.identityID, email.identityID)
        XCTAssertFalse(split.markdown.contains("CONTACT"))
        let linked = try session.sameAs(moved, scope: .justThisOne, target: email.identityID)
        XCTAssertEqual(linked.undoStep?.kind, .sameAs)
        XCTAssertTrue(linked.markdown.contains("CONTACT"))
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

    /// A title lists as typed (no "Sample- services agreement"), and Rename
    /// keeps the open session: it saves on to the renamed file, and the
    /// case named after the document follows it.
    func testRenameKeepsTheTitleAsTypedAndTheSession() throws {
        let core = try core()
        let title = "Sample: services agreement"
        let session = try core.library.importDocument(.text(Self.letter), title: title, into: .newCase(name: title))
        _ = try session.recognize(observer: TestObserver())
        XCTAssertEqual(core.library.snapshot().documents.map(\.title), [title])

        try core.library.renameDocument(session.id, to: "Services: final")
        XCTAssertEqual(session.snapshot().title, "Services: final")
        let renamed = core.library.snapshot()
        XCTAssertEqual(renamed.documents.map(\.title), ["Services: final"])
        XCTAssertEqual(renamed.cases.map(\.name), ["Services: final"])
        XCTAssertEqual(renamed.documents.first?.id, session.id)
        _ = try session.scan(observer: TestObserver())
        XCTAssertEqual(try session.replaceAllPending().phase, .ready)
        XCTAssertThrowsError(try core.library.renameDocument(session.id, to: "  ")) { error in
            XCTAssertEqual(error as? CoreError, .refused("A document needs a title."))
        }

        try core.library.renameCase(try XCTUnwrap(renamed.cases.first?.id), to: "Ellison")
        let reopened = try self.core().library.snapshot()
        XCTAssertEqual(reopened.documents.map(\.title), ["Services: final"])
        XCTAssertEqual(reopened.documents.first?.phase, .ready)
        XCTAssertEqual(reopened.cases.map(\.name), ["Ellison"])
    }

    /// Move to case re-aliases the document for the case it joins; the case
    /// it left stays, and Undo moves it back with the old aliases and takes
    /// its names out of the case it joined.
    func testMoveToCaseAndUndoThroughTheCore() throws {
        let core = try core()
        let letter = try reviewed(core)
        let before = try letter.replaceAllPending().markdown
        XCTAssertTrue(before.contains("EMAIL_1"))
        let note = try core.library.importDocument(
            .text("Write to someone@example.org."), title: "Note", into: .newCase(name: "Other"))
        _ = try note.recognize(observer: TestObserver())
        _ = try note.scan(observer: TestObserver())
        XCTAssertTrue(try note.replaceAllPending().markdown.contains("EMAIL_1"))
        let other = try XCTUnwrap(note.snapshot().caseID)

        let move = try core.library.moveDocument(letter.id, to: .existing(other))
        XCTAssertEqual(move.to.id, other)
        XCTAssertEqual(move.to.name, "Other")
        XCTAssertTrue(move.added.contains("EMAIL_2"))
        let moved = letter.snapshot()
        XCTAssertEqual(moved.caseID, other)
        XCTAssertFalse(moved.markdown.contains("EMAIL_1"))
        XCTAssertTrue(moved.markdown.contains("EMAIL_2"))
        XCTAssertEqual(try core.library.restore(reply: "EMAIL_2", in: other).text, "maya.ellison@example.com")
        let ellison = try XCTUnwrap(move.from)
        XCTAssertEqual(Set(core.library.snapshot().cases.map(\.name)), ["Ellison", "Other"])
        XCTAssertTrue(core.library.snapshot().documents(in: ellison).isEmpty)
        XCTAssertThrowsError(try core.library.moveDocument(letter.id, to: .existing(other)))

        let undone = try core.library.undoMove(move)
        XCTAssertEqual(undone.to.id, ellison)
        XCTAssertEqual(letter.snapshot().markdown, before)
        XCTAssertEqual(letter.snapshot().caseID, ellison)
        XCTAssertEqual(try core.library.restore(reply: "EMAIL_2", in: other).unresolvedCount, 1)
        XCTAssertEqual(try core.library.restore(reply: "EMAIL_1", in: other).text, "someone@example.org")

        let fresh = try core.library.moveDocument(letter.id, to: .newCase(name: "Fresh"))
        XCTAssertEqual(fresh.to.name, "Fresh")
        XCTAssertEqual(letter.snapshot().markdown, before)
    }

    func testSetupOffersOCRBelowTheFloorAndTheDetectorAbove() throws {
        let small = RealSetup(storage: storage, physicalMemory: 4 * Fixture.gigabyte).snapshot()
        XCTAssertEqual(small.tier, .belowFloor)
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

extension RealCoreTests {
    func testAPendingImportSurvivesARestart() throws {
        let first = try core().library.importDocument(.text(Self.letter), title: "Letter", into: .newCase(name: "Ellison"))
        // mdoc stops before recognition runs; the next launch finds it.
        let core = try core()
        let pending = try XCTUnwrap(core.library.snapshot().documents.first { $0.id == first.id })
        XCTAssertEqual(pending.phase, .recognizing)
        let session = try core.library.session(for: first.id)
        XCTAssertEqual(try session.recognize(observer: TestObserver()).phase, .unscanned)
        // Saved, it is no longer pending: one document, not two.
        XCTAssertEqual(try self.core().library.snapshot().documents.count, 1)
    }

    func testDeletingAnImportRemovesItsSourceFiles() throws {
        let core = try core()
        let folder = storage.url(.originals).appendingPathComponent("import", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pdf = folder.appendingPathComponent("upload.pdf")
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
            NSString(string: "Reply to maya.ellison@example.com").draw(at: CGPoint(x: 20, y: 20), withAttributes: nil)
        }
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: .newCase(name: "Statement"))
        let imported = try session.recognize(observer: TestObserver())
        XCTAssertEqual(imported.original, .pdf(folder.appendingPathComponent("Statement.pdf")))
        try core.library.deleteDocument(session.id)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
        XCTAssertTrue(core.library.snapshot().documents.isEmpty)
    }

    func testDeletingAPendingImportRemovesItsSourceFiles() throws {
        let core = try core()
        let folder = storage.url(.originals).appendingPathComponent("scan", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let page = folder.appendingPathComponent("page-1.jpg")
        try UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }.write(to: page)
        let session = try core.library.importDocument(.scan([page]), title: "Scan", into: .newCase(name: "Scan"))
        try core.library.deleteDocument(session.id)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
        XCTAssertTrue(try self.core().library.snapshot().documents.isEmpty)
    }

    func testRestoreCountsDistinctValues() throws {
        let session = try reviewed(core())
        let ready = try session.replaceAllPending()
        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)
        let bogus = "EMAIL_99"
        let restored = try session.restoreReply("\(alias), \(alias), \(bogus), \(bogus).")
        XCTAssertEqual(restored.reply?.restored.restoredCount, 1)
        XCTAssertEqual(restored.reply?.restored.unresolvedCount, 1)
    }

    func testARetryFetchesOnlyTheFilesStillMissing() {
        let files = [
            ModelFile(path: "a.onnx", url: "https://example.com/a", sha256: "", bytes: 100),
            ModelFile(path: "b.onnx", url: "https://example.com/b", sha256: "", bytes: 50),
        ]
        XCTAssertEqual(RealSetup.remaining(files, installed: ["a.onnx"], missingBytes: 50).map(\.path), ["b.onnx"])
        // A record that disagrees with the core fetches everything.
        XCTAssertEqual(RealSetup.remaining(files, installed: ["a.onnx"], missingBytes: 150).map(\.path), ["a.onnx", "b.onnx"])
        XCTAssertEqual(RealSetup.remaining(files, installed: [], missingBytes: 150).count, 2)
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
