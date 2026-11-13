import XCTest

/// Edit mode with Show Markdown on (ADR 0109, slice 2): Edit, typing, ⌘Z
/// and ⇧⌘Z over the one undo line, Done, then a relaunch shows the text as
/// typed, saved by the core.
final class EditModeUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Words no fixture has, typed at the caret.
    static let marker = "Zephyrine notes "

    @MainActor
    func testTypeUndoLeaveAndRelaunch() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))

        // Edit is offered with Show Markdown on, as in live preview.
        XCTAssertTrue(app.buttons["edit-button"].waitForExistence(timeout: MdocUITest.timeout))
        app.buttons["document-menu"].tap()
        let showMarkdown = app.descendants(matching: .any)["show-markdown"].firstMatch
        XCTAssertTrue(showMarkdown.waitForExistence(timeout: MdocUITest.timeout))
        showMarkdown.tap()
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(edit.label, "Edit text")
        edit.tap()

        // Done shows and the review bar stays away; with no selection the
        // keyboard waits for a tap in the text (ADR 0110).
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(done.label, "Done editing")
        XCTAssertFalse(app.descendants(matching: .any)["review-bar"].firstMatch.exists)
        XCTAssertFalse(app.keyboards.firstMatch.waitForExistence(timeout: 2), "the keyboard waits for a tap")
        MdocUITest.tapIntoText(app)

        app.typeText(Self.marker)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", Self.marker), timeout: MdocUITest.timeout))

        // ⌘Z undoes the typing as one step; ⇧⌘Z makes it again. The first
        // hardware key after the software keyboard can be lost in the
        // switch (see `caretToTheEnd`): then ⌘Z goes again, which with the
        // typing the only step changes nothing more.
        let undone = NSPredicate(format: "NOT (value CONTAINS %@)", "Zephyrine")
        app.typeKey("z", modifierFlags: .command)
        if !text.wait(until: undone, timeout: 3) { app.typeKey("z", modifierFlags: .command) }
        XCTAssertTrue(text.wait(until: undone, timeout: 5))
        let redone = NSPredicate(format: "value CONTAINS %@", Self.marker)
        app.typeKey("z", modifierFlags: [.command, .shift])
        if !text.wait(until: redone, timeout: 3) { app.typeKey("z", modifierFlags: [.command, .shift]) }
        XCTAssertTrue(text.wait(until: redone, timeout: MdocUITest.timeout))

        // Done, then mdoc leaves the foreground: the typing is saved.
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        XCUIDevice.shared.press(.home)
        app.terminate()

        // The same library, as the last launch left it.
        let again = XCUIApplication()
        again.launchArguments = ["-mdoc-ui-testing"]
        again.launchEnvironment["MDOC_KEEP_LIBRARY"] = "1"
        again.launch()
        let row = MdocUITest.documentRow(again)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        let reopened = again.textViews["document-text"]
        XCTAssertTrue(reopened.waitForExistence(timeout: 10))
        XCTAssertTrue(reopened.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine notes"), timeout: 10))
    }

    // MARK: Modes (ADR 0110)

    /// Opens `seed`'s document, its text shown, optionally at a larger text size.
    @MainActor
    private func open(seed: String, title: String?, contentSize: String? = nil) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize { app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize] }
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launch()
        let row = title.map { MdocUITest.documentRow(app, title: $0) } ?? MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        XCTAssertTrue(app.buttons["edit-button"].waitForExistence(timeout: 30))
        return (app, text)
    }

    /// The pencil and Done keep the place in the text, and a selection
    /// made in edit mode survives Done and comes back with the keyboard on
    /// the next pencil.
    @MainActor
    func testPencilAndDoneKeepThePlaceAndTheSelection() throws {
        let (app, text) = open(
            seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle,
            contentSize: "UICTContentSizeCategoryAccessibilityL")
        let name = text.links[MdocUITest.markdownEmail].firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 30))
        // The text scrolled so the email, the one name, sits higher up:
        // its place says where the text is. Held at the end, no fling.
        let start = text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
        start.press(
            forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -max(name.frame.minY - 220, 120))),
            withVelocity: .slow, thenHoldForDuration: 0.3)
        MdocUITest.settledScreen()
        let window = app.windows.firstMatch.frame
        XCTAssertTrue(name.frame.minY > window.minY + 120 && name.frame.maxY < window.maxY - 200, "the email in view")
        let label = name.label
        let before = name.frame.minY

        let edit = app.buttons["edit-button"]
        edit.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        // No selection: nothing moves and the keyboard waits for a tap.
        XCTAssertFalse(app.buttons["toolbar-bold"].waitForExistence(timeout: 2), "the keyboard waits for a tap")
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        let after = text.links[label].firstMatch
        XCTAssertTrue(after.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.settledScreen()
        XCTAssertEqual(after.frame.minY, before, accuracy: 24, "the pencil and Done keep the place")

        // A word selected in edit mode stays selected through Done.
        edit.tap()
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        MdocUITest.caretToTheEnd(app)
        app.typeText("\nZephyrine")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Zephyrine"), timeout: MdocUITest.timeout))
        app.typeKey(.leftArrow, modifierFlags: [.option, .shift])
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        // The next pencil brings the keyboard up on it: typing replaces it.
        edit.tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "a selection brings the keyboard up")
        app.typeText("Quillon")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Quillon"), timeout: MdocUITest.timeout))
        XCTAssertFalse((text.value as? String ?? "").contains("Zephyrine"), "the selection was replaced")
    }

    /// iPhone: no Identities button and no right-edge swipe while editing,
    /// and Done brings the button back; Original while editing hides the
    /// keyboard and keeps edit mode, and Text brings the keyboard back at
    /// the same caret. (Two tests until ADR 0112's sweep, which saved a
    /// launch: Identities' header shows with no names too, so the swipe
    /// still has a drawer to open.)
    @MainActor
    func testIdentitiesHideAndTheOriginalRoundTripKeepsTheCaret() throws {
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "the iPhone drawer and switch")
        let (app, text) = open(seed: "tests/fixtures/import/text.pdf", title: nil)
        let identities = app.buttons["identities-button"]
        XCTAssertTrue(identities.exists)
        app.buttons["edit-button"].tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertFalse(identities.exists, "no Identities button while editing")
        // The right edge's swipe opens nothing.
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.995, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)))
        XCTAssertFalse(app.staticTexts["Identities"].firstMatch.waitForExistence(timeout: 2), "no drawer while editing")
        XCTAssertTrue(done.isHittable)
        MdocUITest.tapIntoText(app)
        app.typeText("Zephyrine ")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine "), timeout: MdocUITest.timeout))
        let original = app.segmentedControls.buttons["Original"].firstMatch
        XCTAssertTrue(original.exists, "the switch stays in edit mode")
        // A tap on the switch can be lost on a slow runner while the
        // keyboard is up (queue run 43386960651): tap until Original is
        // chosen, then the keyboard goes.
        let chosen = NSPredicate(format: "isSelected == true")
        for _ in 0..<3 {
            original.tap()
            if original.wait(until: chosen, timeout: MdocUITest.probeTimeout) { break }
        }
        XCTAssertTrue(original.isSelected, "Original is chosen")
        XCTAssertTrue(app.buttons["toolbar-bold"].wait(until: NSPredicate(format: "exists == false"), timeout: 5), "Original hides the keyboard")
        XCTAssertTrue(app.buttons["done-editing"].exists, "edit mode stays on")
        app.segmentedControls.buttons["Text"].firstMatch.tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "Text brings the keyboard back")
        app.typeText("Quillon")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine Quillon"), timeout: MdocUITest.timeout), "the same caret")
        done.tap()
        XCTAssertTrue(identities.waitForExistence(timeout: MdocUITest.timeout), "Done brings Identities back")
    }
}
