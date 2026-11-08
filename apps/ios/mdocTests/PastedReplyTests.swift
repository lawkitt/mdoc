import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import mdoc

/// A reply copied from the ChatGPT app holds no Markdown, only HTML and a
/// flattened plain text (ADR 0105).
@MainActor
final class PastedReplyTests: XCTestCase {
    private let html = "<h3><span><b>Findings</b></span></h3><table><thead><tr><th><p><b>Issue</b></p></th></tr></thead><tbody><tr><td><p>ORG_3 pays</p></td></tr></tbody></table>"
    private let flat = "Findings\n\nIssue\nORG_3 pays\n"
    private let markdown = "### Findings\n\n| Issue |\n| --- |\n| ORG_3 pays |"

    func testTheHTMLGivesTheMarkdownThePlainTextLost() {
        XCTAssertEqual(PastedReply.text(html: html, plain: flat), markdown)
    }

    func testPlainTextAloneOrAlreadyMarkdownStays() {
        XCTAssertEqual(PastedReply.text(html: nil, plain: "Just text"), "Just text")
        XCTAssertEqual(PastedReply.text(html: html, plain: "# Mine\n\nText"), "# Mine\n\nText")
        XCTAssertNil(PastedReply.text(html: nil, plain: nil))
    }

    func testHTMLBytesDecode() {
        XCTAssertEqual(PastedReply.decode(html: Data("<p>é</p>".utf8)), "<p>é</p>")
        XCTAssertEqual(PastedReply.decode(html: Data([0x3C, 0xE9, 0x3E])), "<é>")
        XCTAssertNil(PastedReply.decode(html: nil))
    }

    func testAProviderGivesItsHTML() async throws {
        let provider = NSItemProvider()
        provider.registerDataRepresentation(forTypeIdentifier: UTType.html.identifier, visibility: .all) { done in
            done(Data(self.html.utf8), nil)
            return nil
        }
        provider.registerObject(flat as NSString, visibility: .all)
        let text = await PastedReply.text(from: [provider])
        XCTAssertEqual(text, markdown)
    }

    func testAProviderOfPlainTextGivesIt() async {
        let provider = NSItemProvider(object: "Just text" as NSString)
        let text = await PastedReply.text(from: [provider])
        XCTAssertEqual(text, "Just text")
    }

    /// The box's own Paste, as the edit menu or ⌘V calls it.
    func testTheBoxsPasteReadsTheHTML() throws {
        let pasteboard = UIPasteboard.general
        let saved = pasteboard.items
        defer { pasteboard.items = saved }
        pasteboard.items = [[UTType.html.identifier: html, UTType.utf8PlainText.identifier: flat]]
        let view = ReplyTextView()
        let delegate = Recorder()
        view.delegate = delegate
        view.selectedTextRange = view.textRange(from: view.beginningOfDocument, to: view.beginningOfDocument)
        view.paste(nil)
        XCTAssertEqual(view.text, markdown)
        XCTAssertEqual(delegate.seen, markdown, "the sheet's text follows")
    }

    private final class Recorder: NSObject, UITextViewDelegate {
        var seen: String?
        func textViewDidChange(_ view: UITextView) { seen = view.text }
    }
}
