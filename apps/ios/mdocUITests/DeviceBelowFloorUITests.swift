import XCTest

/// A phone below the detector floor (ADR 0088), which the long-jobs test
/// cannot cover: it needs the detector tier. Below the floor mdoc still
/// reads, recognizes and lets you mark. Settings and the document say the
/// phone cannot find names, and Home has no setup card; a camera scan
/// downloads text recognition for real on Wi-Fi and is recognized while
/// mdoc leaves the screen; selecting a word marks it and the text shares;
/// the 37-page .docx comes in while mdoc is away. Every job goes through the
/// real background task, which the Simulator refuses, so this skips there.
/// `just ios-device-test <udid>` runs it on a phone below the floor (or with
/// `below-floor` on any phone: UI tests take the tier from a stand-in
/// memory size, not the phone's).
final class DeviceBelowFloorUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        #if targetEnvironment(simulator)
            throw XCTSkip("device only: the Simulator refuses BGContinuedProcessingTask")
        #endif
    }

    @MainActor
    func testBelowFloor() throws {
        try scanRecognizeAndMark()
        try importDocxInTheBackground()
    }

    /// Settings' note and no setup card on Home; then the scan waits for
    /// text recognition, Download & continue fetches it and recognizes the
    /// pages with mdoc in the background part of the time. Where Find names
    /// would be, the note; selecting a word and Replace… marks it.
    @MainActor
    private func scanRecognizeAndMark() throws {
        let app = launch(seed: MdocUITest.deviceScan)
        let settings = app.buttons["settings-button"].firstMatch
        XCTAssertTrue(settings.waitForExistence(timeout: 30))
        XCTAssertFalse(app.buttons["setup-download"].exists, "a setup card on Home below the floor")
        settings.tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["below-floor-note"].firstMatch.waitForExistence(timeout: 10),
            "Settings does not say the phone cannot find names")
        app.buttons["Done"].firstMatch.tap()

        let row = Self.row(app, title: MdocUITest.deviceScanTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let download = app.buttons["setup-download"].firstMatch
        XCTAssertTrue(download.waitForExistence(timeout: 30), "no text recognition setup in the scan")
        download.tap()
        if app.buttons["Use cellular"].firstMatch.waitForExistence(timeout: 3) {
            XCTFail("mdoc asks about cellular: connect the phone to Wi-Fi")
            return
        }

        // Leave while the pages are being recognized.
        let progress = app.descendants(matching: .any)["job-progress"].firstMatch
        expect(NSPredicate(format: "label CONTAINS 'Recognizing page'"), on: progress, timeout: 1800)
        try leaveAndReturn(app)

        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        expect(NSPredicate(format: "value CONTAINS 'Harbour Street'"), on: text, timeout: 300)
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "recognition stopped in the background")

        // Where Find names would be, the note.
        XCTAssertTrue(
            app.descendants(matching: .any)["below-floor-note"].firstMatch.waitForExistence(timeout: 10),
            "the document does not say the phone cannot find names")
        XCTAssertFalse(app.buttons["Find names"].exists)

        // Mark by selecting: the first word, from the top, as a person.
        for _ in 0..<4 {
            text.swipeDown(velocity: .fast)
        }
        text.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.04)).press(forDuration: 1.0)
        let replace = app.menuItems["Replace…"].firstMatch
        XCTAssertTrue(replace.waitForExistence(timeout: 5), "no Replace… in the edit menu")
        replace.tap()
        let sheet = app.navigationBars["Replace"]
        XCTAssertTrue(sheet.waitForExistence(timeout: 5))
        sheet.buttons["Replace"].tap()
        // The text view rebuilds its elements after a change; ask the app.
        XCTAssertTrue(app.links["PERSON_1"].firstMatch.waitForExistence(timeout: 10), "the selection was not marked")

        // The marked text shares.
        let primary = app.buttons["primary-action"]
        expect(NSPredicate(format: "label BEGINSWITH 'Share'"), on: primary, timeout: 10)
        primary.tap()
        XCTAssertTrue(app.otherElements["ActivityListView"].firstMatch.waitForExistence(timeout: 10))
        MdocUITest.closeShareSheet(app)
    }

    /// The seed imports the 37-page legal opinion at launch; the test opens
    /// it at once and leaves while it comes in, if it has not already. The
    /// import must finish, not stop with "Import not finished".
    @MainActor
    private func importDocxInTheBackground() throws {
        let app = launch(seed: MdocUITest.longDocx)
        let row = Self.row(app, title: MdocUITest.longDocxTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let running = app.descendants(matching: .any)["job-progress"].firstMatch.waitForExistence(timeout: 5)
        XCTContext.runActivity(named: running ? "left during the import" : "the import ended before mdoc left") { _ in }
        try leaveAndReturn(app, seconds: 15)

        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        expect(NSPredicate(format: "value CONTAINS 'Basis Trustee'"), on: text, timeout: 180)
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "the import stopped in the background")
        XCTAssertFalse(app.buttons["Find names"].exists)
    }

    // MARK: Steps

    /// Below the floor (the UI tests' default tier), with real downloads.
    @MainActor
    private func launch(seed: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launchEnvironment["MDOC_REAL_DOWNLOADS"] = "1"
        app.launch()
        return app
    }

    /// Home, `seconds` in the background, and back. The app must still be
    /// there: a background task handler that trips Swift's isolation check
    /// stops it the moment a job starts (lawkitt/mdoc#290).
    @MainActor
    private func leaveAndReturn(_ app: XCUIApplication, seconds: UInt32 = 10) throws {
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(
            app.wait(for: .runningBackground, timeout: 10) || app.state == .runningBackgroundSuspended,
            "mdoc did not go to the background")
        sleep(seconds)
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
