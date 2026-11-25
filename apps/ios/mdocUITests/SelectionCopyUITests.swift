import UIKit
import XCTest

/// A selection copied by hand (#1209, Vault & Projects item 41): its menu
/// says what it holds, as the core's check finds it. Copy for a clean
/// selection; Replace N and copy, then Copy with names, with names in it
/// not replaced; Copy with names alone in a document never scanned. The
/// copy items come first, then Hide name, then Select All, and Share, Look
/// Up, Translate and Search Web never show. A copy not known to be clean is
/// marked as a restored reply's copy is: on this device only, and gone
/// after a while, which `MDOC_MARKED_LIFETIME` makes a few seconds here.
final class SelectionCopyUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// How long a marked copy stays on the clipboard in these tests.
    static let lifetime: TimeInterval = 3

    /// The system's words a selection's menu must never show.
    static let sendsOn = ["Share…", "Share", "Look Up", "Translate", "Search Web", "Writing Tools"]

    // MARK: Steps

    /// Launches with `seed` imported and opens its document `title`, or
    /// with a typed document when `seed` is nil.
    @MainActor
    private func open(_ seed: String, title: String, detectorTier: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        app.launchEnvironment["MDOC_MARKED_LIFETIME"] = String(Self.lifetime)
        if detectorTier { app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1" }
        app.launch()
        let row = MdocUITest.listedRow(app, title: title)
        XCTAssertTrue(row.waitForExistence(timeout: 120))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))
        return app
    }

    /// A long press on the text's first word, from the top (a double tap
    /// in edit mode, where a long press moves the caret): it is selected
    /// and its menu shows.
    @MainActor
    private func selectFirstWord(_ app: XCUIApplication, doubleTap: Bool = false) {
        let text = app.textViews["document-text"]
        for _ in 0..<3 { text.swipeDown(velocity: .fast) }
        let word = text.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.04))
        if doubleTap { word.doubleTap() } else { word.press(forDuration: 1.0) }
    }

    /// Select All in the open menu: the whole text selected, and its menu
    /// shows again.
    @MainActor
    private func selectAll(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        // A long Hide "…" pushes it to the menu's next page, where Forward
        // opens the menu as a list of buttons.
        let all = app.descendants(matching: .any).matching(NSPredicate(
            format: "(elementType == %d OR elementType == %d) AND (label == 'Select All' OR identifier == 'select-all')",
            XCUIElement.ElementType.menuItem.rawValue, XCUIElement.ElementType.button.rawValue)).firstMatch
        let forward = app.buttons["Forward"].firstMatch
        for _ in 0..<3 where !all.waitForExistence(timeout: 2) && forward.exists { forward.tap() }
        XCTAssertTrue(all.waitForExistence(timeout: MdocUITest.timeout), "no Select All: \(shownItems(app))", file: file, line: line)
        all.tap()
    }

    /// The open selection menu's item `identifier`, once it shows.
    @MainActor
    private func item(_ identifier: String, _ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        let item = menuElement(identifier, app)
        XCTAssertTrue(item.waitForExistence(timeout: MdocUITest.timeout), "no \(identifier) in the menu", file: file, line: line)
        return item
    }

    /// The open menu's item `words` (its identifier or label): a menu item,
    /// or a button once the menu opened as a list.
    @MainActor
    private func menuElement(_ words: String, _ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "(elementType == %d OR elementType == %d) AND (identifier == %@ OR label == %@)",
            XCUIElement.ElementType.menuItem.rawValue, XCUIElement.ElementType.button.rawValue, words, words)).firstMatch
    }

    /// The open menu's items, by identifier or label, in order.
    @MainActor
    private func shownItems(_ app: XCUIApplication) -> [String] {
        app.menuItems.allElementsBoundByIndex.filter(\.exists).map { $0.identifier.isEmpty ? $0.label : $0.identifier }
    }

    /// The menu offers nothing that sends the selection on.
    @MainActor
    private func expectNothingSendsOn(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        for words in Self.sendsOn {
            XCTAssertFalse(app.menuItems.matching(NSPredicate(format: "label == %@", words)).firstMatch.exists, "\(words) in the menu: \(shownItems(app))", file: file, line: line)
        }
    }

    private func clearClipboard() {
        UIPasteboard.general.items = []
    }

    /// Something reached the clipboard.
    @MainActor
    private func expectCopied(file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(
            MdocUITest.wait(timeout: MdocUITest.timeout) { UIPasteboard.general.hasStrings }, "nothing copied",
            file: file, line: line)
    }

    /// The copy is marked: past its lifetime it has left the clipboard. A
    /// copy not marked stays.
    @MainActor
    private func expectMarked(_ marked: Bool, file: StaticString = #filePath, line: UInt = #line) {
        let gone = MdocUITest.wait(timeout: Self.lifetime + 4) { !UIPasteboard.general.hasStrings }
        XCTAssertEqual(gone, marked, marked ? "the copy is not marked" : "the copy is marked", file: file, line: line)
    }

    /// Paste on the document's bar restores what was copied as a reply:
    /// it holds `expected`, and `restored` values put back.
    @MainActor
    private func expectPastes(
        _ expected: String, restored: Int? = nil, _ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line
    ) {
        MdocUITest.tapWhenEnabled(app.descendants(matching: .any)["paste-reply"].firstMatch)
        let reply = app.textViews["restored-text"]
        XCTAssertTrue(reply.waitForExistence(timeout: MdocUITest.timeout), "the copy did not paste", file: file, line: line)
        XCTAssertTrue(
            reply.wait(until: NSPredicate(format: "value CONTAINS %@", expected), timeout: MdocUITest.timeout),
            "the copy lacks “\(expected)”: \(reply.value ?? "")", file: file, line: line)
        if let restored {
            let summary = app.staticTexts["restore-summary"]
            XCTAssertTrue(summary.label.hasPrefix("\(restored) value"), summary.label, file: file, line: line)
        }
    }

    // MARK: Tests

    /// A Ready document: a selection's plain Copy, first in its menu,
    /// then Hide name and Select All; the copy is ordinary.
    @MainActor
    func testAReadyDocumentsSelectionCopiesPlainly() throws {
        let app = open(ReadyChecksUITests.noNames, title: "no-names")
        selectFirstWord(app)
        let copy = item("copy-selection", app)
        XCTAssertEqual(copy.label, "Copy")
        let shown = shownItems(app)
        XCTAssertEqual(shown.first, "copy-selection", "\(shown)")
        let hide = try XCTUnwrap(shown.firstIndex(of: "hide-name"), "\(shown)")
        XCTAssertLessThan(hide, try XCTUnwrap(shown.firstIndex { $0 == "Select All" || $0 == "select-all" }), "\(shown)")
        XCTAssertFalse(menuElement("copy-with-names", app).exists)
        XCTAssertFalse(menuElement("replace-and-copy", app).exists)
        expectNothingSendsOn(app)

        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(false)
        expectPastes("visit", app)
    }

    /// Names in it not replaced: Replace 1 and copy, then Copy with names.
    /// Replace 1 and copy replaces the email in the text, and the copy
    /// holds its alias whole: pasted back, one value is restored.
    @MainActor
    func testReplaceAndCopyReplacesTheNamesAndCopiesTheirAliases() throws {
        let app = open(MdocUITest.defaultSeed, title: MdocUITest.title)
        selectFirstWord(app)
        _ = item("hide-name", app)
        selectAll(app)
        let replace = item("replace-and-copy", app)
        XCTAssertEqual(replace.label, "Replace 1 and copy")
        let shown = shownItems(app)
        XCTAssertEqual(shown.first, "replace-and-copy", "\(shown)")
        XCTAssertFalse(menuElement("copy-selection", app).exists)
        expectNothingSendsOn(app)
        // Copy with names next: on the menu's next page when Replace 1 and
        // copy fills the first, where Forward opens the menu as a list.
        let withNames = menuElement("copy-with-names", app)
        let forward = app.buttons["Forward"].firstMatch
        if !withNames.exists, forward.exists { forward.tap() }
        XCTAssertTrue(withNames.waitForExistence(timeout: MdocUITest.timeout), "no Copy with names: \(shownItems(app))")
        XCTAssertTrue(
            withNames.frame.minY > replace.frame.minY || withNames.frame.minX > replace.frame.minX,
            "Copy with names comes after Replace 1 and copy")

        clearClipboard()
        replace.tap()
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: MdocUITest.timeout), "not replaced")
        XCTAssertFalse(app.links[MdocUITest.email].exists)
        expectCopied()
        expectMarked(false)
        expectPastes(MdocUITest.email, restored: 1, app)
    }

    /// A name kept as written: Copy with names alone, and the copy is
    /// marked.
    @MainActor
    func testAKnownNameWrittenOutCopiesWithNamesMarked() throws {
        let app = open(MdocUITest.defaultSeed, title: MdocUITest.title)
        let text = app.textViews["document-text"]
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        MdocUITest.tapAboveBottomBar(email, in: text, app)
        let keep = app.buttons["review-keep"]
        XCTAssertTrue(keep.waitForExistence(timeout: MdocUITest.timeout))
        keep.tap()
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        selectFirstWord(app)
        _ = item("hide-name", app)
        selectAll(app)
        let copy = item("copy-with-names", app)
        XCTAssertEqual(copy.label, "Copy with names")
        XCTAssertFalse(menuElement("replace-and-copy", app).exists)
        XCTAssertFalse(menuElement("copy-selection", app).exists)
        expectNothingSendsOn(app)

        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(true)
    }

    /// A document never scanned, on a phone that finds names: Copy with
    /// names alone, marked.
    @MainActor
    func testADocumentNeverScannedCopiesWithNamesMarked() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1"
        app.launchEnvironment["MDOC_MARKED_LIFETIME"] = String(Self.lifetime)
        app.launch()
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let new = app.collectionViews.buttons["New document"].firstMatch
        XCTAssertTrue(new.waitForExistence(timeout: MdocUITest.timeout))
        new.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: MdocUITest.timeout))
        text.typeText("Lunch notes\nThe park is lovely today.")
        done.tap()
        XCTAssertTrue(
            app.buttons["primary-action"].wait(until: NSPredicate(format: "label == 'Find names'"), timeout: MdocUITest.timeout))

        selectFirstWord(app)
        let copy = item("copy-with-names", app)
        XCTAssertFalse(menuElement("copy-selection", app).exists)
        XCTAssertFalse(menuElement("replace-and-copy", app).exists)
        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(true)
    }

    /// Replace N on the bar: every name replaced, so the document is Ready
    /// and its bar has Paste.
    @MainActor
    private func replaceAll(_ app: XCUIApplication) {
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Replace'"), timeout: MdocUITest.timeout))
        primary.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))
    }

    /// A table cell's selection has the same menu: a clean cell copies
    /// plainly, as written.
    @MainActor
    func testATableCellCopies() throws {
        let app = open(MdocUITest.markdownFixture, title: MdocUITest.markdownTitle)
        replaceAll(app)
        let cell = TableEditingUITests.cell(app, "Депозит")
        XCTAssertTrue(cell.waitForExistence(timeout: 10))
        let text = app.textViews["document-text"]
        let clear = app.windows.firstMatch.frame.insetBy(dx: 0, dy: 160)
        for _ in 0..<8 where !clear.contains(cell.frame) {
            text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.05, thenDragTo: text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)))
        }
        cell.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)).press(forDuration: 1.0)
        let copy = item("copy-selection", app)
        expectNothingSendsOn(app)
        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(false)
        expectPastes("Депозит", app)
    }

    /// Edit mode copies to the system clipboard, as written: a clean
    /// selection's copy is ordinary, and it pastes in another app.
    @MainActor
    func testEditModeCopiesToTheSystemClipboard() throws {
        let app = open(ReadyChecksUITests.noNames, title: "no-names")
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 30))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        selectFirstWord(app, doubleTap: true)
        let copy = app.menuItems["Copy"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "no Copy in edit mode: \(shownItems(app))")
        expectNothingSendsOn(app)
        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(false)
    }

    /// #1228: a restored reply holds real names. Its selection's menu is
    /// Copy and Select All only, and Copy is marked.
    @MainActor
    func testARestoredReplysSelectionCopiesMarkedAndNothingElse() throws {
        let app = open(MdocUITest.defaultSeed, title: MdocUITest.title)
        replaceAll(app)
        MdocUITest.pasteReply("Maya Bennett will sign the agreement on Friday.", in: app)
        let reply = app.textViews["restored-text"]
        XCTAssertTrue(reply.waitForExistence(timeout: MdocUITest.timeout))
        reply.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.04)).press(forDuration: 1.0)
        let copy = app.menuItems["Copy"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "no Copy: \(shownItems(app))")
        expectNothingSendsOn(app)
        XCTAssertEqual(Set(shownItems(app)).subtracting(["Copy", "Select All", "Select"]), [], "\(shownItems(app))")
        clearClipboard()
        copy.tap()
        expectCopied()
        expectMarked(true)
    }
}
