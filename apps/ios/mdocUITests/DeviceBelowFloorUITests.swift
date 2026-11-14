import XCTest

/// A phone below the detector floor (ADR 0088), which the long-jobs test
/// cannot cover: it needs the detector tier. Below the floor mdoc still
/// reads, recognizes and lets you mark. Settings and the document say the
/// phone cannot find names, and Home has no setup card; a camera scan
/// downloads text recognition for real (on cellular, the test answers Use
/// cellular) and is recognized while mdoc leaves the screen; selecting a word marks it and the text shares;
/// the 37-page .docx comes in and is whole after mdoc is away. Every job
/// goes through the real background task, which the Simulator refuses, so this skips there.
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
    /// would be, the note; selecting a word and Hide name marks it.
    @MainActor
    private func scanRecognizeAndMark() throws {
        let app = launch(seed: MdocUITest.deviceScan)
        MdocUITest.revealSidebar(app)
        let settings = app.buttons["settings-button"].firstMatch
        XCTAssertTrue(settings.waitForExistence(timeout: 30))
        XCTAssertFalse(app.buttons["setup-download"].exists, "a setup card on Home below the floor")
        // iOS's banner for the background job the seed starts can cover the
        // button for a few seconds after launch; tap until Settings opens.
        let note = app.descendants(matching: .any)["below-floor-note"].firstMatch
        for _ in 0..<4 where !note.exists {
            settings.tap()
            _ = note.waitForExistence(timeout: MdocUITest.probeTimeout)
        }
        XCTAssertTrue(note.exists, "Settings does not say the phone cannot find names")
        app.buttons["Done"].firstMatch.tap()

        let row = Self.row(app, title: MdocUITest.deviceScanTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let download = app.buttons["setup-download"].firstMatch
        XCTAssertTrue(download.waitForExistence(timeout: 30), "no text recognition setup in the scan")
        download.tap()
        MdocUITest.answerCellularQuestion(app)

        // Leave while the pages are being recognized (or just after, on a
        // fast phone).
        XCTAssertTrue(
            MdocUITest.waitThroughDownload(app, timeout: 1800) { MdocUITest.recognizingOrRecognized(app) },
            "the pages were never recognized")
        try leaveAndReturn(app)

        // The text shows once recognition ends: minutes on a warm 12 Pro.
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 300), "the pages were never recognized")
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
        let replace = app.menuItems["hide-name"].firstMatch
        XCTAssertTrue(replace.waitForExistence(timeout: MdocUITest.timeout), "no Hide name in the edit menu")
        replace.tap()
        // The mark is a proposal and the review bar opens on it; Replace applies.
        let confirm = app.buttons["review-replace"]
        XCTAssertTrue(confirm.waitForExistence(timeout: MdocUITest.timeout), "no review bar on the mark")
        confirm.tap()
        // The mark is the document's only name, so replacing it ends review
        // (#699): Undo is the toast's, or the bar's when it stays open.
        let undone = app.buttons["review-undo"].exists
            ? app.buttons["review-undo"].isEnabled
            : app.buttons["toast-action"].waitForExistence(timeout: MdocUITest.timeout)
        XCTAssertTrue(undone, "the selection was not marked")
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        // The marked text shares.
        let primary = app.buttons["primary-action"]
        expect(NSPredicate(format: "label BEGINSWITH 'Share'"), on: primary, timeout: 10)
        primary.tap()
        XCTAssertTrue(app.otherElements["ActivityListView"].firstMatch.waitForExistence(timeout: 10))
        MdocUITest.closeShareSheet(app)
    }

    /// The seed imports the 37-page legal opinion at launch, through the
    /// real background task (whose handler crashed every job in
    /// lawkitt/mdoc#290). The conversion takes about 40 ms on the iPhone
    /// 12 Pro, so no stay away can overlap it; the scan above covers leaving
    /// during a job. Here mdoc leaves for 15 s once the document is listed
    /// and must come back with the import whole, not "Import not finished".
    @MainActor
    private func importDocxInTheBackground() throws {
        let app = launch(seed: MdocUITest.longDocx)
        let row = Self.row(app, title: MdocUITest.longDocxTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        try leaveAndReturn(app, seconds: 15)
        XCTAssertFalse(
            app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier == 'document-row' AND label CONTAINS 'Import not finished'"))
                .firstMatch.exists,
            "the import did not finish")
        // A tap just after coming back can be lost (#711): tap again until
        // the document opens.
        let text = app.textViews["document-text"]
        for _ in 0..<3 where !text.exists {
            if row.isHittable { row.tap() }
            if text.waitForExistence(timeout: 10) { break }
        }
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
        // Home can be missed while the background job's banner shows: press
        // it again if mdoc is still in front.
        var left = false
        for _ in 0..<3 where !left {
            XCUIDevice.shared.press(.home)
            left = app.wait(for: .runningBackground, timeout: 10) || app.state == .runningBackgroundSuspended
        }
        XCTAssertTrue(left, "mdoc did not go to the background")
        // The stay itself is the test: a fixed time in the background, not
        // a wait for something to happen.
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

    /// A sidebar row by title, whatever its status.
    @MainActor
    private static func row(_ app: XCUIApplication, title: String) -> XCUIElement {
        MdocUITest.revealSidebar(app)
        return app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", "\(title),"))
            .firstMatch
    }
}
