import MdocFFI
import XCTest
@testable import mdoc

/// Two behaviours #1209 decision 15 asked to confirm (Vault & Projects
/// item 39, iOS): a document scanned with nothing to replace is Ready and
/// Copy for AI takes it; an edit that adds no new name leaves it Ready with
/// no click, once discovery and the save that follow typing have run. An
/// edit that adds a name does not, so the check means something.
final class ReadyChecksTests: XCTestCase {
    /// Imported and scanned with the structured rules, as the phone's own
    /// scan runs below the floor; saved.
    private func scanned(_ text: String) throws -> (RealSession, RealLibrary) {
        let (core, _) = try Fixture.core()
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let session = try XCTUnwrap(
            core.library.importDocument(.text(text), title: "Site visit", into: nil) as? RealSession)
        _ = try session.recognize(observer: TestObserver())
        _ = try session.scan(observer: TestObserver())
        try session.saveChanges()
        return (session, library)
    }

    private func readiness(_ session: RealSession, in library: RealLibrary) throws -> Readiness {
        try library.vault.readiness(id: try XCTUnwrap(session.vaultID))
    }

    /// Typed at the end of the text, as edit mode types it; then Done:
    /// discovery, and the save.
    private func typeAtEnd(_ text: String, in session: RealSession) throws -> DocumentSnapshot {
        let end = session.snapshot().markdown.utf8.count
        _ = try session.type(SourceRange(start: end, end: end), text)
        session.endTyping()
        let found = try session.discover()
        try session.saveChanges()
        return found
    }

    func testAScannedDocumentWithNothingToReplaceIsReady() throws {
        let (session, library) = try scanned("# Site visit\n\nTwo tiles on the north side are cracked.")
        let snapshot = session.snapshot()
        XCTAssertTrue(snapshot.candidates.isEmpty)
        XCTAssertEqual(snapshot.phase, .ready)
        XCTAssertEqual(try readiness(session, in: library), .ready)
        let handOff = try session.copyForAI()
        XCTAssertTrue(handOff.text.contains("Two tiles on the north side are cracked."))
    }

    func testAnEditWithNoNewNameStaysReady() throws {
        let (session, library) = try scanned("# Site visit\n\nWrite to ann.baker@example.com about the roof.")
        _ = try session.replaceAllPending()
        try session.saveChanges()
        XCTAssertEqual(try readiness(session, in: library), .ready)

        let edited = try typeAtEnd("\n\nThe ladder stays in the shed.", in: session)

        XCTAssertEqual(edited.phase, .ready)
        XCTAssertEqual(edited.pendingCount, 0)
        XCTAssertEqual(try readiness(session, in: library), .ready, "the edit needs a click")
        let handOff = try session.copyForAI()
        XCTAssertTrue(handOff.text.contains("The ladder stays in the shed."))
        XCTAssertFalse(handOff.text.contains("ann.baker@example.com"))
    }

    /// The control: an edit that writes out a name under review again
    /// waits for its review. (Discovery looks for the names under review;
    /// a name new to the document waits for Find names again.)
    func testAnEditWithANameUnderReviewIsNotReady() throws {
        let (session, library) = try scanned("# Site visit\n\nWrite to ann.baker@example.com about the roof.")
        _ = try session.replaceAllPending()
        try session.saveChanges()
        XCTAssertEqual(try readiness(session, in: library), .ready)

        let edited = try typeAtEnd("\n\nCopy ann.baker@example.com in.", in: session)

        XCTAssertNotEqual(try readiness(session, in: library), .ready, "\(edited.candidates.map(\.state))")
        XCTAssertThrowsError(try session.copyForAI())
    }
}
