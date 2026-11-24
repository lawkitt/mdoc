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
        try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
    }

    private static let letter = """
        # Letter

        Please reply to maya.ellison@example.com or call 01632 960 418.
        Ms Ellison's account: GB82 WEST 1234 5698 7654 32.
        """

    /// Import, rules-only scan and a review, saved into the Project
    /// "Ellison", which it was started from.
    private func reviewed(_ core: Core) throws -> any mdoc.DocumentSession {
        let ellison = try Fixture.createProject("Ellison", in: core)
        let session = try core.library.importDocument(.text(Self.letter), title: "Letter", into: ellison)
        XCTAssertNil(session.snapshot().phase, "recognizing")
        let imported = try session.recognize(observer: TestObserver())
        XCTAssertEqual(imported.phase, .unscanned)
        XCTAssertEqual(imported.projectName, "Ellison")
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
        let kept = try session.decide(email.id, .keep, scope: .all)
        XCTAssertEqual(kept.identities.first { $0.id == email.identityID }?.state, .kept)
    }

    /// Try a sample: its stored names become proposals with no model.
    func testTheSampleProposesItsNamesWithoutAScan() throws {
        let core = try core()
        let sample = SampleDocument.english
        let session = try core.library.importDocument(.text(sample.text), title: sample.title, into: nil)
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
        let after = try session.decide(email.id, .replace, scope: .all)
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
    func testBelowTheFloorProjectProposalsComeBeforeShare() throws {
        let core = try core()
        let first = try reviewed(core)
        let email = try XCTUnwrap(first.snapshot().candidates.first { $0.category == .email })
        _ = try first.decide(email.id, .replace, scope: .all)
        let projectID = try XCTUnwrap(first.snapshot().projectID)

        let second = try core.library.importDocument(
            .text("Write to maya.ellison@example.com today."), title: "Note", into: projectID)
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

    func testHomeListsTheSavedDocumentInItsProject() throws {
        let core = try core()
        let session = try reviewed(core)
        let snapshot = core.library.snapshot()
        let document = try XCTUnwrap(snapshot.documents.first { $0.id == session.id })
        XCTAssertEqual(document.title, "Letter")
        XCTAssertEqual(snapshot.projects.first { $0.id == document.projectID }?.name, "Ellison")
        XCTAssertGreaterThan(document.pendingCount, 0)
    }

    func testShareWaitsForReviewThenRestoresWithTheProject() throws {
        let core = try core()
        let session = try reviewed(core)
        // The core refuses while proposals wait, with how many.
        let pending = session.snapshot().pendingCount
        XCTAssertThrowsError(try session.copyForAI()) { error in
            XCTAssertEqual(error as? CoreError, .pendingReview(pending))
        }
        let ready = try session.replaceAllPending()
        XCTAssertEqual(ready.phase, .ready)
        let handOff = try session.copyForAI()
        XCTAssertTrue(handOff.text.hasPrefix("Note: names and other identifying details"))
        XCTAssertFalse(handOff.text.contains("maya.ellison@example.com"))
        XCTAssertEqual(handOff.fileName, "Letter.md")

        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)
        let restored = try session.restoreReply("Write to \(alias) by Friday. \(alias.dropLast())9 is new.")
        XCTAssertEqual(restored.reply?.restored.text.hasPrefix("Write to maya.ellison@example.com by Friday."), true)
        XCTAssertEqual(restored.reply?.restored.unresolvedCount, 1)
        XCTAssertEqual(restored.reply?.aliasText.hasPrefix("Write to \(alias)"), true)
    }

    /// The reply and the Project's name are read once, not on every review
    /// tap; a removed reply and a renamed Project still show at once. The
    /// Reply pane lists the replies saved in the Vault, also when the
    /// document opens again (Vault & Projects item 10).
    @MainActor
    func testTheReplyAndProjectNameFollowEveryChange() throws {
        let core = try core()
        let session = try reviewed(core)
        let ready = try session.replaceAllPending()
        _ = try session.copyForAI()
        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)
        XCTAssertNil(session.snapshot().reply)

        let restored = try session.restoreReply("Write to \(alias).")
        XCTAssertEqual(restored.reply?.restored.text, "Write to maya.ellison@example.com.")
        XCTAssertEqual(session.snapshot().reply, restored.reply, "the same reply from the cache")
        let first = try XCTUnwrap(restored.reply?.id)
        XCTAssertEqual(restored.replies.map(\.id), [first])

        // A second reply shows; the pane picks either, and Remove deletes
        // the one shown from the Vault.
        let second = try session.restoreReply("Reply to \(alias).")
        let secondID = try XCTUnwrap(second.reply?.id)
        XCTAssertEqual(second.reply?.aliasText, "Reply to \(alias).")
        XCTAssertEqual(Set(second.replies.map(\.id)), [first, secondID])
        XCTAssertEqual(session.showReply(first).reply?.aliasText, "Write to \(alias).")
        let removed = try session.clearReply()
        XCTAssertEqual(removed.replies.map(\.id), [secondID])
        XCTAssertEqual(removed.reply?.id, secondID, "the newest left")
        XCTAssertNil(try session.clearReply().reply)
        XCTAssertNil(session.snapshot().reply)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        XCTAssertTrue(library.vault.projectReplies(projectId: try XCTUnwrap(ready.projectID).rawValue).isEmpty)

        XCTAssertEqual(session.snapshot().projectName, "Ellison")
        try core.library.renameProject(try XCTUnwrap(ready.projectID), to: "Ellison v Brightwater")
        XCTAssertEqual(session.snapshot().projectName, "Ellison v Brightwater")

        _ = try session.restoreReply("Write to \(alias).")
        try core.library.saveChanges()
        let vaultID = try XCTUnwrap((session as? RealSession)?.vaultID)
        let reopened = try self.core().library.session(for: DocumentID(rawValue: vaultID))
        XCTAssertEqual(
            reopened.snapshot().reply?.restored.text, "Write to maya.ellison@example.com.", "listed when opened again")
    }

    /// A saved reply restores by the identity each alias meant when it was
    /// saved (#1091 Decision 18): what the pane shows follows the names as
    /// saved, once a tap's change is saved.
    @MainActor
    func testASavedReplyFollowsTheNamesAsSaved() throws {
        let core = try core()
        let session = try reviewed(core)
        let ready = try session.replaceAllPending()
        let email = try XCTUnwrap(ready.candidates.first { $0.category == .email })
        _ = try session.restoreReply("Write to \(email.alias).")
        _ = try session.notAName(email.id, scope: .all)
        try session.saveChanges()
        XCTAssertEqual(session.snapshot().reply?.restored.text, "Write to \(email.alias).", "no longer a name")
        _ = try session.undo()
        try session.saveChanges()
        XCTAssertEqual(session.snapshot().reply?.restored.text, "Write to maya.ellison@example.com.")
    }

    /// Copy for AI and Share remember where the text went, by id only:
    /// the Restore extension restores with it by default (#1091, the
    /// maintainer's (a)).
    func testSharingRemembersThePlaceForRestore() throws {
        let core = try core()
        let session = try reviewed(core)
        let ready = try session.replaceAllPending()
        XCTAssertNil(RestorePlaceFile(storage).read())
        _ = try session.copyForAI()
        XCTAssertEqual(RestorePlaceFile(storage).read(), .project(try XCTUnwrap(ready.projectID)))

        let memo = try core.library.importDocument(.text("Write to ann@example.com."), title: "Memo", into: nil)
        _ = try memo.recognize(observer: TestObserver())
        _ = try memo.scan(observer: TestObserver())
        _ = try memo.replaceAllPending()
        _ = try memo.copyForAI()
        let memoID = try XCTUnwrap((memo as? RealSession)?.vaultID)
        XCTAssertEqual(RestorePlaceFile(storage).read(), .document(DocumentID(rawValue: memoID)))
        let file = try String(contentsOf: storage.restorePlace, encoding: .utf8)
        XCTAssertEqual(file, "{\"document\":\"\(memoID)\"}", "the id only")
    }

    /// An Unfiled document has no Project, and renaming it makes none.
    func testAnUnfiledDocumentsRenameMakesNoProject() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("# Memo\n\nCall Ann."), title: "Memo", into: nil)
        _ = try session.recognize(observer: TestObserver())
        XCTAssertNil(session.snapshot().projectID)
        XCTAssertNil(session.snapshot().projectName)
        try core.library.renameDocument(session.id, to: "Note")
        XCTAssertTrue(core.library.snapshot().projects.isEmpty)
        XCTAssertEqual(session.snapshot().title, "Note")
    }

    /// A scan that found nothing is still a scan (#673): after mdoc
    /// restarts the document is ready, not offered Find names again.
    func testAScanThatFoundNothingStaysScannedAfterARestart() throws {
        let session = try core().library.importDocument(
            .text("# Note\n\nNothing to hide here."), title: "Note", into: nil)
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
            Category.allCases.map { "\($0)" },
            ["person", "organization", "email", "url", "phone", "address", "identity", "tax", "bank", "caseNumber", "date", "other"])
    }

    /// File names from titles follow the core's rule (#683, `file_stem` in
    /// `crates/mdoc-ffi/src/file_names.rs`): a look-alike for each character a
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

    func testReopenedLibraryKeepsTheDocumentAndProject() throws {
        let core = try core()
        let session = try reviewed(core)
        _ = try session.replaceAllPending()
        // Saved once mdoc leaves the foreground, if not before.
        try core.library.saveChanges()
        let reopened = try self.core().library.snapshot()
        XCTAssertEqual(reopened.documents.count, 1)
        XCTAssertEqual(reopened.documents.first?.phase, .ready)
        XCTAssertEqual(reopened.projects.map(\.name), ["Ellison"])
    }

    /// Find names that ends while the phone is locked cannot write the
    /// library (Data Protection Complete): the result shows at once and is
    /// saved on unlock. Unlocked, a failed save is still an error.
    func testAScanThatEndsWhileLockedIsSavedOnUnlock() throws {
        let core = try core()
        let session = try core.library.importDocument(.text(Self.letter), title: "Letter", into: nil)
        _ = try session.recognize(observer: TestObserver())
        // Unwritable, as the Vault's folders are once the phone locks.
        try Fixture.setVault(storage, writable: false)
        defer {
            try? Fixture.setVault(storage, writable: true)
            ProtectedData.isAvailable = true
        }
        XCTAssertThrowsError(try session.scan(observer: TestObserver()), "unlocked, a failed save is an error")
        ProtectedData.isAvailable = false
        let scanned = try session.scan(observer: TestObserver())
        XCTAssertEqual(scanned.phase, .reviewing)
        let vaultID = try XCTUnwrap((session as? RealSession)?.vaultID)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        XCTAssertEqual(try library.vault.document(id: vaultID).phase, .unscanned, "not saved while locked")
        XCTAssertEqual(try core.library.saveDeferred(), [], "still locked")
        // Unlocked but still failing (a full disk): reported, and kept.
        ProtectedData.isAvailable = true
        XCTAssertThrowsError(try core.library.saveDeferred())
        try Fixture.setVault(storage, writable: true)
        XCTAssertEqual(try core.library.saveDeferred(), [session.id])
        XCTAssertEqual(try self.core().library.snapshot().documents.first?.phase, .reviewing)
        XCTAssertEqual(try core.library.saveDeferred(), [], "saved once")
    }

    /// The sample's proposals, when the phone locks before they save: in
    /// review at once, saved on unlock, as a scan's result.
    func testSampleProposalsWhileLockedAreSavedOnUnlock() throws {
        let core = try core()
        let sample = SampleDocument.english
        let session = try core.library.importDocument(.text(sample.text), title: sample.title, into: nil)
        _ = try session.recognize(observer: TestObserver())
        try Fixture.setVault(storage, writable: false)
        defer {
            try? Fixture.setVault(storage, writable: true)
            ProtectedData.isAvailable = true
        }
        ProtectedData.isAvailable = false
        let proposed = try session.propose(sample.names)
        XCTAssertEqual(proposed.phase, .reviewing)
        ProtectedData.isAvailable = true
        try Fixture.setVault(storage, writable: true)
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
        XCTAssertEqual(marked.candidate(of: marked.undoStep)?.id, ellison.id)

        let email = try XCTUnwrap(marked.candidates.first { $0.category == .email })
        let replaced = try session.decide(email.id, .replace, scope: .all)
        XCTAssertEqual(replaced.undoStep?.label, "Replace · maya.ellison@example.com")
        let alias = try XCTUnwrap(replaced.candidates.first { $0.identityID == email.identityID })
        let renamed = try session.rename(alias.id, scope: .all, to: "CONTACT")
        XCTAssertTrue(renamed.markdown.contains("CONTACT"))
        XCTAssertEqual(renamed.undoStep?.label, "Rename · CONTACT")
        let contact = try XCTUnwrap(renamed.candidates.first { $0.alias == "CONTACT" })
        XCTAssertEqual(renamed.candidate(of: renamed.undoStep)?.id, contact.id)
        let name = try XCTUnwrap(renamed.candidates.first { $0.original == "Ms Ellison" })
        XCTAssertThrowsError(try session.rename(name.id, scope: .all, to: "CONTACT")) { error in
            XCTAssertEqual(error as? CoreError, .aliasTaken(alias: "CONTACT", by: email.identityID))
        }
        XCTAssertThrowsError(try session.rename(name.id, scope: .all, to: "no spaces")) { error in
            guard case .refused = error as? CoreError else { return XCTFail("\(error)") }
        }

        let undone = try session.undo()
        XCTAssertFalse(undone.markdown.contains("CONTACT"))
        XCTAssertEqual(undone.redoStep?.kind, .rename)
        // Saved after the undo, reopened: still undone.
        let reopened = try core.library.session(for: session.id).snapshot()
        XCTAssertFalse(reopened.markdown.contains("CONTACT"))
        XCTAssertTrue(try session.redo().markdown.contains("CONTACT"))

        let choices = try session.categoryChoices(for: name, scope: .all)
        XCTAssertEqual(choices.count, Category.allCases.count)
        XCTAssertEqual(choices.filter(\.current).map(\.category), [name.category])
        var person = name
        if person.category != .person {
            let recategorized = try session.setCategory(person.id, scope: .all, to: .person)
            XCTAssertEqual(recategorized.undoStep?.label, "Category · Person")
            person = try XCTUnwrap(recategorized.candidates.first { $0.original == "Ms Ellison" })
        }
        // The address belongs to Ms Ellison.
        let owned = try session.setOwner(email.identityID, to: person.identityID)
        XCTAssertEqual(owned.identities.first { $0.id == email.identityID }?.owner, person.identityID)
        XCTAssertEqual(owned.undoStep?.kind, .owner)

        let split = try session.newAlias(contact.id)
        XCTAssertEqual(split.undoStep?.kind, .newAlias)
        let moved = try XCTUnwrap(split.candidate(of: split.undoStep)?.id)
        XCTAssertNotEqual(split.candidate(moved)?.identityID, email.identityID)
        XCTAssertFalse(split.markdown.contains("CONTACT"))
        let linked = try session.sameAs(moved, scope: .this, target: email.identityID)
        XCTAssertEqual(linked.undoStep?.kind, .sameAs)
        XCTAssertTrue(linked.markdown.contains("CONTACT"))
    }

    func testAScanNeedsOCRToReadAPhoto() throws {
        let core = try core()
        let image = UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }
        let folder = storage.url(.intake).appendingPathComponent("photo", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent("page-1.jpg")
        try image.write(to: url)
        let session = try core.library.importDocument(.photos([url]), title: "Receipt", into: nil)
        XCTAssertThrowsError(try session.recognize(observer: TestObserver())) { error in
            XCTAssertEqual(error as? CoreError, .modelMissing)
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: folder.appendingPathComponent("Receipt.jpg").path))
    }

    /// A title lists as typed (no "Sample- services agreement"), and Rename
    /// keeps the open session, which saves on into the Vault.
    func testRenameKeepsTheTitleAsTypedAndTheSession() throws {
        let core = try core()
        let title = "Sample: services agreement"
        let session = try core.library.importDocument(.text(Self.letter), title: title, into: nil)
        _ = try session.recognize(observer: TestObserver())
        XCTAssertEqual(core.library.snapshot().documents.map(\.title), [title])

        try core.library.renameDocument(session.id, to: "Services: final")
        XCTAssertEqual(session.snapshot().title, "Services: final")
        let renamed = core.library.snapshot()
        XCTAssertEqual(renamed.documents.map(\.title), ["Services: final"])
        XCTAssertTrue(renamed.projects.isEmpty, "Unfiled: no Project follows it")
        XCTAssertEqual(renamed.documents.first?.id, session.id)
        _ = try session.scan(observer: TestObserver())
        XCTAssertEqual(try session.replaceAllPending().phase, .ready)
        XCTAssertThrowsError(try core.library.renameDocument(session.id, to: "  ")) { error in
            XCTAssertEqual(error as? CoreError, .invalid("A document needs a title."))
        }

        // Replace all is saved once mdoc leaves the foreground, if not before.
        try core.library.saveChanges()
        let reopened = try self.core().library.snapshot()
        XCTAssertEqual(reopened.documents.map(\.title), ["Services: final"])
        XCTAssertEqual(reopened.documents.first?.phase, .ready)
    }

    /// The aliases a Project's names have now.
    private func aliases(_ project: ProjectID, _ core: Core) throws -> Set<String> {
        let library = try XCTUnwrap(core.library as? RealLibrary)
        return Set(try library.vault.projectPage(id: project.rawValue).names.map(\.alias))
    }

    /// Move to Project re-aliases the document for the Project it joins; the
    /// Project it left stays, and Undo moves it back with the old aliases
    /// and takes its names out of the Project it joined.
    func testMoveToProjectAndUndoThroughTheCore() throws {
        let core = try core()
        let letter = try reviewed(core)
        let before = try letter.replaceAllPending().markdown
        XCTAssertTrue(before.contains("EMAIL_1"))
        let other = try Fixture.createProject("Other", in: core)
        let note = try core.library.importDocument(
            .text("Write to someone@example.org."), title: "Note", into: other)
        _ = try note.recognize(observer: TestObserver())
        _ = try note.scan(observer: TestObserver())
        XCTAssertTrue(try note.replaceAllPending().markdown.contains("EMAIL_1"))
        try core.library.saveChanges()

        let move = try core.library.moveDocument(letter.id, to: .existing(other))
        XCTAssertEqual(move.to?.id, other)
        XCTAssertEqual(move.placeName, "Other")
        let moved = letter.snapshot()
        XCTAssertEqual(moved.projectID, other)
        XCTAssertFalse(moved.markdown.contains("EMAIL_1"))
        XCTAssertTrue(moved.markdown.contains("EMAIL_2"))
        XCTAssertTrue(try aliases(other, core).contains("EMAIL_2"))
        let ellison = try XCTUnwrap(move.from)
        XCTAssertEqual(Set(core.library.snapshot().projects.map(\.name)), ["Ellison", "Other"])
        XCTAssertTrue(core.library.snapshot().documents(in: ellison).isEmpty)
        XCTAssertThrowsError(try core.library.moveDocument(letter.id, to: .existing(other)))

        let undone = try core.library.undoMove(move)
        XCTAssertEqual(undone.to?.id, ellison)
        XCTAssertEqual(letter.snapshot().markdown, before)
        XCTAssertEqual(letter.snapshot().projectID, ellison)
        XCTAssertFalse(try aliases(other, core).contains("EMAIL_2"), "the names the move added went")
        XCTAssertTrue(try aliases(other, core).contains("EMAIL_1"))

        let fresh = try core.library.moveDocument(letter.id, to: .newProject(name: "Fresh"))
        XCTAssertEqual(fresh.to?.name, "Fresh")
        XCTAssertEqual(letter.snapshot().markdown, before)
    }

    /// Undo of a move keeps the names it added where text was shared from
    /// since: a reply to it may name them (#1109's note, through
    /// `Vault::text_for_ai`).
    func testUndoOfAMoveKeepsNamesSharedSince() throws {
        let core = try core()
        let letter = try reviewed(core)
        _ = try letter.replaceAllPending()
        let other = try Fixture.createProject("Other", in: core)
        let note = try core.library.importDocument(.text("Write to someone@example.org."), title: "Note", into: other)
        _ = try note.recognize(observer: TestObserver())
        _ = try note.scan(observer: TestObserver())
        _ = try note.replaceAllPending()
        try core.library.saveChanges()

        let move = try core.library.moveDocument(letter.id, to: .existing(other))
        XCTAssertTrue(try letter.copyForAI().text.contains("EMAIL_2"), "shared from Other")
        _ = try core.library.undoMove(move)
        XCTAssertTrue(try aliases(other, core).contains("EMAIL_2"), "kept: a reply may name it")
    }

    /// A reply pasted into a document is saved in the Vault as an AI Reply
    /// to text shared from it: its Project's, or its own while Unfiled
    /// (#1091 Q2 and Decision 4).
    func testAPastedReplyIsSavedInTheVault() throws {
        let core = try core()
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let letter = try reviewed(core)
        let ready = try letter.replaceAllPending()
        let alias = try XCTUnwrap(ready.candidates.first { $0.category == .email }?.alias)
        _ = try letter.restoreReply("Write to \(alias).")
        let ellison = try XCTUnwrap(ready.projectID)
        let saved = library.vault.projectReplies(projectId: ellison.rawValue)
        XCTAssertEqual(saved.map(\.title), ["Reply to Letter"])
        XCTAssertEqual(try library.vault.reply(id: try XCTUnwrap(saved.first?.id)).markdown, "Write to \(alias).")
        XCTAssertEqual(
            try library.vault.restoreReply(id: try XCTUnwrap(saved.first?.id)).text, "Write to maya.ellison@example.com.")

        let memo = try core.library.importDocument(.text("Write to ann@example.com."), title: "Memo", into: nil)
        _ = try memo.recognize(observer: TestObserver())
        _ = try memo.scan(observer: TestObserver())
        let memoAlias = try XCTUnwrap(try memo.replaceAllPending().candidates.first?.alias)
        _ = try memo.restoreReply("Ask \(memoAlias).")
        let memoID = try XCTUnwrap((memo as? RealSession)?.vaultID)
        XCTAssertEqual(library.vault.documentReplies(documentId: memoID).count, 1, "an Unfiled document's own")
    }

    /// Below the floor, where the phone finds no names, a document whose
    /// names were all decided by hand, never scanned, leaves through Copy
    /// for AI (#1091 Q4); one with a proposal left does not.
    func testANameDecidedByHandIsReadyWhereNoNamesAreFound() throws {
        let core = try core()
        let session = try core.library.importDocument(.text("Ms Ellison signed."), title: "Note", into: nil)
        _ = try session.recognize(observer: TestObserver())
        let text = session.snapshot().markdown
        let range = try XCTUnwrap(text.range(of: "Ms Ellison"))
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let marked = try session.markSelection(SourceRange(start: start, end: start + "Ms Ellison".utf8.count), as: .person)
        XCTAssertEqual(marked.phase, .unscanned)
        XCTAssertThrowsError(try session.copyForAI())
        _ = try session.replaceAllPending()
        let handOff = try session.copyForAI()
        XCTAssertFalse(handOff.text.contains("Ms Ellison"))
        XCTAssertEqual(handOff.fileName, "Note.md")
    }

    /// A fresh import is never Ready before a scan on a phone that can find
    /// names, even before the name finder is installed; below the floor it
    /// is, with its names decided by hand. Both stay unscanned, so the
    /// lists keep their labels (#1091 Q4).
    func testAFreshImportIsReadyBeforeAScanOnlyBelowTheFloor() throws {
        for (memory, ready) in [(Fixture.detectorTier, false), (Fixture.belowFloor, true)] {
            let core = try RealCore.make(storage: try Fixture.temporaryStorage(), keys: FixedVaultKeyStore(), physicalMemory: memory)
            let library = try XCTUnwrap(core.library as? RealLibrary)
            let session = try core.library.importDocument(.text("# Note\n\nNothing to hide."), title: "Note", into: nil)
            _ = try session.recognize(observer: TestObserver())
            let id = try XCTUnwrap((session as? RealSession)?.vaultID)
            XCTAssertEqual(try library.vault.readiness(id: id) == .ready, ready, "memory \(memory)")
            XCTAssertEqual(try library.vault.document(id: id).phase, .unscanned, "memory \(memory)")
            XCTAssertEqual(core.library.snapshot().documents.first?.phase, .unscanned)
        }
    }

    /// A saved PDF's original is viewed from the Vault (#1091 Decision 24):
    /// from memory when small; otherwise decrypted into the app's viewing
    /// cache, the copy going when the document closes and at launch.
    func testAnOriginalIsViewedFromMemoryOrACacheWipedOnClose() throws {
        let core = try core()
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let folder = storage.url(.intake).appendingPathComponent("import", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pdf = folder.appendingPathComponent("upload.pdf")
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
            NSString(string: "Reply to maya.ellison@example.com").draw(at: CGPoint(x: 20, y: 20), withAttributes: nil)
        }
        let bytes = try Data(contentsOf: pdf)
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: nil)
        guard case .pdf(let original) = try session.recognize(observer: TestObserver()).original,
              case .stored = original else { return XCTFail("viewed from the Vault") }
        guard case .memory(let data) = try original.resolved() else { return XCTFail("small: in memory") }
        XCTAssertEqual(data.bytes, bytes)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: storage.viewing.path).isEmpty, "no copy on disk")

        // Over the limit (lowered here), in a fresh run: a viewing copy.
        library.originalsInMemory = 0
        let vaultID = try XCTUnwrap((session as? RealSession)?.vaultID)
        let large = try Self.storedPDF(try reopenedOriginal(library, vaultID))
        guard case .file(let copy) = try large.resolved(), case .stored(let stored) = large else {
            return XCTFail("large: a viewing copy")
        }
        XCTAssertTrue(copy.path.hasPrefix(storage.viewing.path))
        XCTAssertEqual(try Data(contentsOf: copy), bytes)
        XCTAssertEqual(try copy.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup, true)
        stored.close()
        XCTAssertFalse(FileManager.default.fileExists(atPath: copy.path), "wiped on close")

        _ = try large.resolved()
        XCTAssertFalse(try FileManager.default.contentsOfDirectory(atPath: storage.viewing.path).isEmpty)
        _ = try self.core()
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: storage.viewing.path).isEmpty, "wiped at launch")
    }

    /// Document `id`'s original as a fresh session shows it.
    private func reopenedOriginal(_ library: RealLibrary, _ id: String) throws -> OriginalSource {
        let other = try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
        let otherLibrary = try XCTUnwrap(other.library as? RealLibrary)
        otherLibrary.originalsInMemory = library.originalsInMemory
        return try other.library.session(for: DocumentID(rawValue: id)).snapshot().original
    }

    private static func storedPDF(_ original: OriginalSource) throws -> OriginalBytes {
        guard case .pdf(let bytes) = original, case .stored = bytes else { throw CoreError.notFound }
        return bytes
    }

    func testSetupOffersOCRBelowTheFloorAndTheDetectorAbove() throws {
        let small = RealSetup(storage: storage, physicalMemory: Fixture.belowFloor).snapshot()
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
        let setup = RealSetup(storage: storage, physicalMemory: Fixture.belowFloor)
        let request = try XCTUnwrap(setup.downloadRequests(for: "ocr-english-russian").first)
        let file = storage.url(.downloads).appendingPathComponent("wrong")
        try Data("not a model".utf8).write(to: file)
        XCTAssertThrowsError(try setup.install(request, from: file))
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
    }
}

