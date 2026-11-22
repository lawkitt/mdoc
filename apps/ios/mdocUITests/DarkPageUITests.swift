import UIKit
import XCTest

/// The Original's Dark page while the app is dark (#1036, #1070): on from
/// the start, the moon at the top switches it, its hint says where the moon
/// is and closes for good, and neither the title menu nor the text offers
/// it.
final class DarkPageUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "an iPad shows the Original beside the text")
    }

    /// The app in `theme` from its own Settings: the device's appearance,
    /// set from a test, did not reach the app on the hosted runner.
    @MainActor
    private func launchDark(showsTips: Bool, theme choice: String = "Dark") -> XCUIApplication {
        let app = MdocUITest.launch(showsTips: showsTips)
        MdocUITest.revealSidebar(app)
        let settings = app.buttons["settings-button"].firstMatch
        XCTAssertTrue(settings.waitUntilHittable(timeout: MdocUITest.timeout))
        settings.tap()
        let theme = app.descendants(matching: .any)["settings-theme"].firstMatch
        XCTAssertTrue(theme.waitForExistence(timeout: MdocUITest.timeout))
        theme.buttons[choice].tap()
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(theme.waitForNonExistence(timeout: MdocUITest.timeout))
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testThePageIsDarkAndTheMoonSwitchesIt() throws {
        let app = launchDark(showsTips: true)
        let moon = app.buttons["dark-page"]
        XCTAssertFalse(moon.exists, "over the text, no moon")
        let picker = app.segmentedControls["pane-picker"]
        picker.buttons["Original"].tap()
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        // The page's left margin: paper, dark from the start.
        let paper = CGPoint(x: page.frame.minX + 16, y: page.frame.minY + 120)
        XCTAssertTrue(MdocUITest.wait(timeout: 10) { MdocUITest.isDark(at: paper) == true }, "a dark page first")

        // The hint says where the moon is; the moon, on, switches it.
        let hint = app.buttons["dark-page-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(moon.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(moon.value as? String, "On")
        moon.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: paper) == false }, "the page is light")
        XCTAssertEqual(moon.value as? String, "Off")
        XCTAssertTrue(hint.waitForNonExistence(timeout: MdocUITest.timeout), "the moon teaches what the hint would")
        moon.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: paper) == true }, "dark again")

        // The title menu has no Dark page, nor Show Markdown over the
        // Original. Menu items by their words: iOS 27 drops their
        // identifiers.
        let title = app.buttons["title-menu"]
        title.tap()
        XCTAssertTrue(MdocUITest.menuItem("Find in document", app).waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(MdocUITest.menuItem("Dark page", app).exists, "no Dark page in the menu")
        XCTAssertFalse(MdocUITest.menuItem("Show Markdown", app).exists)
    }

    /// A tap on the hint closes it, the page as it was.
    @MainActor
    func testTheHintClosesWithATap() throws {
        let app = launchDark(showsTips: true)
        app.segmentedControls["pane-picker"].buttons["Original"].tap()
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        let paper = CGPoint(x: page.frame.minX + 16, y: page.frame.minY + 120)
        let hint = app.buttons["dark-page-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: MdocUITest.timeout))
        hint.tap()
        XCTAssertTrue(hint.waitForNonExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: paper) == true }, "still dark")
        // For good is TipKit's (`Hints.learn`): UI tests show every tip,
        // closed ones too, so a second look would find it again.
    }

    /// In a light app the page is light and no moon shows over it.
    @MainActor
    func testALightAppHasNoMoon() throws {
        let app = launchDark(showsTips: true, theme: "Light")
        app.segmentedControls["pane-picker"].buttons["Original"].tap()
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        let paper = CGPoint(x: page.frame.minX + 16, y: page.frame.minY + 120)
        XCTAssertTrue(MdocUITest.wait(timeout: 10) { MdocUITest.isDark(at: paper) == false }, "a light page")
        XCTAssertFalse(app.buttons["dark-page"].exists, "no moon")
        XCTAssertFalse(app.buttons["dark-page-hint"].exists, "no hint")
    }
}
