import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// Document notes and Promote to Document (#1091 Decision 3) on the real
/// core: notes save with the document and never leave with its text; a
/// saved reply becomes a new document in its Project, with the way back.
@MainActor
final class DocumentNotesTests: XCTestCase {
    private static let notes = "Call the Halvorsen office on Monday about the deposit."

    /// The Vault's id of a seeded document, as a library opened again
    /// lists it.
    private func vaultID(_ id: DocumentID, in seeded: Seeded) throws -> DocumentID {
        let session = try XCTUnwrap(try seeded.session(id) as? RealSession)
        return DocumentID(rawValue: try XCTUnwrap(session.vaultID))
    }

    /// Typed notes save with the document, and a library opened again
    /// (mdoc after a restart) shows them.
    func testNotesSaveWithTheDocumentAndComeBack() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        XCTAssertEqual(model.notes, "")
        model.setNotes(Self.notes)
        XCTAssertEqual(model.notes, Self.notes)
        model.saveNotes()
        XCTAssertNil(model.errorMessage)

        let again = try RealCore.make(storage: seeded.storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
        let reopened = try again.library.session(for: try vaultID(seeded.quotation, in: seeded))
        XCTAssertEqual(reopened.notes(), Self.notes)
        XCTAssertEqual(reopened.snapshot().phase, .ready, "notes leave the review as it was")
    }

    /// Notes typed and paused on are saved by the save that follows
    /// typing, with no explicit save. The test waits for the notes to be
    /// in the Vault, not for the pause or the app's save event: a loaded
    /// runner may run the utility queue's save late (queue run
    /// 45412213450).
    func testNotesSaveOnceTypingPauses() throws {
        let seeded = try Fixture.seeded()
        let library = try XCTUnwrap(seeded.core.library as? RealLibrary)
        library.typingPause = .milliseconds(10)
        let failures = SaveFailures()
        library.observeSaves { event in
            if case .failed(_, let error) = event { failures.add(error) }
        }
        let id = try vaultID(seeded.quotation, in: seeded).rawValue
        try seeded.session(seeded.quotation).setNotes(Self.notes)
        let vault = library.vault
        let saved = expectation(
            for: NSPredicate { _, _ in Self.storedNotes(id, in: vault) == Self.notes || failures.any }, evaluatedWith: nil)
        wait(for: [saved], timeout: 120)
        XCTAssertEqual(failures.messages, [], "the save failed")
        let again = try RealCore.make(storage: seeded.storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
        XCTAssertEqual(try again.library.session(for: DocumentID(rawValue: id)).notes(), Self.notes)
    }

    /// Document `id`'s notes as its record in `vault` holds them now, read
    /// through a session of its own; nil while it can't be read.
    nonisolated private static func storedNotes(_ id: String, in vault: MdocFFI.Vault) -> String? {
        let session = MdocFFI.DocumentSession()
        guard (try? session.openFromVault(vault: vault, id: id)) != nil else { return nil }
        return session.notes()
    }

    /// Search reads titles and the text, never notes: neither the
    /// sidebar's search nor Find in document finds a word only the notes
    /// have.
    func testNotesAreNotSearched() throws {
        let seeded = try Fixture.seeded()
        let app = Fixture.app(seeded)
        let model = try seeded.model(seeded.quotation)
        model.setNotes(Self.notes)
        model.saveNotes()
        app.library.refresh()
        XCTAssertTrue(app.library.search("Halvorsen").documents.isEmpty)
        XCTAssertTrue(model.matches(of: "Halvorsen").isEmpty)
    }

    /// Copy for AI and Share send the document's text alone: never its
    /// notes, as text or as the shared file.
    func testNotesNeverLeaveWithCopyForAIOrShare() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        model.setNotes(Self.notes)
        model.saveNotes()

        var copied = ""
        model.copyForAI { copied = $0 }
        XCTAssertFalse(copied.isEmpty)
        model.share(asFile: false)
        let shared = try XCTUnwrap(model.share?.items.first as? TitledText)
        let file = try String(contentsOf: try XCTUnwrap(shared.file), encoding: .utf8)
        XCTAssertNil(model.errorMessage)
        for text in [copied, shared.text, file] {
            XCTAssertFalse(text.contains("Halvorsen"), text)
        }
    }

    /// Promote to Document from the Reply pane: a new document in the
    /// reply's Project opens, its text the reply restored and its names
    /// proposed again; the reply stays, and the document's way back shows
    /// it, or that it was deleted.
    func testPromoteOpensANewDocumentWithTheWayBack() throws {
        let seeded = try Fixture.seeded()
        let app = Fixture.app(seeded)
        app.open(seeded.quotation)
        let quotation = try XCTUnwrap(app.openModel)
        let alias = try XCTUnwrap(quotation.snapshot.candidates.first { $0.state == .replaced }?.alias)
        let original = try XCTUnwrap(quotation.snapshot.candidates.first { $0.alias == alias }?.original)
        quotation.restore(reply: "Ask \(alias) to sign.")
        let reply = try XCTUnwrap(quotation.snapshot.reply?.id)

        app.promote(reply)
        let promoted = try XCTUnwrap(app.navigator.openDocument)
        XCTAssertNotEqual(promoted, seeded.quotation, "the new document opens")
        let model = try XCTUnwrap(app.openModel)
        XCTAssertEqual(model.snapshot.projectID, seeded.ellison)
        XCTAssertEqual(model.snapshot.markdown, "Ask \(original) to sign.")
        XCTAssertGreaterThan(model.snapshot.pendingCount, 0, "it goes through review")
        XCTAssertNotNil(model.handOffBlockedReason)
        guard case .reply(let from) = model.snapshot.promotedFrom else {
            return XCTFail("no way back: \(String(describing: model.snapshot.promotedFrom))")
        }
        XCTAssertEqual(from.id, reply)
        XCTAssertEqual(app.library.replies(in: seeded.ellison).map(\.id), [reply], "the reply stays")
        XCTAssertTrue(app.library.documents(in: seeded.ellison).contains { $0.id == promoted })
        XCTAssertNil(try app.documents.open(seeded.quotation).snapshot.promotedFrom)

        _ = try seeded.library.session(for: seeded.quotation).clearReply()
        model.refresh()
        XCTAssertEqual(model.snapshot.promotedFrom, .deletedReply)
    }

    /// A reply deleted before it is promoted makes no document.
    func testAGoneReplyPromotesNothing() throws {
        let seeded = try Fixture.seeded()
        let app = Fixture.app(seeded)
        let before = app.library.documents.count
        app.promote(ReplyID(rawValue: "0123456789abcdef0123456789abcdef"))
        XCTAssertEqual(app.library.documents.count, before)
        XCTAssertNil(app.navigator.openDocument)
    }
}

/// The saves that failed, as the library tells them, from any thread.
private final class SaveFailures: @unchecked Sendable {
    private let lock = NSLock()
    private var errors: [String] = []

    func add(_ error: mdoc.CoreError) {
        lock.withLock { errors.append(error.localizedDescription) }
    }

    var any: Bool { lock.withLock { !errors.isEmpty } }
    var messages: [String] { lock.withLock { errors } }
}
