import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// The document's text view (ADR 0104): a card change recolours the names
/// in place and never builds the document again; a content change does. A
/// reveal waits for the layout and then runs once, also when the snapshot
/// changed and the name under review didn't (undoing a Keep on it).
@MainActor
final class MarkdownTextViewTests: XCTestCase {
    /// Maya Bennett three times: at the top, in a table cell and at the end
    /// of a document a few screens long.
    private static let text: String = {
        let filler = (1...40).map { "Paragraph \($0) of the agreement, with enough words to fill a line or two of the text." }
        return "# Agreement\n\nMaya Bennett signs first.\n\n" + filler.joined(separator: "\n\n")
            + "\n\n| Name | Role |\n| --- | --- |\n| Maya Bennett | Tenant |\n\n"
            + filler.prefix(20).joined(separator: "\n\n") + "\n\nSigned last by Maya Bennett.\n"
    }()

    private struct Harness {
        let session: any mdoc.DocumentSession
        let window: UIWindow
        let view: CandidateTextView
        let coordinator: MarkdownTextView.Coordinator
    }

    private func harness() throws -> Harness {
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(.text(Self.text), title: "Agreement", into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([SampleDocument.Name(wording: "Maya Bennett", category: .person)])
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.isEditable = false
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        view.layoutIfNeeded()
        let coordinator = MarkdownTextView.Coordinator(parent: textView(session.snapshot()))
        coordinator.attach(to: view)
        view.delegate = coordinator
        return Harness(session: session, window: window, view: view, coordinator: coordinator)
    }

    private func textView(
        _ snapshot: DocumentSnapshot, selected: CandidateID? = nil, scoped: Set<CandidateID> = [],
        reveal: RevealRequest? = nil
    ) -> MarkdownTextView {
        MarkdownTextView(
            snapshot: snapshot, selected: selected, scoped: scoped, reveal: reveal,
            onCandidate: { _ in }, onReplace: { _ in }, onReviewNext: {})
    }

    private func show(_ harness: Harness, _ view: MarkdownTextView) {
        harness.coordinator.update(harness.view, with: view, dynamicTypeSize: .large, colorScheme: .light)
        harness.view.layoutIfNeeded()
    }

    /// The decoration keys at a candidate's first character, in the text
    /// view that holds it (the main text or a table cell).
    private func decoration(of candidate: Candidate, in view: CandidateTextView, offsets: TextOffsets) -> [String] {
        let range = offsets.nsRange(candidate.range)
        let cell = view.tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
            NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
        }
        let text: NSAttributedString = cell?.textStorage ?? view.textStorage
        let location = range.location - (cell?.location ?? 0)
        return MarkdownStyler.decorationKeys.map { key in
            let value = text.attribute(key, at: location, effectiveRange: nil)
            guard let colour = value as? UIColor else { return String(describing: value ?? "none") }
            var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
            colour.resolvedColor(with: view.traitCollection).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
            return String(format: "%.3f %.3f %.3f %.3f", red, green, blue, alpha)
        }
    }

    /// The same keys in a document built fresh with this decoration.
    private func fresh(
        _ snapshot: DocumentSnapshot, selected: CandidateID?, scoped: Set<CandidateID>, view: CandidateTextView
    ) throws -> [[String]] {
        let window = UIWindow(frame: view.frame)
        let other = CandidateTextView()
        other.isEditable = false
        other.textContainerInset = view.textContainerInset
        other.frame = view.frame
        window.addSubview(other)
        window.isHidden = false
        other.layoutIfNeeded()
        let document = MarkdownStyler(traits: other.traitCollection, selected: selected, scoped: scoped, width: other.textWidth)
            .document(for: snapshot)
        other.show(document, width: other.textWidth)
        other.layoutIfNeeded()
        let offsets = TextOffsets(snapshot.markdown)
        return snapshot.candidates.map { decoration(of: $0, in: other, offsets: offsets) }
    }

