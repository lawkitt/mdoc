import XCTest

/// Live-preview editing (ADR 0109, slice 3): Edit with Show Markdown off,
/// a list typed with Return continuing it, ⌘Z taking back the last step;
/// and how long a keystroke's restyle and Return's core call take on the
/// 37-page legal opinion (ADR 0109's budgets: restyle under 8 ms a
/// keystroke, structural core calls under 16 ms).
final class LivePreviewUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Opens `seed`'s document, scrolled down `pages` screens, and enters
    /// edit mode in live preview, a tap in the text bringing the keyboard
    /// (ADR 0110).
    @MainActor
    private func edit(seed: String, title: String, pages: Int = 0, contentSize: String? = nil) -> (XCUIApplication, XCUIElement) {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize { app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize] }
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launch()
        // Whatever its status: the rules find no name in the legal opinion.
        MdocUITest.revealSidebar(app)
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", "\(title),"))
            .firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        for _ in 0..<pages { text.swipeUp() }
        // Edit is offered with Show Markdown off.
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 30))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        return (app, text)
    }

    @MainActor
    func testAListTypedWithReturnAndUndone() throws {
        let (app, text) = edit(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        toTheEnd(app, text)
        // Return ends the paragraph; Return in the list continues it. The
        // value is what VoiceOver hears: a line's hidden `- ` is silent,
        // the caret's line shows it.
        app.typeText("\n- Milk\nBread\n")
        XCTAssertTrue(
            text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Bread\n- "), timeout: 5),
            String(describing: (text.value as? String)?.suffix(80)))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Milk\n"), timeout: 5))
        // ⌘Z takes back the last Return, then the word before it.
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "- Bread"), timeout: 5))
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "Bread"), timeout: 5))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Milk\n- "), timeout: 5))
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "- Bread"), timeout: 5))
    }

    /// The caret on an empty line, after Return, is as tall as on the
    /// line of text before it, and stays above the floating toolbar (ADR
    /// 0110). The caret is found where the screen changes as it blinks.
    @MainActor
    func testTheCaretOnAnEmptyLineIsABodyLineTall() throws {
        let (app, text) = edit(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        toTheEnd(app, text)
        // The hardware keys moved it: the toolbar shows at the bottom edge.
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10))
        // A word of text, the caret after it; then Return: a new line.
        app.typeText("Word")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Word"), timeout: 5))
        let onText = try XCTUnwrap(MdocUITest.caretRect(in: text), "the caret blinks after the word")
        app.typeText("\n")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value ENDSWITH %@)", "Word"), timeout: 5))
        let onEmpty = try XCTUnwrap(MdocUITest.caretRect(in: text), "the caret blinks on the empty line")
        XCTAssertLessThan(onEmpty.minX, onText.minX - 10, "the caret went to a new line: \(onText) then \(onEmpty)")
        XCTAssertEqual(onEmpty.height, onText.height, accuracy: 2, "as tall as on the text: \(onText) then \(onEmpty)")
        let toolbar = app.buttons["toolbar-bold"].frame
        XCTAssertLessThan(onEmpty.maxY, toolbar.minY, "above the toolbar: \(onEmpty), \(toolbar)")
    }

    /// Puts the caret at the end of the text. A key sent while edit mode is
    /// still settling can be lost, so it checks with a letter typed there
    /// and taken back.
    @MainActor
    private func toTheEnd(_ app: XCUIApplication, _ text: XCUIElement) {
        var there = false
        for _ in 0..<3 where !there {
            MdocUITest.caretToTheEnd(app)
            app.typeText("Q")
            there = text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Q"), timeout: 3)
            app.typeText(XCUIKeyboardKey.delete.rawValue)
        }
        XCTAssertTrue(there, "the caret reached the end")
        XCTAssertTrue(
            text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "Q"), timeout: 3),
            String(describing: (text.value as? String)?.suffix(60).debugDescription))
    }

    /// Edit mode in live preview at the largest accessibility text size: it
    /// opens, types and continues a list (ADR 0109's acceptance).
    @MainActor
    func testEditingAtAnAccessibilitySize() throws {
        let (app, text) = edit(
            seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle,
            contentSize: "UICTContentSizeCategoryAccessibilityXXXL")
        XCTAssertTrue(app.buttons["done-editing"].isHittable)
        toTheEnd(app, text)
        app.typeText("\n- Milk\n")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Milk\n- "), timeout: 5))
    }

    /// The accessibility audit in live-preview edit mode (ADR 0109): no
    /// issue on the edited text or Done. Issues elsewhere (the system
    /// keyboard, the translucent bar) are printed, as the explore audit does.
    /// Opt-in (`TEST_RUNNER_MDOC_A11Y_EXPLORE=1`): hosted runners are too
    /// slow for audits; the hub runs it at the phone check.
    @MainActor
    func testTheAccessibilityAuditInEditMode() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_A11Y_EXPLORE"] == "1", "audits run with MDOC_A11Y_EXPLORE=1")
        let (app, text) = edit(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        app.typeKey(.upArrow, modifierFlags: .command)
        let ours: Set<String> = ["document-text", "done-editing"]
        var found: [String] = []
        try app.performAccessibilityAudit(for: [.dynamicType, .elementDetection, .sufficientElementDescription, .textClipped, .trait]) { issue in
            let identifier = issue.element?.identifier ?? ""
            let line = "\(issue.auditType) | \(issue.compactDescription) | \(issue.element?.label ?? "-") id=\(identifier)"
            print("A11Y[live-preview-edit] \(line)")
            if ours.contains(identifier) { found.append(line) }
            return true
        }
        XCTAssertEqual(found, [], "issues on the edited text or Done")
        XCTAssertTrue(text.exists)
    }

    /// A keystroke's restyle on the legal opinion ("Restyle" signposts).
    @MainActor
    func testTypingRestyleTime() throws {
        let (app, _) = edit(seed: MdocUITest.longDocx, title: MdocUITest.longDocxTitle, pages: 8)
        let restyle = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Editing", name: "Restyle")
        let typing = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Editing", name: "Typing")
        let options = XCTMeasureOptions()
        options.iterationCount = 10
        measure(metrics: [restyle, typing], options: options) {
            app.typeText("a")
        }
    }

    /// Return on the legal opinion: the editor core's call ("Structural")
    /// and the restyle after it.
    @MainActor
    func testReturnTime() throws {
        let (app, _) = edit(seed: MdocUITest.longDocx, title: MdocUITest.longDocxTitle, pages: 8)
        let structural = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Editing", name: "Structural")
        let restyle = XCTOSSignpostMetric(subsystem: "io.github.lawkitt.mdoc", category: "Editing", name: "Restyle")
        let options = XCTMeasureOptions()
        options.iterationCount = 10
        measure(metrics: [structural, restyle], options: options) {
            app.typeText("\n")
        }
    }
}
