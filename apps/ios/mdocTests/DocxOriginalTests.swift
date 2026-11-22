import MdocFFI
import PDFKit
import XCTest
@testable import mdoc

/// The DOCX original on the real core: docxide renders in process, PDFKit
/// reads the comment marks back, and the app places the comments.
@MainActor
final class DocxOriginalTests: XCTestCase {
    private func render(_ fixture: String) async throws -> RenderedDocx {
        let source = Fixture.repository.appendingPathComponent("tests/fixtures/docx-preview/\(fixture)")
        switch await RenderedDocx.render(source).rendering {
        case .ready(let rendered): return rendered
        case .failed(let message): throw RenderFailed(message: message)
        case .password: throw RenderFailed(message: "password required")
        case .working: throw RenderFailed(message: "still rendering")
        }
    }

    private struct RenderFailed: Error {
        let message: String
    }

    /// Every comment shape the core anchors: placed comments get a marker on
    /// their first line, the one with no place is "Not on the page", and the
    /// shown page keeps no `mdoc-comment:` link.
    func testCommentAnchorsBecomeMarkersOnTheirLines() async throws {
        let rendered = try await render("comment-anchors.docx")
        let comments = rendered.comments
        XCTAssertEqual(comments.all.count, 13)
        XCTAssertEqual(comments.unplaced.map(\.comment.text), ["Nowhere."])
        let placed = comments.markers.flatMap(\.threads).map(\.comment.text)
        XCTAssertEqual(Set(placed).count, 12)
        XCTAssertTrue(placed.contains("Across pages."))
        // Overlapping ranges that start on one line share its marker.
        let shared = try XCTUnwrap(comments.marker(of: "2"))
        XCTAssertEqual(shared.threads.map(\.id), ["2", "3"])
        // The comment across a page break starts on the first page.
        XCTAssertEqual(comments.marker(of: "12")?.page, 0)
        XCTAssertEqual(comments.passages["12"].map { Set($0.map(\.page)) }, [0, 1])

        let view = CommentedPDFContainer(frame: CGRect(x: 0, y: 0, width: 390, height: 700))
        view.load(rendered)
        let document = try XCTUnwrap(view.pdfView.document)
        let marks = (0..<document.pageCount).flatMap { document.page(at: $0)?.annotations ?? [] }
            .compactMap(CommentMarks.target(of:))
        XCTAssertEqual(marks, [])
    }

    /// The marker's label reads each comment whole, for VoiceOver.
    func testAMarkerReadsItsComments() {
        let comments = DocxComments(
            comments: [
                comment("0", "Several lines.", quote: "This clause"),
                comment("1", "Agreed.", parent: "0"),
            ],
            anchors: [anchor("0", y: 0.2), anchor("1", y: 0.2)])
        let label = comments.markers[0].spokenLabel
        XCTAssertTrue(label.hasPrefix("Comment by Reviewer, "), label)
        XCTAssertTrue(label.hasSuffix(", on ‘This clause’: Several lines. 1 reply."), label)
    }

    /// Replies nest under their comment, which alone gets a marker; a
    /// resolved thread keeps its marker but no tint.
    func testRepliesNestAndResolvedThreadsHaveNoTint() {
        let comments = DocxComments(
            comments: [
                comment("0", "Open."),
                comment("1", "A reply.", parent: "0"),
                comment("2", "Done.", resolved: true),
                comment("3", "Reply to nothing.", parent: "9"),
            ],
            anchors: [anchor("0", y: 0.1), anchor("1", y: 0.1), anchor("2", y: 0.5)])
        XCTAssertEqual(comments.markers.map { $0.threads.map(\.id) }, [["0"], ["2"]])
        XCTAssertEqual(comments.markers[0].threads[0].replies.map(\.id), ["1"])
        XCTAssertTrue(comments.markers[1].resolved)
        XCTAssertEqual(Set(comments.passages.keys), ["0"])
        XCTAssertEqual(comments.unplaced.map(\.id), ["3"])
    }

