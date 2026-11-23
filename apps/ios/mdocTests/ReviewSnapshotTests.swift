import SwiftUI
import XCTest
@testable import mdoc

/// The review bar's parts (docs/specs/mobile.md, Review): the full view's
/// mention rows with their state marks, the hint line, and the All reviewed
/// line. The bar itself scrolls and holds menus, which `ImageRenderer`
/// doesn't draw, so its parts are drawn as the bar stacks them.
@MainActor
final class ReviewSnapshotTests: XCTestCase {
    private let person = Category.person.color

    private var rows: some View {
        VStack(alignment: .leading, spacing: 0) {
            MentionRow(
                snippet: MentionSnippet(heading: "Services agreement", before: "…Walk, Exeter EX1 9ZZ (\"the Client\") And: ", name: "Maya Bennett", after: ", trading as Bennett Design, 22 Orchard…"),
                state: .replaced, isCurrent: true, tint: person, onOpen: {}, onToggle: {})
            Divider().padding(.leading, Space.s)
            MentionRow(
                snippet: MentionSnippet(heading: "1. The work", before: "…offices at 3 Mill Lane, Taunton TA1 9ZZ. ", name: "Ms Bennett", after: " will report to Daniel Okafor, the…"),
                state: .kept, isCurrent: false, tint: person, onOpen: {}, onToggle: {})
            Divider().padding(.leading, Space.s)
            MentionRow(
                snippet: MentionSnippet(heading: "3. Contact", before: "…for ORG_1 by Daniel Okafor. Signed by ", name: "Maya Bennett", after: "."),
                state: .proposed, isCurrent: false, tint: person, onOpen: {}, onToggle: {})
        }
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xs)
        .background(Palette.background)
    }

    func testMentionRows() {
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(rows, named: "mention-rows", appearance: appearance)
        }
        Snapshot.assert(rows, named: "mention-rows-ax2", size: .accessibility2)
    }

    func testHintAndAllReviewedLines() {
        let candidates = (0..<15).map { index in
            Candidate(
                id: CandidateID(rawValue: "\(index)"), identityID: IdentityID(rawValue: "\(index)"),
                range: SourceRange(start: index * 10, end: index * 10 + 4), original: "Name", alias: "PERSON_\(index)",
                category: .person, state: index < 12 ? .replaced : .kept)
        }
        let lines = VStack(alignment: .leading, spacing: Space.s) {
            ReviewHintLine(hint: .swipe) {}
            ReviewHintLine(hint: .longPress) {}
            AllReviewedLine(candidates: candidates)
        }
        .padding()
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(lines, named: "review-lines", appearance: appearance)
        }
    }

    /// The card's chips under the strip: the scope chip, All N / This one,
    /// shows from two mentions, and one mention leaves nothing to choose
    /// (#1036). Moved from `ReviewFlowUITests` (#1076); its share-restore
    /// run still checks on the real screen that one mention shows none.
    func testTheScopeChipShowsFromTwoMentions() throws {
        Hints.shared.setAvailableForTesting([])
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(
            .text("Maya Bennett signs for Daniel Okafor.\n\nLater Maya Bennett signs again.\n"),
            title: "Agreement", into: nil)
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([
            SampleDocument.Name(wording: "Maya Bennett", category: .person),
            SampleDocument.Name(wording: "Daniel Okafor", category: .person),
        ])
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()),
            storage: try Fixture.temporaryStorage(), findsNames: false)
        let twice = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Bennett" })
        let once = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Daniel Okafor" })
        let chips = VStack(alignment: .leading, spacing: Space.m) {
            ReviewCard(model: model, candidate: twice, part: .controls)
            Divider()
            ReviewCard(model: model, candidate: once, part: .controls)
        }
        .padding()
        .background(Palette.background)
        // The chips scroll, which `ImageRenderer` leaves blank.
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(chips, named: "review-scope-chip", appearance: appearance, inWindow: true)
        }
    }
}
