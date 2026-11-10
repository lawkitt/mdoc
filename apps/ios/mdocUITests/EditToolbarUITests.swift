import XCTest

/// Edit mode's toolbar, formatting keyboard, links and hardware keys (ADR
/// 0109, slice 4), in live preview: the capsule above the keyboard makes a
/// word bold and shows Bold on; Aa swaps in the formatting keyboard, whose
/// keys turn the line into a heading and whose keyboard key brings the
/// system keyboard back; ⌘B, Tab and ⌘K work from a hardware keyboard; and
/// the accessibility audit passes with the toolbar up.
final class EditToolbarUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Opens the Markdown fixture and enters edit mode in live preview,
    /// with the caret at the end of the text on a new line of its own.
    @MainActor
    private func edit(contentSize: String? = nil) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize { app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize] }
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(MdocUITest.markdownFixture).path
        app.launch()
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 30))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10), "the toolbar above the keyboard")
        // A fresh Simulator's keyboard first shows its slide-to-type tip
        // over the keys and the toolbar.
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 2) { tip.tap() }
        // A key sent while edit mode is still settling can be lost: check
        // the caret reached the end with a letter typed there.
        var there = false
        for _ in 0..<3 where !there {
            MdocUITest.caretToTheEnd(app)
            app.typeText("\nQ")
            there = text.wait(until: NSPredicate(format: "value ENDSWITH %@", "\nQ"), timeout: 3)
        }
        XCTAssertTrue(there, "the caret reached the end")
        app.typeText(XCUIKeyboardKey.delete.rawValue)
        return (app, text)
    }

    @MainActor
    func testToolbarBoldAndTheFormattingKeyboard() throws {
        let (app, text) = edit()
        // Typed only on the software keyboard: an arrow key from the hardware
        // keyboard hides it a moment, and the toolbar moves under the tap.
        app.typeText("Pay ")
        let bold = app.buttons["toolbar-bold"]
        XCTAssertFalse(bold.isSelected)
        // Every button the toolbar shows is a 44 pt target.
        for id in ["undo", "redo", "bold", "italic", "list", "task", "formatting", "hide-keyboard"] {
            let frame = app.buttons["toolbar-\(id)"].frame
            XCTAssertGreaterThanOrEqual(min(frame.width, frame.height), 44, id)
        }
        XCTAssertTrue(app.buttons["toolbar-formatting"].isHittable, "Aa always shows")
        // Bold between words: an empty pair, the caret inside it.
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(bold.waitUntilHittable(timeout: 5))
        bold.tap()
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Pay **"), timeout: 5))
        app.typeText("monthly")
        // The caret is in the bold word: its markers show, and Bold is on.
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Pay **monthly**"), timeout: 5))
        XCTAssertTrue(bold.wait(until: NSPredicate(format: "isSelected == true"), timeout: 5))
        // Aa: the formatting keyboard in place of the system keyboard.
        app.buttons["toolbar-formatting"].tap()
        let heading = app.buttons["turn-heading2"]
        XCTAssertTrue(heading.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["toolbar-formatting"].isSelected)
        heading.tap()
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "## Pay **monthly**"), timeout: 5))
        XCTAssertTrue(heading.wait(until: NSPredicate(format: "isSelected == true"), timeout: 5))
        // Its keyboard key brings the system keyboard back.
        app.buttons["formatting-keyboard-return"].tap()
        XCTAssertTrue(heading.wait(until: NSPredicate(format: "exists == false"), timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        // The toolbar's Undo takes back the heading, one step.
        app.buttons["toolbar-undo"].tap()
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "## Pay"), timeout: 5))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "**monthly**"), timeout: 5))
        // Hide keyboard leaves edit mode on.
        app.buttons["toolbar-hide-keyboard"].tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].wait(until: NSPredicate(format: "exists == false"), timeout: 5))
        XCTAssertTrue(app.buttons["done-editing"].exists)
    }

    /// ⌘B, Tab and ⌘K from a hardware keyboard (ADR 0109's iPad keys).
    @MainActor
    func testHardwareKeys() throws {
        let (app, text) = edit()
        app.typeText("- first\nsecond")
        for _ in 0..<2 { app.typeKey(.leftArrow, modifierFlags: []) }
        app.typeKey("b", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "**second**"), timeout: 5))
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: [])
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "    - **second**"), timeout: 5))
        app.typeKey(XCUIKeyboardKey.tab, modifierFlags: .shift)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "\n- **second**"), timeout: 5))
        // ⌘K: the link sheet, filled from the selection. Return twice ends
        // the list.
        app.typeKey(.rightArrow, modifierFlags: .command)
        app.typeText("\n\nsite")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "\nsite"), timeout: 5))
        app.typeKey(.leftArrow, modifierFlags: [.option, .shift])
        app.typeKey("k", modifierFlags: .command)
        let url = app.textFields["link-url"]
        XCTAssertTrue(url.waitForExistence(timeout: 5), "⌘K opens the link sheet")
        url.typeText("https://example.org")
        app.buttons["link-done"].tap()
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "[site](https://example.org)"), timeout: 5))
        // The caret is at the link: its chip offers Open and Edit.
        XCTAssertTrue(app.buttons["link-edit"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["link-open"].isEnabled)
        app.buttons["link-edit"].tap()
        XCTAssertTrue(app.textFields["link-url"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.textFields["link-url"].value as? String, "https://example.org")
        app.buttons["link-cancel"].tap()
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 5), "the keyboard and toolbar come back")
    }

    /// The accessibility audit with the toolbar up, then with the
    /// formatting keyboard (ADR 0109): no issue on the toolbar's or the
    /// keyboard's keys. Issues elsewhere (the system keyboard) are printed.
    /// Opt-in, as the repository's audits are: hosted runners are too slow
    /// for them ("Audit failed to complete in time"). Run with
    /// `TEST_RUNNER_MDOC_A11Y_EXPLORE=1`; the hub runs it at the phone check.
    /// The 44 pt targets are checked always, by their frames, in
    /// `testToolbarBoldAndTheFormattingKeyboard`.
    @MainActor
    func testTheAccessibilityAuditWithTheToolbarUp() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_A11Y_EXPLORE"] == "1", "audits run with MDOC_A11Y_EXPLORE=1")
        let (app, _) = edit()
        func audit(_ name: String) throws -> [String] {
            var found: [String] = []
            try app.performAccessibilityAudit(for: [.dynamicType, .elementDetection, .sufficientElementDescription, .textClipped, .trait, .hitRegion]) { issue in
                let identifier = issue.element?.identifier ?? ""
                let line = "\(issue.auditType) | \(issue.compactDescription) | \(issue.element?.label ?? "-") id=\(identifier)"
                print("A11Y[\(name)] \(line)")
                let ours = ["toolbar-", "turn-", "format-", "insert-", "formatting-", "link-"].contains { identifier.hasPrefix($0) }
                if ours { found.append(line) }
                return true
            }
            return found
        }
        XCTAssertEqual(try audit("toolbar"), [], "issues on the toolbar")
        app.buttons["toolbar-formatting"].tap()
        XCTAssertTrue(app.buttons["turn-heading1"].waitForExistence(timeout: 5))
        XCTAssertEqual(try audit("formatting-keyboard"), [], "issues on the formatting keyboard")
    }

    /// At the largest accessibility text size the capsule scrolls: its last
    /// button is reached by swiping it, and Aa and hide keyboard stay.
    @MainActor
    func testTheToolbarScrollsAtAnAccessibilitySize() throws {
        let (app, text) = edit(contentSize: "UICTContentSizeCategoryAccessibilityXXXL")
        app.typeText("Item")
        XCTAssertTrue(app.buttons["toolbar-formatting"].isHittable)
        let aa = app.buttons["toolbar-outdent"]
        XCTAssertTrue(aa.exists)
        var tries = 0
        // Undo shows first; the rest is reached by swiping the capsule.
        let undo = app.buttons["toolbar-undo"]
        XCTAssertTrue(undo.isHittable)
        let hide = app.buttons["toolbar-formatting"].frame
        let origin = app.windows.firstMatch.coordinate(withNormalizedOffset: .zero)
        let from = origin.withOffset(CGVector(dx: hide.minX - 30, dy: hide.midY))
        let to = origin.withOffset(CGVector(dx: 30, dy: hide.midY))
        while !aa.isHittable && tries < 8 {
            from.press(forDuration: 0.05, thenDragTo: to)
            tries += 1
        }
        XCTAssertTrue(aa.isHittable, "Outdent is reached by scrolling the capsule")
        let list = app.buttons["toolbar-list"]
        if list.isHittable {
            list.tap()
            XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "- Item"), timeout: 5))
        }
        XCTAssertTrue(app.buttons["toolbar-hide-keyboard"].isHittable)
    }
}
