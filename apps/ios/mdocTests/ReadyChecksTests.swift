import MdocFFI
import XCTest
@testable import mdoc

/// Two behaviours #1209 decision 15 asked to confirm (Vault & Projects
/// item 39, iOS): a document scanned with nothing to replace is Ready and
/// Copy for AI takes it; an edit that adds no new name leaves it Ready with
/// no click, once Done has scanned it (item 44). Each runs on a phone that
/// finds names, where Ready rests on the scan, and below the floor, where
/// names are decided by hand. An edit that writes out a name under review
/// again is not Ready, so the checks mean something.
final class ReadyChecksTests: XCTestCase {
    private static let email = "ann.baker@example.com"
    private static let report = "# Site visit\n\nTwo tiles on the north side are cracked."
    private static let letter = "# Site visit\n\nWrite to \(email) about the roof."

    /// The two tiers: at the detector tier the scan stands in as Try a
    /// sample's does (the tests have no name finder), with the email as
    /// the one name it finds, and an edit's scan runs the rules in the name
    /// finder's place; below the floor both are the structured rules'.
    private static let tiers = [Fixture.detectorTier, Fixture.belowFloor]

    /// Imported, scanned and saved on a phone with `memory`.
    private func scanned(_ text: String, memory: UInt64) throws -> (RealSession, RealLibrary) {
        let (core, _) = try Fixture.core(memory: memory)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        library.rulesStandInForNameFinder = true
        let session = try XCTUnwrap(
            core.library.importDocument(.text(text), title: "Site visit", into: nil) as? RealSession)
        _ = try session.recognize(observer: TestObserver())
        if memory == Fixture.detectorTier {
            _ = try session.propose([.init(wording: Self.email, category: .email)])
        } else {
            _ = try session.scan(observer: TestObserver())
        }
        try session.saveChanges()
        return (session, library)
    }

    /// `letter` scanned, its email replaced, saved: Ready.
    private func reviewedLetter(memory: UInt64) throws -> (RealSession, RealLibrary) {
        let (session, library) = try scanned(Self.letter, memory: memory)
        XCTAssertEqual(session.snapshot().pendingCount, 1, "memory \(memory)")
        _ = try session.replaceAllPending()
        try session.saveChanges()
        XCTAssertEqual(try readiness(session, in: library), .ready, "memory \(memory)")
        return (session, library)
    }

    private func readiness(_ session: RealSession, in library: RealLibrary) throws -> Readiness {
        try library.vault.readiness(id: try XCTUnwrap(session.vaultID))
    }

    /// Typed at the end of the text, as edit mode types it, and Done; with
    /// `discovering`, Done's discovery, edit scan and save run before
    /// anything else.
    @discardableResult
    private func typeAtEnd(_ text: String, in session: RealSession, discovering: Bool = true) throws -> DocumentSnapshot {
        let end = session.snapshot().markdown.utf8.count
        _ = try session.type(SourceRange(start: end, end: end), text)
        session.endTyping()
        guard discovering else { return session.snapshot() }
        _ = try session.discover()
        let found = try session.scanEdits()
        try session.saveChanges()
        return found
    }

    func testAScannedDocumentWithNothingToReplaceIsReady() throws {
        for memory in Self.tiers {
            let (session, library) = try scanned(Self.report, memory: memory)
            let snapshot = session.snapshot()
            XCTAssertTrue(snapshot.candidates.isEmpty, "memory \(memory)")
            XCTAssertEqual(snapshot.phase, .ready, "memory \(memory)")
            XCTAssertEqual(try readiness(session, in: library), .ready, "memory \(memory)")
            let handOff = try session.copyForAI()
            XCTAssertTrue(handOff.text.contains("Two tiles on the north side are cracked."), "memory \(memory)")
        }
    }

    func testAnEditWithNoNewNameStaysReady() throws {
        for memory in Self.tiers {
            let (session, library) = try reviewedLetter(memory: memory)

            let edited = try typeAtEnd("\n\nThe ladder stays in the shed.", in: session)

            XCTAssertEqual(edited.phase, .ready, "memory \(memory)")
            XCTAssertEqual(edited.pendingCount, 0, "memory \(memory)")
            XCTAssertEqual(try readiness(session, in: library), .ready, "the edit needs a click: memory \(memory)")
            let handOff = try session.copyForAI()
            XCTAssertTrue(handOff.text.contains("The ladder stays in the shed."), "memory \(memory)")
            XCTAssertFalse(handOff.text.contains(Self.email), "memory \(memory)")
        }
    }

    /// Copy for AI tapped as Done is tapped, before Done's discovery and
    /// edit scan have run off the main thread: the hand-off runs them
    /// first, so the edit goes then too (the Vault refused it as not
    /// reviewed before).
    func testCopyRightAfterAnEditRunsDonesWorkFirst() throws {
        for memory in Self.tiers {
            let (session, library) = try reviewedLetter(memory: memory)
            try typeAtEnd("\n\nThe ladder stays in the shed.", in: session, discovering: false)

            let handOff = try session.copyForAI()

            XCTAssertTrue(handOff.text.contains("The ladder stays in the shed."), "memory \(memory)")
            XCTAssertEqual(try readiness(session, in: library), .ready, "memory \(memory)")
        }
    }

    /// The control: an edit that writes out a name under review again
    /// waits for its review, Done's discovery run or not. (Discovery looks
    /// for the names under review; a name new to the document is Done's
    /// edit scan's: `EditsScannedTests`.)
    func testAnEditWithANameUnderReviewIsNotReady() throws {
        for memory in Self.tiers {
            for discovering in [true, false] {
                let (session, library) = try reviewedLetter(memory: memory)
                try typeAtEnd("\n\nCopy \(Self.email) in.", in: session, discovering: discovering)

                XCTAssertThrowsError(try session.copyForAI(), "memory \(memory), discovering \(discovering)")
                XCTAssertNotEqual(try readiness(session, in: library), .ready, "memory \(memory)")
            }
        }
    }
}
