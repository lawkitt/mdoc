import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// An edit's new text is scanned (Vault & Projects item 44, iOS): at Done
/// the paragraphs edited are scanned for names as a fresh document is, so
/// a name typed into a Ready document becomes a proposal and never leaves
/// as typed. A Copy for AI tapped before Done's work has run runs it
/// itself, in Done's order, and sees what Done would leave. Each runs at
/// the detector tier, where the rules stand in for the name finder the
/// tests never download, and below the floor, where the rules are the scan.
@MainActor
final class EditsScannedTests: XCTestCase {
    private static let email = "ann.baker@example.com"
    private static let letter = "# Site visit\n\nWrite to \(email) about the roof."
    /// The name typed in edit mode: an email, which the rules find.
    private static let typedName = "bob.stone@example.com"
    private static let typed = "\n\nCopy \(typedName) in."
    private static let tiers = [Fixture.detectorTier, Fixture.belowFloor]

    /// `letter` scanned, its email replaced, saved: Ready. With
    /// `standIn`, the rules stand in for the name finder at the detector
    /// tier.
    private func reviewedLetter(memory: UInt64, standIn: Bool = true) throws -> (DocumentViewModel, RealSession, RealLibrary) {
        let (core, storage) = try Fixture.core(memory: memory)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        library.rulesStandInForNameFinder = standIn
        let session = try XCTUnwrap(
            core.library.importDocument(.text(Self.letter), title: "Site visit", into: nil) as? RealSession)
        _ = try session.recognize(observer: TestObserver())
        if memory == Fixture.detectorTier {
            _ = try session.propose([.init(wording: Self.email, category: .email)])
        } else {
            _ = try session.scan(observer: TestObserver())
        }
        _ = try session.replaceAllPending()
        try session.saveChanges()
        XCTAssertEqual(try readiness(session, in: library), .ready, "memory \(memory)")
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage,
            findsNames: memory == Fixture.detectorTier)
        return (model, session, library)
    }

    private func readiness(_ session: RealSession, in library: RealLibrary) throws -> Readiness {
        try library.vault.readiness(id: try XCTUnwrap(session.vaultID))
    }

    /// `typed` typed at the end of the text in edit mode.
    private func typeAtEnd(_ model: DocumentViewModel) {
        model.enterEditMode()
        XCTAssertTrue(model.isEditing)
        let end = model.snapshot.markdown.utf8.count
        XCTAssertNotNil(model.typed(SourceRange(start: end, end: end), Self.typed))
    }

    /// Waits for `condition`, which Done's work off the main thread makes
    /// true.
    private func until(_ condition: () -> Bool, _ message: String) async throws {
        for _ in 0..<1000 where !condition() {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(condition(), message)
    }

    private func proposed(_ snapshot: DocumentSnapshot) -> [String] {
        snapshot.candidates.filter { $0.state == .proposed }.map(\.original)
    }

    /// Item 44's Done-when: a name typed into a Ready document is proposed
    /// at Done, the document waits for it, and Replace N and copy hands the
    /// text out with it replaced.
    func testANameTypedIntoAReadyDocumentIsProposedAtDone() async throws {
        for memory in Self.tiers {
            let (model, session, library) = try reviewedLetter(memory: memory)
            typeAtEnd(model)

            model.doneEditing()
            try await until({ model.snapshot.pendingCount == 1 }, "memory \(memory)")

            XCTAssertEqual(proposed(model.snapshot), [Self.typedName], "memory \(memory)")
            XCTAssertEqual(model.snapshot.phase, .reviewing, "memory \(memory)")
            let refusal = HandOffRefusal.namesToDecide(names: 1, proposals: 1)
            XCTAssertEqual(model.handOffRefusal, refusal, "memory \(memory)")
            XCTAssertEqual(model.step(for: refusal), .replaceAndCopy(1), "memory \(memory)")
            XCTAssertNotEqual(try readiness(session, in: library), .ready, "memory \(memory)")
            XCTAssertThrowsError(try session.copyForAI(), "memory \(memory)") { error in
                XCTAssertEqual(error as? mdoc.CoreError, .handOffRefused(refusal), "memory \(memory)")
            }

            UIPasteboard.general.string = ""
            model.take(.replaceAndCopy(1))
            let copied = try XCTUnwrap(UIPasteboard.general.string, "memory \(memory)")
            XCTAssertTrue(copied.contains("Copy "), "memory \(memory)")
            XCTAssertFalse(copied.contains(Self.typedName), "memory \(memory)")
            XCTAssertFalse(copied.contains(Self.email), "memory \(memory)")
            XCTAssertEqual(try readiness(session, in: library), .ready, "memory \(memory)")
        }
    }

    /// Copy for AI tapped right after Done, before Done's work has run off
    /// the main thread: the hand-off runs discovery and the edit scan
    /// itself, so the typed name is proposed, never handed out as typed. It
    /// is refused as names left to decide, which the bar shows, and Replace
    /// N and copy copies it replaced (#1214's refusal intact).
    func testCopyRightAfterANameIsTypedProposesIt() async throws {
        for memory in Self.tiers {
            let (model, _, _) = try reviewedLetter(memory: memory)
            typeAtEnd(model)
            model.doneEditing()

            var copied: String?
            model.copyForAI { copied = $0 }

            XCTAssertNil(copied, "handed out as typed: memory \(memory)")
            let refusal = HandOffRefusal.namesToDecide(names: 1, proposals: 1)
            XCTAssertEqual(model.refusal, refusal, "memory \(memory)")
            XCTAssertEqual(refusal.reason, "1 name left to decide")
            XCTAssertEqual(model.step(for: refusal), .replaceAndCopy(1), "memory \(memory)")
            XCTAssertEqual(proposed(model.snapshot), [Self.typedName], "the bar shows it: memory \(memory)")

            UIPasteboard.general.string = ""
            model.take(.replaceAndCopy(1))
            let text = try XCTUnwrap(UIPasteboard.general.string, "memory \(memory)")
            XCTAssertFalse(text.contains(Self.typedName), "memory \(memory)")
            XCTAssertEqual(model.notice, "Copied for AI", "memory \(memory)")
            // Done's own work, after the tap's, changes nothing.
            try await Task.sleep(for: .milliseconds(200))
            XCTAssertEqual(model.snapshot.pendingCount, 0, "memory \(memory)")
        }
    }

    /// At the detector tier with no name finder installed, the rules alone
    /// would miss names: the edit stays owed, so the document waits for
    /// Find names and Copy for AI refuses it, typed name and all.
    func testWithoutTheNameFinderAnEditWaitsForFindNames() async throws {
        let (model, session, library) = try reviewedLetter(memory: Fixture.detectorTier, standIn: false)
        typeAtEnd(model)
        model.doneEditing()
        var copied: String?
        model.copyForAI { copied = $0 }

        XCTAssertNil(copied)
        XCTAssertEqual(model.refusal, .notChecked)
        XCTAssertEqual(model.step(for: .notChecked), .findNames)
        XCTAssertEqual(try readiness(session, in: library), .notReviewed)
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertThrowsError(try session.copyForAI())
    }

    /// A save while the edit is unscanned, as typing's pause saves: the
    /// record is written unscanned, and opened again it is Unscanned and
    /// owes a scan of its text before it can leave.
    func testARecordSavedWithTheEditUnscannedReopensUnscanned() throws {
        for memory in Self.tiers {
            let (model, session, library) = try reviewedLetter(memory: memory)
            typeAtEnd(model)
            try session.saveChanges()

            let id = try XCTUnwrap(session.vaultID)
            XCTAssertNotEqual(try library.vault.readiness(id: id), .ready, "memory \(memory)")
            let reopened = try MdocFFI.DocumentSession().openFromVault(vault: library.vault, id: id)
            XCTAssertFalse(reopened.scanned, "memory \(memory)")
            XCTAssertTrue(reopened.editsUnscanned, "memory \(memory)")
            XCTAssertEqual(reopened.phase, .unscanned, "memory \(memory)")
        }
    }
}
