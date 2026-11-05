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

    /// A table cell's image gets its chip in the cell's text; an image inside
    /// raw HTML stays literal; VoiceOver hears "Image".
    func testCellImagesAndLiteralHTML() throws {
        let text = "<!-- ![](scan.png) -->\n\n| Seal | Note |\n| --- | --- |\n| ![](stamp.png) | ok |"
        let document = render(text)
        let cell = try XCTUnwrap(document.tables.first?.rows[1][0].text)
        let chips = try XCTUnwrap(cell.attribute(.mdocDecoration, at: 0, effectiveRange: nil) as? LineDecoration).chips
        XCTAssertEqual(chips.count, 1)
        XCTAssertEqual(chips.first?.label?.string, "Image")
        XCTAssertEqual(cell.attribute(.accessibilityTextCustom, at: 0, effectiveRange: nil) as? [String], ["Image"])
        XCTAssertNil(document.text.attribute(.kern, at: 1, effectiveRange: nil), "no chip inside an HTML comment")
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

    /// Each grid sits on its own line, after the text above it, also when
    /// the view shows a restyled document while scrolled past the table (as
    /// when Find names ends) and after it scrolls back up.
    @MainActor
    func testTablesSitOnTheirLinesAfterARestyle() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/mobile-markdown/rendering/registration-ru.md")
        // A long document before the tables, as a multi-page import is.
        let filler = (1...60).map { "## Раздел \($0)\n\nСведения о заявителе, пункт \($0): **Фамилия Имя** Дата рождения 01.01.1990 Адрес 190000, г. Санкт-Петербург, ул. Примерная, д. \($0)." }
        let text = filler.joined(separator: "\n\n") + "\n\n" + (try String(contentsOf: url, encoding: .utf8))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = RenderedTextView()
        view.isEditable = false
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        view.layoutIfNeeded()
        let width = view.textWidth
        func show(_ candidates: [Candidate]) {
            let document = MarkdownStyler(traits: view.traitCollection, selected: nil, width: width)
                .document(markdown: text, styling: MarkdownStyling(markdown: text), candidates: candidates)
            view.show(document, width: width)
            view.layoutIfNeeded()
        }
        // Each grid, scrolled into view, sits in its gap: below the text
        // before the table and above the text after it, as drawn.
        func check(_ moment: String) throws {
            XCTAssertEqual(view.tableViews.count, 2, moment)
            let layout = try XCTUnwrap(view.textLayoutManager)
            let content = try XCTUnwrap(layout.textContentManager)
            let documentStart = content.documentRange.location
            let kept = view.contentOffset
            for table in view.tableViews {
                // Twice: the first scroll lands on an estimate.
                for _ in 0..<2 {
                    view.setContentOffset(CGPoint(x: 0, y: max(0, table.frame.minY - 200)), animated: false)
                    view.layoutIfNeeded()
                }
                let before = try XCTUnwrap(content.location(documentStart, offsetBy: table.table.anchor - 1))
                let after = try XCTUnwrap(content.location(documentStart, offsetBy: NSMaxRange(table.table.range)))
                let previous = try XCTUnwrap(layout.textLayoutFragment(for: before))
                let next = try XCTUnwrap(layout.textLayoutFragment(for: after))
                XCTAssertEqual(previous.state, .layoutAvailable, moment)
                XCTAssertEqual(next.state, .layoutAvailable, moment)
                let above = previous.layoutFragmentFrame.maxY + view.textContainerInset.top
                let below = next.layoutFragmentFrame.minY + view.textContainerInset.top
                XCTAssertGreaterThanOrEqual(table.frame.minY, above - 1, "\(moment): table at \(table.table.anchor) covers the text above")
                XCTAssertLessThanOrEqual(table.frame.maxY, below + 1, "\(moment): table at \(table.table.anchor) covers the text below")
            }
            view.setContentOffset(kept, animated: false)
            view.layoutIfNeeded()
        }
        show([])
        try check("first shown")
        // Scrolled down the way a reader does, a screen at a time.
        var y: CGFloat = 0
        while y < view.contentSize.height - view.bounds.height {
            y += view.bounds.height / 2
            view.setContentOffset(CGPoint(x: 0, y: y), animated: false)
            view.layoutIfNeeded()
        }
        try check("scrolled down")
        show([try candidate("0452644", in: text), try candidate("Новак Ян", in: text, 2)])
        try check("restyled while scrolled")
        view.setContentOffset(.zero, animated: false)
        view.layoutIfNeeded()
        try check("scrolled back up")
    }

    /// VoiceOver hears the rendered text: no markers, no folded table source
    /// (the grid reads the table), at unchanged offsets, with a word split by
    /// markers kept whole; review cells read as drawn too.
    @MainActor
    func testVoiceOverHearsTheRenderedText() throws {
        let text = "Some **bold** words, 12**34**56.\n\n| Name | Role |\n| --- | --- |\n| **Maya Bennett** | Tenant |\n\nAfter"
        let view = CandidateTextView()
        view.frame = CGRect(x: 0, y: 0, width: 390, height: 800)
        view.show(render(text), width: 360)
        let whole = try XCTUnwrap(view.textRange(from: view.beginningOfDocument, to: view.endOfDocument))
        let spoken = try XCTUnwrap(view.text(in: whole))
        let visible = spoken.replacingOccurrences(of: "\u{2060}", with: "")
        XCTAssertEqual((spoken as NSString).length, (text as NSString).length)
        XCTAssertTrue(visible.contains("Some bold words, 123456."), visible)
        XCTAssertTrue(visible.contains("After"))
        XCTAssertFalse(visible.contains("|"))
        XCTAssertFalse(visible.contains("Maya"), "the folded table is read from its grid")
        XCTAssertEqual(view.accessibilityValue, spoken)
        // The folded table's line breaks stay: UIKit splits the text into
        // paragraphs by the text and labels them by this, so the two must
        // have the same paragraphs, or the candidates after the table lose
        // their link elements.
        XCTAssertEqual(spoken.components(separatedBy: "\n").map { ($0 as NSString).length },
                       text.components(separatedBy: "\n").map { ($0 as NSString).length })
        // A part of the text keeps its own offsets.
        let start = try XCTUnwrap(view.position(from: view.beginningOfDocument, offset: 5))
        let end = try XCTUnwrap(view.position(from: start, offset: 8))
        XCTAssertEqual(view.text(in: try XCTUnwrap(view.textRange(from: start, to: end)))?.replacingOccurrences(of: "\u{2060}", with: ""), "bold")
        // A review cell reads as drawn; a Reply cell, which copies, keeps its source.
        let cell = try XCTUnwrap(view.tableViews.first?.cells[1][0])
        XCTAssertEqual(cell.accessibilityValue?.replacingOccurrences(of: "\u{2060}", with: ""), "Maya Bennett")
        cell.allowsCopy = true
        let cellText = try XCTUnwrap(cell.textRange(from: cell.beginningOfDocument, to: cell.endOfDocument))
        XCTAssertEqual(cell.text(in: cellText), "**Maya Bennett**")
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
