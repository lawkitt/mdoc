import UIKit
import XCTest

/// A Word file's comments on a phone (originals.md, Phones): no bar over
/// the page; Show comments and "Not on the page (N)" in the title menu;
/// markers that open their thread, and go with Show comments off.
final class CommentsOnPageUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "an iPad's wide pane shows the rail")
    }

    @MainActor
    func testCommentsAreInTheTitleMenu() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] =
            MdocUITest.repo.appendingPathComponent("tests/fixtures/docx-preview/comment-anchors.docx").path
        app.launch()
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))

        let original = app.segmentedControls["pane-picker"].buttons["Original"]
        original.tap()
        XCTAssertTrue(original.wait(until: NSPredicate(format: "isSelected == true"), timeout: 15))
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        let markers = app.buttons.matching(identifier: "comment-marker")
        XCTAssertTrue(markers.firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.buttons["show-comments"].exists, "no comments bar over the page")

        // A marker opens its thread.
        markers.firstMatch.tap()
        let sheet = app.descendants(matching: .any)["comment-sheet"].firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: MdocUITest.timeout))
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: MdocUITest.timeout))

        // The title menu: Show comments with the count, Not on the page.
        let title = app.buttons["title-menu"]
        title.tap()
        let show = app.descendants(matching: .any)["show-comments"].firstMatch
        XCTAssertTrue(show.waitForExistence(timeout: MdocUITest.timeout))
        // The count under the toggle joins its label in the menu.
        let count = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "13 comments"))
        XCTAssertTrue(count.firstMatch.exists, "every comment counts, replies too")
        let notOnPage = app.descendants(matching: .any)["comments-not-on-page"].firstMatch
        XCTAssertTrue(notOnPage.exists)
        XCTAssertTrue(notOnPage.label.contains("Not on the page (1)"), notOnPage.label)
        notOnPage.tap()
        XCTAssertTrue(sheet.waitForExistence(timeout: MdocUITest.timeout))
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: MdocUITest.timeout))

        // Show comments off: no markers, and no Not on the page.
        title.tap()
        XCTAssertTrue(show.waitForExistence(timeout: MdocUITest.timeout))
        show.tap()
        XCTAssertTrue(markers.firstMatch.waitForNonExistence(timeout: MdocUITest.timeout))
        title.tap()
        XCTAssertTrue(show.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.descendants(matching: .any)["comments-not-on-page"].firstMatch.exists)
        show.tap()
        XCTAssertTrue(markers.firstMatch.waitForExistence(timeout: MdocUITest.timeout))
    }
}
