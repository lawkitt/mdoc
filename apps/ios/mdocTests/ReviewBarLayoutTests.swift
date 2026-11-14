import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// The docked review bar's height (docs/mobile-ui.md, Review bar): compact,
/// it keeps one height from card to card, so the text above it doesn't
/// jump. The mentions chip ("↓ 1 of 2") is taller than the name's line, and
/// the bar grew by about 20 pt on a card that showed it.
@MainActor
final class ReviewBarLayoutTests: XCTestCase {
    /// The bar's height on the card under review, laid out at a phone's width.
    private func barHeight(_ model: DocumentViewModel, size: DynamicTypeSize = .large) -> CGFloat {
        let host = UIHostingController(rootView: ReviewBar(model: model).environment(\.dynamicTypeSize, size))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.rootViewController = host
        window.isHidden = false
        host.view.layoutIfNeeded()
        return host.sizeThatFits(in: CGSize(width: 402, height: 874)).height
    }

    func testTheCompactBarKeepsOneHeightWhetherTheNameRepeatsOrNot() throws {
        // A hint line would change the height as cards are seen.
        Hints.shared.setAvailableForTesting([])
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(
            .text("Maya Bennett signs for Daniel Okafor.\n\nLater Maya Bennett signs again.\n"),
            title: "Agreement", into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([
            SampleDocument.Name(wording: "Maya Bennett", category: .person),
            SampleDocument.Name(wording: "Daniel Okafor", category: .person),
        ])
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()),
            storage: try Fixture.temporaryStorage(), findsNames: false)
        let candidates = model.snapshot.candidates
        let repeated = try XCTUnwrap(candidates.first { $0.original == "Maya Bennett" })
        let once = try XCTUnwrap(candidates.first { $0.original == "Daniel Okafor" })
        XCTAssertEqual(model.mentionPosition(of: repeated).count, 2)
        XCTAssertEqual(model.mentionPosition(of: once).count, 1)

        for size in [DynamicTypeSize.large, .accessibility2] {
            model.open(repeated.id)
            XCTAssertEqual(model.reviewing, repeated.id)
            let withChip = barHeight(model, size: size)
            model.open(once.id)
            XCTAssertEqual(model.reviewing, once.id)
            let withoutChip = barHeight(model, size: size)
            XCTAssertGreaterThan(withoutChip, 100, "the bar was laid out")
            XCTAssertEqual(withChip, withoutChip, accuracy: 0.5, "one height at \(size)")
        }
    }

    /// The name wraps to two lines (ADR 0114) and the bar keeps one height
    /// whether it takes one line, two, or more than two.
    func testTheCompactBarKeepsOneHeightWhetherTheNameTakesOneLineOrTwo() throws {
        Hints.shared.setAvailableForTesting([])
        let short = "Daniel Okafor"
        let twoLines = "ФИО Петрова-Голенищева Анастасия Владиславовна"
        let longer = "Общество с ограниченной ответственностью «Северо-Западная инвестиционно-строительная компания»"
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(
            .text("\(short) signs for \(twoLines).\n\nWitness: \(longer).\n"),
            title: "Agreement", into: .newCase(name: "Wrap"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([
            SampleDocument.Name(wording: short, category: .person),
            SampleDocument.Name(wording: twoLines, category: .person),
            SampleDocument.Name(wording: longer, category: .organization),
        ])
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()),
            storage: try Fixture.temporaryStorage(), findsNames: false)
        let ids = try [short, twoLines, longer].map { wording in
            try XCTUnwrap(model.snapshot.candidates.first { $0.original == wording }, wording).id
        }

        for size in [DynamicTypeSize.large, .accessibility2] {
            let heights = ids.map { id in
                model.open(id)
                return barHeight(model, size: size)
            }
            XCTAssertGreaterThan(heights[0], 100, "the bar was laid out")
            XCTAssertEqual(heights[1], heights[0], accuracy: 0.5, "two lines, one height at \(size)")
            XCTAssertEqual(heights[2], heights[0], accuracy: 0.5, "over two lines, one height at \(size)")
        }
    }

    /// The card's name (`UnhyphenatedText` with `lines: 2`) at about the
    /// card's width: a long name takes two lines instead of one cut line,
    /// and a longer one stops at two.
    func testTheCardsNameWrapsToTwoLinesAtMost() {
        func height(_ text: String, lines: Int) -> CGFloat {
            let host = UIHostingController(
                rootView: UnhyphenatedText(text, style: .headline, weight: .semibold, lines: lines))
            return host.sizeThatFits(in: CGSize(width: 240, height: 1000)).height
        }
        let one = height("Daniel Okafor", lines: 2)
        let two = height("1\n2", lines: 2)
        let name = "ФИО Петрова-Голенищева Анастасия Владиславовна"
        let longer = "Общество с ограниченной ответственностью «Северо-Западная инвестиционно-строительная компания»"
        XCTAssertGreaterThan(two, one * 1.5, "two lines are taller than one")
        XCTAssertEqual(height(name, lines: 2), two, accuracy: 0.5, "the name wraps to two lines")
        XCTAssertEqual(height(longer, lines: 2), two, accuracy: 0.5, "a longer name stops at two lines")
        XCTAssertGreaterThan(height(longer, lines: 0), two + 1, "uncapped, it would take more")
    }
}
