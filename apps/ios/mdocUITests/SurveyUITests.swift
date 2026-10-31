import UIKit
import XCTest

/// The UI survey (ADR 0087): every screen the app shows, light and dark,
/// kept as test attachments named `NN-screen-light` and `NN-screen-dark`.
/// Skipped unless the run sets `MDOC_SURVEY=1` (`TEST_RUNNER_MDOC_SURVEY=1`
/// through xcodebuild); `scripts/ios-survey.sh` runs it and exports the
/// shots. Runs on iPhone and iPad; a few phone-only steps skip on iPad.
final class SurveyUITests: XCTestCase {
    private var isPad: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_SURVEY"] == "1", "survey not requested")
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
        super.tearDown()
    }

    /// A first run at the detector tier: setup, paste, import, Settings.
    @MainActor
    func test1FirstRun() throws {
        let app = launch(seed: nil, detectorTier: true)
        XCTAssertTrue(app.buttons["setup-download"].waitForExistence(timeout: 30))
        shot("01-home-empty-setup")

        app.buttons["add-menu"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Paste"].waitForExistence(timeout: 5))
        shot("02-add-menu")
        app.buttons["Paste"].tap()
        let editor = app.textViews.firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        shot("03-paste-sheet-empty")
        editor.tap()
        editor.typeText(Self.pastedText)
        shot("04-paste-sheet-filled")
        nextOrAdd(app)

        if app.buttons["import-add"].waitForExistence(timeout: 5) {
            shot("05-import-sheet")
            app.buttons["import-add"].tap()
        }
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        shot("06-document-before-scan")
        app.buttons["primary-action"].tap()
        XCTAssertTrue(app.buttons["setup-download"].waitForExistence(timeout: 10))
        shot("07-document-needs-setup")

        goHome(app)
        XCTAssertTrue(app.descendants(matching: .any)["document-row"].firstMatch.waitForExistence(timeout: 10))
        shot("08-home-one-document")
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.buttons["About mdoc"].waitForExistence(timeout: 5))
        shot("09-settings")
        app.buttons["About mdoc"].tap()
        XCTAssertTrue(app.staticTexts["Version"].waitForExistence(timeout: 5))
        shot("10-about")
    }

    /// The review of the demo agreement on the rules, through share,
    /// restore and the reply.
    @MainActor
    func test2Review() throws {
        let app = launch(seed: MdocUITest.defaultSeed)
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        shot("11-home-rules-tier-to-review")
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        shot("12-review")
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        scroll(text, toReveal: email)
        shot("13-review-candidate")

        email.tap()
        XCTAssertTrue(app.buttons["candidate-done"].waitForExistence(timeout: 5))
        shot("14-candidate-sheet")
        if !isPad {
            let bar = app.navigationBars.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1))
            bar.press(forDuration: 0.2, thenDragTo: app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.05)))
            sleep(1)
            shot("15-candidate-sheet-large")
        }
        app.buttons["candidate-done"].tap()
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))

        // On iPad the list already sits beside the text.
        if !app.staticTexts["Identities"].waitForExistence(timeout: 2) {
            app.buttons["identities-button"].tap()
        }
        XCTAssertTrue(app.staticTexts["Identities"].waitForExistence(timeout: 5))
        shot("17-identities")
        if !isPad {
            closeSheet(app)
        }

        // Replace… for a selected word: the title's first word, from the top.
        for _ in 0..<4 {
            text.swipeDown(velocity: .fast)
        }
        text.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.04)).press(forDuration: 1.0)
        let replace = app.menuItems["Replace…"].firstMatch
        XCTAssertTrue(replace.waitForExistence(timeout: 5), "no Replace… in the edit menu")
        shot("18-selection-edit-menu")
        replace.tap()
        XCTAssertTrue(app.navigationBars["Replace"].waitForExistence(timeout: 5))
        shot("19-manual-replace-sheet")
        app.buttons["Cancel"].firstMatch.tap()

        let primary = app.buttons["primary-action"]
        if primary.label.hasPrefix("Replace") {
            primary.tap()
        }
        XCTAssertTrue(app.buttons["paste-reply"].waitForExistence(timeout: 10))
        shot("16-review-replaced")
        share(app)
        shot("21-share-sheet")
        restoreInExtension(app)

        app.buttons["paste-reply"].tap()
        let reply = app.textViews["reply-editor"]
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        shot("23-reply-sheet-empty")
        reply.tap()
        reply.typeText(Self.replyText)
        app.buttons["reply-restore"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["restore-summary"].waitForExistence(timeout: 10))
        shot("24-reply-pane")
        app.buttons["reply-actions"].firstMatch.tap()
        sleep(1)
        shot("25-reply-actions-menu")
    }

    /// A PDF: its text and the original page.
    @MainActor
    func test3Original() throws {
        let app = launch(seed: "tests/fixtures/import/text.pdf")
        let row = app.descendants(matching: .any)["document-row"].firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        if isPad {
            sleep(2)
            shot("26-pdf-text-original-identities")
            return
        }
        shot("26-pdf-text")
        app.buttons["Original"].firstMatch.tap()
        sleep(2)
        shot("27-pdf-original")
    }

    /// The Markdown-rich fixture, and the review at a large text size.
    @MainActor
    func test4Markdown() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        shot("28-markdown-review")
        text.swipeUp(velocity: .fast)
        sleep(1)
        shot("29-markdown-review-end")

        let large = launch(seed: MdocUITest.markdownFixture, contentSize: "UICTContentSizeCategoryAccessibilityXL")
        let largeRow = MdocUITest.documentRow(large, title: MdocUITest.markdownTitle)
        // The setup card can fill the screen at this size; the list makes
        // rows only as they scroll in.
        var swipes = 0
        while !largeRow.waitForExistence(timeout: 10), swipes < 6 {
            large.swipeUp()
            swipes += 1
        }
        XCTAssertTrue(largeRow.exists)
        largeRow.tap()
        let largeText = large.textViews["document-text"]
        XCTAssertTrue(largeText.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        scroll(largeText, toReveal: largeText.links[MdocUITest.markdownEmail].firstMatch)
        shot("30-review-ax-text")
    }

    // MARK: Steps

    private static let pastedText = """
        # Settlement agreement

        Ms Maya Bennett of Flat 3, 41 Larch Road, Bristol agrees the terms below \
        with Harrow & Finch Ltd. Write to maya.bennett@example.com.
        """
    private static let replyText = "Thanks. Write to EMAIL_1 today about the payment. EMAIL_9 is new.\n\n## Next steps\n\n- Confirm the **£18,500** figure\n- Send the draft to EMAIL_1"

    @MainActor
    private func launch(seed: String?, detectorTier: Bool = false, contentSize: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize {
            app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize]
        }
        if let seed {
            app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        }
        if detectorTier {
            app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1"
        }
        app.launch()
        return app
    }

    /// Paste's sheet ends in Next (then the import sheet) or Add.
    @MainActor
    private func nextOrAdd(_ app: XCUIApplication) {
        let next = app.buttons["Next"].firstMatch
        if next.exists {
            next.tap()
        } else {
            app.buttons["import-add"].firstMatch.tap()
        }
    }

    @MainActor
    private func goHome(_ app: XCUIApplication) {
        if isPad { return }
        app.navigationBars.buttons.element(boundBy: 0).tap()
    }

    @MainActor
    private func closeSheet(_ app: XCUIApplication) {
        let done = app.buttons["Done"].firstMatch
        if done.waitForExistence(timeout: 1) {
            done.tap()
        } else {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()
        }
        sleep(1)
    }

    /// Share: one tap, or the older Share menu's "Share text".
    @MainActor
    private func share(_ app: XCUIApplication) {
        app.buttons["primary-action"].tap()
        let menuItem = app.buttons["Share text"]
        if menuItem.waitForExistence(timeout: 2) {
            shot("20-share-menu")
            menuItem.tap()
        }
        XCTAssertTrue(app.otherElements["ActivityListView"].firstMatch.waitForExistence(timeout: 10))
        sleep(1)
    }

    @MainActor
    private func restoreInExtension(_ app: XCUIApplication) {
        let sheet = app.otherElements["ActivityListView"].firstMatch
        let restore = sheet.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN {'Restore with mdoc', 'mdoc'}"))
            .firstMatch
        XCTAssertTrue(restore.waitForExistence(timeout: 5), "Restore with mdoc is not in the share sheet")
        restore.tap()
        // The extension runs in its own process; its title may not be in
        // the app's tree, so give its sheet time to show.
        _ = app.staticTexts["Restore with mdoc"].waitForExistence(timeout: 10)
        sleep(2)
        shot("22-restore-extension")
        let done = app.buttons["Done"].firstMatch
        if done.waitForExistence(timeout: 3) {
            done.tap()
        }
        _ = app.buttons["paste-reply"].waitForExistence(timeout: 10)
    }

    @MainActor
    private func scroll(_ text: XCUIElement, toReveal element: XCUIElement) {
        var tries = 0
        while tries < 8, !(element.isHittable && element.frame.maxY < text.frame.maxY - 80) {
            text.swipeUp(velocity: .slow)
            tries += 1
        }
    }

    /// The screen in light, then in dark.
    @MainActor
    private func shot(_ name: String) {
        for (appearance, suffix) in [(XCUIDevice.Appearance.light, "light"), (.dark, "dark")] {
            XCUIDevice.shared.appearance = appearance
            sleep(1)
            let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
            attachment.name = "\(name)-\(suffix)"
            attachment.lifetime = .keepAlways
            add(attachment)
        }
        XCUIDevice.shared.appearance = .light
    }
}
