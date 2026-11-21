import UIKit
import XCTest

/// The Original's Dark page while the app is dark (#1036): the hint turns
/// it on once, the title menu's toggle turns it off and on, Show Markdown
/// is not in that menu, and the page itself goes dark.
final class DarkPageUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "an iPad shows the Original beside the text")
        XCUIDevice.shared.appearance = .dark
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
    }

    @MainActor
    func testTheHintAndTheMenuTurnThePageDark() throws {
        let app = MdocUITest.launch(showsTips: true)
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))

        let picker = app.segmentedControls["pane-picker"]
        let original = picker.buttons["Original"]
        XCTAssertTrue(original.waitForExistence(timeout: MdocUITest.timeout))
        original.tap()
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        // The page's left margin, white paper until Dark page.
        let paper = CGPoint(x: page.frame.minX + 16, y: page.frame.minY + 120)
        XCTAssertTrue(MdocUITest.wait(timeout: 10) { MdocUITest.isDark(at: paper) == false }, "a light page first")

        // The hint, once: it turns the page dark and says where the toggle is.
        let hint = app.buttons["dark-page-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: MdocUITest.timeout))
        hint.tap()
        XCTAssertTrue(hint.waitForNonExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(app.descendants(matching: .any)["toast"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: paper) == true }, "the page is dark")

        // The title menu: Dark page on, no Show Markdown over the Original.
        let title = app.buttons["title-menu"]
        title.tap()
        let toggle = app.descendants(matching: .any)["dark-page"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.descendants(matching: .any)["show-markdown"].firstMatch.exists)
        toggle.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: paper) == false }, "the page is light again")
        XCTAssertFalse(hint.exists, "the hint is learned")

        // Back on Text, Show Markdown is in the menu and Dark page is not.
        if title.isHittable == false { app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)).tap() }
        picker.buttons["Text"].tap()
        title.tap()
        XCTAssertTrue(app.descendants(matching: .any)["show-markdown"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.descendants(matching: .any)["dark-page"].firstMatch.exists)
    }
}
