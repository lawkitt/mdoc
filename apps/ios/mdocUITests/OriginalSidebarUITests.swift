import XCTest

/// An iPad shows the comment rail once the Original pane is 560 pt wide; with
/// the standing sidebar and Text beside it the pane is narrower, so the
/// document offers Hide sidebar, and ☰ shows it again (#962). It skips on
/// an iPhone, which CI runs.
final class OriginalSidebarUITests: XCTestCase {
    /// The rail's width, as `DocxOriginalView.showsRail` has it.
    private let railWidth: CGFloat = 560

    override func setUp() {
        continueAfterFailure = false
    }

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
        super.tearDown()
    }

    @MainActor
    func testHideSidebarWidensTheOriginalToTheRailAndBack() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .pad, "the rail is iPad's")
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] =
            MdocUITest.repo.appendingPathComponent("tests/fixtures/docx-preview/comments.docx").path
        app.launch()
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let original = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(original.waitForExistence(timeout: 60))
        // A 13-inch iPad opens Identities beside the document too, which
        // leaves the Original under 560 pt even without the sidebar; the
        // rail needs that column closed as well, by its own button.
        let identities = app.descendants(matching: .any)["identities-panel"].firstMatch
        if identities.waitForExistence(timeout: MdocUITest.probeTimeout) {
            app.buttons["identities-button"].firstMatch.tap()
            XCTAssertTrue(identities.waitForNonExistence(timeout: MdocUITest.timeout))
        }
        let hide = app.buttons["hide-sidebar"].firstMatch
        XCTAssertTrue(hide.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.buttons["show-sidebar"].firstMatch.exists, "one of the two at a time")
        let beside = original.frame.width
        XCTAssertLessThan(beside, railWidth, "with the sidebar the pane is too narrow for the rail")

        hide.tap()
        let show = app.buttons["show-sidebar"].firstMatch
        XCTAssertTrue(show.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(hide.waitForNonExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(
            MdocUITest.wait(timeout: MdocUITest.timeout) { original.frame.width >= railWidth },
            "without the sidebar the pane is wide enough for the rail: \(original.frame.width)")

        show.tap()
        XCTAssertTrue(app.buttons["hide-sidebar"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(
            MdocUITest.wait(timeout: MdocUITest.timeout) { abs(original.frame.width - beside) < 1 },
            "the column returns and the pane narrows again: \(original.frame.width)")
    }
}
