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
        switch await RenderedDocx.render(source) {
        case .ready(let rendered): return rendered
        case .failed(let message): throw RenderFailed(message: message)
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
        view.load(url: rendered.pdf, comments: comments)
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

    /// The rendered PDF goes with the object, as the pane discards it.
    func testTheRenderedPDFGoesWithTheOriginal() async throws {
        var rendered: RenderedDocx? = try await render("coverage.docx")
        let folder = try XCTUnwrap(rendered?.pdf.deletingLastPathComponent())
        XCTAssertNotNil(rendered.flatMap { PDFDocument(url: $0.pdf) })
        rendered = nil
        // The core removes it on a cleanup thread.
        for _ in 0..<500 where FileManager.default.fileExists(atPath: folder.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
    }
}
