import XCTest

/// Table editing in live preview (ADR 0109, 0110): a tap on a drawn cell
/// puts the caret there and typing goes into that cell; the toolbar shows
/// the table controls, whose Row below adds a row and whose ⋯ menu holds
/// the rest; Aa is the formatting keyboard in a cell too; the
/// accessibility audit (opt-in) passes with the table controls up.
final class TableEditingUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Opens the Markdown fixture in edit mode in live preview, the
    /// keyboard hidden so its table shows, and taps the cell holding
    /// `cell`: the caret goes in it and the keyboard comes up.
    @MainActor
    static func editCell(_ cell: String, contentSize: String? = nil) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize { app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize] }
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(MdocUITest.markdownFixture).path
        MdocUITest.usePrebuiltLibrary(app)
        app.launch()
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 30))
        edit.tap()
        // No selection: the keyboard waits for the tap in the cell (ADR 0110).
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        let target = Self.cell(app, cell)
        // The cells take no touches in edit mode (the grid does): the
        // cell is found, and the grid under it tapped.
        XCTAssertTrue(target.waitForExistence(timeout: 10), "the cell \(cell) shows")
        // At a large text size the table is further down: the text is
        // dragged up until the cell is clear of the bars.
        let text = app.textViews["document-text"]
        let clear = app.windows.firstMatch.frame.insetBy(dx: 0, dy: 120)
        for _ in 0..<8 where !clear.contains(target.frame) {
            text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.05, thenDragTo: text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)))
        }
        XCTAssertTrue(app.windows.firstMatch.frame.contains(target.frame), "the cell \(cell) is on screen")
        // Past the text's end: the caret at the end of the cell's text.
        target.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "the keyboard comes back")
        return (app, target)
    }

    /// The grid's cell whose text is `text`: a text view of its own.
    @MainActor
    static func cell(_ app: XCUIApplication, _ text: String) -> XCUIElement {
        app.textViews.matching(NSPredicate(format: "value == %@", text)).firstMatch
    }

    /// Done, then edit mode again with the caret still in the cell: the new
    /// toolbar starts scrolled to ⋯ as well (review finding on #783; its own
    /// test until ADR 0112's sweep, which saved the launch). Then typing
    /// goes into the cell, Tab moves to the next, Row below adds a row
    /// with the caret in it, and the toolbar's Undo takes both back.
    @MainActor
    func testTypingIntoACellAndAddingARow() throws {
        let (app, target) = Self.editCell("Депозит")
        let more = app.buttons["table-more"]
        let shown = { more.exists && more.frame.maxX <= app.buttons["toolbar-formatting"].frame.minX - 4 }
        XCTAssertTrue(MdocUITest.wait(timeout: MdocUITest.timeout, until: shown), "⋯ shows whole")
        app.buttons["done-editing"].tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].wait(until: NSPredicate(format: "exists == false"), timeout: 5))
        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        if !app.buttons["toolbar-bold"].waitForExistence(timeout: 2) {
            target.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        }
        XCTAssertTrue(app.buttons["table-row-below"].waitForExistence(timeout: 10), "the caret is in the cell again")
        XCTAssertTrue(MdocUITest.wait(timeout: MdocUITest.timeout, until: shown), "⋯ shows whole again")

        app.typeText(" paid")
        let typed = Self.cell(app, "Депозит paid")
        XCTAssertTrue(typed.waitForExistence(timeout: MdocUITest.timeout), "the typed text shows in its cell")
        XCTAssertTrue(Self.cell(app, "£1,725").exists, "the cell beside it is untouched")
        // Tab to the next cell, then back: the caret moves, nothing types.
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        app.typeText("!")
        XCTAssertTrue(Self.cell(app, "!£1,725").waitForExistence(timeout: MdocUITest.timeout), "Tab moves to the next cell's start")
        // The toolbar's table controls (ADR 0110): Row below adds an empty
        // row; they take the list and task buttons' place.
        let cells = app.textViews.count
        let below = app.buttons["table-row-below"]
        XCTAssertTrue(below.waitForExistence(timeout: MdocUITest.timeout), "the table controls in a cell")
        XCTAssertFalse(app.buttons["toolbar-list"].exists, "no list button in a cell")
        XCTAssertFalse(app.buttons["toolbar-task"].exists, "no task button in a cell")
        XCTAssertFalse(app.buttons["toolbar-indent"].exists, "no indent in a cell, where it never applies")
        // The capsule scrolled to its end: ⋯ shows whole, left of Aa.
        XCTAssertTrue(
            MdocUITest.wait(timeout: MdocUITest.timeout) { more.frame.maxX <= app.buttons["toolbar-formatting"].frame.minX - 4 },
            "⋯ shows whole")
        for id in ["table-row-below", "table-column-right", "table-more"] {
            let frame = app.buttons[id].frame
            XCTAssertGreaterThanOrEqual(min(frame.width, frame.height), 44, id)
        }
        // Undo and Redo have a capsule of their own: they show whole in a
        // cell too, left of the buttons that scroll.
        let undo = app.buttons["toolbar-undo"].frame, redo = app.buttons["toolbar-redo"].frame
        XCTAssertGreaterThanOrEqual(undo.minX, 4, "Undo shows whole")
        XCTAssertEqual(redo.minX, undo.maxX, accuracy: 1, "Redo beside it")
        XCTAssertGreaterThanOrEqual(min(undo.width, redo.width), 44)
        below.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: MdocUITest.timeout) { app.textViews.count == cells + 2 }, "a row of two cells")
        // The caret is in the new row, in the same column (ADR 0110).
        app.typeText("Z")
        let typedZ = Self.cell(app, "Z")
        XCTAssertTrue(typedZ.waitForExistence(timeout: MdocUITest.timeout), "typed into a cell")
        let above = Self.cell(app, "!£1,725").frame
        XCTAssertEqual(typedZ.frame.minX, above.minX, accuracy: 1, "in the same column")
        XCTAssertEqual(typedZ.frame.minY, above.maxY, accuracy: 1, "in the new row below")
        // The toolbar's Undo takes the typing, then the row back.
        app.buttons["toolbar-undo"].tap()
        XCTAssertTrue(typedZ.wait(until: NSPredicate(format: "exists == false"), timeout: 5), "Undo takes the typing back")
        app.buttons["toolbar-undo"].tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { app.textViews.count == cells }, "Undo takes the row back")
        XCTAssertTrue(Self.cell(app, "!£1,725").exists)
    }

    /// The ⋯ menu (ADR 0110): its items, Delete row off in the header, the
    /// column's alignment checked in Align column, and Delete column
    /// taking the caret's column.
    @MainActor
    func testTheTableMenu() throws {
        let (app, _) = Self.editCell("Сумма")
        let more = app.buttons["table-more"]
        XCTAssertTrue(more.waitForExistence(timeout: MdocUITest.timeout))
        more.tap()
        let deleteTable = app.buttons["Delete table"]
        XCTAssertTrue(deleteTable.waitForExistence(timeout: MdocUITest.timeout), "the ⋯ menu opens")
        for item in ["Row above", "Column left", "Delete row", "Delete column", "Align column"] {
            XCTAssertTrue(app.buttons[item].exists, item)
        }
        XCTAssertFalse(app.buttons["Row above"].isEnabled, "nothing goes above the header")
        XCTAssertFalse(app.buttons["Delete row"].isEnabled, "the header stays")
        // Align column: the column's alignment is checked.
        app.buttons["Align column"].tap()
        let right = app.buttons["Right"]
        XCTAssertTrue(right.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(right.isSelected, "the amount column is right-aligned")
        XCTAssertFalse(app.buttons["Left"].isSelected)
        right.tap()
        XCTAssertTrue(right.wait(until: NSPredicate(format: "exists == false"), timeout: 5))
        // Delete column: the caret's column goes, the keyboard stays.
        let cells = app.textViews.count
        more.tap()
        XCTAssertTrue(app.buttons["Delete column"].waitForExistence(timeout: MdocUITest.timeout))
        app.buttons["Delete column"].tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { app.textViews.count < cells }, "the column is gone")
        XCTAssertFalse(Self.cell(app, "Сумма").exists)
        XCTAssertTrue(app.buttons["toolbar-bold"].exists, "the toolbar stays up")
    }

    /// Aa in a cell (ADR 0110): the ordinary formatting keyboard, its
    /// formats on and its blocks off; Strikethrough works in the cell.
    @MainActor
    func testTheFormattingKeyboardInACell() throws {
        let (app, _) = Self.editCell("Депозит")
        app.buttons["toolbar-formatting"].tap()
        let strike = app.buttons["format-strikethrough"]
        XCTAssertTrue(strike.waitForExistence(timeout: MdocUITest.timeout), "the formatting keyboard in a cell")
        for id in ["format-strikethrough", "format-code", "format-highlight", "format-link"] {
            XCTAssertTrue(app.buttons[id].isEnabled, id)
        }
        // Insert is below the fold; TableEditingTests checks Rule and Table.
        for id in ["turn-text", "turn-heading1", "turn-quote"] {
            XCTAssertFalse(app.buttons[id].isEnabled, id)
        }
        XCTAssertFalse(app.buttons["formatting-keyboard-return"].exists, "no keyboard key")
        XCTAssertTrue(app.buttons["table-row-below"].exists, "the table controls stay in the toolbar")
        strike.tap()
        // The caret is in struck-through text now: the key shows on.
        XCTAssertTrue(strike.wait(until: NSPredicate(format: "isSelected == true"), timeout: MdocUITest.timeout), "strikethrough in the cell")
        XCTAssertTrue(app.buttons["toolbar-undo"].isEnabled, "one edit made")
    }

    /// At the largest accessibility text size the capsule scrolls in a
    /// cell too: entering the cell scrolls it to ⋯, Undo is reached by
    /// swiping it back, and Aa and hide keyboard stay.
    @MainActor
    func testTheTableControlsScrollAtAnAccessibilitySize() throws {
        let (app, _) = Self.editCell("Депозит", contentSize: "UICTContentSizeCategoryAccessibilityXXXL")
        let more = app.buttons["table-more"]
        XCTAssertTrue(more.waitUntilHittable(timeout: MdocUITest.timeout), "the capsule scrolls to ⋯ in a cell")
        let undo = app.buttons["toolbar-undo"]
        let aa = app.buttons["toolbar-formatting"].frame
        let origin = app.windows.firstMatch.coordinate(withNormalizedOffset: .zero)
        let from = origin.withOffset(CGVector(dx: 30, dy: aa.midY))
        let to = origin.withOffset(CGVector(dx: aa.minX - 30, dy: aa.midY))
        var tries = 0
        while !undo.isHittable && tries < 8 {
            from.press(forDuration: 0.05, thenDragTo: to)
            tries += 1
        }
        XCTAssertTrue(undo.isHittable, "Undo is reached by scrolling the capsule back")
        XCTAssertTrue(app.buttons["toolbar-formatting"].isHittable)
        XCTAssertTrue(app.buttons["toolbar-hide-keyboard"].isHittable)
    }

    /// The accessibility audit with the table controls up (ADR 0110): no
    /// issue on the controls. Issues elsewhere (the system keyboard) are
    /// printed. Opt-in, as the repository's audits are: hosted ci/ios cannot
    /// finish it in time; run with `TEST_RUNNER_MDOC_A11Y_EXPLORE=1`.
    @MainActor
    func testTheAccessibilityAuditWithTheTableControlsUp() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_A11Y_EXPLORE"] == "1", "exploration only")
        let (app, _) = Self.editCell("Депозит")
        XCTAssertTrue(app.buttons["table-row-below"].waitForExistence(timeout: MdocUITest.timeout))
        var found: [String] = []
        try app.performAccessibilityAudit(for: [.dynamicType, .elementDetection, .sufficientElementDescription, .textClipped, .trait, .hitRegion]) { issue in
            let identifier = issue.element?.identifier ?? ""
            let line = "\(issue.auditType) | \(issue.compactDescription) | \(issue.element?.label ?? "-") id=\(identifier)"
            print("A11Y[table-controls] \(line)")
            let ours = ["toolbar-", "table-", "format-", "formatting-"].contains { identifier.hasPrefix($0) }
            if ours { found.append(line) }
            return true
        }
        XCTAssertEqual(found, [], "issues on the table controls")
    }
}
