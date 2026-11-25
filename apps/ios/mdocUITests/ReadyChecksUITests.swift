import UIKit
import XCTest

/// Two behaviours #1209 decision 15 asked to confirm in a running build
/// (Vault & Projects item 39, iOS): a document scanned with nothing to
/// replace is Ready and Copy for AI takes it; an edit that adds no new name
/// returns to Ready without a click. The UI tests' seed is scanned with the
/// structured rules, below the floor.
final class ReadyChecksUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// The site visit report, which names nobody.
    static let noNames = "tests/fixtures/mobile-markdown/no-names.md"

    /// Copy on the bar, with the clipboard emptied first: Paste lights up
    /// once the text is on it, and no alert shows. (The "Copied for AI"
    /// notice can be gone by the time a tap returns.) Then Paste restores
    /// what was copied as a reply, which shows it holds `expected`, and
    /// `restored` values when names went out replaced.
    @MainActor
    private func copyForAI(
        _ app: XCUIApplication, holding expected: String, restored: Int,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        UIPasteboard.general.items = []
        let copy = app.buttons["copy-for-ai"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "no Copy on the bar", file: file, line: line)
        copy.tap()
        let paste = app.descendants(matching: .any)["paste-reply"].firstMatch
        XCTAssertTrue(
            paste.wait(until: NSPredicate(format: "isEnabled == true"), timeout: MdocUITest.timeout), "nothing copied",
            file: file, line: line)
        XCTAssertFalse(app.alerts.firstMatch.exists, "an alert: \(app.alerts.firstMatch.label)", file: file, line: line)

        paste.tap()
        let reply = app.textViews["restored-text"]
        XCTAssertTrue(reply.waitForExistence(timeout: MdocUITest.timeout), "the copy did not paste", file: file, line: line)
        XCTAssertTrue(
            reply.wait(until: NSPredicate(format: "value CONTAINS %@", expected), timeout: MdocUITest.timeout),
            "the copy lacks “\(expected)”", file: file, line: line)
        let summary = app.staticTexts["restore-summary"]
        if restored > 0 {
            XCTAssertTrue(summary.label.contains("\(restored) value"), summary.label, file: file, line: line)
        }
    }

    /// Scanned, nothing found: Share and Copy straight away.
    @MainActor
    func testNothingToReplaceIsReady() throws {
        let app = MdocUITest.launch(seed: Self.noNames)
        let row = MdocUITest.listedRow(app, title: "no-names")
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        XCTAssertTrue(row.wait(until: NSPredicate(format: "label CONTAINS 'Nothing found'"), timeout: 60), row.label)
        row.tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(primary.label.hasPrefix("Share"), primary.label)
        copyForAI(app, holding: "the gutter above the porch", restored: 0)
    }

    /// The agreement reviewed, then a sentence typed in edit mode: after
    /// Done, Share and Copy work with no other tap.
    @MainActor
    func testAnEditWithNoNewNameReturnsToReady() throws {
        let app = MdocUITest.launch()
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(primary.label.hasPrefix("Replace"), primary.label)
        primary.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))

        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        edit.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        let sentence = " The ladder stays in the shed."
        app.typeText(sentence)
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", sentence), timeout: MdocUITest.timeout))
        done.tap()

        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))
        // The edit went out, and the email with it as its alias.
        copyForAI(app, holding: "The ladder stays in the shed.", restored: 1)
    }

    /// Item 44: the agreement reviewed, then a new email typed in edit
    /// mode: at Done it is proposed, so Replace comes back before Share,
    /// and the copy holds it replaced: Paste restores two values.
    @MainActor
    func testANewNameTypedIsProposedAtDone() throws {
        let app = MdocUITest.launch()
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(primary.label.hasPrefix("Replace"), primary.label)
        primary.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))

        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        edit.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        MdocUITest.tapIntoText(app)
        let sentence = " Copy bob.stone@example.com in."
        app.typeText(sentence)
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value CONTAINS %@", sentence), timeout: MdocUITest.timeout))
        done.tap()

        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(
            primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Replace'"), timeout: MdocUITest.timeout),
            primary.label)
        primary.tap()
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))
        copyForAI(app, holding: "Copy bob.stone@example.com in.", restored: 2)
    }
}
