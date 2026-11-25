import UIKit
import XCTest
@testable import mdoc

/// Copy for AI and Share refusals in plain words (#1209 decisions 13 and
/// 14, Vault & Projects item 37): each refusal has its own sentence and its
/// step, nothing of the core's wording reaches the screen, and below the
/// floor the bar and the title menu agree.
@MainActor
final class HandOffRefusalTests: XCTestCase {
    /// The core's words for its refusals, which never show.
    private static let coreWords = ["not ready for AI", "NamesNotReplaced", "ChangedSinceReview", "NotReviewed", "Vault"]

    private func assertPlain(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        for word in Self.coreWords {
            XCTAssertFalse(text.contains(word), "\(text) holds the core's \(word)", file: file, line: line)
        }
    }

    /// The letter with Maya Ellison kept as written and every other name
    /// replaced: the review is done, but the quotation replaced her in the
    /// same Project, so the Vault refuses one known name written out.
    private func keptKnownName() throws -> (DocumentViewModel, Seeded) {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        let maya = try XCTUnwrap(model.snapshot.identities.first { $0.primaryWording == "Maya Ellison" })
        model.decide(identities: [maya.id], .keep)
        model.replaceAllPending()
        XCTAssertEqual(model.snapshot.phase, .ready)
        try model.session.saveChanges()
        return (model, seeded)
    }

    /// A known name written out: the tap refuses in the app's words, and
    /// once saved the bar and the menu say so before any tap, with Review,
    /// which opens the bar on the kept name.
    func testAKnownNameWrittenOutIsRefusedWithReview() async throws {
        let (model, _) = try keptKnownName()

        XCTAssertThrowsError(try model.session.copyForAI()) { error in
            guard case CoreError.handOffRefused(.namesWrittenOut(let count)) = error else {
                return XCTFail("\(error)")
            }
            XCTAssertGreaterThan(count, 0)
            assertPlain(error.localizedDescription)
        }
        model.copyForAI { _ in XCTFail("copied") }
        let refusal = try XCTUnwrap(model.refusal)
        XCTAssertNil(model.errorMessage)
        assertPlain(refusal.reason)
        assertPlain(refusal.detail)

        await model.checkSaved().value
        XCTAssertEqual(model.handOffRefusal, refusal)
        XCTAssertEqual(model.handOffBlockedReason, refusal.reason)
        XCTAssertEqual(model.primaryAction, .handOff)
        XCTAssertEqual(model.step(for: refusal), .review)

        model.take(.review)
        XCTAssertNil(model.refusal)
        XCTAssertEqual(model.review.reviewedCandidate?.original, "Maya Ellison")
    }

    /// Replacing the kept name, the step's purpose, makes it Ready again.
    func testReplacingTheKeptNameMakesItReady() async throws {
        let (model, _) = try keptKnownName()
        await model.checkSaved().value
        XCTAssertNotNil(model.handOffRefusal)

        let maya = try XCTUnwrap(model.snapshot.identities.first { $0.primaryWording == "Maya Ellison" })
        model.toggle(maya)
        XCTAssertNil(model.handOffRefusal, "the record is of the text before the change")
        try model.session.saveChanges()
        await model.checkSaved().value
        XCTAssertNil(model.handOffRefusal)
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertNotNil(copied)
    }

    /// Not yet reviewed: N names left to decide, and Replace N and copy
    /// replaces them as one step and copies.
    func testReplaceAndCopyReplacesThenCopies() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        let pending = model.snapshot.pendingCount
        let names = model.identityCounts.toReview
        XCTAssertGreaterThan(names, 1)
        let refusal = HandOffRefusal.namesToDecide(names: names, proposals: pending)
        XCTAssertEqual(model.handOffRefusal, refusal)
        XCTAssertEqual(model.handOffBlockedReason, "\(names) names left to decide")
        XCTAssertEqual(model.step(for: refusal), .replaceAndCopy(pending))
        XCTAssertEqual(HandOffRefusal.Step.replaceAndCopy(pending).title, "Replace \(pending) and copy")

        UIPasteboard.general.string = ""
        model.take(.replaceAndCopy(pending))

