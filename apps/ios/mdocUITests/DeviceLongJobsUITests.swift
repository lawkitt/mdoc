import XCTest

/// The long jobs on a phone (ADR 0088), which the Simulator cannot run: it
/// refuses `BGContinuedProcessingTask`, so every job here goes through the
/// real background task only on a device. A camera scan downloads text
/// recognition for real (on cellular, the test answers Use cellular) and is
/// recognized while mdoc leaves the screen; the name detector finds names in it; Find names on a 37-page
/// .docx keeps running in the background past the short grace iOS gives
/// without that task; a text PDF comes in. `just ios-device-test <udid>`
/// copies the fixtures into the app's container and runs this; it skips on the Simulator. It needs a phone at
/// the detector tier (8 GB); the detector comes from the phone's own
/// library or, the first time, a download that later runs keep.
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
        try findNamesInTheBackground()
        try importTextPDF()
    }

    /// The scan waits for text recognition; Download & continue downloads
    /// it and recognizes the pages, with mdoc in the background part of the
    /// time. Then Find names, which downloads the detector first when neither
    /// library has it.
    @MainActor
    private func scanDownloadRecognizeAndFindNames() throws {
        let app = launch(seed: MdocUITest.deviceScan)
        let row = Self.row(app, title: MdocUITest.deviceScanTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let download = app.buttons["setup-download"].firstMatch
        XCTAssertTrue(download.waitForExistence(timeout: 30))
        download.tap()
        MdocUITest.answerCellularQuestion(app)

        // Leave while the pages are being recognized (or just after, on a
        // fast phone).
        XCTAssertTrue(
            MdocUITest.waitThroughDownload(app, timeout: 1800) { MdocUITest.recognizingOrRecognized(app) },
            "the pages were never recognized")
        try leaveAndReturn(app)

        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        expect(NSPredicate(format: "value CONTAINS 'Harbour Street'"), on: text, timeout: 300)
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "recognition stopped in the background")

        // Find names with the detector.
        let primary = app.buttons["primary-action"]
        expect(NSPredicate(format: "label == 'Find names'"), on: primary, timeout: 30)
        primary.tap()
        // A phone with no detector in either library: Find names offers its
        // download (ADR 0090) and goes on once it is in.
        let detector = app.buttons["setup-download"].firstMatch
        if detector.waitForExistence(timeout: 5) {
            detector.tap()
            MdocUITest.answerCellularQuestion(app)
        }
        let name = app.links.matching(NSPredicate(format: "label CONTAINS 'Whitlow'")).firstMatch
        XCTAssertTrue(
            MdocUITest.waitThroughDownload(app, timeout: 1800) { name.exists },
            "the detector found no name in the scan")
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "Find names stopped in the background")
    }

    /// The seed imports the 37-page legal opinion and starts Find names,
    /// which takes over a minute on the iPhone 17 Pro (20 pages in 35–43 s,
    /// ADR 0088's measurement). mdoc leaves for 45 s while it runs: past the
    /// grace iOS gives without a continued processing task, after which
    /// mdoc would be suspended and the job stopped. With the task, mdoc
    /// still runs in the background and the job finishes.
    @MainActor
    private func findNamesInTheBackground() throws {
        let app = launch(seed: MdocUITest.longDocx)
        let row = Self.row(app, title: MdocUITest.longDocxTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let progress = app.descendants(matching: .any)["job-progress"].firstMatch
        expect(NSPredicate(format: "label CONTAINS 'Finding names'"), on: progress, timeout: 120)
        // The bar has a fraction once the first windows are done.
        expect(NSPredicate(format: "value CONTAINS '%'"), on: progress, timeout: 60)
        let before = Self.percent(progress)
        try leaveAndReturn(app, seconds: 45, stillWorking: true)
        let primary = app.buttons["primary-action"]
        // Done as mdoc came back: the bar is gone and Replace shows.
        let after = progress.exists ? Self.percent(progress) : (primary.label.hasPrefix("Replace") ? 100 : 0)
        let note = XCTAttachment(string: "Find names: \(before)% before leaving, \(after)% on return after 45 s away")
        note.name = "background-progress"
        note.lifetime = .keepAlways
        add(note)
        // More than the second or two between coming back and this read
        // could add, had the job been paused all along (45 s away gave 65).
        XCTAssertGreaterThan(after, before + 25, "Find names made no progress in the background")
        expect(NSPredicate(format: "label BEGINSWITH 'Replace'"), on: primary, timeout: 180)
        XCTAssertFalse(app.buttons["interruption-resume"].exists, "Find names stopped in the background")
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

    /// The bar's percentage, 0 when it shows none.
    @MainActor
    private static func percent(_ progress: XCUIElement) -> Int {
        let value = (progress.value as? String) ?? ""
        return Int(value.filter(\.isNumber)) ?? 0
    }

    /// Home, `seconds` in the background, and back. The app must still be
    /// there: a background task handler that trips Swift's isolation check
    /// stops it the moment a job starts (lawkitt/mdoc#290). `stillWorking`:
    /// the job outlasts the stay, so mdoc must still be running, not
    /// suspended, which only the continued processing task allows.
    @MainActor
    private func leaveAndReturn(_ app: XCUIApplication, seconds: UInt32 = 10, stillWorking: Bool = false) throws {
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(
            app.wait(for: .runningBackground, timeout: 10) || app.state == .runningBackgroundSuspended,
            "mdoc did not go to the background")
        sleep(seconds)
        XCTAssertNotEqual(app.state, .notRunning, "mdoc stopped in the background")
        if stillWorking {
            XCTAssertEqual(app.state, .runningBackground, "mdoc was suspended: no continued processing task")
        }
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
    /// A scrubbed 37-page legal opinion, the floor measurement's document.
    static let longDocx = "tests/fixtures/docx-preview/legal-opinion.docx"
    static let longDocxTitle = "legal-opinion"
    /// A PDF with a text layer and no names.
    static let textPDF = "tests/fixtures/import/text.pdf"
    static let textPDFTitle = "text"
}
