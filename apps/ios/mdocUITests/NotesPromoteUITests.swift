import XCTest

/// Document notes and Promote to Document (#1091 Decision 3) through the
/// screens: Notes from the document's menu keeps what was typed; Promote
/// on the Reply pane opens a new document whose menu leads back to the
/// reply.
final class NotesPromoteUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// The seeded agreement, open and with every name replaced.
    @MainActor
    private func openReviewed() -> XCUIApplication {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: 30))
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }
        if primary.label.hasPrefix("Replace") { primary.tap() }
        return app
    }

    @MainActor
    private func openNotes(_ app: XCUIApplication) -> XCUIElement {
        app.buttons["title-menu"].tap()
        let notes = MdocUITest.menuItem("Notes", app)
        XCTAssertTrue(notes.waitForExistence(timeout: MdocUITest.timeout))
        notes.tap()
        let editor = app.textViews["notes-editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: MdocUITest.timeout))
        return editor
    }

    /// Notes typed and closed are there when Notes opens again, under the
    /// line that says they stay on the device.
    @MainActor
    func testNotesKeepWhatWasTyped() throws {
        let app = openReviewed()
        let editor = openNotes(app)
        XCTAssertTrue(app.descendants(matching: .any)["notes-hint"].firstMatch.exists)
        if !editor.wait(until: NSPredicate(format: "hasKeyboardFocus == true"), timeout: 5) { editor.tap() }
        editor.typeText("Ring the bank on Monday.")
        app.buttons["notes-done"].tap()
        XCTAssertTrue(editor.waitForNonExistence(timeout: MdocUITest.timeout))

        let again = openNotes(app)
        XCTAssertTrue(
            again.wait(until: NSPredicate(format: "value CONTAINS %@", "Ring the bank on Monday."), timeout: 10),
            "the notes, kept")
    }

    /// Promote on the Reply pane: a new document opens, titled after the
    /// reply, saying where it came from; its menu shows the reply restored.
    @MainActor
    func testAPromotedReplyLeadsBackToTheReply() throws {
        let app = openReviewed()
        MdocUITest.pasteReply("Call EMAIL_1 back.", in: app)
        let restored = app.textViews["restored-text"]
        XCTAssertTrue(restored.waitForExistence(timeout: 10))

        let promote = app.buttons["promote-reply"].firstMatch
        XCTAssertTrue(promote.waitUntilHittable(timeout: MdocUITest.timeout))
        promote.tap()
        let title = app.buttons["title-menu"]
        XCTAssertTrue(
            title.wait(until: NSPredicate(format: "label BEGINSWITH 'Reply to'"), timeout: MdocUITest.timeout),
            "the new document opens")
        XCTAssertEqual(title.value as? String, "From AI reply")
        let text = app.textViews["document-text"]
        XCTAssertTrue(
            text.wait(until: NSPredicate(format: "value CONTAINS %@", MdocUITest.email), timeout: 10),
            "the reply's text with the real name back")

        app.buttons["title-menu"].tap()
        let back = MdocUITest.menuItem("From AI reply", app)
        XCTAssertTrue(back.waitForExistence(timeout: MdocUITest.timeout))
        back.tap()
        let shown = app.textViews["restored-text"]
        XCTAssertTrue(shown.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(
            shown.wait(until: NSPredicate(format: "value CONTAINS %@", "Call \(MdocUITest.email) back."), timeout: 10))
    }
}
