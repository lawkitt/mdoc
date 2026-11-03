import UIKit
import XCTest
@testable import mdoc

/// The rendered view (ADR 0096): markers hidden over the unchanged source,
/// lines of markers folded, list labels as desktop paints them, and tables
/// as grids whose cells keep document offsets and candidate links.
final class RenderedMarkdownTests: XCTestCase {
    private let traits = UITraitCollection(userInterfaceStyle: .light)

    private func render(_ text: String, width: CGFloat? = 360, candidates: [Candidate] = []) -> RenderedDocument {
        MarkdownStyler(traits: traits, selected: nil, width: width)
            .document(markdown: text, styling: MarkdownStyling(markdown: text), candidates: candidates)
    }

    private func candidate(_ word: String, in text: String, _ index: Int = 1) throws -> Candidate {
        let start = try XCTUnwrap(text.range(of: word)).lowerBound
        let bytes = text[..<start].utf8.count
        return Candidate(
            id: CandidateID(rawValue: "c\(index)"), identityID: IdentityID(rawValue: "i\(index)"),
            range: SourceRange(start: bytes, end: bytes + word.utf8.count),
            original: word, alias: "PERSON_\(index)", category: .person, state: .proposed)
    }

    private func isHidden(_ text: NSAttributedString, _ location: Int) -> Bool {
        text.attribute(.mdocHidden, at: location, effectiveRange: nil) != nil
    }

    private func decoration(_ text: NSAttributedString, at word: String) -> LineDecoration? {
        let location = (text.string as NSString).range(of: word).location
        return text.attribute(.mdocDecoration, at: location, effectiveRange: nil) as? LineDecoration
    }

    /// The text is the source, character for character, so every offset is.
    func testTheTextStaysTheSource() {
        let text = MarkdownStylingTests.fixture
        XCTAssertEqual(render(text).text.string, text)
    }

    func testMarkersAreHiddenAndContentIsNot() {
        let text = "Some **bold** and `code`, 1\\. kept."
        let styled = render(text).text
        let ns = text as NSString
        XCTAssertTrue(isHidden(styled, ns.range(of: "**").location))
        XCTAssertFalse(isHidden(styled, ns.range(of: "bold").location))
        XCTAssertTrue(isHidden(styled, ns.range(of: "`").location))
        XCTAssertTrue(isHidden(styled, ns.range(of: "\\").location))
        XCTAssertFalse(isHidden(styled, ns.range(of: ".").location))
        XCTAssertEqual(styled.attribute(.foregroundColor, at: ns.range(of: "**").location, effectiveRange: nil) as? UIColor, .clear)
    }

