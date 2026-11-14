import XCTest

/// Identities' header and Select (ADR 0114): no close button and no hint
/// line; Select is a capsule, and in Select the header is Cancel, "N
/// selected" and Select All, as in Mail. The first Select with two names
/// plays the Select demo under its caption; any tap ends it and still does
/// what it taps. On iPhone in the drawer, on iPad beside the text.
final class IdentitiesSelectUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testSelectHasMailsHeaderAndADemoThatATapEnds() throws {
        // TipKit shows every tip: the demo is due.
        let app = MdocUITest.launch(showsTips: true)
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 30))
        // The agreement's email, and its surname marked from Find: two names.
        MdocUITest.hideFromFind("Bennett", app)
        XCTAssertTrue(MdocUITest.showIdentities(app))

        XCTAssertFalse(app.buttons["identities-done"].exists, "no close button")
        XCTAssertFalse(app.descendants(matching: .any)["review-hint-line"].exists, "no hint line in the header")
        let select = app.buttons["identities-select"].firstMatch
        XCTAssertTrue(select.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(select.label, "Select")
        select.tap()

        let caption = app.descendants(matching: .any)["select-demo-caption"].firstMatch
        XCTAssertTrue(caption.waitForExistence(timeout: MdocUITest.timeout), "the first Select plays the demo")
        XCTAssertEqual(caption.label, "Tick names to act on them, or drag ≡ onto another name to make them one")
        let count = app.staticTexts["identities-selected-count"].firstMatch
        XCTAssertEqual(count.label, "Select names")
        XCTAssertTrue(app.buttons["identities-select-cancel"].firstMatch.exists)
        let all = app.buttons["identities-select-all"].firstMatch
        XCTAssertEqual(all.label, "Select All")
        XCTAssertFalse(select.exists, "Select gives way to Cancel")

        // A tap on a row ends the demo and ticks the row.
        let rows = app.descendants(matching: .any).matching(identifier: "identity-row")
        XCTAssertGreaterThanOrEqual(rows.count, 2)
        rows.element(boundBy: 0).tap()
        XCTAssertTrue(caption.waitForNonExistence(timeout: 5), "a tap ends the demo")
        XCTAssertTrue(count.wait(until: NSPredicate(format: "label == '1 selected'"), timeout: 5), "and still ticks the row")

        all.tap()
        XCTAssertTrue(count.wait(until: NSPredicate(format: "label == %@", "\(rows.count) selected"), timeout: 5))
        XCTAssertEqual(all.label, "Deselect All")
        all.tap()
        XCTAssertTrue(count.wait(until: NSPredicate(format: "label == 'Select names'"), timeout: 5))

        app.buttons["identities-select-cancel"].firstMatch.tap()
        XCTAssertTrue(select.waitUntilHittable(timeout: 5), "Cancel leaves Select")
        XCTAssertFalse(app.descendants(matching: .any)["identity-drag-handle"].exists)
    }
}
