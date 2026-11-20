import XCTest

/// A partial import's notice card sits over the text and closes with ×; light
/// and dark kept as attachments. A phone's import reads scanned pages rather
/// than skip them, so the notice comes from `MDOC_UI_NOTICE`.
final class NoticeCardUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testPartialImportCardCloses() throws {
        defer { XCUIDevice.shared.appearance = .light }
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo
            .appendingPathComponent(MdocUITest.markdownFixture).path
        app.launchEnvironment["MDOC_UI_NOTICE"] = "Partial import: pages 2, 5 of 5 require OCR and were skipped."
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
        let light = keep("notice-card-light")
        XCUIDevice.shared.appearance = .dark
        keep("notice-card-dark", unlike: light)
        XCUIDevice.shared.appearance = .light

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

    @MainActor
    @discardableResult
    private func keep(_ name: String, unlike previous: XCUIScreenshot? = nil) -> XCUIScreenshot {
        let screen = MdocUITest.settledScreen(unlike: previous)
        let shot = XCTAttachment(screenshot: screen)
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
        return screen
    }
}
