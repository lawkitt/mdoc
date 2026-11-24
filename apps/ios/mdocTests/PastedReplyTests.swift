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

    /// What Restore with mdoc is shared: an attachment's text, as Markdown
    /// from its HTML, before the item's own text; else the item's own text;
    /// else the next item's.
    func testTheRestoreExtensionTakesTheFirstSharedText() async {
        func item(_ providers: [NSItemProvider], text: String? = nil) -> NSExtensionItem {
            let item = NSExtensionItem()
            item.attachments = providers
            item.attributedContentText = text.map { NSAttributedString(string: $0) }
            return item
        }
        let webPage = NSItemProvider()
        webPage.registerDataRepresentation(forTypeIdentifier: UTType.html.identifier, visibility: .all) { done in
            done(Data(self.html.utf8), nil)
            return nil
        }
        webPage.registerObject(flat as NSString, visibility: .all)
        let image = NSItemProvider(item: Data([0xFF, 0xD8]) as NSData, typeIdentifier: UTType.jpeg.identifier)

        let fromHTML = await PastedReply.sharedText(in: [item([image, webPage], text: "The post's caption")])
        XCTAssertEqual(fromHTML, markdown)
        let ownText = await PastedReply.sharedText(in: [item([image], text: "PERSON_1 agrees.")])
        XCTAssertEqual(ownText, "PERSON_1 agrees.")
        let next = await PastedReply.sharedText(in: [item([image], text: ""), item([NSItemProvider(object: "Second" as NSString)])])
        XCTAssertEqual(next, "Second")
        let nothing = await PastedReply.sharedText(in: [item([image]), item([], text: "")])
        XCTAssertNil(nothing)
        let empty = await PastedReply.sharedText(in: [])
        XCTAssertNil(empty)
    }
}
