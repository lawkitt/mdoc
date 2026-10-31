import XCTest

/// A guide, like the accessibility audit: the main screens at the largest
/// accessibility text size, saved as PNGs to the folder in
/// `TEST_RUNNER_MDOC_SHOTS_DIR`. It skips without it.
final class LargeTextExploreUITests: XCTestCase {
    private var dir = ""

    override func setUpWithError() throws {
        dir = ProcessInfo.processInfo.environment["MDOC_SHOTS_DIR"] ?? ""
        try XCTSkipIf(dir.isEmpty, "exploration only")
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        continueAfterFailure = true
    }

    @MainActor
    func testLargestText() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing", "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(MdocUITest.defaultSeed).path
        app.launch()
        let row = app.descendants(matching: .any).matching(identifier: "document-row").firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        sleep(2)
        save(app, "01-home")
        app.buttons["settings-button"].firstMatch.tap()
        sleep(1)
        save(app, "02-settings")
        app.swipeUp()
        save(app, "03-settings-end")
        app.buttons["Done"].firstMatch.tap()
        row.tap()
        sleep(2)
        save(app, "04-review")
        let review = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Review'")).firstMatch
        if review.waitForExistence(timeout: 5) {
            review.tap()
            sleep(1)
            save(app, "05-candidate")
            app.swipeUp()
            save(app, "06-candidate-end")
            if app.buttons["candidate-done"].exists { app.buttons["candidate-done"].tap() }
        }
        if MdocUITest.showIdentities(app) {
            save(app, "07-identities")
            if app.buttons["identities-done"].exists { app.buttons["identities-done"].tap() }
        }
        let primary = app.buttons["primary-action"]
        if primary.exists && primary.label.hasPrefix("Replace") { primary.tap() }
        sleep(2)
        save(app, "08-replaced")
        if app.buttons["paste-reply"].waitForExistence(timeout: 5) {
            app.buttons["paste-reply"].tap()
            sleep(1)
            save(app, "09-reply-sheet")
        }
    }

    @MainActor
    private func save(_ app: XCUIApplication, _ name: String) {
        let png = app.screenshot().pngRepresentation
        do {
            try png.write(to: URL(fileURLWithPath: dir).appendingPathComponent(name + ".png"))
        } catch {
            XCTFail("could not save \(name): \(error)")
        }
    }
}