        XCTAssertEqual(model.snapshot.pendingCount, 0)
        XCTAssertNil(model.refusal)
        XCTAssertEqual(model.notice, "Copied for AI")
        let copied = try XCTUnwrap(UIPasteboard.general.string)
        XCTAssertFalse(copied.contains("Maya Ellison"))
        XCTAssertEqual(model.toast?.actionTitle, "Undo", "one undoable step")
    }

    /// Below the floor a document with no names left goes: the title menu
    /// agrees with the bar's Share instead of saying Find names first. At
    /// the detector tier the same document waits for Find names.
    func testBelowTheFloorTheMenuAgreesWithTheBar() throws {
        let (core, storage) = try Fixture.core(memory: Fixture.belowFloor)
        let session = try core.library.importDocument(.text("Lunch notes\n\nThe park is lovely today."), title: "Lunch", into: nil)
        _ = try session.recognize(observer: TestObserver())
        let jobs = LongJobRunner(background: NoBackgroundContinuation())

        let below = DocumentViewModel(session: session, jobs: jobs, storage: storage, findsNames: false)
        XCTAssertEqual(below.snapshot.phase, .unscanned)
        XCTAssertEqual(below.primaryAction, .handOff)
        XCTAssertNil(below.handOffRefusal)
        XCTAssertNil(below.handOffBlockedReason)
        var copied: String?
        below.copyForAI { copied = $0 }
        XCTAssertNotNil(copied, "the menu's Copy for AI goes, as the bar's does")

        let detector = DocumentViewModel(session: session, jobs: jobs, storage: storage, findsNames: true)
        XCTAssertEqual(detector.primaryAction, .findNames)
        XCTAssertEqual(detector.handOffRefusal, .notScanned)
        XCTAssertEqual(detector.step(for: .notScanned), .findNames)
    }

    /// A name another document of the Project replaced after this review
    /// began: the open review lacks it, so Review opens the document again
    /// from the Vault, which proposes it, and the bar opens on it.
    func testReviewProposesANameAnotherDocumentAddedSince() async throws {
        let (core, storage) = try Fixture.core()
        let ellison = try Fixture.createProject(Letters.ellisonProject, in: core)
        let note = try core.library.importDocument(.text("# Note\n\nCall Zara Quinn on Monday."), title: "Note", into: ellison)
        _ = try note.recognize(observer: TestObserver())
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let model = DocumentViewModel(session: note, jobs: jobs, storage: storage, findsNames: false)
        try note.saveChanges()
        await model.checkSaved().value
        XCTAssertNil(model.handOffRefusal, "nothing to replace, decided by hand")

        let other = try core.library.importDocument(.text("Zara Quinn signed."), title: "Other", into: ellison)
        _ = try other.recognize(observer: TestObserver())
        _ = try other.propose([.init(wording: "Zara Quinn", category: .person)])
        _ = try other.replaceAllPending()
        try other.saveChanges()

        await model.checkSaved().value
        XCTAssertEqual(model.handOffRefusal, .namesWrittenOut(1))
        XCTAssertTrue(model.snapshot.candidates.isEmpty, "the open review lacks it")
        model.take(.review)
        XCTAssertEqual(model.snapshot.pendingCount, 1)
        XCTAssertEqual(model.review.reviewedCandidate?.original, "Zara Quinn")
    }

    /// A save that fails before the hand-off refuses in the app's words,
    /// not the save's, which name the Vault's records.
    func testAFailedSaveIsRefusedInPlainWords() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.replaceAllPending()
        try Fixture.setVault(seeded.storage, writable: false)
        defer { try? Fixture.setVault(seeded.storage, writable: true) }
        XCTAssertThrowsError(try model.session.copyForAI()) { error in
            XCTAssertEqual(error as? CoreError, .handOffRefused(.failed))
        }
    }

    /// Each refusal reads in its own words, none the core's, with its step.
    func testEachRefusalHasItsOwnSentenceAndStep() {
        let refusals: [HandOffRefusal] = [
            .namesToDecide(names: 3, proposals: 5), .notScanned, .notRead, .notChecked, .changedSinceReview, .namesWrittenOut(2),
            .excludedFromAI, .failed,
        ]
        XCTAssertEqual(Set(refusals.map(\.reason)).count, refusals.count, "a sentence each")
        for refusal in refusals {
            assertPlain(refusal.reason)
            assertPlain(refusal.detail)
            assertPlain(CoreError.handOffRefused(refusal).localizedDescription)
        }
        XCTAssertEqual(HandOffRefusal(.ready), nil)
        XCTAssertEqual(HandOffRefusal(.changedSinceReview), .changedSinceReview)
        XCTAssertEqual(HandOffRefusal(.namesNotReplaced(2)), .namesWrittenOut(2))
        XCTAssertEqual(HandOffRefusal(.excludedFromAI), .excludedFromAI)
        XCTAssertEqual(HandOffRefusal(.notReviewed), .notChecked)
        XCTAssertEqual(HandOffRefusal.changedSinceReview.step(findsNames: true), .findNames)
        XCTAssertEqual(HandOffRefusal.namesWrittenOut(1).step(findsNames: true), .review)
        XCTAssertNil(HandOffRefusal.excludedFromAI.step(findsNames: true))
    }
}
