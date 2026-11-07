import XCTest

/// A four-page camera scan: the OCR model downloads, recognition is
/// cancelled part-way, and the document offers Resume recognition, which
/// finishes the text, light and dark, kept as test attachments. Downloads
/// the model, so it runs only with `MDOC_OCR_TESTS=1`
/// (`TEST_RUNNER_MDOC_OCR_TESTS=1` through xcodebuild).
final class OCRResumeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_OCR_TESTS"] == "1", "OCR tests not requested")
    }

    @MainActor
    func testOCRResume() throws {
        defer { XCUIDevice.shared.appearance = .light }
        let pages = try MdocUITest.scanPages(named: "Tenancy scan", count: 4)
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = pages.path
        app.launchEnvironment["MDOC_REAL_DOWNLOADS"] = "1"
        app.launch()

        MdocUITest.revealSidebar(app)
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

        // Cancel once the first page is recognized and before the last
        // starts, so there is text to keep and pages to resume.
        let progress = app.descendants(matching: .any)["job-progress"]
        let pastFirstPage = NSPredicate(format: "label MATCHES '.*page [23] of 4.*'")
        expectation(for: pastFirstPage, evaluatedWith: progress)
        waitForExpectations(timeout: 300)
        app.buttons["Cancel"].firstMatch.tap()
        let primary = app.buttons["primary-action"]
        expectation(for: NSPredicate(format: "label == 'Resume recognition'"), evaluatedWith: primary)
        waitForExpectations(timeout: 60)
        let stopped = keep("ocr-stopped-light")
        XCUIDevice.shared.appearance = .dark
        keep("ocr-stopped-dark", unlike: stopped)

        primary.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 300))
        expectation(for: NSPredicate(format: "value CONTAINS 'Harbour Street'"), evaluatedWith: text)
        waitForExpectations(timeout: 300)
        let resumed = keep("ocr-resumed-dark")
        XCUIDevice.shared.appearance = .light
        keep("ocr-resumed-light", unlike: resumed)
    }

    /// The settled screen as an attachment; with `unlike`, once it differs
    /// from that one (an appearance change draws a moment later).
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
