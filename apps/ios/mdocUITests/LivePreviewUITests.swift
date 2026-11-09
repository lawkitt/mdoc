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
    /// edit mode in live preview: the caret starts at the first line in view.
    @MainActor
    private func edit(seed: String, title: String, pages: Int = 0) -> (XCUIApplication, XCUIElement) {
        let app = MdocUITest.launch(seed: seed)
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
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
        return (app, text)
    }

    @MainActor
    func testAListTypedWithReturnAndUndone() throws {
        let (app, text) = edit(seed: MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        app.typeKey(.downArrow, modifierFlags: .command)
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
