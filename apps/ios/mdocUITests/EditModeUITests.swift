import XCTest

/// Edit mode with Show Markdown on (ADR 0109, slice 2): Edit, typing, ⌘Z
/// and ⇧⌘Z over the one undo line, Done, then a relaunch shows the text as
/// typed, saved by the core.
final class EditModeUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Words no fixture has, typed at the caret.
    static let marker = "Zephyrine notes "

    @MainActor
    func testTypeUndoLeaveAndRelaunch() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))

        // Edit is offered with Show Markdown on.
        XCTAssertFalse(app.buttons["edit-button"].exists, "the rendered view stays read-only in this slice")
        app.buttons["document-menu"].tap()
        let showMarkdown = app.descendants(matching: .any)["show-markdown"].firstMatch
        XCTAssertTrue(showMarkdown.waitForExistence(timeout: 5))
        showMarkdown.tap()
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        XCTAssertEqual(edit.label, "Edit text")
        edit.tap()

        // The keyboard comes up, the review bar stays away, Done shows.
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertEqual(done.label, "Done editing")
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.descendants(matching: .any)["review-bar"].firstMatch.exists)

        app.typeText(Self.marker)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", Self.marker), timeout: 5))

        // ⌘Z undoes the typing as one step; ⇧⌘Z makes it again.
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "Zephyrine"), timeout: 5))
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", Self.marker), timeout: 5))

        // Done, then mdoc leaves the foreground: the typing is saved.
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        XCUIDevice.shared.press(.home)
        app.terminate()

        // The same library, as the last launch left it.
        let again = XCUIApplication()
        again.launchArguments = ["-mdoc-ui-testing"]
        again.launchEnvironment["MDOC_KEEP_LIBRARY"] = "1"
        again.launch()
        let row = MdocUITest.documentRow(again)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        let reopened = again.textViews["document-text"]
        XCTAssertTrue(reopened.waitForExistence(timeout: 10))
        XCTAssertTrue(reopened.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine notes"), timeout: 10))
    }
}
