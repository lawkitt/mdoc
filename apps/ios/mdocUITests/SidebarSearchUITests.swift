import XCTest

/// ⌘K from a hardware keyboard (#675): with a document open, the sidebar
/// shows and its search field takes the keyboard, on iPhone (the drawer is
/// built only once it opens) and on iPad (in portrait the sidebar column
/// hides while a document is open).
final class SidebarSearchUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testCommandKShowsTheSidebarAndFocusesItsSearch() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        // On iPhone the row's tap closes the drawer: press once it is shut.
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        let gone = NSPredicate(format: "exists == false")
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: gone, evaluatedWith: sidebar)], timeout: 5), .completed)

        // After a test typed with the software keyboard, the Simulator
        // drops the next launch's first hardware key: Shift alone, which
        // does nothing, takes that place.
        app.typeKey(XCUIKeyboardKey.shift.rawValue, modifierFlags: [])
        app.typeKey("k", modifierFlags: .command)
        let field = app.textFields["sidebar-search"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5), "⌘K shows the sidebar")
        let focused = NSPredicate(format: "hasKeyboardFocus == true")
        XCTAssertEqual(
            XCTWaiter.wait(for: [expectation(for: focused, evaluatedWith: field)], timeout: 10), .completed,
            "⌘K focuses the sidebar's search")
        field.typeText("zzzz")
        XCTAssertTrue(app.staticTexts["No documents or cases match “zzzz”."].waitForExistence(timeout: 5))
    }
}
