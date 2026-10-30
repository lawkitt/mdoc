import XCTest

/// Light and dark screenshots of Home, the document with the candidate sheet
/// and the Restore extension, kept as test attachments. Skipped unless the
/// run sets `MDOC_SCREENSHOTS=1` (`TEST_RUNNER_MDOC_SCREENSHOTS=1` through
/// xcodebuild).
final class ScreenshotUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_SCREENSHOTS"] == "1", "screenshots not requested")
    }

    @MainActor
    func testLightScreenshots() throws {
        try capture(.light, name: "light")
    }

    @MainActor
    func testDarkScreenshots() throws {
        try capture(.dark, name: "dark")
    }

    @MainActor
    private func capture(_ appearance: XCUIDevice.Appearance, name: String) throws {
        XCUIDevice.shared.appearance = appearance
        defer { XCUIDevice.shared.appearance = .light }
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launch()

        let letter = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH 'Letter before claim'"))
            .firstMatch
        XCTAssertTrue(letter.waitForExistence(timeout: 10))
        keep("home-\(name)")

        letter.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        text.links["Maya Ellison"].firstMatch.tap()
        XCTAssertTrue(app.buttons["candidate-done"].waitForExistence(timeout: 5))
        keep("candidate-sheet-\(name)")

        // Finish the review so the text can be shared, then share it to the
        // Restore with mdoc extension from the share sheet's app row.
        app.buttons["candidate-done"].tap()
        app.buttons["primary-action"].tap()
        app.buttons["primary-action"].tap()
        app.buttons["Share text"].tap()
        let sheet = app.otherElements["ActivityListView"].firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10))
        // The app row names a share extension after its app.
        let restore = sheet.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN {'Restore with mdoc', 'mdoc'}"))
            .firstMatch
        if !restore.waitForExistence(timeout: 3) {
            keep("share-sheet-\(name)")
            throw XCTSkip("Restore with mdoc is not in the share sheet's app row")
        }
        restore.tap()
        // The extension runs in its own process; give its sheet time to show.
        _ = app.staticTexts["Restore with mdoc"].waitForExistence(timeout: 10)
        sleep(2)
        keep("restore-extension-\(name)")
    }

    @MainActor
    private func keep(_ name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
