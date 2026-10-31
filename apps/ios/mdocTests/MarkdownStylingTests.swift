import UIKit
import XCTest
@testable import mdoc

/// The text view's styling comes from the core's Markdown spans
/// (`crates/mdoc-ffi/src/markdown.rs`) on the shared fixture: headings,
/// emphasis, links, code, a table, Cyrillic text and emoji, so UTF-8 and
/// UTF-16 offsets differ.
final class MarkdownStylingTests: XCTestCase {
    static let fixture: String = {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/mobile-markdown/review.md")
        return (try? String(contentsOf: url, encoding: .utf8)) ?? ""
    }()

    private let styler = MarkdownStyler(traits: UITraitCollection(userInterfaceStyle: .light), selected: nil)

    /// The source text of a byte range, read as UTF-8.
    private func source(_ range: SourceRange, in text: String) -> String {
        let bytes = Array(text.utf8)[range.start..<range.end]
        return String(decoding: bytes, as: UTF8.self)
    }

    func testTheCoreFindsEveryKindInTheFixture() {
        let text = Self.fixture
        XCTAssertFalse(text.isEmpty, "the fixture is missing")
        let styling = MarkdownStyling(markdown: text)
        func spans(_ kind: MarkdownSpan.Kinds) -> [String] {
            styling.spans.filter { $0.kinds.contains(kind) }.map { source($0.range, in: text) }
        }
        XCTAssertTrue(spans(.emphasis).contains("Анна Смирнова"))
        XCTAssertTrue(spans(.link).contains("реестр"))
        XCTAssertTrue(spans(.code).contains("пункт 4.2"))
        XCTAssertTrue(spans(.strong).contains("monthly"))
        XCTAssertTrue(spans(.strikethrough).contains("old"))
        XCTAssertTrue(spans(.highlight).contains("check"))
        XCTAssertTrue(spans(.marker).contains("| --- | ---: |"))
        // A converter's `1\.`: only the backslash is a marker.
        XCTAssertTrue(spans(.marker).contains("\\"))
        XCTAssertEqual(styling.spans.compactMap(\.color), [0xC039_2BFF])
        let kinds = styling.lines.map(\.kind)
        for kind: MarkdownLine.Kind in [.heading(level: 1), .heading(level: 2), .quote, .tableHeader, .tableRow, .codeBlock, .rule] {
            XCTAssertTrue(kinds.contains(kind), "no \(kind) line")
        }
    }

    /// Every UTF-16 unit is styled as the spans and lines covering its source
    /// byte say: bold, italic, monospaced, dimmed, link colour, struck.
    func testStyledRangesMatchTheCoresSpans() {
        let text = Self.fixture
        let styling = MarkdownStyling(markdown: text)
        let styled = styler.attributedText(markdown: text, styling: styling, candidates: [])
        let offsets = TextOffsets(text)
        let units = styled.length
        XCTAssertEqual(units, (text as NSString).length)

        var expected = Array(repeating: Expected(), count: units)
        func mark(_ range: SourceRange, _ change: (inout Expected) -> Void) {
            let ns = offsets.nsRange(range)
            for unit in ns.location..<(ns.location + ns.length) {
                change(&expected[unit])
            }
        }
        for line in styling.lines {
            switch line.kind {
            case .heading, .tableHeader: mark(line.range) { $0.bold = true }
            case .codeBlock: mark(line.range) { $0.monospaced = true }
            default: break
            }
        }
        for span in styling.spans {
            mark(span.range) { unit in
                if span.kinds.contains(.strong) { unit.bold = true }
                if span.kinds.contains(.emphasis) { unit.italic = true }
                if span.kinds.contains(.code) { unit.monospaced = true }
                if span.kinds.contains(.marker) { unit.marker = true }
                if span.kinds.contains(.link) { unit.link = true }
                if span.kinds.contains(.strikethrough) { unit.struck = true }
            }
        }

        let nsText = text as NSString
        for unit in 0..<units {
            let attributes = styled.attributes(at: unit, effectiveRange: nil)
            let font = attributes[.font] as? UIFont
            let traits = font?.fontDescriptor.symbolicTraits ?? []
            let colour = attributes[.foregroundColor] as? UIColor
            let actual = Expected(
                bold: traits.contains(.traitBold),
                italic: traits.contains(.traitItalic),
                monospaced: traits.contains(.traitMonoSpace),
                marker: colour == MarkdownStyler.marker,
                link: colour == UIColor.tintColor,
                struck: attributes[.strikethroughStyle] != nil)
            XCTAssertEqual(
                actual, expected[unit],
                "unit \(unit) “\(nsText.substring(with: nsText.rangeOfComposedCharacterSequence(at: unit)))”")
        }
    }

