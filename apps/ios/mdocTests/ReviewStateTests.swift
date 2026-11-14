import XCTest
@testable import mdoc

/// The review bar's state on its own (ADR 0104): a value changed only by
/// its transitions, fed the real core's snapshots. No view model and no
/// view; the session only makes the snapshots.
@MainActor
final class ReviewStateTests: XCTestCase {
    private func letter() throws -> (any mdoc.DocumentSession, ReviewState) {
        let seeded = try Fixture.seeded()
        let session = try seeded.session(seeded.letter)
        return (session, ReviewState(session.snapshot()))
    }

    private func proposed(_ state: ReviewState) -> [Candidate] {
        state.document.candidates.filter { $0.state == .proposed }
    }

    func testTheOrderIsSortedOnceAndFollowsTheSnapshot() throws {
        let (session, state) = try letter()
        let snapshot = session.snapshot()
        XCTAssertEqual(state.document.candidates, snapshot.candidates.sorted { $0.range.start < $1.range.start })
        XCTAssertEqual(state.order, state.document, "no focus: the whole document")
        let repeated = try XCTUnwrap(snapshot.identities.first { $0.mentionCount > 1 })
        let mentions = state.sortedMentions(of: repeated.id)
        XCTAssertEqual(mentions.count, repeated.mentionCount)
        XCTAssertEqual(mentions.map(\.range.start), mentions.map(\.range.start).sorted())
    }

    func testBeginSelectsTheFirstProposalAndEndClearsTheBar() throws {
        var (_, state) = try letter()
        XCTAssertTrue(state.begin())
        XCTAssertEqual(state.reviewing, proposed(state).first?.id)
        XCTAssertEqual(state.scope, .all)
        XCTAssertEqual(state.cardsSeen, 1)
        state.setExpanded(true)
        state.end()
        XCTAssertNil(state.reviewing)
        XCTAssertFalse(state.expanded)
        XCTAssertNil(state.focus)
    }

    func testTheScopeHoldsWithinAnIdentityAndResetsOnTheNext() throws {
        var (_, state) = try letter()
        let repeated = try XCTUnwrap(state.document.candidates.first { candidate in
            state.sortedMentions(of: candidate.identityID).count > 1
        })
        let mentions = state.sortedMentions(of: repeated.identityID)
        state.select(mentions[0].id)
        state.setScope(.this)
        state.select(mentions[1].id)
        XCTAssertEqual(state.scope, .this, "the same identity keeps it")
        let other = try XCTUnwrap(state.document.candidates.first { $0.identityID != repeated.identityID })
        state.select(other.id)
        XCTAssertEqual(state.scope, .all, "the next identity starts at All")
    }

    func testSteppingStopsAtTheEndsAndFocusLimitsIt() throws {
        var (_, state) = try letter()
        state.begin()
        XCTAssertFalse(state.step(-1))
        for _ in 1..<state.order.count { XCTAssertTrue(state.step(1)) }
        XCTAssertFalse(state.step(1))
        let repeated = try XCTUnwrap(state.document.candidates.first { state.sortedMentions(of: $0.identityID).count > 1 })
        state.begin(focus: .identity(repeated.identityID))
        XCTAssertTrue(state.order.candidates.allSatisfy { $0.identityID == repeated.identityID })
        // A name outside the focus ends it.
        let outside = try XCTUnwrap(state.document.candidates.first { $0.identityID != repeated.identityID })
        state.select(outside.id)
        XCTAssertNil(state.focus)
        XCTAssertEqual(state.order, state.document)
    }

    /// A decision moves on to the next undecided name; the last one says
    /// All reviewed and ends review.
    func testAdvanceMovesOnThenSaysAllReviewed() throws {
        var (session, state) = try letter()
        state.begin()
        var guardRail = 100
        var last: ReviewState.Advance?
        while let current = state.reviewedCandidate, guardRail > 0 {
            let index = state.order.index(of: current.id)
            state.apply(try session.decide(current.id, .keep, scope: .all))
            state.decided()
            last = state.advance(from: index)
            guardRail -= 1
        }
        XCTAssertEqual(last, .allReviewed(count: state.document.count, focused: false))
        XCTAssertFalse(state.isReviewing)
        XCTAssertTrue(state.documentIsDone)
        XCTAssertGreaterThan(state.decisionsHere, 0)
    }

