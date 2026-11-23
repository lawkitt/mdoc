import UIKit
import XCTest

/// The Original's chrome on iPhone: over the Original no bottom bar and no
/// Identities, a double-tap zooms and ⤡ fits again, and Text brings the review
/// back as it was; on Text, ↑ goes back to the top after a screen of
/// scrolling.
final class OriginalChromeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "an iPad shows the Original beside the text")
    }

    @MainActor
    func testALockedScanCanShowItsOriginalWhileImportNeedsTheOCRModel() throws {
        let app = MdocUITest.launch(seed: "tests/fixtures/pdf-source/partly-scanned-aes256.pdf")
        let row = MdocUITest.listedRow(
            app, NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH 'partly-scanned-aes256'"))
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()

        let importPassword = app.secureTextFields["document-password"]
        XCTAssertTrue(importPassword.waitForExistence(timeout: MdocUITest.timeout))
        let picker = app.segmentedControls["pane-picker"]
        picker.buttons["Original"].tap()
        XCTAssertTrue(importPassword.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.secureTextFields["original-password"].exists)
        importPassword.tap()
        importPassword.typeText("Petrov-2026")
        app.buttons["unlock-document"].tap()
        XCTAssertTrue(importPassword.waitForNonExistence(timeout: MdocUITest.timeout))

        picker.buttons["Text"].tap()
        XCTAssertTrue(app.buttons["setup-download"].waitForExistence(timeout: MdocUITest.timeout))
        picker.buttons["Original"].tap()
        let originalPassword = app.secureTextFields["original-password"]
        XCTAssertTrue(originalPassword.waitForExistence(timeout: MdocUITest.timeout))
        originalPassword.tap()
        originalPassword.typeText("Petrov-2026")
        app.buttons["unlock-original"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["Original PDF"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
    }

    @MainActor
    func testTheOriginalHidesTheBarsAndTextBringsThemBack() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["primary-action"].waitForExistence(timeout: 30))

        // ↑ after a screen of text, back to the top.
        let control = app.buttons["page-control"]
        for _ in 0..<4 where !control.exists {
            text.swipeUp(velocity: .fast)
        }
        XCTAssertTrue(control.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(control.label, "Back to top")
        control.tap()
        XCTAssertTrue(control.waitForNonExistence(timeout: MdocUITest.timeout))

        // A name under review.
        let name = text.links.firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapAboveBottomBar(name, in: text, app)
        let bar = app.descendants(matching: .any)["review-bar"].firstMatch
        XCTAssertTrue(bar.waitForExistence(timeout: MdocUITest.timeout))
        let progress = app.buttons["review-progress"]
        let place = progress.label
        XCTAssertFalse(control.exists, "no ↑ while a name is under review")

        // The Original: no review bar, no Identities, no Find names.
        let picker = app.segmentedControls["pane-picker"]
        let original = picker.buttons["Original"]
        original.tap()
        XCTAssertTrue(original.wait(until: NSPredicate(format: "isSelected == true"), timeout: 15))
        XCTAssertTrue(bar.waitForNonExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(app.buttons["identities-button"].exists)
        XCTAssertFalse(app.buttons["primary-action"].exists)

        // A double-tap zooms in; ⤡ fits the page again.
        let page = app.descendants(matching: .any)["docx-original"].firstMatch
        XCTAssertTrue(page.waitForExistence(timeout: 60))
        page.doubleTap()
        XCTAssertTrue(control.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(control.label, "Fit page")
        XCTAssertGreaterThanOrEqual(control.frame.width, 44)
        control.tap()
        XCTAssertTrue(control.waitForNonExistence(timeout: MdocUITest.timeout))

        // Text: the review as it was.
        picker.buttons["Text"].tap()
        XCTAssertTrue(bar.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(progress.label, place)
        XCTAssertTrue(app.buttons["identities-button"].exists)
    }
}