    /// A line of nothing but markers (an empty heading, a fence) folds to
    /// no height, or to a card's padding for a fence; raw HTML stays,
    /// greyed, as on desktop.
    func testLinesOfMarkersFold() throws {
        let text = "Before\n#\n<!-- note -->\nAfter"
        let styled = render(text).text
        let html = (text as NSString).range(of: "<!--").location
        XCTAssertFalse(isHidden(styled, html))
        XCTAssertEqual(styled.attribute(.foregroundColor, at: html, effectiveRange: nil) as? UIColor, .secondaryLabel)
        let location = (text as NSString).range(of: "#").location
        let style = try XCTUnwrap(styled.attribute(.paragraphStyle, at: location, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertLessThan(style.maximumLineHeight, 1)
        XCTAssertGreaterThan(style.maximumLineHeight, 0)
        let fenced = render("```\nlet x = 1\n```").text
        let fence = try XCTUnwrap(fenced.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertLessThan(fence.maximumLineHeight, 12)
        XCTAssertNotNil(decoration(fenced, at: "let")?.card)
    }

    /// Lists are numbered as desktop numbers them, nested ones by letter,
    /// with the body indented past the label.
    func testListsGetDesktopsLabels() throws {
        let text = "1. One\n3. Two\n    1. Deep\n- Bullet\n- [x] Done"
        let styled = render(text).text
        XCTAssertEqual(decoration(styled, at: "One")?.label?.text.string, "1.")
        XCTAssertEqual(decoration(styled, at: "Two")?.label?.text.string, "2.")
        XCTAssertEqual(decoration(styled, at: "Deep")?.label?.text.string, "a.")
        XCTAssertEqual(decoration(styled, at: "Bullet")?.label?.text.string, "•")
        XCTAssertEqual(decoration(styled, at: "Done")?.task?.checked, true)
        let deep = (text as NSString).range(of: "Deep").location
        let one = try XCTUnwrap(styled.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)
        let nested = try XCTUnwrap(styled.attribute(.paragraphStyle, at: deep, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertGreaterThan(one.headIndent, 0)
        XCTAssertEqual(one.headIndent, one.firstLineHeadIndent)
        XCTAssertGreaterThan(nested.headIndent, one.headIndent)
    }

    func testAlertsQuotesAndRulesAreDrawn() {
        let text = "> [!WARNING]\n> Keep keys safe.\n\n> Plain quote\n\n---\n\nEnd"
        let styled = render(text).text
        XCTAssertEqual(decoration(styled, at: "[!WARNING]")?.title?.text.string, "Warning")
        XCTAssertNotNil(decoration(styled, at: "Keep")?.card)
        XCTAssertNotNil(decoration(styled, at: "Plain")?.bar)
        XCTAssertNil(decoration(styled, at: "Plain")?.card)
        XCTAssertNotNil(decoration(styled, at: "---")?.rule)
    }

    /// A table becomes a grid: its lines fold except the first, which keeps
    /// the grid's height; cells keep their document location and links.
    func testTablesBecomeGrids() throws {
        let text = "Intro\n\n| Name | Role |\n| --- | :-: |\n| Maya Bennett | Tenant |\n\nAfter"
        let maya = try candidate("Maya Bennett", in: text)
        let document = render(text, candidates: [maya])
        let table = try XCTUnwrap(document.tables.first)
        XCTAssertEqual(table.rows.count, 2)
        XCTAssertEqual(table.columns.count, 2)
        XCTAssertEqual(table.width, 360, accuracy: 2, "a narrow table fills the column")
        let cell = table.rows[1][0]
        XCTAssertEqual(cell.text.string, "Maya Bennett")
        XCTAssertEqual((text as NSString).substring(with: NSRange(location: cell.location, length: cell.text.length)), "Maya Bennett")
        let link = cell.text.attribute(.link, at: 0, effectiveRange: nil) as? URL
        XCTAssertEqual(link.flatMap(MarkdownStyler.candidateID), maya.id)
        let header = try XCTUnwrap(table.rows[0][0].text.attribute(.font, at: 0, effectiveRange: nil) as? UIFont)
        XCTAssertTrue(header.fontDescriptor.symbolicTraits.contains(.traitBold))
        let ns = text as NSString
        let anchor = try XCTUnwrap(document.text.attribute(.paragraphStyle, at: ns.range(of: "| Name").location, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertEqual(anchor.minimumLineHeight, table.height, accuracy: 0.5)
        let body = try XCTUnwrap(document.text.attribute(.paragraphStyle, at: ns.range(of: "| Maya").location, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertLessThan(body.maximumLineHeight, 1)
        XCTAssertNil(document.text.attribute(.link, at: ns.range(of: "Maya").location, effectiveRange: nil), "folded text keeps no link")
        XCTAssertEqual(table.anchor, ns.range(of: "| Name").location)
    }

    /// Long bilingual cells wrap in columns no narrower than the minimum, so
    /// a three-column contract scrolls sideways on a phone.
    func testLongCellsWrapAndTheTableScrolls() throws {
        let long = String(repeating: "Индивидуальный предприниматель Сильвестрова ", count: 6)
        let text = "| RU | EN | ZH |\n| --- | --- | --- |\n| \(long) | \(long) | \(long) |"
        let table = try XCTUnwrap(render(text).tables.first)
        XCTAssertGreaterThan(table.width, 360)
        for column in table.columns {
            XCTAssertGreaterThanOrEqual(column, 140)
        }
        XCTAssertGreaterThan(table.heights[1], table.heights[0] * 3, "the body row wraps")
    }

    /// An identifier longer than the column cap widens its column rather
    /// than breaking (the table scrolls).
    func testALongIdentifierWidensItsColumn() throws {
        let identifier = String(repeating: "A1B2C3D4E5", count: 6)
        let text = "| Key | Value |\n| --- | --- |\n| \(identifier) | ok |"
        let table = try XCTUnwrap(render(text).tables.first)
        let cell = table.rows[1][0].text
        let width = cell.size().width + table.padding.left + table.padding.right
        XCTAssertGreaterThanOrEqual(table.columns[0], width - 1)
        XCTAssertGreaterThan(table.width, 360)
    }

    /// A table inside fenced code stays code, pipes and all.
    func testAFencedTableStaysCode() {
        let text = "```\n| A | B |\n| --- | --- |\n| a | b |\n```"
        let document = render(text)
        XCTAssertTrue(document.tables.isEmpty)
        XCTAssertFalse(isHidden(document.text, (text as NSString).range(of: "| a").location))
    }

    /// An image becomes a chip: space kerned after its hidden `[`, labelled
    /// "Image" when it has no alt text, and its line never folds away.
    func testImagesBecomeChips() throws {
        let text = "Plan: ![Floor plan](plan.png)\n![](scan.png)\nEnd"
        let styled = render(text).text
        let ns = text as NSString
        let first = ns.range(of: "![Floor").location
        XCTAssertTrue(isHidden(styled, first))
        XCTAssertGreaterThan(styled.attribute(.kern, at: first + 1, effectiveRange: nil) as? CGFloat ?? 0, 10)
        XCTAssertFalse(isHidden(styled, ns.range(of: "Floor plan").location))
        let chips = try XCTUnwrap(decoration(styled, at: "Plan:")?.chips)
        XCTAssertEqual(chips.count, 1)
        XCTAssertNil(chips[0].label)
        let empty = try XCTUnwrap(decoration(styled, at: "![](scan")?.chips.first)
        XCTAssertEqual(empty.label?.string, "Image")
        let style = try XCTUnwrap(styled.attribute(.paragraphStyle, at: ns.range(of: "![](scan").location, effectiveRange: nil) as? NSParagraphStyle)
        XCTAssertNotEqual(style.maximumLineHeight, BlockLayout.folded, "an image's line does not fold")
    }

    /// Without a width yet, a table stays as its source rows.
    func testNoWidthLeavesTheSource() {
        let text = "| A | B |\n| --- | --- |\n| a | b |"
        let document = render(text, width: nil)
        XCTAssertTrue(document.tables.isEmpty)
        XCTAssertFalse(isHidden(document.text, (text as NSString).range(of: "a |").location))
    }

    func testRenderedParagraphsNeverHyphenate() {
        let text = MarkdownStylingTests.fixture
        let document = render(text)
        var styles: [NSParagraphStyle] = []
        document.text.enumerateAttribute(.paragraphStyle, in: NSRange(location: 0, length: document.text.length)) { value, _, _ in
            if let style = value as? NSParagraphStyle { styles.append(style) }
        }
        for row in document.tables.flatMap(\.rows) {
            for cell in row where cell.text.length > 0 {
                if let style = cell.text.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle {
                    styles.append(style)
                }
            }
        }
        XCTAssertFalse(styles.isEmpty)
        for style in styles {
            XCTAssertFalse(style.usesDefaultHyphenation)
            XCTAssertEqual(style.hyphenationFactor, 0)
        }
    }

    @MainActor
    func testTableCellsCannotBeCopied() {
        let cell = CellTextView(usingTextLayoutManager: false)
        cell.text = "Maya Bennett"
        XCTAssertFalse(cell.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        cell.allowsCopy = true
        cell.selectedRange = NSRange(location: 0, length: 4)
        XCTAssertTrue(cell.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
    }
}
