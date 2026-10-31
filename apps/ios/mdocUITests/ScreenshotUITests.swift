import XCTest

/// Light and dark screenshots of Home, the document with the candidate sheet,
/// the Restore extension and the review of the Markdown-rich fixture, on the
/// Rust core, kept as test attachments. Skipped unless the run sets
/// `MDOC_SCREENSHOTS=1` (`TEST_RUNNER_MDOC_SCREENSHOTS=1` through xcodebuild).
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
    func testMarkdownReviewLight() throws {
        try captureMarkdownReview(.light, name: "light")
    }

    @MainActor
    func testMarkdownReviewDark() throws {
        try captureMarkdownReview(.dark, name: "dark")
    }

    /// The review screen on the Markdown-rich fixture: the core's spans
    /// style the text, with the rules' candidates underlined over them.
    @MainActor
    private func captureMarkdownReview(_ appearance: XCUIDevice.Appearance, name: String) throws {
        XCUIDevice.shared.appearance = appearance
        defer { XCUIDevice.shared.appearance = .light }
        let app = MdocUITest.launch(seed: MdocUITest.markdownFixture)
        let document = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        keep("markdown-review-\(name)")
        // The end: the fenced block, the rule and the document's own colour.
        text.swipeUp(velocity: .fast)
        sleep(1)
        keep("markdown-review-end-\(name)")
    }

    /// A four-page camera scan: the OCR model downloads, recognition is
    /// cancelled part-way, and the document offers Resume recognition, which
    /// finishes the text. Downloads the model, so it also needs
    /// `MDOC_OCR_TESTS=1`.
    @MainActor
    func testOCRResume() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_OCR_TESTS"] == "1", "OCR tests not requested")
        defer { XCUIDevice.shared.appearance = .light }
        let pages = try MdocUITest.scanPages(named: "Tenancy scan", count: 4)
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = pages.path
        app.launchEnvironment["MDOC_REAL_DOWNLOADS"] = "1"
        app.launch()

        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH 'Tenancy scan'"))
            .firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let download = app.buttons["setup-download"]
        XCTAssertTrue(download.waitForExistence(timeout: 30))
        download.tap()
        // The Simulator's network can count as cellular: allow it.
        let cellular = app.buttons["Use cellular"]
        if cellular.waitForExistence(timeout: 5) {
            cellular.tap()
        }

        // Cancel once recognition has run a while.
        let progress = app.descendants(matching: .any)["job-progress"]
        let recognizing = NSPredicate(format: "label CONTAINS 'Recogniz'")
        expectation(for: recognizing, evaluatedWith: progress)
        waitForExpectations(timeout: 300)
        sleep(6)
        app.buttons["Cancel"].firstMatch.tap()
        let primary = app.buttons["primary-action"]
        expectation(for: NSPredicate(format: "label == 'Resume recognition'"), evaluatedWith: primary)
        waitForExpectations(timeout: 60)
        keep("ocr-stopped-light")
        XCUIDevice.shared.appearance = .dark
        sleep(1)
        keep("ocr-stopped-dark")

        primary.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 300))
        expectation(for: NSPredicate(format: "value CONTAINS 'Harbour Street'"), evaluatedWith: text)
        waitForExpectations(timeout: 300)
        keep("ocr-resumed-dark")
        XCUIDevice.shared.appearance = .light
        sleep(1)
        keep("ocr-resumed-light")
    }

    @MainActor
    private func capture(_ appearance: XCUIDevice.Appearance, name: String) throws {
        XCUIDevice.shared.appearance = appearance
        defer { XCUIDevice.shared.appearance = .light }
        let app = MdocUITest.launch()

        // Home, then the imported document's review with the rules' candidates.
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        keep("home-\(name)")

        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        email.tap()
        XCTAssertTrue(app.buttons["candidate-done"].waitForExistence(timeout: 5))
        keep("candidate-sheet-\(name)")

        // Finish the review so the text can be shared, then share it to the
        // Restore with mdoc extension from the share sheet's app row.
        app.buttons["candidate-done"].tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
        if primary.label.hasPrefix("Replace") {
            primary.tap()
        }
        primary.tap()
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
