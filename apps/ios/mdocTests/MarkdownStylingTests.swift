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
            case .heading: mark(line.range) { $0.bold = true }
            case .tableHeader: mark(line.range) { $0.bold = true; $0.monospaced = true }
            case .codeBlock, .tableRow: mark(line.range) { $0.monospaced = true }
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

    /// A replaced alias in a table keeps the table's size, and a header's
    /// bold (#286).
    func testAReplacedAliasKeepsItsTablesFont() throws {
        let text = "| Name | Role |\n| --- | --- |\n| Maya Bennett | Tenant |"
        let styling = MarkdownStyling(markdown: text)
        func candidate(_ word: String, _ index: Int) throws -> Candidate {
            let start = try XCTUnwrap(text.range(of: word)).lowerBound.utf16Offset(in: text)
            return Candidate(
                id: CandidateID(rawValue: "c\(index)"), identityID: IdentityID(rawValue: "i\(index)"),
                range: SourceRange(start: start, end: start + word.utf8.count),
                original: word, alias: "PERSON_\(index)", category: .person, state: .replaced)
        }
        let candidates = [try candidate("Name", 1), try candidate("Maya Bennett", 2)]
        let plain = styler.attributedText(markdown: text, styling: styling, candidates: [])
        let styled = styler.attributedText(markdown: text, styling: styling, candidates: candidates)
        for word in ["Name", "Maya Bennett"] {
            let at = (text as NSString).range(of: word).location
            let before = try XCTUnwrap(plain.attribute(.font, at: at, effectiveRange: nil) as? UIFont)
            let after = try XCTUnwrap(styled.attribute(.font, at: at, effectiveRange: nil) as? UIFont)
            XCTAssertEqual(after.pointSize, before.pointSize, accuracy: 0.01, word)
            XCTAssertEqual(
                after.fontDescriptor.symbolicTraits.contains(.traitBold),
                before.fontDescriptor.symbolicTraits.contains(.traitBold), word)
            XCTAssertTrue(after.fontDescriptor.symbolicTraits.contains(.traitMonoSpace), word)
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

    /// A hyphen inside a name or an email would read as part of it
    /// (ADR 0087): no paragraph of the review text, or of a value label,
    /// may hyphenate.
    func testNothingIsHyphenated() {
        let text = Self.fixture
        let styled = styler.attributedText(markdown: text, styling: MarkdownStyling(markdown: text), candidates: [])
        var paragraphs = 0
        styled.enumerateAttribute(.paragraphStyle, in: NSRange(location: 0, length: styled.length)) { value, _, _ in
            guard let style = value as? NSParagraphStyle else { return XCTFail("a range without a paragraph style") }
            XCTAssertFalse(style.usesDefaultHyphenation)
            XCTAssertEqual(style.hyphenationFactor, 0)
            XCTAssertEqual(style.lineBreakStrategy, [])
            paragraphs += 1
        }
        XCTAssertGreaterThan(paragraphs, 0)
        // A text-style font hyphenates over-long words on iOS 26.
        styled.enumerateAttribute(.font, in: NSRange(location: 0, length: styled.length)) { value, _, _ in
            let style = (value as? UIFont)?.fontDescriptor.object(forKey: .textStyle) as? String
            XCTAssertFalse(style?.hasPrefix("UICTFontTextStyle") ?? false, "a Dynamic Type text-style font")
        }

        let value = UnhyphenatedText.attributed("anna.smirnova@example.com", font: .systemFont(ofSize: 17), color: .label)
        XCTAssertEqual(value.string.replacingOccurrences(of: "\u{200B}", with: ""), "anna.smirnova@example.com")
        let style = value.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle
        XCTAssertEqual(style?.usesDefaultHyphenation, false)
        XCTAssertEqual(style?.hyphenationFactor, 0)
        XCTAssertEqual(style?.lineBreakStrategy, [])
    }

    /// Dark mode (ADR 0087, P16): every category's underline reaches 3:1
    /// against the background, and the fills are stronger than in light.
    func testDarkHighlightsStandOut() {
        let dark = UITraitCollection(userInterfaceStyle: .dark)
        let background = UIColor.systemBackground.resolvedColor(with: dark)
        for category in Category.allCases {
            let underline = category.uiColor.resolvedColor(with: dark)
            XCTAssertGreaterThanOrEqual(Self.contrast(underline, background), 3, "\(category)")
        }
        let light = MarkdownStyler.fill(dark: false)
        let strong = MarkdownStyler.fill(dark: true)
        XCTAssertGreaterThan(strong.proposed, light.proposed)
        XCTAssertGreaterThan(strong.replaced, light.replaced)
    }

    /// WCAG contrast ratio of two opaque colours.
    static func contrast(_ first: UIColor, _ second: UIColor) -> CGFloat {
        func luminance(_ colour: UIColor) -> CGFloat {
            var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
            colour.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
            func linear(_ value: CGFloat) -> CGFloat {
                value <= 0.039_28 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
        }
        let (a, b) = (luminance(first), luminance(second))
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
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
