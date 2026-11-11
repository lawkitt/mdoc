import MdocFFI
import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// Live-preview editing (ADR 0109, slice 3), on the real core: the rendered
/// view is edited with its string staying the source; the caret's construct
/// and line prefix show dimmed and the rest stays hidden; the caret never
/// rests in a hidden run and goes into a table's cells; Return, ⌫ and ⌦ are the editor
/// core's, one core edit each; and each keystroke restyles only its block,
/// to exactly what a whole restyle draws.
@MainActor
final class LivePreviewTests: XCTestCase {
    static let text = """
        # Lease

        The **tenant** pays [rent](https://example.com) monthly.

        - first item
        - [ ] a task

        > quoted words

        | A | B |
        | --- | --- |
        | 1 | 2 |

        ![](scan.png)

        ```
        code **literal**
        ```

        ---

        Last line.
        """

    struct Harness {
        let session: any mdoc.DocumentSession
        let model: DocumentViewModel
        let window: UIWindow
        let view: CandidateTextView
        let coordinator: MarkdownTextView.Coordinator
        let rendered: Bool

        var shown: String { view.textStorage.string }
        var string: NSString { view.textStorage.string as NSString }
    }

    func harness(text: String = text, rendered: Bool = true) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Lease", into: .newCase(name: "Lease"))
        _ = try session.recognize(observer: TestObserver())
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: Self.textView(model, rendered: rendered))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(
            session: session, model: model, window: window, view: view, coordinator: coordinator, rendered: rendered)
        show(harness)
        harness.model.enterEditMode()
        show(harness)
        XCTAssertTrue(view.isEditable)
        return harness
    }

    static func textView(_ model: DocumentViewModel, rendered: Bool) -> MarkdownTextView {
        MarkdownTextView(
            snapshot: model.snapshot, selected: nil, rendered: rendered,
            onCandidate: { _ in }, onReplace: { _ in }, onReviewNext: {},
            edit: model.isEditing ? EditActions(model) : nil)
    }

    func show(_ harness: Harness, rendered: Bool? = nil) {
        harness.coordinator.update(
            harness.view, with: Self.textView(harness.model, rendered: rendered ?? harness.rendered), dynamicTypeSize: .large,
            colorScheme: .light)
        harness.view.layoutIfNeeded()
    }

    /// The reader puts the caret at `location`.
    func caret(_ harness: Harness, at location: Int) {
        harness.view.selectedRange = NSRange(location: location, length: 0)
        harness.coordinator.textViewDidChangeSelection(harness.view)
    }

    /// One change as UIKit makes it, at the caret; refused when the core
    /// made it itself.
    @discardableResult
    func change(_ harness: Harness, _ range: NSRange, _ text: String) -> Bool {
        let view = harness.view
        guard harness.coordinator.textView(view, shouldChangeTextIn: range, replacementText: text) else { return false }
        view.textStorage.replaceCharacters(in: range, with: text)
        view.selectedRange = NSRange(location: range.location + (text as NSString).length, length: 0)
        harness.coordinator.textViewDidChange(view)
        return true
    }

    func type(_ harness: Harness, _ text: String) {
        for character in text {
            let at = harness.view.selectedRange
            change(harness, at, String(character))
        }
    }

    /// ⌫ as UIKit sends it: the character before the caret.
    func backspace(_ harness: Harness) {
        let caret = harness.view.selectedRange.location
        let range = harness.string.rangeOfComposedCharacterSequence(at: caret - 1)
        change(harness, range, "")
    }

    func location(of needle: String, in harness: Harness) -> Int {
        harness.string.range(of: needle).location
    }

    func hidden(_ harness: Harness, at location: Int) -> Bool {
        harness.view.textStorage.attribute(.mdocHidden, at: location, effectiveRange: nil) != nil
    }

    func decoration(_ harness: Harness, at location: Int) -> LineDecoration? {
        harness.view.textStorage.attribute(.mdocDecoration, at: location, effectiveRange: nil) as? LineDecoration
    }

    func coreText(_ harness: Harness) -> String {
        harness.session.snapshot().markdown
    }

    // MARK: The caret on an empty line (ADR 0110)

    /// The caret's height where it rests now, and the body line's.
    func caretHeight(_ harness: Harness) throws -> (caret: CGFloat, body: CGFloat) {
        harness.view.layoutIfNeeded()
        let end = try XCTUnwrap(harness.view.selectedTextRange?.end)
        let body = MarkdownStyler.font(.body, traits: harness.view.traitCollection)
        return (harness.view.caretRect(for: end).height, body.lineHeight)
    }

    func assertBodyTyping(_ harness: Harness, _ message: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let (caret, body) = try caretHeight(harness)
        XCTAssertGreaterThanOrEqual(caret, body * 0.95, "the caret is a body line tall \(message)", file: file, line: line)
        XCTAssertLessThanOrEqual(caret, ceil(body) + 0.5, "and no taller \(message)", file: file, line: line)
        let font = try XCTUnwrap(harness.view.typingAttributes[.font] as? UIFont, file: file, line: line)
        XCTAssertEqual(font.pointSize, MarkdownStyler.font(.body, traits: harness.view.traitCollection).pointSize,
                       "what is typed next is body-sized \(message)", file: file, line: line)
    }

    func testTheCaretOnAnEmptyLineIsABodyLineTall() throws {
        for rendered in [true, false] {
            let harness = try harness(rendered: rendered)
            let mode = rendered ? "in live preview" : "with Show Markdown"
            // A blank line between paragraphs.
            let blank = location(of: "The **tenant**", in: harness) - 1
            XCTAssertEqual(harness.string.character(at: blank), 0x0A)
            caret(harness, at: blank)
            try assertBodyTyping(harness, "on a blank line \(mode)")
            // A whole restyle, the caret staying there, keeps it tall.
            harness.coordinator.restyleWhole(harness.view)
            try assertBodyTyping(harness, "on a blank line after a whole restyle \(mode)")
            // It shrinks back once the caret leaves, as a whole restyle draws it.
            caret(harness, at: location(of: "monthly", in: harness))
            assertMatchesAWholeRestyle(harness, mode)
            // After Return at the end of a paragraph, and after a heading.
            caret(harness, at: location(of: "Last line.", in: harness) + 10)
            type(harness, "\n")
            try assertBodyTyping(harness, "after Return \(mode)")
            type(harness, "x")
            XCTAssertTrue(harness.shown.hasSuffix("x"))
            caret(harness, at: location(of: "# Lease", in: harness) + 7)
            type(harness, "\n")
            try assertBodyTyping(harness, "after a heading \(mode)")
            assertMatchesAWholeRestyle(harness, mode)
        }
    }

    /// The caret's whole line is kept above the toolbar's room: after a
    /// command, a keyboard change or the room growing, the text scrolls it
    /// back up; the room comes off when edit mode ends (ADR 0110).
    func testTheCaretsLineStaysAboveTheToolbar() throws {
        let paragraphs = (1...60).map { "Paragraph \($0) of the lease." }.joined(separator: "\n\n")
        let harness = try harness(text: Self.text + "\n\n" + paragraphs + "\n")
        let view = harness.view, coordinator = harness.coordinator
        harness.window.makeKeyAndVisible()
        XCTAssertTrue(view.becomeFirstResponder())
        let inset = view.contentInset.bottom
        coordinator.setToolbarRoom(300, in: view)
        XCTAssertEqual(view.contentInset.bottom, inset + 300)
        // The caret on a line the toolbar's room covers.
        view.layoutIfNeeded()
        let hidden = try XCTUnwrap(view.closestPosition(to: CGPoint(x: 40, y: view.contentOffset.y + view.bounds.height - 120)))
        caret(harness, at: view.offset(from: view.beginningOfDocument, to: hidden))
        XCTAssertGreaterThan(view.caretRect(for: try XCTUnwrap(view.selectedTextRange?.end)).maxY,
                             view.contentOffset.y + view.bounds.height - view.adjustedContentInset.bottom, "the caret starts under the room")
        coordinator.scrollCaretLineIntoView(view, animated: false)
        view.layoutIfNeeded()
        let end = try XCTUnwrap(view.selectedTextRange?.end)
        let line = view.caretRect(for: end)
        let bottom = view.contentOffset.y + view.bounds.height - view.adjustedContentInset.bottom
        XCTAssertLessThanOrEqual(line.maxY, bottom + 0.5, "the caret's line is above the room: \(line), \(bottom)")
        XCTAssertGreaterThan(line.minY, view.contentOffset.y + view.adjustedContentInset.top, "and still in view")
        // With no keyboard showing there is no room.
        coordinator.updateToolbarRoom(view)
        XCTAssertEqual(view.contentInset.bottom, inset)
    }

    // MARK: The reveal rule

    func testTheCaretsBoldShowsItsMarkersDimmed() throws {
        let harness = try harness()
        let bold = location(of: "**tenant**", in: harness)
        let link = location(of: "[rent]", in: harness)
        XCTAssertTrue(hidden(harness, at: bold), "away from the caret the markers hide")
        caret(harness, at: bold + 4)
        for offset in [0, 1, 8, 9] {
            XCTAssertFalse(hidden(harness, at: bold + offset), "the caret's bold shows its markers")
            let colour = harness.view.textStorage.attribute(.foregroundColor, at: bold + offset, effectiveRange: nil) as? UIColor
            XCTAssertEqual(colour, MarkdownStyler.marker, "dimmed")
        }
        XCTAssertTrue(hidden(harness, at: link), "the link on the same line keeps its markers hidden")
        // Leaving the line hides them again.
        caret(harness, at: (harness.shown as NSString).length)
        XCTAssertTrue(hidden(harness, at: bold))
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testTheCaretsLineShowsItsPrefix() throws {
        let harness = try harness()
        let heading = location(of: "# Lease", in: harness)
        let item = location(of: "- first", in: harness)
        let task = location(of: "- [ ] a task", in: harness)
        caret(harness, at: heading + 4)
        XCTAssertFalse(hidden(harness, at: heading), "the heading's # shows on its line")
        XCTAssertTrue(hidden(harness, at: item))
        XCTAssertNotNil(decoration(harness, at: item)?.label, "another line keeps its bullet")
        caret(harness, at: item + 5)
        XCTAssertTrue(hidden(harness, at: heading))
        XCTAssertFalse(hidden(harness, at: item), "the item's - shows")
        XCTAssertNil(decoration(harness, at: item)?.label, "and no bullet beside it")
        caret(harness, at: task + 8)
        XCTAssertTrue(hidden(harness, at: task), "a task's prefix stays hidden on its line")
        XCTAssertTrue(hidden(harness, at: task + 2))
        XCTAssertNotNil(decoration(harness, at: task)?.task, "and its box stays drawn")
        // A caret put in the hidden prefix goes to the task's text.
        caret(harness, at: task + 1)
        XCTAssertEqual(harness.view.selectedRange.location, task + 6)
        // A hidden fence line is stepped over, onto its code, where both
        // fences show their source; a rule on the caret's line shows too.
        let fence = location(of: "```", in: harness)
        let code = location(of: "code **literal**", in: harness)
        let close = location(of: "```\n\n---", in: harness)
        caret(harness, at: fence + 3)
        XCTAssertEqual(harness.view.selectedRange.location, code)
        XCTAssertFalse(hidden(harness, at: fence))
        XCTAssertFalse(hidden(harness, at: close))
        caret(harness, at: fence + 3)
        XCTAssertEqual(harness.view.selectedRange.location, fence + 3, "a shown fence takes the caret")
        let rule = location(of: "---\n\nLast", in: harness)
        caret(harness, at: rule + 1)
        XCTAssertFalse(hidden(harness, at: rule))
        XCTAssertNil(decoration(harness, at: rule)?.rule)
    }

    /// The caret's list line draws its source marker in the list's column,
    /// and a task keeps its box, so the item's text does not move sideways
    /// as the caret enters or leaves it (ADR 0110): bullets, tasks, nested
    /// items, a two-digit number, and at the largest text size.
    func testTheCaretsListLineDoesNotShift() throws {
        let numbered = (1...11).map { "\($0). Clause \($0)" }.joined(separator: "\n")
        let text = """
            Intro.

            - Bullet item
              - Nested item
                - Deep item
            - [ ] Open task
            - [x] Done task

            \(numbered)

            Last line.
            """
        for size in [DynamicTypeSize.large, .accessibility5] {
            let harness = try harness(text: text)
            harness.coordinator.update(
                harness.view, with: Self.textView(harness.model, rendered: true), dynamicTypeSize: size, colorScheme: .light)
            harness.view.layoutIfNeeded()
            func x(of word: String) throws -> CGFloat {
                harness.view.layoutIfNeeded()
                let start = location(of: word, in: harness)
                let view = harness.view
                let from = try XCTUnwrap(view.position(from: view.beginningOfDocument, offset: start))
                let to = try XCTUnwrap(view.position(from: from, offset: 1))
                return view.firstRect(for: try XCTUnwrap(view.textRange(from: from, to: to))).minX
            }
            for word in ["Bullet item", "Nested item", "Deep item", "Open task", "Done task", "Clause 1\n", "Clause 11"] {
                caret(harness, at: location(of: "Last line.", in: harness))
                let away = try x(of: word)
                caret(harness, at: location(of: word, in: harness) + 3)
                // A task keeps its box; any other item shows its marker.
                XCTAssertEqual(
                    hidden(harness, at: location(of: word, in: harness) - 2), word.hasSuffix("task"),
                    "\(word): its marker shows, or its box stays")
                let on = try x(of: word)
                XCTAssertEqual(on, away, accuracy: 0.5, "\(word) stays put as the caret enters its line (\(size))")
                // The caret at the text's start stands at the text, not in the
                // room before it.
                caret(harness, at: location(of: word, in: harness))
                harness.view.layoutIfNeeded()
                let start = try XCTUnwrap(harness.view.selectedTextRange?.start)
                XCTAssertEqual(harness.view.caretRect(for: start).minX, away, accuracy: 2, "\(word): the caret at its start (\(size))")
                // What is typed there is the text's size, not a narrowed marker's.
                let textFont = harness.view.textStorage.attribute(.font, at: location(of: word, in: harness), effectiveRange: nil) as? UIFont
                XCTAssertEqual((harness.view.typingAttributes[.font] as? UIFont)?.pointSize, textFont?.pointSize, "\(word): typing at its start (\(size))")
                caret(harness, at: location(of: "Last line.", in: harness))
                XCTAssertEqual(try x(of: word), away, accuracy: 0.5, "and as it leaves (\(size))")
            }
            // Return after a done task gives a new task, its box drawn; what
            // is typed next is the text's size.
            let body = harness.view.textStorage.attribute(.font, at: location(of: "Done task", in: harness), effectiveRange: nil) as? UIFont
            caret(harness, at: location(of: "Done task", in: harness) + 9)
            type(harness, "\n")
            XCTAssertTrue(harness.shown.contains("- [x] Done task\n- [ ] \n"), "a new task")
            XCTAssertEqual((harness.view.typingAttributes[.font] as? UIFont)?.pointSize, body?.pointSize, "typing in a new task (\(size))")
            type(harness, "Next")
            let next = harness.view.textStorage.attribute(.font, at: location(of: "Next", in: harness), effectiveRange: nil) as? UIFont
            XCTAssertEqual(next?.pointSize, body?.pointSize, "the typed text (\(size))")
        }
    }

    func testVoiceOverDoesNotHearHiddenMarkers() throws {
        let harness = try harness()
        caret(harness, at: (harness.shown as NSString).length)
        let value = try XCTUnwrap(harness.view.accessibilityValue)
        XCTAssertFalse(value.contains("**tenant**"))
        XCTAssertTrue(value.contains("tenant"))
        XCTAssertEqual((value as NSString).length, harness.string.length, "offsets line up")
        // Copy takes the source, not what VoiceOver hears.
        let bold = location(of: "**tenant**", in: harness)
        harness.view.selectedRange = NSRange(location: bold, length: 10)
        harness.view.copySelection()
        XCTAssertEqual(InAppClipboard.shared.read(), "**tenant**")
    }

    // MARK: Caret over hidden markers

    func testTheCaretSnapsOutOfHiddenRuns() throws {
        let harness = try harness()
        caret(harness, at: (harness.shown as NSString).length)
        let bold = location(of: "**tenant**", in: harness)
        // A tap where the hidden `**` draws lands after it, inside the bold.
        caret(harness, at: bold)
        XCTAssertEqual(harness.view.selectedRange.location, bold + 2)
        XCTAssertFalse(hidden(harness, at: bold), "and the bold reveals")
        // Onto a hidden line prefix: after it.
        caret(harness, at: (harness.shown as NSString).length)
        let item = location(of: "- first", in: harness)
        caret(harness, at: item)
        XCTAssertEqual(harness.view.selectedRange.location, item + 2)
    }

    /// Slice 5 (ADR 0109): a caret that lands in a table's hidden source
    /// is kept in a cell, and a tap on the grid puts it in the cell tapped.
    func testTheCaretGoesIntoATablesCells() throws {
        let harness = try harness()
        let table = location(of: "| A | B |", in: harness)
        XCTAssertTrue(hidden(harness, at: table))
        // Landing on the row's first pipe: the first cell's text.
        caret(harness, at: table)
        XCTAssertEqual(harness.view.selectedRange.location, table + 2)
        // On the `| --- |` row: the header's cell above.
        caret(harness, at: location(of: "| --- | ---", in: harness) + 8)
        XCTAssertEqual(harness.view.selectedRange.location, location(of: "B |", in: harness))
        // A tap on a cell: the caret in that cell's source.
        let grid = try XCTUnwrap(harness.view.tableViews.first)
        caret(harness, at: 0)
        let two = grid.cells[1][1]
        harness.view.onTableTap?(grid, CGPoint(x: two.frame.maxX - 4, y: two.frame.midY))
        harness.coordinator.textViewDidChangeSelection(harness.view)
        XCTAssertEqual(harness.view.selectedRange.location, location(of: "2 |", in: harness) + 1)
        XCTAssertTrue(grid.cells.joined().allSatisfy { !$0.isUserInteractionEnabled }, "the text view keeps the caret")
    }

    // MARK: Structural keys

    func testReturnContinuesAndEndsAList() throws {
        let harness = try harness()
        let end = location(of: "- first item", in: harness) + 12
        caret(harness, at: end)
        XCTAssertFalse(change(harness, NSRange(location: end, length: 0), "\n"), "the core makes Return's edit")
        XCTAssertTrue(harness.shown.contains("- first item\n- \n- [ ] a task"))
        XCTAssertEqual(harness.view.selectedRange.location, end + 3)
        XCTAssertEqual(coreText(harness), harness.shown)
        type(harness, "second")
        XCTAssertTrue(harness.shown.contains("- first item\n- second\n"))
        // A task continues as a task; Return on the empty one ends the list.
        let task = location(of: "- [ ] a task", in: harness) + 12
        caret(harness, at: task)
        change(harness, NSRange(location: task, length: 0), "\n")
        XCTAssertTrue(harness.shown.contains("- [ ] a task\n- [ ] \n"))
        let empty = harness.view.selectedRange
        change(harness, empty, "\n")
        XCTAssertTrue(harness.shown.contains("- [ ] a task\n\n"))
        XCTAssertFalse(harness.shown.contains("- [ ] \n"))
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testReturnContinuesAQuoteAndNumbers() throws {
        let harness = try harness(text: "1. one\n\n> said\n")
        caret(harness, at: 6)
        change(harness, NSRange(location: 6, length: 0), "\n")
        XCTAssertTrue(harness.shown.hasPrefix("1. one\n2. "))
        let quote = location(of: "> said", in: harness) + 6
        caret(harness, at: quote)
        change(harness, NSRange(location: quote, length: 0), "\n")
        XCTAssertTrue(harness.shown.hasSuffix("> said\n>\n> \n"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testReturnInCodeIsANewlineAndAFenceAboveCodeDoesNotClose() throws {
        let harness = try harness(text: "a\n\n```\ncode\n```\n")
        let code = location(of: "code", in: harness) + 4
        caret(harness, at: code)
        change(harness, NSRange(location: code, length: 0), "\n")
        XCTAssertTrue(harness.shown.contains("code\n\n```"))
        // A fence typed above a code block pairs with its opening fence:
        // Return types a newline.
        caret(harness, at: 1)
        type(harness, "\n```")
        let at = harness.view.selectedRange.location
        change(harness, NSRange(location: at, length: 0), "\n")
        XCTAssertTrue(harness.shown.hasPrefix("a\n\n```\n\n"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    /// ADR 0109's typing helpers: a fence typed where nothing closes it
    /// closes itself, the caret on the line between, one undo step.
    func testATypedFenceClosesItself() throws {
        let harness = try harness(text: "a")
        caret(harness, at: harness.string.length)
        type(harness, "\n```swift")
        XCTAssertEqual(harness.shown, "a\n\n```swift")
        let at = harness.view.selectedRange.location
        change(harness, NSRange(location: at, length: 0), "\n")
        XCTAssertTrue(harness.shown.hasSuffix("```swift\n\n```"), harness.shown)
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.length - 4)
        XCTAssertEqual(coreText(harness), harness.shown)
        harness.model.undoTapped()
        show(harness)
        XCTAssertTrue(harness.shown.hasSuffix("```swift"), harness.shown)
    }

    func testBackspaceJoinsParagraphsAndEmptiesAPair() throws {
        let harness = try harness(text: "one\n\ntwo **b** z\n")
        caret(harness, at: 5)
        backspace(harness)
        XCTAssertEqual(harness.shown, "onetwo **b** z\n", "one ⌫ undoes Return's paragraph break")
        XCTAssertEqual(harness.view.selectedRange.location, 3)
        // The caret on the bold's letter reveals it; then after the letter.
        let b = location(of: "b**", in: harness)
        caret(harness, at: b)
        caret(harness, at: b + 1)
        backspace(harness)
        XCTAssertEqual(harness.shown, "onetwo  z\n", "the emptied bold goes with its letter")
        XCTAssertEqual(coreText(harness), harness.shown)
        // Each is one step on the undo line.
        harness.model.undoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, "onetwo **b** z\n")
    }

    func testDeleteTakesAnImageLineWholeAndLeavesTablesAlone() throws {
        let harness = try harness()
        let image = location(of: "![](scan.png)", in: harness)
        let end = image + 13
        caret(harness, at: end)
        backspace(harness)
        XCTAssertFalse(harness.shown.contains("scan.png"), "the image's line goes whole")
        XCTAssertEqual(coreText(harness), harness.shown)
        // ⌫ at the start of the line after a table would join it: nothing.
        let harness2 = try self.harness(text: "| A |\n| - |\n| 1 |\nnext\n")
        let next = location(of: "next", in: harness2)
        let before = harness2.shown
        harness2.view.selectedRange = NSRange(location: next, length: 0)
        change(harness2, NSRange(location: next - 1, length: 1), "")
        XCTAssertEqual(harness2.shown, before)
    }

    func testTypingAfterATableAtTheEndStartsAParagraph() throws {
        let harness = try harness(text: "a\n\n| A |\n| - |\n| 1 |")
        let end = harness.string.length
        caret(harness, at: end)
        XCTAssertEqual(harness.view.selectedRange.location, end, "the end of the text is not in a cell")
        XCTAssertFalse(change(harness, NSRange(location: end, length: 0), "x"))
        XCTAssertEqual(harness.shown, "a\n\n| A |\n| - |\n| 1 |\n\nx", "the row keeps its cells")
        XCTAssertEqual(harness.view.selectedRange.location, end + 3)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testDeletesBesideATableAtTheStartStillWork() throws {
        let harness = try harness(text: "| A |\n| - |\n| 1 |\nnext\n")
        let next = location(of: "next", in: harness)
        caret(harness, at: next)
        caret(harness, at: next + 1)
        backspace(harness)
        XCTAssertEqual(harness.shown, "| A |\n| - |\n| 1 |\next\n")
        caret(harness, at: next)
        change(harness, NSRange(location: next, length: 1), "")
        XCTAssertEqual(harness.shown, "| A |\n| - |\n| 1 |\nxt\n")
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    /// Show Markdown switched after typing, before the pause's snapshot:
    /// the view restyles its own text in the new look at once.
    func testShowMarkdownSwitchedBeforeThePauseRestyles() throws {
        let harness = try harness(text: "Some **bold** words.\n")
        caret(harness, at: 0)
        type(harness, "x")
        show(harness, rendered: false)
        let storage = harness.view.textStorage
        var hidden = false
        storage.enumerateAttribute(.mdocHidden, in: NSRange(location: 0, length: storage.length)) { value, _, _ in
            if value != nil { hidden = true }
        }
        XCTAssertFalse(hidden, "Show Markdown hides nothing")
        let colour = storage.attribute(.foregroundColor, at: location(of: "**", in: harness), effectiveRange: nil) as? UIColor
        XCTAssertEqual(colour, MarkdownStyler.marker)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testStructuralKeysInShowMarkdown() throws {
        let harness = try harness(text: "- a\n", rendered: false)
        caret(harness, at: 3)
        XCTAssertFalse(change(harness, NSRange(location: 3, length: 0), "\n"))
        XCTAssertEqual(harness.shown, "- a\n- \n")
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testOutcomesAreOneReplacement() {
        let text = "- a\n- " as NSString
        XCTAssertEqual(
            StructuralKeys.outcome(.enter, text: "- a", selection: NSRange(location: 3, length: 0), native: NSRange(location: 3, length: 0), rendered: true),
            .replace(range: NSRange(location: 3, length: 0), text: "\n- ", selection: NSRange(location: 6, length: 0)))
        XCTAssertEqual(
            StructuralKeys.outcome(.enter, text: text, selection: NSRange(location: 6, length: 0), native: NSRange(location: 6, length: 0), rendered: true),
            .replace(range: NSRange(location: 4, length: 2), text: "\n", selection: NSRange(location: 5, length: 0)))
        // A plain ⌫ is UIKit's own; past a surrogate pair the offsets stay UTF-16.
        XCTAssertEqual(
            StructuralKeys.outcome(.backspace, text: "ab", selection: NSRange(location: 2, length: 0), native: NSRange(location: 1, length: 1), rendered: true),
            .native)
        XCTAssertEqual(
            StructuralKeys.outcome(.enter, text: "😀 - x", selection: NSRange(location: 6, length: 0), native: NSRange(location: 6, length: 0), rendered: true),
            .replace(range: NSRange(location: 6, length: 0), text: "\n\n", selection: NSRange(location: 8, length: 0)))
        XCTAssertNil(StructuralKey(range: NSRange(location: 0, length: 3), text: "", selection: NSRange(location: 3, length: 0), in: "abc"),
                     "holding ⌫ deletes a word: UIKit's")
    }

    // MARK: Incremental restyle

    /// Each keystroke restyles its block and the caret's lines, and the
    /// view ends up exactly as a whole restyle draws it.
    func testARestyleTouchesOnlyTheBlockAndMatchesAWholeOne() throws {
        let paragraphs = (1...40).map { "Paragraph \($0) has **bold** and *italic* words." }.joined(separator: "\n\n")
        let harness = try harness(text: Self.text + "\n\n" + paragraphs + "\n")
        let rebuilds = harness.coordinator.rebuilds
        let target = location(of: "Paragraph 20 has", in: harness) + 10
        caret(harness, at: target)
        type(harness, "x**y**")
        XCTAssertEqual(harness.coordinator.rebuilds, rebuilds, "no whole rebuild per keystroke")
        let styled = harness.coordinator.livePreview.lastStyled
        XCTAssertFalse(styled.isEmpty)
        XCTAssertTrue(styled.allSatisfy { $0.length < 120 }, "only the paragraph: \(styled)")
        assertMatchesAWholeRestyle(harness)
    }

    /// Random edits and caret moves in live preview: the core's text stays
    /// the view's string, saves write exactly it, and the attributes stay a
    /// whole restyle's (ADR 0109's acceptance).
    func testTypingFuzzInLivePreview() throws {
        let harness = try harness()
        var random = SeededRandom(seed: 0x6c697665)
        let pieces = ["a", " ", "\n", "**", "*", "#", "# ", "- ", "> ", "`", "```", "[x](y)", "é", "😀", "|", "---"]
        for step in 1...300 {
            let length = harness.string.length
            func boundary(_ value: Int) -> Int {
                guard value < length else { return length }
                return harness.string.rangeOfComposedCharacterSequence(at: value).location
            }
            let at = boundary(random.next(below: length + 1))
            caret(harness, at: at)
            let caret = harness.view.selectedRange.location
            let op = random.next(below: 10)
            switch op {
            case 0...4:
                type(harness, pieces[random.next(below: pieces.count)])
            case 5, 6:
                if caret > 0 { backspace(harness) }
            case 7:
                if caret < harness.string.length {
                    change(harness, harness.string.rangeOfComposedCharacterSequence(at: caret), "")
                }
            case 8:
                harness.model.undoTapped()
                show(harness)
            default:
                harness.model.redoTapped()
                show(harness)
            }
            let core = harness.session.snapshot()
            if core.markdown != harness.shown {
                XCTFail("step \(step) op \(op) at \(at) caret \(caret): core \(core.markdown.debugDescription) view \(harness.shown.debugDescription)")
                return
            }
            if harness.coordinator.content?.textRevision != core.textRevision {
                XCTFail("step \(step) op \(op) at \(at) caret \(caret): the shown revision lags the core's, \(core.markdown.debugDescription)")
                return
            }
            if step % 50 == 0 {
                try harness.session.saveChanges()
                let path = try XCTUnwrap((harness.session as? RealSession)?.path)
                let saved = try Data(contentsOf: URL(fileURLWithPath: path))
                XCTAssertEqual(saved, Data(harness.shown.utf8), "saved bytes at step \(step)")
                assertMatchesAWholeRestyle(harness, "step \(step)")
            }
        }
    }

    /// The view's attributes are what a whole restyle of its text gives,
    /// with the same reveal.
    func assertMatchesAWholeRestyle(_ harness: Harness, _ message: String = "", file: StaticString = #filePath, line: UInt = #line) {
        let text = harness.shown
        let preview = harness.coordinator.livePreview
        var styler = MarkdownStyler(
            traits: harness.view.traitCollection, selected: nil, scoped: [], rendered: harness.rendered,
            width: harness.view.textWidth)
        styler.editing = true
        // Tables keep their columns while their cells are typed into.
        styler.keptTables = Dictionary(
            harness.view.tableViews.map { ($0.table.anchor, $0.table) }, uniquingKeysWith: { first, _ in first })
        let styling = MarkdownStyling(markdown: text)
        styler.revealed = preview.visible + (preview.caretLines.map { LivePreview.shownLines(in: styling, caret: $0) } ?? [])
        styler.caretLine = preview.caretLine
        let expected = styler.document(markdown: text, styling: styling, candidates: preview.candidates).text
        let actual = harness.view.textStorage
        XCTAssertEqual(expected.length, actual.length, file: file, line: line)
        for index in 0..<min(expected.length, actual.length) {
            let want = Self.summary(expected.attributes(at: index, effectiveRange: nil))
            let got = Self.summary(actual.attributes(at: index, effectiveRange: nil))
            if want != got {
                let around = (text as NSString).substring(with: (text as NSString).lineRange(for: NSRange(location: index, length: 0)))
                XCTFail("\(message) at \(index) in \(around.debugDescription): want \(want), got \(got)", file: file, line: line)
                return
            }
        }
    }

    /// What a character's attributes draw, comparable.
    static func summary(_ attributes: [NSAttributedString.Key: Any]) -> String {
        let traits = UITraitCollection(userInterfaceStyle: .light)
        func colour(_ key: NSAttributedString.Key) -> String {
            guard let colour = attributes[key] as? UIColor else { return "-" }
            var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            colour.resolvedColor(with: traits).getRed(&r, green: &g, blue: &b, alpha: &a)
            return String(format: "%.2f,%.2f,%.2f,%.2f", r, g, b, a)
        }
        let font = (attributes[.font] as? UIFont).map { "\($0.fontName) \($0.pointSize)" } ?? "-"
        let paragraph = (attributes[.paragraphStyle] as? NSParagraphStyle).map {
            "\($0.firstLineHeadIndent) \($0.headIndent) \($0.minimumLineHeight) \($0.maximumLineHeight) \($0.paragraphSpacing) \($0.paragraphSpacingBefore)"
        } ?? "-"
        let decoration = (attributes[.mdocDecoration] as? LineDecoration).map {
            "label:\($0.label?.text.string ?? "-") task:\($0.task.map { "\($0.checked)" } ?? "-") bar:\($0.bar != nil) card:\($0.card.map { "\($0.top)\($0.bottom)" } ?? "-") title:\($0.title?.text.string ?? "-") rule:\($0.rule != nil) chips:\($0.chips.count)"
        } ?? "-"
        return [
            font, colour(.foregroundColor), colour(.backgroundColor), "\(attributes[.underlineStyle] ?? "-")",
            "\(attributes[.mdocHidden] != nil)", paragraph, decoration, "\(attributes[.kern] ?? "-")",
        ].joined(separator: " | ")
    }
}
