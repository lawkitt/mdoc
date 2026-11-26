import UIKit
import XCTest

/// "I checked the names myself" (Vault & Projects item 48): below the
/// floor, where the UI tests run, a pasted document's Copy for AI, Share,
/// Ask AI and the title menu's Share each ask first. Cancel hands out nothing;
/// confirming goes on with the same exit. It asks again after an edit, and
/// never for unchanged text, relaunched too.
final class NamesCheckedUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private static let title = "Lunch notes"
    private static let text = "Lunch notes\n\nThe park is lovely today."

    // MARK: Steps

    /// The + menu's Paste: the note opens as a new Unfiled document, and
    /// the clipboard is emptied after.
    @MainActor
    private func pasted() -> XCUIApplication {
        MdocUITest.copy(Self.text)
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launch()
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let paste = MdocUITest.menuItem("Paste", app)
        XCTAssertTrue(paste.waitForExistence(timeout: MdocUITest.timeout))
        paste.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(
            app.buttons["primary-action"].wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout),
            "the bar offers no Share")
        clearClipboard()
        return app
    }

    /// The question, once it shows.
    @MainActor
    private func question(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        let alert = app.alerts["Check the names yourself"]
        XCTAssertTrue(alert.waitForExistence(timeout: MdocUITest.timeout), "no question", file: file, line: line)
        XCTAssertTrue(
            alert.staticTexts["This phone can't find names. Read the text and mark any name still in it before it goes to an AI."].exists,
            "its sentence", file: file, line: line)
        XCTAssertTrue(alert.buttons["I checked the names myself"].exists, file: file, line: line)
        XCTAssertTrue(alert.buttons["Cancel"].exists, file: file, line: line)
        return alert
    }

    /// No question shows.
    @MainActor
    private func expectNoQuestion(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertFalse(
            app.alerts["Check the names yourself"].waitForExistence(timeout: MdocUITest.probeTimeout), "asked", file: file,
            line: line)
    }

    private func clearClipboard() {
        UIPasteboard.general.items = []
    }

    /// Copy on the bar, with the clipboard emptied first.
    @MainActor
    private func tapCopy(_ app: XCUIApplication) {
        clearClipboard()
        let copy = app.buttons["copy-for-ai"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "no Copy on the bar")
        copy.tap()
    }

    /// Something reached the clipboard, or nothing did. (Reading the copy
    /// itself would ask to paste from mdoc; HandOffRefusalTests and
    /// NamesCheckedTests check the text.)
    @MainActor
    private func expectCopied(_ copied: Bool, file: StaticString = #filePath, line: UInt = #line) {
        if copied {
            XCTAssertTrue(
                MdocUITest.wait(timeout: MdocUITest.timeout) { UIPasteboard.general.hasStrings }, "nothing copied",
                file: file, line: line)
        } else {
            XCTAssertFalse(
                MdocUITest.wait(timeout: MdocUITest.probeTimeout) { UIPasteboard.general.hasStrings }, "copied",
                file: file, line: line)
        }
    }

    /// The share sheet shows, with no question left over it.
    @MainActor
    private func expectShareSheet(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let sheet = app.otherElements["ActivityListView"].firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: MdocUITest.timeout), "no share sheet", file: file, line: line)
        XCTAssertFalse(app.alerts.firstMatch.exists, "an alert over it", file: file, line: line)
    }

    // MARK: Tests

    /// Copy asks; Cancel hands out nothing and the next Copy asks again;
    /// confirming copies. Unchanged, it never asks again, relaunched too.
    @MainActor
    func testCopyAsksUntilTheNamesAreChecked() throws {
        let app = pasted()
        tapCopy(app)
        question(app).buttons["Cancel"].tap()
        expectCopied(false)

        tapCopy(app)
        question(app).buttons["I checked the names myself"].tap()
        expectCopied(true)

        tapCopy(app)
        expectNoQuestion(app)
        expectCopied(true)

        app.terminate()
        let again = XCUIApplication()
        again.launchArguments = ["-mdoc-ui-testing"]
        again.launchEnvironment["MDOC_KEEP_LIBRARY"] = "1"
        again.launch()
        let row = MdocUITest.listedRow(again, title: Self.title)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        XCTAssertTrue(again.textViews["document-text"].waitForExistence(timeout: MdocUITest.timeout))
        tapCopy(again)
        expectNoQuestion(again)
        expectCopied(true)
    }

    /// Checked, then a sentence typed in edit mode: the next Copy asks
    /// again.
    @MainActor
    func testAnEditAsksAgain() throws {
        let app = pasted()
        tapCopy(app)
        question(app).buttons["I checked the names myself"].tap()
        expectCopied(true)

        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        edit.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        MdocUITest.caretToTheEnd(app)
        app.typeText(" Bring a coat.")
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS 'Bring a coat.'"), timeout: MdocUITest.timeout))
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))

        tapCopy(app)
        question(app).buttons["Cancel"].tap()
        expectCopied(false)
    }

    /// The bar's Share asks, and confirming opens the share sheet.
    @MainActor
    func testShareAsksThenShares() throws {
        let app = pasted()
        app.buttons["primary-action"].tap()
        question(app).buttons["I checked the names myself"].tap()
        expectShareSheet(app)
    }

    /// Ask AI asks, and confirming opens the share sheet for the AI app.
    @MainActor
    func testAskAIAsksThenShares() throws {
        let app = pasted()
        MdocUITest.openSettings(app)
        let toggle = app.switches["settings-ai-connector-preview"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: MdocUITest.timeout))
        for _ in 0..<5 where !toggle.isHittable { app.swipeUp() }
        toggle.switches.firstMatch.tap()
        XCTAssertTrue(toggle.wait(until: NSPredicate(format: "value == '1'"), timeout: 5), "the preview turned on")
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(toggle.waitForNonExistence(timeout: MdocUITest.timeout))
        let row = MdocUITest.listedRow(app, title: Self.title)
        XCTAssertTrue(row.waitForExistence(timeout: MdocUITest.timeout))
        row.tap()
        let ask = app.buttons["ask-ai"].firstMatch
        XCTAssertTrue(ask.waitForExistence(timeout: MdocUITest.timeout), "no Ask AI")
        ask.tap()
        question(app).buttons["Cancel"].tap()
        XCTAssertFalse(app.otherElements["ActivityListView"].firstMatch.exists, "shared on Cancel")
        ask.tap()
        question(app).buttons["I checked the names myself"].tap()
        expectShareSheet(app)
    }

    /// The title menu's Share, which a row's Share shares its path with,
    /// asks, and confirming shares from there. (A row lists Share only for
    /// a scanned document, which never asks.)
    @MainActor
    func testTheMenusShareAsksThenShares() throws {
        let app = pasted()
        app.buttons["title-menu"].tap()
        let share = MdocUITest.menuItem("Share", app)
        XCTAssertTrue(share.waitForExistence(timeout: MdocUITest.timeout), "no Share in the menu")
        XCTAssertTrue(share.isEnabled)
        share.tap()
        question(app).buttons["I checked the names myself"].tap()
        expectShareSheet(app)
    }
}
