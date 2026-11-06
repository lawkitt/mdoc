import XCTest
@testable import mdoc

/// The review bar's full view rows (docs/mobile-ui.md): the name whole, in
/// its original wording, with the words of its own paragraph and its
/// heading, and no Markdown markers.
final class MentionSnippetTests: XCTestCase {
    private func snippet(_ markdown: String, _ target: String, name: String? = nil, radius: Int = MentionSnippet.radius) -> MentionSnippet {
        let range = markdown.range(of: target)!
        let start = markdown.utf8.distance(from: markdown.startIndex, to: range.lowerBound)
        let end = markdown.utf8.distance(from: markdown.startIndex, to: range.upperBound)
        return MentionSnippet(markdown: markdown, range: SourceRange(start: start, end: end), name: name ?? target, radius: radius)
    }

    func testTheOriginalNameStandsInForItsAlias() {
        let s = snippet("**And:** PERSON_1, trading as Bennett Design.", "PERSON_1", name: "Maya Bennett")
        XCTAssertEqual(s.name, "Maya Bennett")
        XCTAssertEqual(s.before, "And: ")
        XCTAssertEqual(s.after, ", trading as Bennett Design.")
    }

    func testTheHeadingComesFromAboveAndStaysOutOfTheWords() {
        let markdown = "# Services agreement\n\n## 1. The work\n\nThe Designer will redesign the offices. Ms Bennett will report."
        let s = snippet(markdown, "Ms Bennett")
        XCTAssertEqual(s.heading, "1. The work")
        XCTAssertEqual(s.before, "The Designer will redesign the offices. ")
        XCTAssertFalse(s.before.contains("work"))
    }

    func testTheWordsStayInTheNamesParagraph() {
        let markdown = "First paragraph ends here.\n\nSecond starts with Maya Bennett and goes on.\n\nThird paragraph."
        let s = snippet(markdown, "Maya Bennett")
        XCTAssertEqual(s.before, "Second starts with ")
        XCTAssertEqual(s.after, " and goes on.")
    }

    func testLongContextIsCutAtWordsWithAnEllipsis() {
        let words = Array(repeating: "alpha beta gamma", count: 10).joined(separator: " ")
        let s = snippet("\(words) Daniel Okafor \(words)", "Daniel Okafor", radius: 20)
        XCTAssertTrue(s.before.hasPrefix("…"))
        XCTAssertTrue(s.after.hasSuffix("…"))
        XCTAssertLessThanOrEqual(s.before.count, 21)
        XCTAssertFalse(s.before.dropFirst().hasPrefix(" "))
        let first = s.before.dropFirst().split(separator: " ").first.map(String.init)
        XCTAssertTrue(["alpha", "beta", "gamma"].contains(first ?? ""), "starts on a whole word: \(s.before)")
    }

    func testMarkdownMarkersGoButALoneUnderscoreStays() {
        let s = snippet("| **Item** | Due |\n|---|---|\n| Paid to ORG_2 by `[Riverside](https://x.example)` | Maya Bennett |", "Maya Bennett")
        XCTAssertFalse(s.before.contains("**"))
        XCTAssertFalse(s.before.contains("`"))
        XCTAssertFalse(s.before.contains("https"))
        XCTAssertTrue(s.before.contains("ORG_2"))
        XCTAssertTrue(s.before.contains("Riverside"))
    }

    func testMultibyteTextKeepsItsOffsets() {
        let s = snippet("Заявление: Иван Петров подал документы.", "Иван Петров")
        XCTAssertEqual(s.before, "Заявление: ")
        XCTAssertEqual(s.after, " подал документы.")
    }

    func testWindowsLineEndsAreLines() {
        let s = snippet("# Parties\r\n\r\nFirst paragraph here.\r\n\r\nThe client Maya Bennett signs.", "Maya Bennett")
        XCTAssertEqual(s.heading, "Parties")
        XCTAssertEqual(s.before, "The client ")
    }

    func testALinkCutByTheWindowKeepsItsText() {
        let long = String(repeating: "word ", count: 80)
        let s = snippet("\(long)[Riverside Holdings](https://example.com/a/long/path) and Maya Bennett", "Maya Bennett", radius: 30)
        XCTAssertFalse(s.before.contains("]("), s.before)
        XCTAssertFalse(s.before.contains("https"), s.before)
    }

    func testTheHeadingIsFoundFarAbove() {
        let body = String(repeating: "Line of text.\n", count: 400)
        let s = snippet("## 4. Terms\n\n\(body)\nSigned by Maya Bennett.", "Maya Bennett")
        XCTAssertEqual(s.heading, "4. Terms")
    }
}
