import UIKit
import XCTest

/// Ask AI and "Paste reply?" (#1164 decision 11, Milestone 2 item 18):
/// hidden until Settings › AI connector (preview) is on; then Ask AI opens
/// the share sheet, and back from another app the banner restores the
/// pasted answer.
final class AskAIUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testAskAIThenPasteTheReplyOnReturn() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: 30))
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }
        if primary.label.hasPrefix("Replace") { primary.tap() }

        // Off by default: Share leads the bar and Ask AI is nowhere.
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))
        XCTAssertFalse(app.buttons["ask-ai"].exists)

        // Settings › AI connector (preview).
        MdocUITest.openSettings(app)
        let toggle = app.switches["settings-ai-connector-preview"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: MdocUITest.timeout))
        for _ in 0..<5 where !toggle.isHittable { app.swipeUp() }
        toggle.switches.firstMatch.tap()
        XCTAssertTrue(toggle.wait(until: NSPredicate(format: "value == '1'"), timeout: 5), "the preview turned on")
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(toggle.waitForNonExistence(timeout: MdocUITest.timeout))

        // Ask AI in Share's place: the share sheet, finished by an entry
        // that hands the text on (Copy).
        let document2 = MdocUITest.listedRow(app, title: MdocUITest.title)
        XCTAssertTrue(document2.waitForExistence(timeout: MdocUITest.timeout))
        document2.tap()
        let askAI = app.buttons["ask-ai"].firstMatch
        XCTAssertTrue(askAI.waitUntilHittable(timeout: MdocUITest.timeout))
        askAI.tap()
        let sheet = app.otherElements["ActivityListView"].firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10))
        let copy = sheet.descendants(matching: .any).matching(NSPredicate(format: "label == 'Copy'")).firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "Copy is not in the share sheet")
        for _ in 0..<3 where !copy.isHittable { sheet.swipeUp() }
        copy.tap()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 10), "the share sheet stayed open")
        let banner = app.descendants(matching: .any)["reply-banner"].firstMatch
        XCTAssertFalse(banner.exists, "not before mdoc is left")

        // To another app and back: "Paste reply?".
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: MdocUITest.timeout))
        app.activate()
        XCTAssertTrue(banner.waitForExistence(timeout: MdocUITest.timeout))

        MdocUITest.copy("Write to EMAIL_1 today.")
        MdocUITest.tapWhenEnabled(app.descendants(matching: .any)["reply-banner-paste"].firstMatch)
        let restored = app.textViews["restored-text"]
        XCTAssertTrue(restored.waitForExistence(timeout: 10))
        XCTAssertTrue(
            restored.wait(until: NSPredicate(format: "value CONTAINS %@", "Write to \(MdocUITest.email) today."), timeout: 10),
            "the pasted reply, restored")
        XCTAssertTrue(banner.waitForNonExistence(timeout: MdocUITest.timeout))
    }
}
