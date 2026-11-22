import XCTest

/// Live-preview editing (ADR 0109, slice 3): Edit with Show Markdown off,
/// a list typed with Return continuing it, ⌘Z taking back the last step;
/// and how long a keystroke's restyle and Return's core call take on the
/// 37-page legal opinion (ADR 0109's budgets: restyle under 8 ms a
/// keystroke, structural core calls under 16 ms).
final class LivePreviewUITests: XCTestCase {
    /// Whether the timing tests run: the nightly sets it (ci-ios.yml's
    /// `timing`).
    private static let timing = ProcessInfo.processInfo.environment["MDOC_TIMING_TESTS"] == "1"

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
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        return (app, text)
    }

    /// The caret on an empty line, then a list typed and undone, in one
    /// edit of the Markdown fixture: two tests with a launch each until
    /// #1076's cleanup. The caret's part leaves the caret on a line of its
    /// own at the end, and the list's part types after it.
    @MainActor
    func testTheCaretOnAnEmptyLineThenAListTypedWithReturnAndUndone() throws {
        let (app, text) = edit(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        try caretOnAnEmptyLineIsABodyLineTall(app, text)
        aListTypedWithReturnAndUndone(app, text)
    }

    /// Return ends a paragraph and continues a list; ⌘Z takes back the
    /// last step, as the system groups real typing.
    @MainActor
    private func aListTypedWithReturnAndUndone(_ app: XCUIApplication, _ text: XCUIElement) {
        toTheEnd(app, text)
        // Return ends the paragraph; Return in the list continues it. The
        // value is what VoiceOver hears: a line's hidden `- ` is silent,
        // the caret's line shows it.
        app.typeText("\n- Milk\nBread\n")
        XCTAssertTrue(
            text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Bread\n- "), timeout: MdocUITest.timeout),
            String(describing: (text.value as? String)?.suffix(80)))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Milk\n"), timeout: MdocUITest.timeout))
        // ⌘Z takes back the last Return, then the word before it.
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "- Bread"), timeout: MdocUITest.timeout))
        app.typeKey("z", modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value CONTAINS %@)", "Bread"), timeout: 5))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Milk\n- "), timeout: MdocUITest.timeout))
        app.typeKey("z", modifierFlags: [.command, .shift])
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "- Bread"), timeout: MdocUITest.timeout))
    }

    /// The caret on an empty line, after Return, is as tall as on the
    /// line of text before it, and stays above the floating toolbar (ADR
    /// 0110). The caret is found where the screen changes as it blinks.
    @MainActor
    private func caretOnAnEmptyLineIsABodyLineTall(_ app: XCUIApplication, _ text: XCUIElement) throws {
        toTheEnd(app, text)
        // The hardware keys moved it: the toolbar shows at the bottom edge.
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10))
        // A word of text, the caret after it; then Return: a new line.
        app.typeText("Word")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Word"), timeout: MdocUITest.timeout))
        let onText = try XCTUnwrap(MdocUITest.caretRect(in: text), "the caret blinks after the word")
        app.typeText("\n")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value ENDSWITH %@)", "Word"), timeout: MdocUITest.timeout))
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
    /// opens, Done takes taps and typing reaches the text (ADR 0109's
    /// acceptance). A list continued with Return is the same at any size:
    /// `testTheCaretOnAnEmptyLineThenAListTypedWithReturnAndUndone` and
    /// LivePreviewTests type one (ADR 0112's sweep took it, and moving the
    /// caret to the end, out).
    @MainActor
    func testEditingAtAnAccessibilitySize() throws {
        let (app, text) = edit(
            seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle,
            contentSize: "UICTContentSizeCategoryAccessibilityXXXL")
        XCTAssertTrue(app.buttons["done-editing"].isHittable)
        app.typeText("Zephyrine")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", "Zephyrine"), timeout: MdocUITest.timeout))
    }

    /// A keystroke's restyle on the legal opinion ("Restyle" signposts).
    /// Nightly only (`TEST_RUNNER_MDOC_TIMING_TESTS=1`, ADR 0112): it only
    /// measures speed.
    @MainActor
    func testTypingRestyleTime() throws {
        try XCTSkipUnless(Self.timing, "timing tests run nightly")
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
    /// and the restyle after it. Nightly only, as the typing time.
    @MainActor
    func testReturnTime() throws {
        try XCTSkipUnless(Self.timing, "timing tests run nightly")
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
