import UIKit
import XCTest

/// The panes' row (ADR 0111): Text, Original and Reply switch from a row of
/// their own under the navigation bar, whose centre stays the title's menu;
/// on the Reply pane the bottom bar shares and copies the reply.
final class PanesRowUITests: XCTestCase {
    private var isPad: Bool { UIDevice.current.userInterfaceIdiom == .pad }
    /// `TEST_RUNNER_MDOC_SHOTS_DIR`: where `testShots` saves its screens.
    private var dir: String { ProcessInfo.processInfo.environment["MDOC_SHOTS_DIR"] ?? "" }

    override func setUp() {
        continueAfterFailure = false
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
        super.tearDown()
    }

    /// A PDF has an original: the row shows under the title, clear of the
    /// bar's buttons, and the reply adds its segment and its own bar.
    @MainActor
    func testThePanesHaveARowUnderTheTitle() throws {
        let app = open(seed: "tests/fixtures/import/text.pdf")
        let title = app.buttons["title-menu"]
        let picker = app.segmentedControls["pane-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(title.exists, "the bar's centre is the title's menu with two panes too")
        XCTAssertEqual(picker.buttons.count, 2)
        assertRowIsClear(picker, of: title, in: app)

        // One document menu, the title's (ADR 0114): no ⋯ beside it, and
        // Find in document and Show Markdown are in it with Rename….
        XCTAssertFalse(app.buttons["document-menu"].exists, "no ⋯ in the bar")
        title.tap()
        XCTAssertTrue(app.descendants(matching: .any)["find-in-document"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(app.descendants(matching: .any)["show-markdown"].firstMatch.exists)
        XCTAssertTrue(app.buttons["Rename…"].firstMatch.exists)
        dismissMenu(app)

        if !isPad {
            let original = picker.buttons["Original"]
            original.tap()
            // The PDF's accessibility tree makes one query take over 4 s on a
            // hosted runner, so 5 s allowed a single check (queue run
            // 43359359817); the wait returns as soon as the segment is on.
            XCTAssertTrue(original.wait(until: NSPredicate(format: "isSelected == true"), timeout: 15))
            XCTAssertTrue(picker.exists, "the row stays over the Original")
            picker.buttons["Text"].tap()
        }

        // A reply: its segment joins the row, and its pane's bar shares and
        // copies it in place of the document's buttons. Copy, and the bar
        // acting on the document again away from the reply, are
        // PanesRowTests' (ADR 0112's sweep).
        restoreReply(app)
        XCTAssertEqual(picker.buttons.count, 3)
        XCTAssertTrue(picker.buttons["Reply"].wait(until: NSPredicate(format: "isSelected == true"), timeout: MdocUITest.timeout))
        XCTAssertTrue(title.exists)
        assertRowIsClear(picker, of: title, in: app)
        XCTAssertTrue(app.buttons["share-reply"].exists)
        XCTAssertTrue(app.buttons["copy-reply"].exists)
        XCTAssertFalse(app.buttons["primary-action"].exists, "the document's buttons are away")
    }

    /// A guide: the row's states as PNGs in `TEST_RUNNER_MDOC_SHOTS_DIR`,
    /// at the text size in `TEST_RUNNER_MDOC_SHOTS_SIZE` (a
    /// `UICTContentSizeCategory…` name) and dark with
    /// `TEST_RUNNER_MDOC_SHOTS_DARK=1`. It skips without the folder.
    @MainActor
    func testShots() throws {
        try XCTSkipIf(dir.isEmpty, "exploration only")
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let environment = ProcessInfo.processInfo.environment
        let size = environment["MDOC_SHOTS_SIZE"]
        if environment["MDOC_SHOTS_DARK"] == "1" { XCUIDevice.shared.appearance = .dark }

        var app = open(seed: MdocUITest.defaultSeed, contentSize: size)
        save("1-one-pane")
        app.terminate()

        app = open(seed: "tests/fixtures/import/text.pdf", contentSize: size)
        let picker = app.segmentedControls["pane-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: MdocUITest.timeout))
        save("2-text-original")
        if !isPad {
            picker.buttons["Original"].tap()
            save("3-original")
            picker.buttons["Text"].tap()
        }
        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        _ = app.keyboards.firstMatch.waitForExistence(timeout: MdocUITest.probeTimeout)
        save("4-edit-keyboard")
        app.buttons["done-editing"].tap()
        restoreReply(app)
        save("5-reply")
        if !isPad {
            picker.buttons["Text"].tap()
            save("6-text-original-reply")
            app.swipeUp()
            save("7-text-scrolled")
        }
    }

    // MARK: Steps

    @MainActor
    private func open(seed: String, contentSize: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize {
            app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize]
        }
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launch()
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))
        // The document is read and its bar is up.
        XCTAssertTrue(app.buttons["primary-action"].waitForExistence(timeout: 30))
        return app
    }

    /// Paste reply with a line of sample text.
    @MainActor
    private func restoreReply(_ app: XCUIApplication) {
        let paste = app.buttons["paste-reply"]
        XCTAssertTrue(paste.waitForExistence(timeout: 10))
        paste.tap()
        let editor = app.textViews["reply-editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: MdocUITest.timeout))
        editor.tap()
        editor.typeText("Thanks. The fixture document reads well.")
        app.buttons["reply-restore"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["restore-summary"].firstMatch.waitForExistence(timeout: 10))
    }

    /// The row lies under the title, inside the window, and under none of
    /// the bar's buttons.
    @MainActor
    private func assertRowIsClear(
        _ picker: XCUIElement, of title: XCUIElement, in app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line
    ) {
        let row = picker.frame, window = app.windows.firstMatch.frame
        XCTAssertGreaterThanOrEqual(row.minY, title.frame.maxY, "the row is under the title", file: file, line: line)
        XCTAssertGreaterThanOrEqual(row.minX, window.minX, file: file, line: line)
        XCTAssertLessThanOrEqual(row.maxX, window.maxX, file: file, line: line)
        for id in ["show-sidebar", "edit-button", "identities-button"] {
            let button = app.buttons[id].firstMatch
            if button.exists {
                XCTAssertFalse(button.frame.intersects(row), "\(id) covers the row", file: file, line: line)
            }
        }
    }

    @MainActor
    private func dismissMenu(_ app: XCUIApplication) {
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)).tap()
        XCTAssertTrue(app.buttons["Find in document"].firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Rename…"].firstMatch.waitForNonExistence(timeout: 5))
    }

    /// The screen as a PNG in the folder, once it has stopped moving.
    @MainActor
    private func save(_ name: String) {
        let png = MdocUITest.settledScreen().pngRepresentation
        do {
            try png.write(to: URL(fileURLWithPath: dir).appendingPathComponent(name + ".png"))
        } catch {
            XCTFail("could not save \(name): \(error)")
        }
    }
}
