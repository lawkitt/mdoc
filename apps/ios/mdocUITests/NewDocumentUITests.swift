import XCTest

/// New document and Find names again (ADR 0109, slice 6): New document is
/// first in the sidebar's + menu and opens in edit mode with the keyboard
/// up; it takes its first line's title and leaves no row when left empty;
/// Find names again proposes a name typed after the first Find names.
final class NewDocumentUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A fresh library. `rulesFindNames`: Find names runs the structured
    /// rules in the detector's place, so an email typed is found without
    /// the model.
    @MainActor
    private func launch(rulesFindNames: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if rulesFindNames { app.launchEnvironment["MDOC_UI_RULES_FIND_NAMES"] = "1" }
        app.launch()
        return app
    }

    /// New document from the sidebar's + menu, where it is the first item;
    /// edit mode and the keyboard come up. Returns the text view.
    @MainActor
    private func newDocument(_ app: XCUIApplication) -> XCUIElement {
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let item = app.collectionViews.buttons["New document"].firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        // First in the menu: nearest the + button, which is below the menu,
        // so the last row, with the camera farthest away (ADR 0088).
        let items = app.collectionViews.buttons
        XCTAssertEqual(items.element(boundBy: items.count - 1).label, "New document", "nearest the button")
        item.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10), "the keyboard comes up")
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        return text
    }

    @MainActor
    private func row(_ app: XCUIApplication, title: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", title))
            .firstMatch
    }

    /// Typed, a heading names the document: the title and the sidebar's
    /// row follow it.
    @MainActor
    func testANewDocumentIsNamedAfterItsHeading() throws {
        let app = launch()
        let text = newDocument(app)
        XCTAssertEqual(app.buttons["title-menu"].label, "New document")
        app.typeText("# Lease agreement\n\nThe tenant pays rent.")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'The tenant pays rent'"), timeout: 10))
        app.buttons["done-editing"].tap()
        XCTAssertTrue(app.buttons["edit-button"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["title-menu"].wait(until: NSPredicate(format: "label == 'Lease agreement'"), timeout: 10))
        MdocUITest.revealSidebar(app)
        XCTAssertTrue(row(app, title: "Lease agreement").waitForExistence(timeout: 10))
    }

    /// Left untouched, a new document leaves no row behind.
    @MainActor
    func testAnUntouchedNewDocumentLeavesNoRow() throws {
        let app = launch()
        _ = newDocument(app)
        MdocUITest.revealSidebar(app)
        XCTAssertTrue(row(app, title: "New document").waitForExistence(timeout: 5), "listed while it is open")
        let documents = app.descendants(matching: .any)["sidebar-documents"].firstMatch
        XCTAssertTrue(documents.waitForExistence(timeout: 5))
        documents.tap()
        XCTAssertTrue(row(app, title: "New document").waitForNonExistence(timeout: 10))
        XCTAssertFalse(app.descendants(matching: .any)["document-row"].firstMatch.exists)
    }

    /// Find names again, from the title's menu, proposes an email typed
    /// after the first Find names; the replaced one stays replaced.
    @MainActor
    func testFindNamesAgainProposesATypedName() throws {
        let app = launch(rulesFindNames: true)
        let text = newDocument(app)
        app.typeText("Write to ann@example.com today.")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'today.'"), timeout: 10))
        app.buttons["done-editing"].tap()

        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label == 'Find names'"), timeout: 10))
        primary.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label CONTAINS 'Replace' AND label CONTAINS '1'"), timeout: 30))
        primary.tap()
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'EMAIL_1'"), timeout: 10))

        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
        app.typeText(" Copy bob@example.com ")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'bob@example.com'"), timeout: 10))
        app.buttons["done-editing"].tap()
        XCTAssertTrue(app.buttons["edit-button"].waitForExistence(timeout: 5))

        app.buttons["title-menu"].tap()
        let again = app.descendants(matching: .any)["find-names-again"].firstMatch
        XCTAssertTrue(again.waitForExistence(timeout: 5))
        again.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label CONTAINS 'Replace' AND label CONTAINS '1'"), timeout: 30))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'EMAIL_1'"), timeout: 5), "the first stays replaced")
    }
}