extension RealCoreTests {
    func testAPendingImportSurvivesARestart() throws {
        let first = try core().library.importDocument(.text(Self.letter), title: "Letter", into: nil)
        // mdoc stops before recognition runs; the next launch finds it.
        let core = try core()
        let pending = try XCTUnwrap(core.library.snapshot().documents.first { $0.id == first.id })
        XCTAssertNil(pending.phase, "recognizing")
        let session = try core.library.session(for: first.id)
        XCTAssertEqual(try session.recognize(observer: TestObserver()).phase, .unscanned)
        // Saved, it is no longer pending: one document, not two.
        XCTAssertEqual(try self.core().library.snapshot().documents.count, 1)
    }

    func testDeletingAnImportRemovesItsSourceFiles() throws {
        let core = try core()
        let folder = storage.url(.intake).appendingPathComponent("import", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pdf = folder.appendingPathComponent("upload.pdf")
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
            NSString(string: "Reply to maya.ellison@example.com").draw(at: CGPoint(x: 20, y: 20), withAttributes: nil)
        }
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: nil)
        XCTAssertEqual(session.snapshot().original, .pdf(.file(folder.appendingPathComponent("Statement.pdf"))))
        _ = try session.recognize(observer: TestObserver())
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path), "gone at its first save")
        try core.library.deleteDocument(session.id)
        XCTAssertTrue(core.library.snapshot().documents.isEmpty)
        XCTAssertTrue(try self.core().library.snapshot().documents.isEmpty)
    }

    func testDeletingAPendingImportRemovesItsSourceFiles() throws {
        let core = try core()
        let folder = storage.url(.intake).appendingPathComponent("scan", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let page = folder.appendingPathComponent("page-1.jpg")
        try UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).jpegData(withCompressionQuality: 0.9) { _ in }.write(to: page)
        let session = try core.library.importDocument(.scan([page]), title: "Scan", into: nil)
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

// MARK: Delete Project's writes (#1140)

extension RealCoreTests {
    /// The Project a pending import's record names, or nil when no record
    /// is left; `.some(nil)` is Unfiled.
    private func recordedProject(_ document: DocumentID, in storage: MdocStorage) -> String?? {
        PendingImports(storage: storage).all().first { $0.id == document }.map(\.projectID)
    }

    /// An import not read yet that was to join a Project deleted with its
    /// documents moved to Unfiled has its record moved to Unfiled before
    /// the Project goes: a stop between the two writes leaves no record
    /// naming a Project that is gone. A record that can't be written stops
    /// the delete; a delete that fails puts the record back.
    func testAProjectDeleteMovesAPendingImportsRecordFirst() throws {
        let seeded = try Fixture.seeded()
        let library = try XCTUnwrap(seeded.library as? RealLibrary)
        let ellison = seeded.ellison.rawValue
        let note = try library.importDocument(.text("Ann Lee signed."), title: "Note", into: seeded.ellison)
        XCTAssertEqual(recordedProject(note.id, in: seeded.storage), .some(ellison))

        // The record's folder unwritable: written after the Project's
        // delete, the record would still name the Project, as after a stop
        // between the two.
        try Fixture.setFolder(seeded.storage.intake, writable: false)
        defer { try? Fixture.setFolder(seeded.storage.intake, writable: true) }
        XCTAssertThrowsError(try library.deleteProject(seeded.ellison, documents: .moveToUnfiled))
        try Fixture.setFolder(seeded.storage.intake, writable: true)
        XCTAssertTrue(library.vault.projects().contains { $0.id == ellison }, "the Project stays")
        XCTAssertEqual(recordedProject(note.id, in: seeded.storage), .some(ellison))
        XCTAssertEqual(note.snapshot().projectID, seeded.ellison)

        // The Vault unwritable: the delete fails, and the record is back.
        try Fixture.setVault(seeded.storage, writable: false)
        defer { try? Fixture.setVault(seeded.storage, writable: true) }
        XCTAssertThrowsError(try library.deleteProject(seeded.ellison, documents: .moveToUnfiled))
        try Fixture.setVault(seeded.storage, writable: true)
        XCTAssertEqual(recordedProject(note.id, in: seeded.storage), .some(ellison))
        XCTAssertEqual(note.snapshot().projectID, seeded.ellison)

        try library.deleteProject(seeded.ellison, documents: .moveToUnfiled)
        XCTAssertEqual(recordedProject(note.id, in: seeded.storage), .some(nil))
        XCTAssertNil(note.snapshot().projectID)
    }

    /// An import's first save that lands as Delete Project lists the
    /// Project's documents is not missed: moved, its session shows it in
    /// Unfiled; deleted, its session goes with it.
    func testAFirstSaveAsAProjectIsDeletedIsNotMissed() throws {
        for documents in [ProjectDeletion.moveToUnfiled, .deleteDocuments] {
            let seeded = try Fixture.seeded()
            let library = try XCTUnwrap(seeded.library as? RealLibrary)
            let note = try library.importDocument(.text("Ann Lee signed."), title: "Note", into: seeded.ellison)
            library.listingProject = {
                library.listingProject = nil
                do {
                    _ = try note.recognize(observer: TestObserver())
                } catch {
                    XCTFail("its first save: \(error)")
                }
            }
            try library.deleteProject(seeded.ellison, documents: documents)
            XCTAssertNil(library.listingProject, "it saved while the Project was listed")
            let vaultID = try XCTUnwrap((note as? RealSession)?.vaultID)
            switch documents {
            case .moveToUnfiled:
                XCTAssertNil(try library.vault.document(id: vaultID).projectId, "moved in the Vault")
                XCTAssertNil(note.snapshot().projectID, "and shown in Unfiled")
            case .deleteDocuments:
                XCTAssertThrowsError(try library.vault.document(id: vaultID), "deleted in the Vault")
                XCTAssertFalse(library.snapshot().documents.contains { $0.id == note.id }, "and listed nowhere")
            }
        }
    }

    /// A Project delete that fails part way, a document already moved to
    /// Unfiled: its open session shows it there before the error does; the
    /// one not moved still shows the Project.
    func testAPartialProjectDeleteShowsWhatMovedInUnfiled() throws {
        let seeded = try Fixture.seeded()
        let library = try XCTUnwrap(seeded.library as? RealLibrary)
        let ellison = seeded.ellison.rawValue
        let sessions = try [seeded.letter, seeded.quotation].map { try XCTUnwrap(seeded.session($0) as? RealSession) }
        // The Vault moves them in this order; the second can't be written.
        let members = library.vault.documents().filter { $0.projectId == ellison }.map(\.id)
        XCTAssertEqual(Set(members), Set(sessions.compactMap(\.vaultID)))
        let stuck = seeded.storage.vault.appendingPathComponent("docs").appendingPathComponent(members[1])
        try Fixture.setFolder(stuck, writable: false)
        defer { try? Fixture.setFolder(stuck, writable: true) }

        XCTAssertThrowsError(try library.deleteProject(seeded.ellison, documents: .moveToUnfiled))
        try Fixture.setFolder(stuck, writable: true)
        XCTAssertNil(try library.vault.document(id: members[0]).projectId, "moved before the failure")
        XCTAssertEqual(try library.vault.document(id: members[1]).projectId, ellison)
        let moved = try XCTUnwrap(sessions.first { $0.vaultID == members[0] })
        let kept = try XCTUnwrap(sessions.first { $0.vaultID == members[1] })
        XCTAssertNil(moved.snapshot().projectID, "the moved one shows Unfiled")
        XCTAssertEqual(kept.snapshot().projectID, seeded.ellison)
    }
}

/// The Keychain store, on the Simulator's keychain under a test service.
final class KeychainVaultKeyStoreTests: XCTestCase {
    func testTheKeyIsMadeOnceAndKept() throws {
        let store = KeychainVaultKeyStore(service: "io.github.lawkitt.mdoc.tests", account: UUID().uuidString)
        defer { store.remove() }
        let first = try store.key()
        XCTAssertEqual(first.count, 32)
        XCTAssertEqual(try store.key(), first)
    }
}
