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
}
