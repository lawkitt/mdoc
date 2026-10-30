import XCTest

/// Open → review → share → restore with the fake core (ADR 0085's short
/// XCUITest).
final class ReviewFlowUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testOpenReviewShareRestore() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launch()

        // Open: the fixture letter from Home.
        let letter = app.staticTexts["Letter before claim"].firstMatch
        XCTAssertTrue(letter.waitForExistence(timeout: 10))
        letter.tap()

        // Review: tap an underlined name, replace every occurrence.
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        let candidate = text.links["Maya Ellison"].firstMatch
        XCTAssertTrue(candidate.waitForExistence(timeout: 5))
        candidate.tap()
        let done = app.buttons["candidate-done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertTrue(app.textFields["candidate-alias"].exists)
        done.tap()
        XCTAssertTrue(text.links["PERSON_1"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(text.links["Maya Ellison"].exists)

        // Replace the remaining proposals, as reviewed.
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.label.hasPrefix("Replace"))
        primary.tap()

        // Share: the share sheet opens with the pseudonymized text.
        XCTAssertTrue(primary.waitForExistence(timeout: 5))
        XCTAssertTrue(primary.label.hasPrefix("Share"))
        primary.tap()
        app.buttons["Share text"].tap()
        let shareSheet = app.otherElements["ActivityListView"].firstMatch
        XCTAssertTrue(shareSheet.waitForExistence(timeout: 10))
        closeShareSheet(app)

        // Restore: paste the AI's reply into the document.
        app.buttons["paste-reply"].tap()
        let editor = app.textViews["reply-editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        editor.tap()
        editor.typeText("PERSON_1 should hear from ORG_1 by Friday. ORG_9 is new.")
        app.buttons["reply-restore"].tap()

        let restored = app.staticTexts["restored-text"]
        XCTAssertTrue(restored.waitForExistence(timeout: 5))
        XCTAssertTrue(restored.label.contains("Maya Ellison should hear from Brightwater Joinery Ltd by Friday."))
        let summary = app.staticTexts["restore-summary"]
        XCTAssertTrue(summary.label.contains("2 values restored"))
        XCTAssertTrue(summary.label.contains("1 unrecognised"))
    }

    @MainActor
    private func closeShareSheet(_ app: XCUIApplication) {
        let close = app.buttons["Close"].firstMatch
        if close.waitForExistence(timeout: 2) {
            close.tap()
        } else {
            app.otherElements["ActivityListView"].firstMatch.swipeDown(velocity: .fast)
        }
        XCTAssertTrue(app.buttons["paste-reply"].waitForExistence(timeout: 5))
    }
}
