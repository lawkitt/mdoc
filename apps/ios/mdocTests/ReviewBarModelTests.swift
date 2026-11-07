import MdocFFI
import XCTest
@testable import mdoc

/// The review bar's model (ADR 0098): stepping in document order, moving on
/// after a decision, All reviewed, undo moving the bar back, focus and the
/// identity tools, over the real core and the seeded letter.
@MainActor
final class ReviewBarModelTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let seeded = try Fixture.seeded()
        return try seeded.model(seeded.letter)
    }

    func testBeginReviewOpensTheFirstProposalInDocumentOrder() throws {
        let model = try model()
        model.beginReview()
        let first = model.snapshot.candidates.filter { $0.state == .proposed }.min { $0.range.start < $1.range.start }
        XCTAssertEqual(model.reviewing, first?.id)
        XCTAssertEqual(model.reviewScope, .all)
    }

    func testSteppingStopsAtTheEnds() throws {
        let model = try model()
        model.beginReview()
        XCTAssertFalse(model.step(-1), "nothing before the first")
        XCTAssertTrue(model.step(1))
        XCTAssertTrue(model.step(-1))
        let last = model.reviewOrder.count - 1
        for _ in 0..<last { model.step(1) }
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), last)
        XCTAssertFalse(model.step(1), "nothing after the last")
    }

    func testADecisionMovesToTheNextUndecided() throws {
        let model = try model()
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        model.decideCurrent(.keep)
        let now = try XCTUnwrap(model.reviewedCandidate)
        XCTAssertEqual(now.state, .proposed)
        XCTAssertNotEqual(now.identityID, first.identityID, "All covered the first name's other mentions")
        XCTAssertTrue(model.canUndo)
    }

    func testTheLastDecisionEndsReviewWithAllReviewed() throws {
        let model = try model()
        model.beginReview()
        var guardRail = 100
        while model.isReviewing && guardRail > 0 {
            model.decideCurrent(.replace)
            guardRail -= 1
        }
        XCTAssertFalse(model.isReviewing)
        XCTAssertEqual(model.snapshot.pendingCount, 0)
        XCTAssertTrue(model.toast?.text.hasPrefix("All") ?? false, "\(String(describing: model.toast?.text))")
    }

    func testUndoMovesTheBarBackAndOffersRedo() throws {
        let model = try model()
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        model.decideCurrent(.replace)
        XCTAssertNotEqual(model.reviewedCandidate?.identityID, first.identityID)
        model.undo()
        XCTAssertEqual(model.reviewedCandidate?.original, first.original)
        XCTAssertEqual(model.reviewedCandidate?.state, .proposed)
        XCTAssertEqual(model.toast?.actionTitle, "Redo")
        XCTAssertTrue(model.canRedo)
        model.redo()
        XCTAssertEqual(model.snapshot.candidates.first { $0.original == first.original }?.state, .replaced)
    }

    func testScopeResetsOnTheNextIdentity() throws {
        let model = try model()
        model.beginReview()
        model.reviewScope = .this
        let identity = model.reviewedCandidate?.identityID
        while model.reviewedCandidate?.identityID == identity, model.step(1) {}
        XCTAssertEqual(model.reviewScope, .all)
    }

    func testFocusKeepsSteppingInsideTheIdentity() throws {
        let model = try model()
        let repeated = try XCTUnwrap(model.snapshot.identities.first { $0.mentionCount > 1 })
        model.beginReview(focus: .identity(repeated.id))
        XCTAssertTrue(model.reviewOrder.candidates.allSatisfy { $0.identityID == repeated.id })
        XCTAssertEqual(model.reviewedCandidate?.identityID, repeated.id)
        model.clearFocus()
        XCTAssertNil(model.focus)
        XCTAssertTrue(model.isReviewing)
    }

    func testMentionsChipCyclesThroughTheIdentity() throws {
        let model = try model()
        let repeated = try XCTUnwrap(model.snapshot.identities.first { $0.mentionCount > 1 })
        let mentions = model.snapshot.mentions(of: repeated.id).sorted { $0.range.start < $1.range.start }
        model.select(mentions[0].id)
        model.nextMention()
        XCTAssertEqual(model.reviewing, mentions[1].id)
        for _ in 1..<mentions.count { model.nextMention() }
        XCTAssertEqual(model.reviewing, mentions[0].id, "cycles round")
    }

    func testSameAsAllJoinsTheTargetAndUndoes() throws {
        let model = try model()
        let people = model.snapshot.identities.filter { $0.category == .person }
        XCTAssertGreaterThan(people.count, 1)
        let source = people[1], target = people[0]
        let mention = try XCTUnwrap(model.snapshot.mentions(of: source.id).first)
        model.select(mention.id)
        model.sameAs(target.id)
        XCTAssertNil(model.snapshot.identities.first { $0.id == source.id })
        XCTAssertEqual(model.reviewedCandidate?.alias, target.alias)
        XCTAssertTrue(model.toast?.text.hasPrefix("Same as") ?? false)
        model.undo()
        XCTAssertNotNil(model.snapshot.identities.first { $0.id == source.id })
    }

    func testRenameToATakenAliasOffersSameAs() throws {
        let model = try model()
        let people = model.snapshot.identities.filter { $0.category == .person }
        let mention = try XCTUnwrap(model.snapshot.mentions(of: people[1].id).first)
        model.select(mention.id)
        XCTAssertEqual(model.rename(to: people[0].alias), .taken(people[0]))
        XCTAssertEqual(model.rename(to: "CLIENT"), .done)
        XCTAssertEqual(model.reviewedCandidate?.alias, "CLIENT")
    }

    func testMarkingASelectionOpensTheBarOnAProposal() throws {
        let model = try model()
        let text = model.snapshot.markdown
        let word = "staircase"
        let found = try XCTUnwrap(text.range(of: word))
        let byteStart = text.utf8.distance(from: text.startIndex, to: found.lowerBound)
        model.markSelection(SourceRange(start: byteStart, end: byteStart + word.utf8.count))
        XCTAssertTrue(model.isReviewing)
        XCTAssertEqual(model.reviewedCandidate?.state, .proposed)
    }

    func testFindReturnsEveryMatchAsSourceBytes() throws {
        let model = try model()
        let matches = model.matches(of: "ELLISON")
        XCTAssertGreaterThan(matches.count, 1)
        let bytes = Array(model.snapshot.markdown.utf8)
        for match in matches {
            XCTAssertEqual(String(decoding: bytes[match.start..<match.end], as: UTF8.self).lowercased(), "ellison")
        }
        XCTAssertTrue(model.matches(of: "   ").isEmpty)
    }

    func testSystemUndoFollowsTheCore() throws {
        let model = try model()
        let manager = UndoManager()
        manager.groupsByEvent = false
        model.systemUndo = manager
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        manager.beginUndoGrouping()
        model.decideCurrent(.replace)
        manager.endUndoGrouping()
        XCTAssertTrue(manager.canUndo)
        manager.undo()
        XCTAssertEqual(model.snapshot.candidates.first { $0.original == first.original }?.state, .proposed)
        XCTAssertTrue(manager.canRedo)
        manager.redo()
        XCTAssertEqual(model.snapshot.candidates.first { $0.original == first.original }?.state, .replaced)
        // The bar's Undo goes through the manager too.
        model.undoTapped()
        XCTAssertEqual(model.snapshot.candidates.first { $0.original == first.original }?.state, .proposed)
    }

    func testASelectionLosesItsLineEnd() {
        let text = "# Title words\nNext"
        let trimmed = DocumentViewModel.trimmed(SourceRange(start: 8, end: 14), in: text)
        XCTAssertEqual(trimmed, SourceRange(start: 8, end: 13))
        XCTAssertEqual(DocumentViewModel.trimmed(SourceRange(start: 13, end: 14), in: text).length, 0)
    }

    // MARK: Full view and decided cards (2026-11-06 grill)

    func testAMentionPillChangesOnlyThatMentionAndTheBarStays() throws {
        let model = try model()
        let repeated = try XCTUnwrap(model.snapshot.identities.first { $0.mentionCount > 1 })
        let mentions = model.snapshot.mentions(of: repeated.id).sorted { $0.range.start < $1.range.start }
        model.select(mentions[0].id)
        let place = model.reviewOrder.index(of: model.reviewing)
        model.decideMention(mentions[1].id, .replace)
        let now = model.snapshot.mentions(of: repeated.id).sorted { $0.range.start < $1.range.start }
        XCTAssertEqual(now.map(\.state).filter { $0 == .replaced }.count, 1, "only the one mention")
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), place, "the bar stays where it was")
        XCTAssertTrue(model.canUndo)
    }

    func testInsteadKeepsTheCardAndNextMovesOn() throws {
        let model = try model()
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        model.decideCurrent(.replace)
        model.select(try XCTUnwrap(model.snapshot.mentions(of: first.identityID).first).id)
        let place = model.reviewOrder.index(of: model.reviewing)
        model.changeCurrent(.keep)
        XCTAssertEqual(model.reviewedCandidate?.state, .kept)
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), place, "Keep instead stays on the card")
        model.goToNextUndecided()
        XCTAssertEqual(model.reviewedCandidate?.state, .proposed)
    }

    /// The long press's Keep instead and Replace instead, and K and Return
    /// on a decided card, change it in place, as the card's own buttons do
    /// (ADR 0101, #678).
    func testChangingADecidedNameStaysOnItFromEveryEntryPoint() throws {
        let model = try model()
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        model.decideCurrent(.replace)
        model.select(try XCTUnwrap(model.snapshot.mentions(of: first.identityID).first).id)
        let place = model.reviewOrder.index(of: model.reviewing)
        model.decideCurrent(.keep)
        XCTAssertEqual(model.reviewedCandidate?.state, .kept)
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), place, "Keep instead stays on the name")
        model.decideCurrent(.replace)
        XCTAssertEqual(model.reviewedCandidate?.state, .replaced)
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), place, "Replace instead stays on the name")
        XCTAssertNil(model.errorMessage)
    }

    /// Return is the card's filled button: Replace on a proposed name,
    /// Replace instead on a kept one (which stays), and Next on a replaced
    /// one, as the decided card's filled Next (ADR 0101, review of #678).
    func testReturnReplacesThenMovesOnFromAReplacedName() throws {
        let model = try model()
        model.beginReview()
        let first = try XCTUnwrap(model.reviewedCandidate)
        model.decideCurrent(.keep)
        model.select(try XCTUnwrap(model.snapshot.mentions(of: first.identityID).first).id)
        let place = model.reviewOrder.index(of: model.reviewing)
        model.returnKey()
        XCTAssertEqual(model.reviewedCandidate?.state, .replaced, "Replace instead")
        XCTAssertEqual(model.reviewOrder.index(of: model.reviewing), place, "and stays")
        model.returnKey()
        XCTAssertEqual(model.reviewedCandidate?.state, .proposed, "then Next, to the next undecided name")
    }

    /// Each document has its own undo history (ADR 0104, #672): after a
    /// change in one document, the window's undo in another never reaches
    /// it, and back in the first its step undoes again.
    func testSwitchingDocumentsNeverUndoesTheOthersStep() throws {
        let seeded = try Fixture.seeded()
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let letter = try seeded.model(seeded.letter, jobs: jobs)
        let other = try seeded.model(seeded.quotation, jobs: jobs)
        let window = UndoManager()
        window.groupsByEvent = false
        let letterScreen = UUID(), otherScreen = UUID()
        letter.attachSystemUndo(window, for: letterScreen)
        letter.beginReview()
        let first = try XCTUnwrap(letter.reviewedCandidate)
        func state() -> MentionState? { letter.snapshot.candidates.first { $0.original == first.original }?.state }
        window.beginUndoGrouping()
        letter.decideCurrent(.replace)
        window.endUndoGrouping()
        XCTAssertEqual(state(), .replaced)

        // The other document opens in the same window.
        letter.detachSystemUndo(for: letterScreen)
        other.attachSystemUndo(window, for: otherScreen)
        XCTAssertFalse(window.canUndo, "the window holds no step of the letter's")
        other.undoTapped()
        XCTAssertEqual(state(), .replaced, "the other document's Undo leaves the letter alone")

        // Back to the letter: ⌘Z reaches its step again.
        other.detachSystemUndo(for: otherScreen)
        letter.attachSystemUndo(window, for: UUID())
        XCTAssertTrue(window.canUndo)
        window.undo()
        XCTAssertEqual(state(), .proposed)
    }

    /// A size-class change builds a new screen for the same document, and
    /// the old one may leave after the new one appeared: its leaving keeps
    /// the document's steps in the window (review of #672).
    func testANewScreenForTheSameDocumentKeepsItsSteps() throws {
        let model = try model()
        let window = UndoManager()
        window.groupsByEvent = false
        let wide = UUID(), compact = UUID()
        model.attachSystemUndo(window, for: wide)
        model.beginReview()
        window.beginUndoGrouping()
        model.decideCurrent(.replace)
        window.endUndoGrouping()
        model.attachSystemUndo(window, for: compact)
        model.detachSystemUndo(for: wide)
        XCTAssertTrue(window.canUndo, "the old screen leaving takes nothing")
        window.beginUndoGrouping()
        model.decideCurrent(.keep)
        window.endUndoGrouping()
        window.undo()
        window.undo()
        XCTAssertFalse(window.canUndo)
        XCTAssertEqual(model.snapshot.pendingCount, try self.model().snapshot.pendingCount, "both steps undone")
    }

    /// Find's previews cut on characters, never inside a letter (#679).
    func testFindPreviewsCutOnCharacters() throws {
        let (core, storage) = try Fixture.core()
        let text = String(repeating: "а", count: 60) + " Иван " + String(repeating: "б", count: 60)
        let session = try core.library.importDocument(.text(text), title: "Письмо", into: .newCase(name: "Петров"))
        _ = try session.recognize(observer: TestObserver())
        let model = DocumentViewModel(session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let match = try XCTUnwrap(model.matches(of: "Иван").first)
        let bytes = Array(model.snapshot.markdown.utf8)
        XCTAssertEqual(bytes[match.start - 50] & 0xC0, 0x80, "50 bytes back is inside a letter")
        let preview = model.context(of: match)
        XCTAssertFalse(preview.contains("\u{FFFD}"), preview)
        XCTAssertTrue(preview.contains("а Иван б"), preview)
        XCTAssertTrue(preview.hasPrefix("…") && preview.hasSuffix("…"), preview)
        XCTAssertEqual(preview, "…" + String(repeating: "а", count: 49) + " Иван " + String(repeating: "б", count: 49) + "…")
    }

    func testNextOnTheLastDecidedCardEndsReview() throws {
        let model = try model()
        model.beginReview()
        model.replaceAllPending()
        model.beginReview()
        XCTAssertTrue(model.reviewOrder.isDone)
        model.goToNextUndecided()
        XCTAssertFalse(model.isReviewing)
    }

    func testOneHintPerDocumentAtItsMoment() throws {
        let wasEnabled = ReviewHint.enabled
        ReviewHint.resetAll()
        ReviewHint.enabled = true
        defer {
            ReviewHint.resetAll()
            ReviewHint.enabled = wasEnabled
        }
        let model = try model()
        model.beginReview()
        XCTAssertNil(model.hint, "not on the first card")
        model.step(1)
        XCTAssertEqual(model.hint, .swipe, "after the second card")
        model.learn(.swipe)
        XCTAssertNil(model.hint, "gone once used")
        for _ in 0..<3 where model.isReviewing { model.decideCurrent(.keep) }
        XCTAssertNil(model.hint, "no second hint in this document")
        XCTAssertTrue(ReviewHint.swipe.isLearned)
    }
}
