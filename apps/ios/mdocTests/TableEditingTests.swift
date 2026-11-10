import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// Table editing in live preview (ADR 0109, slice 5), on the real core: a
/// tap puts the caret in a cell; typing, ⌫ and paste change only that
/// cell's bytes, through the typing path, and the grid lays out the edited
/// row in place; Return, Tab and the arrows move between cells; each table
/// control is one core edit and one undo step.
@MainActor
final class TableEditingTests: XCTestCase {
    static let text = """
        # Rent

        | Item | Amount |
        | --- | ---: |
        | Rent | £1,150 |
        | Deposit | £1,725 |

        After the table.
        """

    typealias Harness = EditToolbarTests.Harness

    func harness(text: String = text, rendered: Bool = true) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Rent", into: .newCase(name: "Rent"))
        _ = try session.recognize(observer: TestObserver())
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: LivePreviewTests.textView(model, rendered: rendered))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(
            session: session, model: model, window: window, view: view, coordinator: coordinator, rendered: rendered)
        show(harness)
        model.enterEditMode()
        show(harness)
        XCTAssertTrue(view.isEditMode)
        return harness
    }

    func show(_ harness: Harness) {
        harness.coordinator.update(
            harness.view, with: LivePreviewTests.textView(harness.model, rendered: harness.rendered),
            dynamicTypeSize: .large, colorScheme: .light)
        harness.view.layoutIfNeeded()
    }

    func select(_ harness: Harness, _ range: NSRange) {
        harness.view.selectedRange = range
        harness.coordinator.textViewDidChangeSelection(harness.view)
        harness.coordinator.refreshToolbar(harness.view)
    }

    /// `act` is one core edit and one Formatting step: Undo takes back
    /// exactly it, Redo brings it again.
    func assertOneStep(_ harness: Harness, file: StaticString = #filePath, line: UInt = #line, _ act: () -> Void) {
        let before = harness.shown
        let sent = harness.coordinator.bridge.sent
        act()
        let after = harness.shown
        XCTAssertNotEqual(after, before, "nothing changed", file: file, line: line)
        XCTAssertEqual(harness.coordinator.bridge.sent, sent + 1, "one core edit", file: file, line: line)
        XCTAssertEqual(coreText(harness), after, file: file, line: line)
        XCTAssertEqual(harness.session.snapshot().undoStep?.label, "Formatting", file: file, line: line)
        harness.model.undoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, before, "one undo step", file: file, line: line)
        harness.model.redoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, after, file: file, line: line)
    }

    func caret(_ harness: Harness, at needle: String, offset: Int = 0) {
        select(harness, NSRange(location: harness.string.range(of: needle).location + offset, length: 0))
    }

    func coreText(_ harness: Harness) -> String {
        harness.session.snapshot().markdown
    }

    /// One change as UIKit makes it; false when edit mode made it itself.
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
            change(harness, harness.view.selectedRange, String(character))
        }
    }

    func backspace(_ harness: Harness) {
        let caret = harness.view.selectedRange.location
        change(harness, harness.string.rangeOfComposedCharacterSequence(at: caret - 1), "")
    }

    var grid: (Harness) throws -> MarkdownTableView = { harness in
        try XCTUnwrap(harness.view.tableViews.first)
    }

    func tap(_ harness: Harness, row: Int, column: Int) throws {
        let table = try grid(harness)
        let cell = table.cells[row][column]
        harness.view.onTableTap?(table, CGPoint(x: cell.frame.maxX - 2, y: cell.frame.midY))
        harness.coordinator.textViewDidChangeSelection(harness.view)
        harness.coordinator.refreshToolbar(harness.view)
    }

    /// The text outside `span` (UTF-16, in `before`) is the same in `after`.
    func assertOnlyInside(_ span: NSRange, _ before: String, _ after: String, file: StaticString = #filePath, line: UInt = #line) {
        let old = before as NSString, new = after as NSString
        let grown = new.length - old.length
        XCTAssertEqual(new.substring(to: span.location), old.substring(to: span.location), "before the cell", file: file, line: line)
        XCTAssertEqual(
            new.substring(from: NSMaxRange(span) + grown), old.substring(from: NSMaxRange(span)), "after the cell",
            file: file, line: line)
    }

    // MARK: Cells

    func testATapPutsTheCaretInTheCellAndTheToolbarKnowsIt() throws {
        let harness = try harness()
        try tap(harness, row: 2, column: 0)
        let deposit = harness.string.range(of: "Deposit").location
        XCTAssertEqual(harness.view.selectedRange, NSRange(location: deposit + 7, length: 0))
        let cell = try XCTUnwrap(harness.toolbar.state?.cell)
        XCTAssertEqual([cell.row, cell.column, cell.rows, cell.columns], [2, 0, 3, 2])
        XCTAssertEqual(try grid(harness).cell(at: deposit + 7)?.offset, 7)
        // The caret is drawn in the cell, not where the hidden source lies.
        let rect = harness.view.caretRect(for: try XCTUnwrap(harness.view.selectedTextRange?.end))
        let frame = try grid(harness).cells[2][0].convert(try grid(harness).cells[2][0].bounds, to: harness.view)
        XCTAssertTrue(frame.insetBy(dx: -1, dy: -1).contains(CGPoint(x: rect.midX, y: rect.midY)), "\(rect) in \(frame)")
        // Blocks are off in a cell; formats and Aa's table controls are on.
        XCTAssertFalse(harness.toolbar.isEnabled(.list))
        XCTAssertFalse(harness.toolbar.isEnabled(.task))
        XCTAssertFalse(harness.toolbar.isEnabled(.indent))
        XCTAssertTrue(harness.toolbar.isEnabled(.format(.bold)))
        XCTAssertTrue(harness.toolbar.inCell)
    }

    func testTypingChangesOnlyTheCellAndRelaysOutItsRowInPlace() throws {
        let harness = try harness()
        let table = try grid(harness)
        let columns = table.table.columns
        let otherCell = table.cells[1][1]
        let otherText = otherCell.attributedText
        try tap(harness, row: 2, column: 0)
        let span = try XCTUnwrap(table.span(of: table.cells[2][0]))
        let before = harness.shown
        let sent = harness.coordinator.bridge.sent
        type(harness, " box")
        XCTAssertEqual(harness.coordinator.bridge.sent, sent + 4, "each key one core edit on the typing path")
        XCTAssertEqual(harness.session.snapshot().undoStep?.label, "Typing")
        XCTAssertTrue(harness.shown.contains("| Deposit box | £1,725 |"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
        assertOnlyInside(span, before, harness.shown)
        // The same grid, its columns kept, the cell showing what was typed.
        XCTAssertTrue(try grid(harness) === table, "the grid is updated in place")
        XCTAssertEqual(table.table.columns, columns)
        XCTAssertEqual(table.cells[2][0].textStorage.string, "Deposit box")
        XCTAssertTrue(table.cells[1][1] === otherCell)
        XCTAssertEqual(otherCell.attributedText, otherText, "other cells are left alone")
        // One undo takes the typing back.
        harness.model.undoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, before)
    }

    func testPipesBackslashesAndLineBreaksStayInTheCell() throws {
        let harness = try harness()
        try tap(harness, row: 1, column: 0)
        type(harness, "|")
        XCTAssertTrue(harness.shown.contains("| Rent\\| | £1,150 |"), harness.shown)
        // A line break arriving as text (dictation, a suggestion): `<br>`.
        XCTAssertFalse(change(harness, harness.view.selectedRange, "a\nb"))
        XCTAssertTrue(harness.shown.contains("| Rent\\|a<br>b | £1,150 |"), harness.shown)
        // Paste: one line, its pipes escaped, as desktop pastes into a cell.
        InAppClipboard.shared.write("x|y\nz")
        harness.view.paste(nil)
        XCTAssertTrue(harness.shown.contains("| Rent\\|a<br>bx\\|y z | £1,150 |"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
        let cell = try grid(harness).cells[1][0]
        XCTAssertEqual(cell.source(in: NSRange(location: 0, length: cell.textStorage.length)), "Rent\\|a<br>bx\\|y z")
        XCTAssertEqual(cell.breaks, [7], "the `<br>` draws a line break")
        let scan = MarkdownStyling(markdown: harness.shown)
        XCTAssertEqual(scan.tables.first?.rows.map(\.cells.count), [2, 2, 2], "every row keeps its cells")
    }

    func testBackspaceStopsAtTheCellsStart() throws {
        let harness = try harness()
        try tap(harness, row: 1, column: 1)
        let before = harness.shown
        for _ in 0..<12 { backspace(harness) }
        XCTAssertTrue(harness.shown.contains("| Rent |  |"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
        let span = try XCTUnwrap(try grid(harness).span(of: try grid(harness).cells[1][1]))
        XCTAssertEqual(span.length, 2, "the cell's spaces stay")
        XCTAssertTrue(before.contains("| Rent | £1,150 |"))
        // A word delete reaching past the pipe stops at the cell.
        try tap(harness, row: 2, column: 0)
        let caret = harness.view.selectedRange.location
        XCTAssertFalse(change(harness, NSRange(location: caret - 10, length: 10), ""))
        XCTAssertTrue(harness.shown.contains("\n| | £1,725 |") || harness.shown.contains("\n|  | £1,725 |"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    /// A selection from the text above into a cell, deleted: the text
    /// restyles whole, its grids as the core reads the tables.
    func testADeleteAcrossATablesEdgeRestylesTheWholeText() throws {
        let harness = try harness()
        let start = harness.string.range(of: "Rent\n").location
        let end = harness.string.range(of: "| Item").location + 3
        let range = NSRange(location: start, length: end - start)
        harness.view.selectedRange = range
        change(harness, range, "")
        XCTAssertTrue(harness.shown.hasPrefix("# tem |"), harness.shown)
        XCTAssertEqual(coreText(harness), harness.shown)
        XCTAssertEqual(harness.view.tableViews.count, MarkdownStyling(markdown: harness.shown).tables.count)
    }

    /// A selection in a cell deleted: the core deletes it, so a backslash
    /// left before the cell's border is doubled and the border stays one.
    func testDeletingASelectionKeepsTheCellsBorder() throws {
        let harness = try harness(text: "| A | B |\n| - | - |\n| \\word| b |")
        let word = harness.string.range(of: "word")
        harness.view.selectedRange = word
        harness.coordinator.textViewDidChangeSelection(harness.view)
        XCTAssertEqual(harness.view.selectedRange, word, "a selection in one cell is kept")
        XCTAssertFalse(change(harness, word, ""))
        XCTAssertTrue(harness.shown.hasSuffix("| \\\\| b |"), harness.shown)
        XCTAssertEqual(MarkdownStyling(markdown: harness.shown).tables.first?.rows.map(\.cells.count), [2, 2])
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    /// Text holding a whole table replaced (Select All, typing): the grid
    /// goes with it.
    func testReplacingTextHoldingATableTakesItsGrid() throws {
        let harness = try harness()
        let all = NSRange(location: 0, length: harness.string.length)
        harness.view.selectedRange = all
        change(harness, all, "x")
        XCTAssertEqual(harness.shown, "x")
        XCTAssertTrue(harness.view.tableViews.isEmpty, "the old grid is gone")
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    /// A `<br>` in a cell is drawn as a line break, its text the same
    /// length as the source; the caret steps over the tag whole.
    func testABreakInACellDrawsASecondLine() throws {
        let harness = try harness(text: "| A | B |\n| - | - |\n| one<br>two | b |")
        let table = try grid(harness)
        let cell = table.cells[1][0]
        XCTAssertEqual(cell.textStorage.length, ("one<br>two" as NSString).length)
        XCTAssertEqual(cell.breaks, [3])
        XCTAssertGreaterThan(table.table.heights[1], table.table.heights[0] * 1.5, "two lines")
        let tag = harness.string.range(of: "<br>").location
        select(harness, NSRange(location: tag + 2, length: 0))
        XCTAssertEqual(harness.view.selectedRange.location, tag + 4, "never inside the tag")
        let below = harness.view.caretRect(for: try XCTUnwrap(harness.view.selectedTextRange?.end))
        select(harness, NSRange(location: tag, length: 0))
        let above = harness.view.caretRect(for: try XCTUnwrap(harness.view.selectedTextRange?.end))
        XCTAssertGreaterThan(below.minY, above.maxY - 1, "after the tag the caret is on the second line")
        cell.allowsCopy = true
        XCTAssertEqual(cell.source(in: NSRange(location: 0, length: cell.textStorage.length)), "one<br>two")
    }

    // MARK: Moving between cells

    func testReturnTabAndArrowsMoveBetweenCells() throws {
        let harness = try harness()
        try tap(harness, row: 0, column: 0)
        let item = harness.string.range(of: "Item").location
        XCTAssertEqual(harness.view.selectedRange.location, item + 4)
        // Return: the cell below, the column kept (ADR 0060).
        XCTAssertFalse(change(harness, harness.view.selectedRange, "\n"))
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.range(of: "Rent |").location + 4)
        // Tab (from a hardware keyboard) and ⇧Tab.
        harness.view.indentKey()
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.range(of: "£1,150").location)
        harness.view.outdentKey()
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.range(of: "Rent |").location)
        // Arrows step a character, then over the pipe.
        harness.toolbar.run(.moveInTable(.left))
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.range(of: "Amount").location + 6)
        harness.toolbar.run(.moveInTable(.right))
        harness.toolbar.run(.moveInTable(.down))
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.range(of: "Deposit").location)
        XCTAssertEqual(harness.shown, Self.text, "moves edit nothing")
        // Tab from the last cell leaves the table, as Return from an empty
        // last row does (ADR 0110): a paragraph below it.
        harness.toolbar.run(.moveInTable(.next))
        harness.toolbar.run(.moveInTable(.next))
        XCTAssertTrue(harness.shown.contains("| £1,725 |\n\n\n"), harness.shown)
        XCTAssertNil(harness.view.caretCell)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testAWideTableScrollsToTheCaretsCell() throws {
        let wide = (1...8).map { "Column \($0) heading" }
        let text = "| \(wide.joined(separator: " | ")) |\n|\(String(repeating: " --- |", count: 8))\n| \(wide.joined(separator: " | ")) |"
        let harness = try harness(text: text)
        let table = try grid(harness)
        XCTAssertGreaterThan(table.contentSize.width, table.bounds.width, "the table scrolls sideways")
        try tap(harness, row: 0, column: 0)
        for _ in 0..<7 { harness.toolbar.run(.moveInTable(.next)) }
        XCTAssertEqual(harness.toolbar.state?.cell?.column, 7)
        XCTAssertGreaterThan(table.contentOffset.x, 0, "scrolled to the last column")
        let caret = harness.view.caretRect(for: try XCTUnwrap(harness.view.selectedTextRange?.end))
        XCTAssertTrue(table.frame.insetBy(dx: -1, dy: -1).contains(CGPoint(x: caret.midX, y: caret.midY)), "the caret shows")
        // Typing there keeps the grid scrolled where it is.
        let offset = table.contentOffset.x
        change(harness, harness.view.selectedRange, "x")
        XCTAssertEqual(table.contentOffset.x, offset)
    }

    // MARK: Table controls

    /// Each control: one core edit, one Formatting step, Undo takes back
    /// exactly it; the grid shows the new shape.
    func testEachTableControlIsOneEditAndOneUndoStep() throws {
        let controls: [(EditorTableCommand, String)] = [
            (.insertRowAbove, "| Rent | £1,150 |\n|  |  |\n| Deposit"),
            (.insertRowBelow, "| Deposit | £1,725 |\n|  |  |\n\nAfter"),
            (.duplicateRow, "| Deposit | £1,725 |\n| Deposit | £1,725 |"),
            (.deleteRow, "| --- | ---: |\n| Rent | £1,150 |\n\nAfter"),
            (.insertColumnLeft, "|  | Deposit | £1,725 |"),
            (.insertColumnRight, "| Deposit |  | £1,725 |"),
            (.deleteColumn, "| £1,725 |"),
            (.align(align: .center), "| :-: | --: |"),
            (.align(align: .right), "| --: | --: |"),
            (.deleteTable, "# Rent\n\n\nAfter the table."),
        ]
        for (command, expected) in controls {
            let harness = try harness()
            try tap(harness, row: 2, column: 0)
            XCTAssertTrue(harness.toolbar.isEnabled(.table(command)), "\(command)")
            assertOneStep(harness) { harness.toolbar.run(.table(command)) }
            XCTAssertTrue(harness.shown.contains(expected), "\(command): \(harness.shown)")
            let tables = MarkdownStyling(markdown: harness.shown).tables
            XCTAssertEqual(harness.view.tableViews.count, tables.count, "\(command)")
            if let table = tables.first {
                XCTAssertEqual(try grid(harness).cells.count, table.rows.count, "\(command): the grid's rows")
            }
        }
    }

    func testControlsThatCannotApplyAreOff() throws {
        let harness = try harness(text: "| Only |\n| --- |\n| one |")
        try tap(harness, row: 0, column: 0)
        XCTAssertFalse(harness.toolbar.isEnabled(.table(.insertRowAbove)), "nothing goes above the header")
        XCTAssertFalse(harness.toolbar.isEnabled(.table(.deleteRow)), "the header stays")
        XCTAssertFalse(harness.toolbar.isEnabled(.table(.deleteColumn)), "the last column stays")
        XCTAssertTrue(harness.toolbar.isEnabled(.table(.insertRowBelow)))
        XCTAssertTrue(harness.toolbar.isOn(.table(.align(align: .left))))
        XCTAssertFalse(harness.toolbar.isOn(.table(.align(align: .right))))
        // Outside a table the controls are off.
        let prose = try self.harness(text: "Just text.")
        caret(prose, at: "text")
        XCTAssertFalse(prose.toolbar.isEnabled(.table(.insertRowBelow)))
        XCTAssertFalse(prose.toolbar.inCell)
    }

    func testANewTableTakesTheCaretInItsFirstCell() throws {
        let harness = try harness(text: "Intro")
        caret(harness, at: "Intro", offset: 5)
        harness.toolbar.run(.insertTable)
        XCTAssertEqual(harness.toolbar.state?.cell?.row, 0)
        type(harness, "Name")
        harness.view.indentKey()
        type(harness, "Role")
        XCTAssertTrue(harness.shown.contains("| Name | Role |\n| --- | --- |\n|  |  |"), harness.shown)
        XCTAssertEqual(try grid(harness).cells[0].map(\.textStorage.string), ["Name", "Role"])
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    // MARK: Show Markdown, VoiceOver

    func testWithShowMarkdownOnTablesAreSourceText() throws {
        let harness = try harness(rendered: false)
        XCTAssertTrue(harness.view.tableViews.isEmpty)
        caret(harness, at: "Rent |", offset: 4)
        XCTAssertNil(harness.view.caretCell)
        type(harness, "|")
        XCTAssertTrue(harness.shown.contains("| Rent| | £1,150 |"), "typed as is: \(harness.shown)")
    }

    func testVoiceOverEditsACellWithItsRowAndColumn() throws {
        let harness = try harness()
        let table = try grid(harness)
        let cell = table.cells[2][1]
        XCTAssertEqual(cell.accessibilityRowRange(), NSRange(location: 2, length: 1))
        XCTAssertEqual(cell.accessibilityColumnRange(), NSRange(location: 1, length: 1))
        XCTAssertNotNil(cell.accessibilityHint)
        XCTAssertTrue(cell.accessibilityActivate())
        harness.coordinator.textViewDidChangeSelection(harness.view)
        XCTAssertEqual(harness.view.caretCell?.view, cell, "double-tap puts the caret in the cell")
        XCTAssertTrue(table.accessibilityHeaderElements(forColumn: 1)?.first === table.cells[0][1])
    }

    // MARK: The byte rule

    /// Random typing, deletes, pastes and moves in random cells: after each
    /// step only the bytes between that cell's pipes changed, every row
    /// keeps its cells, and the core's text is the view's string.
    func testCellTypingFuzzChangesOnlyThatCell() throws {
        let harness = try harness()
        var random = SeededRandom(seed: 0x7461626c)
        let pieces = ["a", " ", "|", "\\", "\\|", "я", "😀", "**b**", "x\ny", "`"]
        for step in 1...200 {
            let table = try grid(harness)
            let row = random.next(below: table.cells.count)
            let column = random.next(below: table.cells[row].count)
            try tap(harness, row: row, column: column)
            for _ in 0..<random.next(below: 4) { harness.toolbar.run(.moveInTable(.left)) }
            guard let hit = harness.view.caretCell else { continue }
            let span = try XCTUnwrap(hit.table.span(of: hit.view))
            let before = harness.shown
            let counts = MarkdownStyling(markdown: before).tables.first?.rows.map(\.cells.count)
            let op = random.next(below: 8)
            switch op {
            case 0...3:
                let piece = pieces[random.next(below: pieces.count)]
                if piece.contains("\n") || piece.count > 1 {
                    change(harness, harness.view.selectedRange, piece)
                } else {
                    type(harness, piece)
                }
            case 4, 5:
                if harness.view.selectedRange.location > 0 { backspace(harness) }
            case 6:
                InAppClipboard.shared.write(pieces[random.next(below: pieces.count)])
                harness.view.paste(nil)
            default:
                harness.toolbar.run(.moveInTable([.left, .right][random.next(below: 2)]))
            }
            assertOnlyInside(span, before, harness.shown)
            XCTAssertEqual(
                MarkdownStyling(markdown: harness.shown).tables.first?.rows.map(\.cells.count), counts,
                "step \(step) op \(op): \(harness.shown.debugDescription)")
            if coreText(harness) != harness.shown {
                XCTFail("step \(step) op \(op): core \(coreText(harness).debugDescription) view \(harness.shown.debugDescription)")
                return
            }
        }
    }
}
