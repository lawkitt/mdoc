import XCTest

/// How long a review tap takes on a phone (ADR 0104: core calls stay on the
/// main actor while a tap stays under about 16 ms): the "Review tap"
/// signpost around the core call (`RealCore.review`), measured over Replace
/// and Undo on the seed's email; and a drawer drag's hitches (ADR 0114),
/// on a phone or, with the timing tests (`TEST_RUNNER_MDOC_TIMING_TESTS=1`),
/// on a Simulator. Run them with
/// `-only-testing:mdocUITests/DeviceTapTimingUITests` after
/// `scripts/ios-device-test.sh` has copied the fixtures.
final class DeviceTapTimingUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testAReviewTapsTime() throws {
        #if targetEnvironment(simulator)
        try XCTSkipIf(true, "Timing a review tap needs a phone.")
        #endif
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        MdocUITest.tapAboveBottomBar(email, in: text, app)
        let replace = app.buttons["review-replace"]
        XCTAssertTrue(replace.waitForExistence(timeout: MdocUITest.timeout))

        let tap = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Review", name: "Review tap")
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        measure(metrics: [tap], options: options) {
            replace.tap()
            XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
            let undo = app.buttons["toast-action"]
            XCTAssertTrue(undo.waitForExistence(timeout: MdocUITest.timeout))
            undo.tap()
            XCTAssertTrue(app.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 10))
            XCTAssertTrue(replace.waitForExistence(timeout: MdocUITest.timeout))
        }
    }

    /// A drawer drag drops no frames on a phone (ADR 0114): the "Drawer
    /// drag" animation interval, from the finger's first move to the end of
    /// the slide, opening the sidebar from the middle of the text and
    /// closing it from the sliver. XCTest reports its hitches and frame rate.
    @MainActor
    func testADrawerDragsHitches() throws {
        #if targetEnvironment(simulator)
        try XCTSkipUnless(
            ProcessInfo.processInfo.environment["MDOC_TIMING_TESTS"] == "1", "on a Simulator, with the timing tests only")
        #endif
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        XCTAssertTrue(text.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 30))
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        let origin = app.coordinate(withNormalizedOffset: .zero)
        let width = app.frame.width, middle = app.frame.height * 0.5
        func drag(fromX x: CGFloat, by dx: CGFloat) {
            let start = origin.withOffset(CGVector(dx: x, dy: middle))
            // A finger's pace: about a third of a second across.
            start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: 0)),
                        withVelocity: 900, thenHoldForDuration: 0.05)
        }

        let hitches = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Drawer", name: "Drawer drag")
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        measure(metrics: [hitches], options: options) {
            drag(fromX: width * 0.3, by: width * 0.6)
            XCTAssertTrue(sidebar.waitUntilHittable(timeout: 5))
            drag(fromX: width - 30, by: -(width * 0.6))
            XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5))
        }
    }
}
