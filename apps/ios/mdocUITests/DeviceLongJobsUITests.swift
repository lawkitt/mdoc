import XCTest

/// The long jobs on a phone (ADR 0088), which the Simulator cannot run: it
/// refuses `BGContinuedProcessingTask`, so every job here goes through the
/// real background task only on a device. A camera scan downloads text
/// recognition for real on Wi-Fi and is recognized while mdoc leaves the
/// screen; the name detector finds names in it; a .docx and a text PDF
/// come in. `just ios-device-test <udid>` copies the fixtures into the app's
/// container and runs this; it skips on the Simulator. It needs a phone at
/// the detector tier (8 GB); the first run also downloads the detector,
/// which later runs keep.
final class DeviceLongJobsUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        #if targetEnvironment(simulator)
            throw XCTSkip("device only: the Simulator refuses BGContinuedProcessingTask")
        #endif
    }

    @MainActor
    func testLongJobs() throws {
        try scanDownloadRecognizeAndFindNames()
        try importDocx()
        try importTextPDF()
    }

    /// The scan waits for text recognition; Download & continue downloads
    /// it (and the detector, the first time) and recognizes the pages, with
    /// mdoc in the background part of the time. Then Find names.
    @MainActor
    private func scanDownloadRecognizeAndFindNames() throws {
        let app = launch(seed: MdocUITest.deviceScan)
        let row = Self.row(app, title: MdocUITest.deviceScanTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let download = app.buttons["setup-download"].firstMatch
        XCTAssertTrue(download.waitForExistence(timeout: 30))
        download.tap()
        if app.buttons["Use cellular"].firstMatch.waitForExistence(timeout: 3) {
            XCTFail("mdoc asks about cellular: connect the phone to Wi-Fi")
            return
        }

        // Leave while the pages are being recognized. The first run's
        // download includes the detector, so allow for it.
        let progress = app.descendants(matching: .any)["job-progress"].firstMatch
        expect(NSPredicate(format: "label CONTAINS 'Recognizing page'"), on: progress, timeout: 1800)
        try leaveAndReturn(app)

        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        expect(NSPredicate(format: "value CONTAINS 'Harbour Street'"), on: text, timeout: 300)
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "recognition stopped in the background")

        // Find names with the detector, leaving at once.
        let primary = app.buttons["primary-action"]
        expect(NSPredicate(format: "label == 'Find names'"), on: primary, timeout: 30)
        primary.tap()
        try leaveAndReturn(app)
        let name = app.links.matching(NSPredicate(format: "label CONTAINS 'Whitlow'")).firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 120), "the detector found no name in the scan")
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "Find names stopped in the background")
    }

    /// The seed imports the demo agreement and finds names with the detector.
    @MainActor
    private func importDocx() throws {
        let app = launch(seed: MdocUITest.defaultSeed)
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 180))
        row.tap()
        let name = app.links.matching(NSPredicate(format: "label CONTAINS 'Bennett'")).firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 30), "the detector found no name in the .docx")
    }

    /// A PDF with a text layer reads without text recognition.
    @MainActor
    private func importTextPDF() throws {
        let app = launch(seed: MdocUITest.textPDF)
        let row = Self.row(app, title: MdocUITest.textPDFTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        expect(NSPredicate(format: "value CONTAINS 'Fixture Document'"), on: text, timeout: 120)
    }

    // MARK: Steps

    /// Real downloads at the detector tier.
    @MainActor
    private func launch(seed: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launchEnvironment["MDOC_REAL_DOWNLOADS"] = "1"
        app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1"
        app.launch()
        return app
    }

    /// Home, ten seconds in the background, and back. The app must still
    /// be running: a background task handler that trips Swift's isolation
    /// check stops it the moment a job starts (lawkitt/mdoc#290).
    @MainActor
    private func leaveAndReturn(_ app: XCUIApplication) throws {
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(
            app.wait(for: .runningBackground, timeout: 10) || app.state == .runningBackgroundSuspended,
            "mdoc did not go to the background")
        sleep(10)
        XCTAssertNotEqual(app.state, .notRunning, "mdoc stopped in the background")
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10))
    }

    @MainActor
    private func expect(_ predicate: NSPredicate, on element: XCUIElement, timeout: TimeInterval) {
        let found = XCTNSPredicateExpectation(predicate: predicate, object: element)
        XCTAssertEqual(XCTWaiter().wait(for: [found], timeout: timeout), .completed, "\(element) never matched \(predicate)")
    }

    /// A Home row by title, whatever its status.
    @MainActor
    private static func row(_ app: XCUIApplication, title: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", "\(title),"))
            .firstMatch
    }
}

extension MdocUITest {
    /// Camera pages of a fictional tenancy, which `just ios-device-test`
    /// renders into `target/ios/device-fixtures` and copies to the phone.
    static let deviceScan = "target/ios/device-fixtures/Tenancy scan"
    static let deviceScanTitle = "Tenancy scan"
    /// A PDF with a text layer and no names.
    static let textPDF = "tests/fixtures/import/text.pdf"
    static let textPDFTitle = "text"
}
