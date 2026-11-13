import XCTest

/// How long a review tap takes on a phone (ADR 0104: core calls stay on the
/// main actor while a tap stays under about 16 ms): the "Review tap"
/// signpost around the core call (`RealCore.review`), measured over Replace
/// and Undo on the seed's email. A phone only; run it with
/// `-only-testing:mdocUITests/DeviceTapTimingUITests` after
/// `scripts/ios-device-test.sh` has copied the fixtures.
final class DeviceTapTimingUITests: XCTestCase {
    override func setUpWithError() throws {
        #if targetEnvironment(simulator)
        throw XCTSkip("Timing a review tap needs a phone.")
        #endif
        continueAfterFailure = false
    }

    @MainActor
    func testAReviewTapsTime() throws {
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
}
