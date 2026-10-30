import UIKit
import XCTest
@testable import mdoc

final class TextOffsetsTests: XCTestCase {
    func testASCIIOffsetsAreEqual() {
        let offsets = TextOffsets("Maya Ellison")
        XCTAssertEqual(offsets.nsRange(SourceRange(start: 5, end: 12)), NSRange(location: 5, length: 7))
    }

    func testMultiByteScalarsShiftUTF16Offsets() {
        // "£" is 2 UTF-8 bytes and 1 UTF-16 unit; "Ж" the same; "😀" is 4 and 2.
        let text = "£4 Жанна 😀 Okafor"
        let offsets = TextOffsets(text)
        let start = text.utf8.count - "Okafor".utf8.count
        let range = offsets.nsRange(SourceRange(start: start, end: text.utf8.count))
        XCTAssertEqual((text as NSString).substring(with: range), "Okafor")
        XCTAssertEqual(offsets.sourceRange(range), SourceRange(start: start, end: text.utf8.count))
    }

    func testCyrillicRangeRoundTrips() {
        let text = "Иванов Иван"
        let offsets = TextOffsets(text)
        let range = offsets.nsRange(SourceRange(start: 0, end: "Иванов".utf8.count))
        XCTAssertEqual((text as NSString).substring(with: range), "Иванов")
        XCTAssertEqual(offsets.sourceRange(range).end, "Иванов".utf8.count)
    }

    func testOffsetsOutsideTheTextAreClamped() {
        let offsets = TextOffsets("ab")
        XCTAssertEqual(offsets.utf16(-3), 0)
        XCTAssertEqual(offsets.utf16(99), 2)
    }
}

final class MarkdownStylerTests: XCTestCase {
    func testCandidatesBecomeCandidateLinksAtTheirRanges() throws {
        let core = Fixture.core()
        let snapshot = try core.library.session(for: Fixture.letter).snapshot()
        let text = MarkdownStyler(traits: UITraitCollection(), selected: nil).attributedText(for: snapshot)
        let offsets = TextOffsets(snapshot.markdown)
        for candidate in snapshot.candidates {
            let range = offsets.nsRange(candidate.range)
            let link = text.attribute(.link, at: range.location, effectiveRange: nil) as? URL
            XCTAssertEqual(link.flatMap(MarkdownStyler.candidateID), candidate.id)
            XCTAssertEqual(text.attributedSubstring(from: range).string, candidate.original)
        }
    }

    func testMarkersAreDimmed() throws {
        let core = Fixture.core()
        let snapshot = try core.library.session(for: Fixture.letter).snapshot()
        let text = MarkdownStyler(traits: UITraitCollection(), selected: nil).attributedText(for: snapshot)
        // The letter starts with "# ", a heading marker.
        let colour = text.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? UIColor
        XCTAssertEqual(colour, UIColor.tertiaryLabel)
    }

    @MainActor
    func testSelectionCannotBeCopiedOrShared() {
        let view = CandidateTextView(usingTextLayoutManager: true)
        view.text = "Maya Ellison"
        view.selectedRange = NSRange(location: 0, length: 4)
        XCTAssertFalse(view.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        XCTAssertFalse(view.canPerformAction(#selector(UIResponderStandardEditActions.cut(_:)), withSender: nil))
        XCTAssertFalse(view.canPerformAction(NSSelectorFromString("_share:"), withSender: nil))
        XCTAssertTrue(view.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
    }

    func testCandidateLinkRoundTrips() {
        let id = CandidateID(rawValue: "m12")
        XCTAssertEqual(MarkdownStyler.candidateID(from: MarkdownStyler.link(for: id)), id)
        XCTAssertNil(MarkdownStyler.candidateID(from: URL(string: "https://example.com")!))
    }
}