    /// Markers whose 44 pt touch targets would overlap share the first one,
    /// on one page only (#962): 14 pt and 33 pt apart join, 50 pt apart
    /// stay apart, and a new page starts a new marker.
    func testMarkersThatWouldOverlapShareOne() {
        func at(_ y: CGFloat, page: Int = 0) -> (page: Int, frame: CGRect) {
            (page, CGRect(x: 300, y: y, width: 30, height: 30))
        }
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(14), at(28)]), [[0, 1, 2]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(33)]), [[0, 1]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(50)]), [[0], [1]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(10, page: 1)]), [[0], [1]])
        // A group is measured from its first marker, so a chain can't creep.
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(30), at(60)]), [[0, 1], [2]])
        // A first marker scrolled out of view never hides one in it; two
        // out of view, or two in it, still share.
        let view = CGRect(x: 0, y: 20, width: 390, height: 700)
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(-14), at(10)], visible: view), [[0], [1]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(-60), at(-40)], visible: view), [[0, 1]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(0), at(14)], visible: view), [[0, 1]])
        XCTAssertEqual(CommentedPDFContainer.markerGroups([at(700), at(714)], visible: view), [[0, 1]])
    }

    /// A shared marker of resolved threads still shows how many (#962).
    func testASharedResolvedMarkerShowsItsCount() {
        let comments = DocxComments(
            comments: [comment("0", "Done.", resolved: true), comment("1", "Also done.", resolved: true)],
            anchors: [anchor("0", y: 0.2), anchor("1", y: 0.6)])
        let button = CommentMarkerButton(marker: comments.markers[0])
        XCTAssertNil(button.configuration?.title)
        let merged = DocxComments.Marker(
            id: "0", page: 0, line: comments.markers[0].line, threads: comments.markers.flatMap(\.threads))
        XCTAssertTrue(button.show(merged))
        XCTAssertEqual(button.configuration?.title, "2")
        XCTAssertTrue(button.accessibilityLabel?.hasPrefix("2 comments.") == true, button.accessibilityLabel ?? "")
    }

    /// On a phone-width page the adjacent commented lines of comments.docx
    /// share one marker that opens their threads in page order, and the
    /// Comments rotor walks only the markers that show, as they are when it
    /// searches, after a zoom too (#962).
    func testAdjacentLinesShareAMarkerAndTheRotorFollowsZoom() async throws {
        let rendered = try await render("comments.docx")
        let view = CommentedPDFContainer(frame: CGRect(x: 0, y: 0, width: 390, height: 700))
        view.load(rendered)
        var shown: [[String]] = []
        for _ in 0..<200 {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            shown = view.shownMarkerThreads
            if !shown.isEmpty, view.pdfView.scaleFactor > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let ordered = rendered.comments.markers.flatMap { $0.threads.map(\.id) }
        XCTAssertEqual(shown.flatMap { $0 }, ordered, "every placed thread, once, in page order")
        XCTAssertLessThan(shown.count, rendered.comments.markers.count, "close lines share a marker")

        let rotor = try XCTUnwrap(view.accessibilityCustomRotors?.first)
        func walk() -> Int {
            var count = 0
            var current: Any? = nil
            while true {
                let predicate = UIAccessibilityCustomRotorSearchPredicate()
                predicate.searchDirection = .next
                predicate.currentItem = UIAccessibilityCustomRotorItemResult(
                    targetElement: (current as? NSObjectProtocol) ?? NSObject(), targetRange: nil)
                guard let next = rotor.itemSearchBlock(predicate) else { return count }
                XCTAssertFalse((next.targetElement as? UIView)?.isHidden ?? true, "a hidden marker")
                count += 1
                current = next.targetElement
                if count > 50 { return count }
            }
        }
        XCTAssertEqual(walk(), shown.count)
        // Zoomed in, the lines spread apart and the same rotor finds more.
        view.pdfView.scaleFactor *= 4
        view.setNeedsLayout()
        view.layoutIfNeeded()
        XCTAssertGreaterThan(view.shownMarkerThreads.count, shown.count)
        XCTAssertEqual(walk(), view.shownMarkerThreads.count)

        // Zoomed out again, a marker the rotor returned may have merged into
        // the one above it: the rotor goes on from that group, both ways.
        var spread: [CommentMarkerButton] = []
        var current: Any? = nil
        while spread.count < 50 {
            let predicate = UIAccessibilityCustomRotorSearchPredicate()
            predicate.searchDirection = .next
            predicate.currentItem = UIAccessibilityCustomRotorItemResult(
                targetElement: (current as? NSObjectProtocol) ?? NSObject(), targetRange: nil)
            guard let next = rotor.itemSearchBlock(predicate)?.targetElement as? CommentMarkerButton else { break }
            spread.append(next)
            current = next
        }
        view.pdfView.scaleFactor /= 4
        view.setNeedsLayout()
        view.layoutIfNeeded()
        let groups = view.shownMarkerThreads
        let merged = try XCTUnwrap(spread.first { $0.isHidden }, "a marker merges when zoomed out")
        let group = try XCTUnwrap(groups.firstIndex { $0.contains(merged.marker.threads[0].id) })
        for (direction, expected) in [(UIAccessibilityCustomRotor.Direction.next, group + 1), (.previous, group - 1)] {
            let predicate = UIAccessibilityCustomRotorSearchPredicate()
            predicate.searchDirection = direction
            predicate.currentItem = UIAccessibilityCustomRotorItemResult(targetElement: merged, targetRange: nil)
            let found = (rotor.itemSearchBlock(predicate)?.targetElement as? CommentMarkerButton)?.shown.threads.map(\.id)
            XCTAssertEqual(found, groups.indices.contains(expected) ? groups[expected] : nil, "\(direction)")
        }
    }

    /// A tap where passages overlap opens the first of them in reading
    /// order, every time: not whichever the passages' dictionary yields.
    func testATapOnOverlappingPassagesOpensTheFirstInReadingOrder() {
        // Twelve passages over one stretch of a line, each starting a
        // little further right; their ids run backwards to that order.
        let ids = (0..<12).map { String(11 - $0) }
        let comments = DocxComments(
            comments: ids.map { comment($0, "Note \($0).") },
            anchors: ids.enumerated().map { index, id in
                CommentAnchor(
                    id: id, page: 0, rects: [PageRect(x: 0.1 + 0.01 * Float(index), y: 0.2, w: 0.5, h: 0.02)],
                    point: false)
            })
        XCTAssertEqual(comments.ordered.first?.id, "11")
        // A page 1000 pt square.
        func rect(_ place: PageRect) -> CGRect {
            CGRect(x: CGFloat(place.x), y: CGFloat(place.y), width: CGFloat(place.w), height: CGFloat(place.h))
                .applying(CGAffineTransform(scaleX: 1000, y: 1000))
        }
        func hit(_ x: CGFloat, page: Int = 0) -> String? {
            CommentedPDFContainer.thread(at: CGPoint(x: x, y: 210), page: page, in: comments, rect: rect)
        }
        for _ in 0..<5 {
            XCTAssertEqual(hit(300), "11", "all twelve overlap here")
        }
        XCTAssertEqual(hit(650), "6", "the first one that reaches this far")
        XCTAssertNil(hit(300, page: 1))
        XCTAssertNil(hit(50))
    }

    /// The rail's cards keep a height for each expansion: an expanded card
    /// gets its taller height, and its old one back when it folds; a larger
    /// text size measures and places them again at once.
    func testRailCardsKeepTheirHeightForTheirExpansion() async throws {
        let rendered = try await render("comments.docx")
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 1000, height: 700))
        let view = CommentedPDFContainer(frame: window.bounds)
        window.addSubview(view)
        window.isHidden = false
        view.load(rendered)
        view.update(showsComments: true, rail: true, active: nil)
        func cards() -> [CommentRailCard] {
            (view.accessibilityElements ?? []).compactMap { $0 as? CommentRailCard }
        }
        for _ in 0..<200 {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            if cards().contains(where: { $0.frame.height > 0 }), view.pdfView.scaleFactor > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let all = cards()
        XCTAssertFalse(all.isEmpty)
        let width = try XCTUnwrap(all.first?.frame.width)
        let folded = Dictionary(uniqueKeysWithValues: all.map { ($0.commentID, $0.frame.height) })
        // The card that grows most when it opens: its replies or more text.
        func openHeight(_ card: CommentRailCard) -> CGFloat {
            guard let thread = rendered.comments.ordered.first(where: { $0.id == card.commentID }) else { return 0 }
            let probe = CommentRailCard(thread: thread)
            probe.expanded = true
            return probe.systemLayoutSizeFitting(
                CGSize(width: width, height: UIView.layoutFittingCompressedSize.height),
                withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel).height
        }
        let grown = all.map { ($0, openHeight($0)) }
            .max { $0.1 - (folded[$0.0.commentID] ?? 0) < $1.1 - (folded[$1.0.commentID] ?? 0) }
        let (card, open) = try XCTUnwrap(grown)
        let closed = try XCTUnwrap(folded[card.commentID])
        XCTAssertGreaterThan(open, closed + 1, "a card that grows when it opens")
        view.update(showsComments: true, rail: true, active: card.commentID)
        XCTAssertEqual(card.frame.height, open, accuracy: 0.5, "expanded, its own height")
        view.update(showsComments: true, rail: true, active: nil)
        XCTAssertEqual(card.frame.height, closed, accuracy: 0.5, "folded again")
        // No scroll or zoom follows a new text size: the trait change
        // alone lays the rail out again.
        view.traitOverrides.preferredContentSizeCategory = .accessibilityExtraLarge
        view.layoutIfNeeded()
        XCTAssertGreaterThan(card.frame.height, closed + 10, "measured again at the larger size")
    }

    private func comment(
        _ id: String, _ text: String, quote: String = "", parent: String? = nil, resolved: Bool = false
    ) -> OriginalComment {
        OriginalComment(
            id: id, author: "Reviewer", date: "2026-11-18T10:00:00Z", text: text, quote: quote, parent: parent,
            resolved: resolved)
    }

    private func anchor(_ id: String, y: Float) -> CommentAnchor {
        CommentAnchor(id: id, page: 0, rects: [PageRect(x: 0.1, y: y, w: 0.5, h: 0.02)], point: false)
    }

    func testTheOriginalOpensFromPDFBytes() async throws {
        let rendered = try await render("coverage.docx")
        let document = try XCTUnwrap(PDFDocument(data: rendered.pdf))
        XCTAssertNil(document.documentURL)
        XCTAssertNotNil(document.page(at: 0))
    }
}
