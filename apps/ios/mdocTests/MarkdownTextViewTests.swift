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

    private func harness(text: String = text, name: String = "Maya Bennett") throws -> Harness {
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Agreement", into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([SampleDocument.Name(wording: name, category: .person)])
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

    /// The name under review keeps its category's fill and gets an ink
    /// outline, black in light mode and white in dark mode, in the text and
    /// in a table cell; the mentions its scope reaches get none (ADR 0099).
    func testTheNameUnderReviewIsOutlinedInInk() throws {
        let harness = try harness()
        let snapshot = harness.session.snapshot()
        let mentions = snapshot.candidates.sorted { $0.range.start < $1.range.start }
        let offsets = TextOffsets(snapshot.markdown)
        let outline = MarkdownStyler.decorationKeys.firstIndex(of: .mdocOutline)!
        let fill = MarkdownStyler.decorationKeys.firstIndex(of: .backgroundColor)!
        func ink(_ style: UIUserInterfaceStyle) -> String {
            var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
            UIColor.ink.resolvedColor(with: UITraitCollection(userInterfaceStyle: style)).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
            return String(format: "%.3f %.3f %.3f %.3f", red, green, blue, alpha)
        }
        show(harness, textView(snapshot))
        let plain = mentions.map { decoration(of: $0, in: harness.view, offsets: offsets) }
        XCTAssertEqual(plain.map { $0[outline] }, ["none", "none", "none"])

        for (index, selected) in mentions.prefix(2).enumerated() {
            let others = Set(mentions.map(\.id)).subtracting([selected.id])
            show(harness, textView(snapshot, selected: selected.id, scoped: others))
            let decorated = mentions.map { decoration(of: $0, in: harness.view, offsets: offsets) }
            XCTAssertEqual(decorated[index][outline], ink(.light), "mention \(index) under review")
            XCTAssertNotEqual(decorated[index][fill], "none", "it keeps its category's fill")
            for other in decorated.indices where other != index {
                XCTAssertEqual(decorated[other][outline], "none", "mention \(other) in the scope")
            }
        }

        let dark = MarkdownStyler.decoration(of: mentions[0], selected: true, scoped: false, dark: true)
        var white: CGFloat = 0, alpha: CGFloat = 0
        (dark[.mdocOutline] as? UIColor)?.getWhite(&white, alpha: &alpha)
        XCTAssertGreaterThan(white, 0.9, "white in dark mode")
        XCTAssertEqual(ink(.dark), "0.949 0.949 0.969 1.000")
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
    /// VoiceOver's value is built once per content (#712): asking again, or
    /// recolouring a name, reuses it; new content builds it again.
    func testTheSpokenValueIsBuiltOncePerContent() throws {
        let harness = try harness()
        let snapshot = harness.session.snapshot()
        show(harness, textView(snapshot))
        let first = harness.view.accessibilityValue
        for _ in 0..<5 { _ = harness.view.accessibilityValue }
        XCTAssertEqual(harness.view.spokenBuilds, 1)

        let mention = try XCTUnwrap(snapshot.candidates.first)
        show(harness, textView(snapshot, selected: mention.id))
        XCTAssertEqual(harness.view.accessibilityValue, first)
        XCTAssertEqual(harness.view.spokenBuilds, 1, "a recolour kept the spoken value")

        let replaced = try harness.session.decide(mention.id, .replace, scope: .all)
        show(harness, textView(replaced))
        XCTAssertNotEqual(harness.view.accessibilityValue, first, "new content is read anew")
        XCTAssertEqual(harness.view.spokenBuilds, 2)
    }

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

    /// A request made before the view existed (the text pane came back, or
    /// the document opened again) is not run: the new view starts where it
    /// should, not at an old name (review of #699).
    func testANewViewIgnoresAnEarlierReveal() throws {
        let (core, _) = try Fixture.core()
        let session = try core.library.importDocument(.text(Self.text), title: "Agreement", into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        let snapshot = try session.propose([SampleDocument.Name(wording: "Maya Bennett", category: .person)])
        let last = try XCTUnwrap(snapshot.candidates.max { $0.range.start < $1.range.start })
        let old = RevealRequest(id: 7, target: .candidate(last.id))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.isEditable = false
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let shown = textView(snapshot, reveal: old)
        let coordinator = MarkdownTextView.Coordinator(parent: shown)
        coordinator.attach(to: view)
        coordinator.update(view, with: shown, dynamicTypeSize: .large, colorScheme: .light)
        view.layoutIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        XCTAssertEqual(view.contentOffset.y, -view.adjustedContentInset.top, accuracy: 1, "still at the top")
        XCTAssertNil(coordinator.pendingReveal)
    }

    /// Replacing can leave text that fits the screen. The scroll kept from
    /// before must not outlive it: a text view whose content fits doesn't
    /// pan, so the top of the document was out of reach for good.
    func testAScrolledDocumentThatNowFitsGoesBackToItsTop() throws {
        let name = "Alexandria Montgomery-Featherstonehaugh"
        let paragraph = Array(repeating: name, count: 6).joined(separator: " and ")
        let text = Array(repeating: paragraph, count: 12).joined(separator: "\n\n") + "\n"
        let harness = try harness(text: text, name: name)
        show(harness, textView(harness.session.snapshot()))
        harness.view.setContentOffset(CGPoint(x: 0, y: 500), animated: false)
        harness.view.layoutIfNeeded()
        XCTAssertEqual(harness.view.contentOffset.y, 500, "the document is long enough to scroll")

        show(harness, textView(try harness.session.replaceAllPending()))

        let insets = harness.view.adjustedContentInset
        XCTAssertLessThan(
            harness.view.contentSize.height + insets.top + insets.bottom, harness.view.bounds.height,
            "the replaced text fits")
        XCTAssertEqual(harness.view.contentOffset.y, -insets.top, "back at the top")
    }

    /// A drag always takes, so a document that fits never feels frozen.
    func testADocumentThatFitsStillTakesADrag() {
        XCTAssertTrue(CandidateTextView().alwaysBounceVertical)
    }

    /// A long document keeps its place across a Replace.
    func testALongDocumentKeepsItsScrollAcrossAReplace() throws {
        let harness = try harness()
        show(harness, textView(harness.session.snapshot()))
        harness.view.setContentOffset(CGPoint(x: 0, y: 800), animated: false)
        harness.view.layoutIfNeeded()

        show(harness, textView(try harness.session.replaceAllPending()))

        XCTAssertEqual(harness.view.contentOffset.y, 800)
    }
}
