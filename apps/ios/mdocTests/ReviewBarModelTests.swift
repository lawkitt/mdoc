import XCTest
@testable import mdoc

/// The review bar's model (ADR 0098): stepping in document order, moving on
/// after a decision, All reviewed, undo moving the bar back, focus and the
/// identity tools, over the fake core.
@MainActor
final class ReviewBarModelTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let core = Fixture.core()
        let session = try core.library.session(for: Fixture.letter)
        return DocumentViewModel(session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)))
    }

    func testBeginReviewOpensTheFirstProposalInDocumentOrder() throws {
        let model = try model()
        model.beginReview()
        let first = model.snapshot.candidates.filter { $0.state == .proposed }.min { $0.range.start < $1.range.start }
        XCTAssertEqual(model.reviewing, first?.id)
        XCTAssertEqual(model.reviewScope, .allOccurrences)
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
        model.reviewScope = .justThisOne
        let identity = model.reviewedCandidate?.identityID
        while model.reviewedCandidate?.identityID == identity, model.step(1) {}
        XCTAssertEqual(model.reviewScope, .allOccurrences)
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
}
