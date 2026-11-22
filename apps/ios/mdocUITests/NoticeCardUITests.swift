import XCTest

/// A partial import's notice card sits over the text and closes with ×. A
/// phone's import reads scanned pages rather than skip them, so the notice
/// comes from `MDOC_UI_NOTICE`. Its light and dark screenshots are the
/// survey's (`SurveyUITests`, 68-notice-card), off the queue (#1076).
final class NoticeCardUITests: XCTestCase {
    /// The notice the import leaves.
    static let notice = "Partial import: pages 2, 5 of 5 require OCR and were skipped."

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testPartialImportCardCloses() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo
            .appendingPathComponent(MdocUITest.markdownFixture).path
        app.launchEnvironment["MDOC_UI_NOTICE"] = Self.notice
        app.launch()
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let card = app.descendants(matching: .any)["import-warning"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 30))
        XCTAssertTrue(
            app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH 'Partial import'")).firstMatch.exists)
        // Above the text, under the panes' row.
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertLessThanOrEqual(card.frame.maxY, text.frame.minY + 1)

        // Edit mode hides it; Done brings it back.
        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        let gone = NSPredicate(format: "exists == false")
        expectation(for: gone, evaluatedWith: card)
        waitForExpectations(timeout: MdocUITest.timeout)
        app.buttons["done-editing"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: MdocUITest.timeout))

        let close = app.buttons["notice-close"]
        XCTAssertTrue(close.isHittable)
        XCTAssertGreaterThanOrEqual(close.frame.width, 44)
        XCTAssertGreaterThanOrEqual(close.frame.height, 44)
        close.tap()
        expectation(for: gone, evaluatedWith: card)
        waitForExpectations(timeout: MdocUITest.timeout)
    }
}
