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
        XCTAssertTrue(app.buttons["edit-button"].waitForExistence(timeout: 5))
        app.buttons["document-menu"].tap()
        let showMarkdown = app.descendants(matching: .any)["show-markdown"].firstMatch
        XCTAssertTrue(showMarkdown.waitForExistence(timeout: 5))
        showMarkdown.tap()
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        XCTAssertEqual(edit.label, "Edit text")
        edit.tap()

        // Done shows and the review bar stays away; with no selection the
        // keyboard waits for a tap in the text (ADR 0110).
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertEqual(done.label, "Done editing")
        XCTAssertFalse(app.descendants(matching: .any)["review-bar"].firstMatch.exists)
        XCTAssertFalse(app.keyboards.firstMatch.waitForExistence(timeout: 2), "the keyboard waits for a tap")
        MdocUITest.tapIntoText(app)

        app.typeText(Self.marker)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", Self.marker), timeout: 5))

        // ⌘Z undoes the typing as one step; ⇧⌘Z makes it again.
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "Zephyrine"), timeout: 5))
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", Self.marker), timeout: 5))

        // Done, then mdoc leaves the foreground: the typing is saved.
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
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
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        text.swipeUp()
        MdocUITest.settledScreen()
        let window = app.windows.firstMatch.frame
        // A name in view below the bar, whose place says where the text is.
        guard let name = text.links.allElementsBoundByIndex.first(where: {
            $0.frame.minY > window.minY + 150 && $0.frame.maxY < window.maxY - 150
        }) else { return XCTFail("a name in view after the scroll") }
        let label = name.label
        let before = name.frame.minY

        let edit = app.buttons["edit-button"]
        edit.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        // No selection: nothing moves and the keyboard waits for a tap.
        XCTAssertFalse(app.buttons["toolbar-bold"].waitForExistence(timeout: 2), "the keyboard waits for a tap")
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        let after = text.links[label].firstMatch
        XCTAssertTrue(after.waitForExistence(timeout: 5))
        MdocUITest.settledScreen()
        XCTAssertEqual(after.frame.minY, before, accuracy: 24, "the pencil and Done keep the place")

        // A word selected in edit mode stays selected through Done.
        edit.tap()
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        MdocUITest.caretToTheEnd(app)
        app.typeText("\nZephyrine")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Zephyrine"), timeout: 5))
        app.typeKey(.leftArrow, modifierFlags: [.option, .shift])
        done.tap()
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        // The next pencil brings the keyboard up on it: typing replaces it.
        edit.tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "a selection brings the keyboard up")
        app.typeText("Quillon")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Quillon"), timeout: 5))
        XCTAssertFalse((text.value as? String ?? "").contains("Zephyrine"), "the selection was replaced")
    }

    /// iPhone: no Identities button and no right-edge swipe while editing;
    /// Done brings the button back.
    @MainActor
    func testIdentitiesHiddenWhileEditing() throws {
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "the iPhone drawer")
        let (app, text) = open(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        let identities = app.buttons["identities-button"]
        XCTAssertTrue(identities.exists)
        app.buttons["edit-button"].tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertFalse(identities.exists, "no Identities button while editing")
        // The right edge's swipe opens nothing.
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.995, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)))
        XCTAssertFalse(app.staticTexts["Identities"].firstMatch.waitForExistence(timeout: 2), "no drawer while editing")
        XCTAssertTrue(done.isHittable)
        done.tap()
        XCTAssertTrue(identities.waitForExistence(timeout: 5))
    }

    /// iPhone: Original while editing hides the keyboard and keeps edit
    /// mode; Text brings the keyboard back at the same caret.
    @MainActor
    func testTheOriginalRoundTripKeepsTheCaret() throws {
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .pad, "the iPhone switch")
        let (app, text) = open(seed: "tests/fixtures/import/text.pdf", title: nil)
        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        app.typeText("Zephyrine ")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine "), timeout: 5))
        let original = app.segmentedControls.buttons["Original"].firstMatch
        XCTAssertTrue(original.exists, "the switch stays in edit mode")
        original.tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].wait(until: NSPredicate(format: "exists == false"), timeout: 5), "Original hides the keyboard")
        XCTAssertTrue(app.buttons["done-editing"].exists, "edit mode stays on")
        app.segmentedControls.buttons["Text"].firstMatch.tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "Text brings the keyboard back")
        app.typeText("Quillon")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine Quillon"), timeout: 5), "the same caret")
    }
}
