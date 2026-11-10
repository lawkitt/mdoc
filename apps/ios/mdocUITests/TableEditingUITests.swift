import XCTest

/// Table editing in live preview (ADR 0109, slice 5): a tap on a drawn
/// cell puts the caret there and typing goes into that cell; Aa shows the
/// table controls, whose Row below adds a row; the accessibility audit
/// (opt-in) passes with the table controls up.
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
        app.launch()
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 30))
        edit.tap()
        // No selection: the keyboard waits for the tap in the cell (ADR 0110).
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        let target = Self.cell(app, cell)
        // The cells take no touches in edit mode (the grid does): the
        // cell is found, and the grid under it tapped.
        XCTAssertTrue(target.waitForExistence(timeout: 10), "the cell \(cell) shows")
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

    @MainActor
    func testTypingIntoACellAndAddingARow() throws {
        let (app, _) = Self.editCell("Депозит")
        app.typeText(" paid")
        let typed = Self.cell(app, "Депозит paid")
        XCTAssertTrue(typed.waitForExistence(timeout: 5), "the typed text shows in its cell")
        XCTAssertTrue(Self.cell(app, "£1,725").exists, "the cell beside it is untouched")
        // Tab to the next cell, then back: the caret moves, nothing types.
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        app.typeText("!")
        XCTAssertTrue(Self.cell(app, "!£1,725").waitForExistence(timeout: 5), "Tab moves to the next cell's start")
        // Aa: the table controls; Row below adds an empty row.
        let cells = app.textViews.count
        app.buttons["toolbar-formatting"].tap()
        let below = app.buttons["table-row-below"]
        XCTAssertTrue(below.waitForExistence(timeout: 5), "the table controls in a cell")
        XCTAssertFalse(app.buttons["turn-heading1"].exists, "no formatting keys in a cell")
        below.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { app.textViews.count == cells + 2 }, "a row of two cells")
        // The toolbar's Undo takes the row back, one step.
        app.buttons["toolbar-undo"].tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { app.textViews.count == cells }, "Undo takes the row back")
        XCTAssertTrue(Self.cell(app, "!£1,725").exists)
    }

    /// The accessibility audit with the table controls up (ADR 0109): no
    /// issue on the controls. Issues elsewhere (the system keyboard) are
    /// printed. Opt-in, as the repository's audits are: hosted ci/ios cannot
    /// finish it in time; run with `TEST_RUNNER_MDOC_A11Y_EXPLORE=1`.
    @MainActor
    func testTheAccessibilityAuditWithTheTableControlsUp() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_A11Y_EXPLORE"] == "1", "exploration only")
        let (app, _) = Self.editCell("Депозит")
        app.buttons["toolbar-formatting"].tap()
        XCTAssertTrue(app.buttons["table-row-below"].waitForExistence(timeout: 5))
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
