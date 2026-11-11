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
        harness.window.overrideUserInterfaceStyle = .light
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

    /// The boxes the layout fragment draws for the outline at `location` of
    /// a text view, in its text container's coordinates.
    private func outlineBoxes(in textView: UITextView, at location: Int) -> [CGRect] {
        guard let layout = textView.textLayoutManager, let content = layout.textContentManager,
              let at = content.location(content.documentRange.location, offsetBy: location) else { return [] }
        layout.ensureLayout(for: content.documentRange)
        guard let fragment = layout.textLayoutFragment(for: at) as? MarkdownLayoutFragment else { return [] }
        return fragment.outlineBoxes(at: fragment.layoutFragmentFrame.origin).map(\.box)
    }

    /// The outline spans the name: a Hebrew name in an English line, whose
    /// two ends fall at one x where the directions meet (the box collapsed
    /// when it ran from end to end), and a name that ends its table cell.
    func testTheOutlineSpansTheName() throws {
        let width: (String) -> CGFloat = { name in
            (name as NSString).size(withAttributes: [.font: MarkdownStyler.font(.body, traits: UITraitCollection(userInterfaceStyle: .light))]).width
        }
        let hebrew = try harness(text: "Hello שלום world, and more words after it.\n", name: "שלום")
        let snapshot = hebrew.session.snapshot()
        let name = try XCTUnwrap(snapshot.candidates.first)
        show(hebrew, textView(snapshot, selected: name.id))
        let boxes = outlineBoxes(in: hebrew.view, at: TextOffsets(snapshot.markdown).nsRange(name.range).location)
        XCTAssertEqual(boxes.count, 1)
        XCTAssertGreaterThanOrEqual(boxes.first?.width ?? 0, width("שלום") * 0.9, "\(boxes)")

        let table = try harness()
        let tableSnapshot = table.session.snapshot()
        let mentions = tableSnapshot.candidates.sorted { $0.range.start < $1.range.start }
        show(table, textView(tableSnapshot, selected: mentions[1].id))
        let range = TextOffsets(tableSnapshot.markdown).nsRange(mentions[1].range)
        let cell = try XCTUnwrap(table.view.tableViews.flatMap { $0.cells.joined() }.first { cell in
            NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
        })
        let cellBoxes = outlineBoxes(in: cell, at: range.location - cell.location)
        XCTAssertEqual(cellBoxes.count, 1)
        XCTAssertGreaterThanOrEqual(cellBoxes.first?.width ?? 0, width("Maya Bennett") * 0.9, "\(cellBoxes)")
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

    /// A long document of uneven blocks (headings, long and short
    /// paragraphs, lists, tall tables) with Maya Bennett seventeen times,
    /// two of them in table cells: TextKit 2 estimates the blocks it has not
    /// laid out, and these estimates are far off.
    private static let longText: String = {
        var blocks: [String] = []
        let long = Array(repeating: "The parties agree that the services are provided with reasonable care and skill, in line with good industry practice, and on the terms below.", count: 6).joined(separator: " ")
        for section in 1...16 {
            blocks.append("## \(section). Section \(section)")
            blocks.append(section % 3 == 0 ? long : "Short line \(section).")
            if section % 4 == 1 {
                let rows = (1...14).map { "| Row \($0) of table \(section) | \(section == 9 && $0 == 7 ? "Maya Bennett" : "Somebody else") |" }
                blocks.append((["| Name | Role |", "| --- | --- |"] + rows).joined(separator: "\n"))
            }
            blocks.append((1...(section % 5 + 1)).map { "- Item \($0) of the list, a line long." }.joined(separator: "\n"))
            if section != 9 { blocks.append("Here Maya Bennett is mentioned in section \(section).") }
            blocks.append(long)
        }
        blocks.append("| Signed | By |\n| --- | --- |\n| Today | Maya Bennett |")
        return blocks.joined(separator: "\n\n") + "\n"
    }()

    /// Runs the run loop until the text stops scrolling.
    private func settle(_ view: UIScrollView) {
        let deadline = Date().addingTimeInterval(4)
        var last = view.contentOffset
        var still = Date()
        while Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            if view.contentOffset != last {
                last = view.contentOffset
                still = Date()
            } else if Date().timeIntervalSince(still) > 0.4 {
                return
            }
        }
    }

    /// Stepping to a name far down a long document lands with the name in
    /// view above the docked bar (ADR 0098, amended 2026-11-11), in the
    /// text and in a table cell: the text is laid out up to the name
    /// before it is measured, and measured again once the scroll lands.
    /// Before, the scroll trusted TextKit 2's estimate and the name stayed
    /// off screen ("14 of 16").
    func testSteppingFarDownKeepsTheNameInView() throws {
        let probe = try harness(text: Self.longText)
        let mentions = probe.session.snapshot().candidates.sorted { $0.range.start < $1.range.start }
        XCTAssertEqual(mentions.count, 17)
        for index in [13, 8, 16, 5] {
            let harness = try harness(text: Self.longText)
            let view = harness.view
            // The docked bar, as the screen's safe area gives it.
            view.contentInset.bottom = 190
            let snapshot = harness.session.snapshot()
            let offsets = TextOffsets(snapshot.markdown)
            let sorted = snapshot.candidates.sorted { $0.range.start < $1.range.start }
            show(harness, textView(snapshot, selected: sorted[0].id))
            settle(view)
            show(harness, textView(snapshot, selected: sorted[index].id))
            settle(view)
            // Where the name is drawn, after the pass the scroll's end brings.
            view.setNeedsLayout()
            view.layoutIfNeeded()
            let rect = try XCTUnwrap(harness.coordinator.place(of: offsets.nsRange(sorted[index].range), in: view, onlyShown: true)?.rect)
            let insets = view.adjustedContentInset
            let top = view.contentOffset.y + insets.top
            // Above the bar; room added below the end is no bar.
            let visible = view.bounds.height - insets.top - 190
            XCTAssertGreaterThanOrEqual(rect.minY, top - 1, "mention \(index + 1) of 17 below the top: \(rect), top \(top)")
            XCTAssertLessThanOrEqual(rect.maxY, top + visible + 1, "mention \(index + 1) of 17 above the bar: \(rect), top \(top)")
        }
    }

    /// A drag always takes, so a document that fits never feels frozen.
    func testADocumentThatFitsStillTakesADrag() {
        XCTAssertTrue(CandidateTextView().alwaysBounceVertical)
    }

    /// A long document keeps its place across a Replace: the line at the
    /// top of the view stays there, though the name replaced above it
    /// changed how the text before it wraps.
    func testALongDocumentKeepsItsScrollAcrossAReplace() throws {
        let harness = try harness()
        let view = harness.view
        show(harness, textView(harness.session.snapshot()))
        view.setContentOffset(CGPoint(x: 0, y: 800), animated: false)
        view.layoutIfNeeded()
        // A paragraph in view, and how far below the view's top it is drawn.
        let paragraph = "Paragraph 16 of the agreement"
        func place() throws -> CGFloat {
            let range = (view.textStorage.string as NSString).range(of: paragraph)
            XCTAssertNotEqual(range.location, NSNotFound)
            let rect = try XCTUnwrap(view.laidOutRect(of: range, onlyShown: true), "in view")
            return rect.minY - view.contentOffset.y
        }
        let before = try place()
        XCTAssertGreaterThan(before, 0)
        XCTAssertLessThan(before, view.bounds.height)

        show(harness, textView(try harness.session.replaceAllPending()))

        XCTAssertEqual(try place(), before, accuracy: 1)
        XCTAssertEqual(view.contentOffset.y, 800, accuracy: 60, "about where it was")
    }
}
