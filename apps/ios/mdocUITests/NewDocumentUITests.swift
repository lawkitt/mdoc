import XCTest

/// New document (ADR 0109, slice 6): New document is first in the
/// sidebar's + menu and ⌘N, and opens in edit mode with the keyboard up;
/// left empty it leaves no row. Its title and Find names again are
/// NewDocumentTests' (ADR 0112's sweep). Paste opens in Unfiled.
final class NewDocumentUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A fresh library.
    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
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
        XCTAssertTrue(item.waitForExistence(timeout: MdocUITest.timeout))
        // First in the menu: nearest the + button, which is below the menu,
        // so the last row, with the camera farthest away (ADR 0088).
        let items = app.collectionViews.buttons
        XCTAssertEqual(items.element(boundBy: items.count - 1).label, "New document", "nearest the button")
        item.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10), "the keyboard comes up")
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: MdocUITest.timeout))
        return text
    }

    @MainActor
    private func row(_ app: XCUIApplication, title: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", title))
            .firstMatch
    }

    /// ⌘N on a hardware keyboard opens a new document, as the + menu's
    /// first item does (ADR 0109; it went to Start before).
    @MainActor
    func testCommandNOpensANewDocument() throws {
        let app = launch()
        XCTAssertTrue(app.descendants(matching: .any)["start"].firstMatch.waitForExistence(timeout: 30), "Start")
        // On a slow runner the first hardware key after launch can be
        // lost, as ⌘↓ was (#771): press again while edit mode is absent.
        let done = app.buttons["done-editing"]
        for _ in 0..<3 where !done.exists {
            app.typeKey("n", modifierFlags: .command)
            _ = done.waitForExistence(timeout: 15)
        }
        XCTAssertTrue(done.exists, "edit mode")
        XCTAssertEqual(app.buttons["title-menu"].label, "New document")
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: MdocUITest.timeout))
    }

    /// Left untouched, a new document leaves no row behind.
    @MainActor
    func testAnUntouchedNewDocumentLeavesNoRow() throws {
        let app = launch()
        _ = newDocument(app)
        MdocUITest.revealSidebar(app)
        // The sidebar's search lists recent documents before anything is
        // typed.
        let search = app.textFields["sidebar-search"].firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: MdocUITest.timeout))
        search.tap()
        XCTAssertTrue(row(app, title: "New document").waitForExistence(timeout: MdocUITest.timeout), "listed while it is open")
        app.buttons["Cancel search"].firstMatch.tap()
        let documents = app.descendants(matching: .any)["sidebar-documents"].firstMatch
        XCTAssertTrue(documents.waitForExistence(timeout: MdocUITest.timeout))
        documents.tap()
        XCTAssertTrue(row(app, title: "New document").waitForNonExistence(timeout: 10))
        XCTAssertFalse(app.descendants(matching: .any)["document-row"].firstMatch.exists)
    }

    /// One tap on the + menu's Paste opens the clipboard's text as a
    /// document titled by its first line, with no sheet and no paste
    /// prompt, in Unfiled: no import makes a Project of its own (#1091
    /// Decision 35).
    @MainActor
    func testOneTapPastesIntoUnfiled() throws {
        MdocUITest.copy("Lease for Ann Lee\n\nAnn Lee signed the lease.")
        let app = launch()
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let paste = app.descendants(matching: .any)["paste-text"].firstMatch
        XCTAssertTrue(paste.waitForExistence(timeout: MdocUITest.timeout))
        paste.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: MdocUITest.timeout), "no document after one tap")
        XCTAssertEqual(app.buttons["title-menu"].label, "Lease for Ann Lee")
        MdocUITest.revealSidebar(app)
        let unfiled = app.descendants(matching: .any)["sidebar-unfiled"].firstMatch
        XCTAssertTrue(unfiled.waitForExistence(timeout: MdocUITest.timeout))
        unfiled.tap()
        XCTAssertTrue(row(app, title: "Lease for Ann Lee").waitForExistence(timeout: MdocUITest.timeout), "listed in Unfiled")
    }
}