    func testACardChangeRecoloursInPlaceAndNeverRebuilds() throws {
        let harness = try harness()
        let snapshot = harness.session.snapshot()
        let mentions = snapshot.candidates.sorted { $0.range.start < $1.range.start }
        XCTAssertEqual(mentions.count, 3)
        let offsets = TextOffsets(snapshot.markdown)
        show(harness, textView(snapshot))
        XCTAssertEqual(harness.view.tableViews.count, 1)
        let built = harness.coordinator.rebuilds
        XCTAssertGreaterThan(built, 0)

        // The bar on the first mention, All reaching the other two (one in
        // the table): recoloured as a fresh build would draw them.
        let scoped = Set(mentions.dropFirst().map(\.id))
        show(harness, textView(snapshot, selected: mentions[0].id, scoped: scoped))
        XCTAssertEqual(harness.coordinator.rebuilds, built, "a card change builds nothing")
        let decorated = snapshot.candidates.map { decoration(of: $0, in: harness.view, offsets: offsets) }
        XCTAssertEqual(decorated, try fresh(snapshot, selected: mentions[0].id, scoped: scoped, view: harness.view))
        XCTAssertNotEqual(decorated, try fresh(snapshot, selected: nil, scoped: [], view: harness.view))

        // On to the table's mention with This one, then off the bar.
        show(harness, textView(snapshot, selected: mentions[1].id))
        XCTAssertEqual(
            snapshot.candidates.map { decoration(of: $0, in: harness.view, offsets: offsets) },
            try fresh(snapshot, selected: mentions[1].id, scoped: [], view: harness.view))
        show(harness, textView(snapshot))
        XCTAssertEqual(
            snapshot.candidates.map { decoration(of: $0, in: harness.view, offsets: offsets) },
            try fresh(snapshot, selected: nil, scoped: [], view: harness.view))
        XCTAssertEqual(harness.coordinator.rebuilds, built)

        // A decision is a content change: built again.
        let kept = try harness.session.decide(mentions[0].id, .keep, scope: .this)
        show(harness, textView(kept, selected: kept.candidates.first?.id))
        XCTAssertEqual(harness.coordinator.rebuilds, built + 1)
    }

    /// The bar on the last mention, kept; the reader scrolls to the top;
    /// Undo puts the bar back on the same name and asks for a reveal. The
    /// snapshot changed and the selection didn't: the text still scrolls.
    func testARevealAfterAnUndoScrolls() throws {
        let harness = try harness()
        let view = harness.view
        let last = try XCTUnwrap(harness.session.snapshot().candidates.max { $0.range.start < $1.range.start })
        let kept = try harness.session.decide(last.id, .keep, scope: .this)
        let keptLast = try XCTUnwrap(kept.candidates.first { $0.range == last.range })
        show(harness, textView(kept, selected: keptLast.id))
        view.setContentOffset(.zero, animated: false)
        view.layoutIfNeeded()
        XCTAssertEqual(view.contentOffset.y, -view.adjustedContentInset.top, accuracy: 1)

        let undone = try harness.session.undo()
        let back = try XCTUnwrap(undone.candidates.first { $0.range == last.range })
        XCTAssertEqual(back.id, keptLast.id, "the same name, as the bar sees it")
        show(harness, textView(undone, selected: back.id, reveal: RevealRequest(id: 1, target: .candidate(back.id))))
        let deadline = Date().addingTimeInterval(3)
        while view.contentOffset.y < view.bounds.height, Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertGreaterThan(view.contentOffset.y, view.bounds.height, "scrolled down to the last mention")

        // The same request again runs no second time.
        view.setContentOffset(.zero, animated: false)
        show(harness, textView(undone, selected: back.id, reveal: RevealRequest(id: 1, target: .candidate(back.id))))
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        XCTAssertEqual(view.contentOffset.y, -view.adjustedContentInset.top, accuracy: 1)
    }
}