    /// Each span's UTF-16 range holds exactly its UTF-8 source text.
    func testSpanOffsetsConvertToUTF16Exactly() {
        let text = Self.fixture
        let styling = MarkdownStyling(markdown: text)
        let offsets = TextOffsets(text)
        let nsText = text as NSString
        for span in styling.spans {
            XCTAssertEqual(nsText.substring(with: offsets.nsRange(span.range)), source(span.range, in: text))
        }
        for line in styling.lines {
            XCTAssertEqual(nsText.substring(with: offsets.nsRange(line.range)), source(line.range, in: text))
        }
    }

    /// A candidate after Cyrillic and emoji is underlined over exactly its
    /// own characters.
    func testCandidateUnderlinesLineUpAfterCyrillicAndEmoji() throws {
        let text = Self.fixture
        let styling = MarkdownStyling(markdown: text)
        let words = ["Анна Смирнова", "anna.smirnova@example.com", "Депозит"]
        let candidates = try words.enumerated().map { index, word in
            let bytes = Array(text.utf8)
            let needle = Array(word.utf8)
            let start = try XCTUnwrap((0...(bytes.count - needle.count)).first { Array(bytes[$0..<($0 + needle.count)]) == needle })
            return Candidate(
                id: CandidateID(rawValue: "c\(index)"), identityID: IdentityID(rawValue: "i\(index)"),
                range: SourceRange(start: start, end: start + needle.count),
                original: word, alias: "PERSON_\(index + 1)", category: .person, state: .proposed)
        }
        let styled = styler.attributedText(markdown: text, styling: styling, candidates: candidates)
        for (candidate, word) in zip(candidates, words) {
            let expected = (text as NSString).range(of: word)
            var effective = NSRange()
            let link = styled.attribute(.link, at: expected.location, longestEffectiveRange: &effective, in: NSRange(location: 0, length: styled.length))
            XCTAssertEqual((link as? URL).flatMap(MarkdownStyler.candidateID), candidate.id)
            XCTAssertEqual(effective, expected, word)
        }
    }

    /// A heading after a list marker (`- ## Notes`) is a heading: its size
    /// and VoiceOver's heading level, as on the desktop.
    func testAHeadingInAListItemIsAHeading() {
        let text = "Plain\n- ## Notes"
        let styled = styler.attributedText(markdown: text, styling: MarkdownStyling(markdown: text), candidates: [])
        let heading = (text as NSString).range(of: "Notes").location
        let font = styled.attribute(.font, at: heading, effectiveRange: nil) as? UIFont
        let body = styled.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
        XCTAssertGreaterThan(font?.pointSize ?? 0, body?.pointSize ?? 0)
        XCTAssertEqual(styled.attribute(.accessibilityTextHeadingLevel, at: heading, effectiveRange: nil) as? Int, 2)
        XCTAssertNil(styled.attribute(.accessibilityTextHeadingLevel, at: 0, effectiveRange: nil))
    }

    func testDocumentColoursAreReadAsRGBA() {
        let colour = UIColor(rgba: 0xC039_2BFF)
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        colour.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        XCTAssertEqual(red * 255, 0xC0, accuracy: 0.5)
        XCTAssertEqual(green * 255, 0x39, accuracy: 0.5)
        XCTAssertEqual(blue * 255, 0x2B, accuracy: 0.5)
        XCTAssertEqual(alpha, 1, accuracy: 0.01)
    }

    private struct Expected: Equatable, CustomStringConvertible {
        var bold = false
        var italic = false
        var monospaced = false
        var marker = false
        var link = false
        var struck = false

        var description: String {
            [("bold", bold), ("italic", italic), ("mono", monospaced), ("marker", marker), ("link", link), ("struck", struck)]
                .filter(\.1).map(\.0).joined(separator: " ")
        }
    }
}