    /// Each opening of the bar is a visit: its changes, less those undone,
    /// decide whether Done says All reviewed (ADR 0116).
    func testAVisitCountsItsOwnChanges() throws {
        var (_, state) = try letter()
        state.begin()
        XCTAssertFalse(state.changedThisVisit)
        state.changed()
        XCTAssertTrue(state.changedThisVisit)
        state.undid()
        state.undid()
        XCTAssertFalse(state.changedThisVisit, "never below none")
        state.changed()
        state.end()
        state.begin()
        XCTAssertFalse(state.changedThisVisit, "a new visit starts with none")
        state.changed()
        let other = try XCTUnwrap(state.document.candidates.last)
        state.select(other.id)
        XCTAssertTrue(state.changedThisVisit, "stepping stays in the visit")
    }

    /// Undo puts the bar back on the name it undid, out of a focus that
    /// doesn't hold it, and asks the text to scroll there. Each request is
    /// new, even for the same name, so none is lost (the dropped reveal).
    func testFollowingAnUndoRevealsTheNameEveryTime() throws {
        var (session, state) = try letter()
        state.begin()
        let first = try XCTUnwrap(state.reviewedCandidate)
        state.apply(try session.decide(first.id, .keep, scope: .this))
        state.reselect(near: first, index: state.order.index(of: first.id))
        XCTAssertEqual(state.reviewedCandidate?.state, .kept)
        let undone = try session.undo()
        state.apply(undone)
        state.follow(undone.redoStep)
        XCTAssertEqual(state.reviewedCandidate?.original, first.original)
        XCTAssertEqual(state.reviewedCandidate?.state, .proposed)
        let reveal = try XCTUnwrap(state.reveal)
        XCTAssertEqual(reveal.target, .candidate(try XCTUnwrap(state.reviewing)))
        let redone = try session.redo()
        state.apply(redone)
        state.follow(redone.undoStep)
        XCTAssertEqual(state.reviewedCandidate?.original, first.original, "the bar stays on the same name")
        XCTAssertGreaterThan(try XCTUnwrap(state.reveal).id, reveal.id, "a new request for the same name")
    }

    /// A snapshot without the name under review closes the bar; a step
    /// that moved its range finds it again by position.
    func testASnapshotWithoutTheNameClosesTheBar() throws {
        var (session, state) = try letter()
        state.begin()
        let first = try XCTUnwrap(state.reviewedCandidate)
        let index = state.order.index(of: first.id)
        let replaced = try session.decide(first.id, .replace, scope: .this)
        state.apply(replaced)
        if replaced.candidate(first.id) == nil {
            XCTAssertNil(state.reviewing, "its id is gone")
        }
        state.reselect(near: first, index: index)
        XCTAssertEqual(state.order.index(of: state.reviewing), index, "back on it by position")
        XCTAssertEqual(state.reviewedCandidate?.state, .replaced)
    }

    /// One hint at most, at its moment, and only one still to teach.
    func testHintsShowOnceAtTheirMoment() throws {
        var (_, state) = try letter()
        state.setTeachable([.swipe, .longPress])
        state.begin()
        XCTAssertNil(state.hint, "not on the first card")
        state.step(1)
        XCTAssertEqual(state.hint, .swipe, "from the second card")
        state.learned([.longPress])
        XCTAssertNil(state.hint, "gone once learned")
        for _ in 0..<3 { state.decided() }
        state.step(1)
        XCTAssertNil(state.hint, "no second hint in this document")

        var (_, fresh) = try letter()
        fresh.setTeachable([])
        fresh.begin()
        fresh.step(1)
        XCTAssertNil(fresh.hint, "nothing left to teach, nothing shows")
    }

    func testOpeningTheBarAsksForAReveal() throws {
        var (_, state) = try letter()
        state.begin()
        XCTAssertNil(state.reveal)
        state.setExpanded(true)
        XCTAssertEqual(state.reveal?.target, .candidate(try XCTUnwrap(state.reviewing)))
        let id = state.reveal?.id
        state.setExpanded(true)
        XCTAssertEqual(state.reveal?.id, id, "already open: no new scroll")
        state.reveal(SourceRange(start: 0, end: 4))
        XCTAssertEqual(state.reveal?.target, .range(SourceRange(start: 0, end: 4)))
    }
}
