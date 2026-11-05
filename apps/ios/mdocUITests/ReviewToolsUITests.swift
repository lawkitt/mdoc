import XCTest

/// The review bar's tools on the Rust core (ADR 0098, ROADMAP iOS 0.2 items
/// 4, 5 and 7): Find and Hide name, the mentions chip, swiping the card,
/// the expanded view, a hardware key, New alias, and Identities' search and
/// Show filter. The demo agreement repeats "Bennett", which Find marks.
final class ReviewToolsUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testFindMarkAndTheBarsTools() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))

        // Find in document, then Hide name: the bar opens on the proposal.
        app.buttons["document-menu"].tap()
        let find = app.buttons["find-in-document"]
        XCTAssertTrue(find.waitForExistence(timeout: 5))
        find.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("Bennett")
        let hide = app.buttons["find-hide-name"]
        XCTAssertTrue(hide.waitForExistence(timeout: 5))
        XCTAssertTrue(hide.isEnabled)
        hide.tap()
        let bar = app.descendants(matching: .any)["review-bar"].firstMatch
        XCTAssertTrue(bar.waitForExistence(timeout: 10))

        // The mentions chip steps through the name's repeats.
        let mentions = app.buttons["review-mentions"]
        XCTAssertTrue(mentions.waitForExistence(timeout: 5), "the marked surname has repeats")
        let before = mentions.label
        mentions.tap()
        XCTAssertNotEqual(app.buttons["review-mentions"].label, before)

        // Swiping the card steps in document order.
        let progress = app.buttons["review-progress"]
        let start = progress.label
        app.descendants(matching: .any)["review-card"].firstMatch.swipeLeft()
        expectChange(of: progress, from: start)

        // Swipe up on the grabber: the expanded view; down closes it.
        let grabber = app.descendants(matching: .any)["review-grabber"].firstMatch
        grabber.swipeUp()
        XCTAssertTrue(app.descendants(matching: .any)["review-details"].firstMatch.waitForExistence(timeout: 5))
        grabber.swipeDown()

        // A hardware key: K keeps, which Undo can take back.
        app.typeKey("k", modifierFlags: [])
        expectEnabled(app.buttons["review-undo"])

        // New alias splits this mention off, with Undo in the toast.
        app.buttons["review-alias"].tap()
        let newAlias = app.buttons["New alias"]
        if newAlias.waitForExistence(timeout: 5), newAlias.isEnabled {
            newAlias.tap()
            XCTAssertTrue(app.buttons["toast-action"].waitForExistence(timeout: 5))
        } else {
            app.tap()
        }
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        // Identities: search and the Show filter.
        XCTAssertTrue(MdocUITest.showIdentities(app))
        let search = app.textFields["identities-search"]
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap()
        search.typeText("zzzz")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label CONTAINS 'Bennett'")).firstMatch.exists)
        app.buttons["Clear"].firstMatch.tap()
        let show = app.segmentedControls["identities-show"].firstMatch
        XCTAssertTrue(show.waitForExistence(timeout: 5))
        show.buttons["Kept"].tap()
        XCTAssertTrue(show.buttons["Kept"].isSelected)
    }

    private func expectChange(of element: XCUIElement, from label: String) {
        let changed = NSPredicate(format: "label != %@", label)
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: changed, evaluatedWith: element)], timeout: 5), .completed)
    }

    private func expectEnabled(_ element: XCUIElement) {
        let enabled = NSPredicate(format: "isEnabled == true")
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: enabled, evaluatedWith: element)], timeout: 5), .completed)
    }
}
