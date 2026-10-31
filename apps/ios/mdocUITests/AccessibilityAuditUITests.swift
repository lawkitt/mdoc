import XCTest

/// Exploration: runs Xcode's accessibility audit on the main screens and
/// prints every issue instead of failing. Only with
/// `TEST_RUNNER_MDOC_A11Y_EXPLORE=1`.
final class AccessibilityAuditExploreUITests: XCTestCase {
    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_A11Y_EXPLORE"] == "1", "exploration only")
        continueAfterFailure = true
    }

    @MainActor
    func testAuditMainScreens() throws {
        let app = MdocUITest.launch()
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        audit(app, "home")
        row.tap()
        XCTAssertTrue(app.buttons["primary-action"].waitForExistence(timeout: 10))
        audit(app, "review")
        let email = app.textViews.firstMatch.links[MdocUITest.email].firstMatch
        if email.waitForExistence(timeout: 5) {
            email.tap()
            if app.buttons["candidate-done"].waitForExistence(timeout: 5) {
                audit(app, "candidate-sheet")
                app.buttons["candidate-done"].tap()
            }
        }
        if MdocUITest.showIdentities(app) {
            audit(app, "identities")
            if app.buttons["identities-done"].exists { app.buttons["identities-done"].tap() }
        }
        let primary = app.buttons["primary-action"]
        if primary.label.hasPrefix("Replace") { primary.tap() }
        if app.buttons["paste-reply"].waitForExistence(timeout: 10) {
            audit(app, "replaced")
            app.buttons["paste-reply"].tap()
            if app.buttons["reply-restore"].waitForExistence(timeout: 5) {
                audit(app, "reply-sheet")
                app.buttons["Cancel"].firstMatch.tap()
            }
        }
    }

    @MainActor
    private func audit(_ app: XCUIApplication, _ screen: String) {
        do {
            try app.performAccessibilityAudit { issue in
                let element = issue.element.map { "\($0.elementType.rawValue) '\($0.label)' id=\($0.identifier)" } ?? "-"
                print("A11Y[\(screen)] \(issue.auditType) | \(issue.compactDescription) | \(element)")
                return true
            }
        } catch {
            print("A11Y[\(screen)] audit failed: \(error)")
        }
    }
}
