import UIKit
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
        typeInFindField(app, "Bennett")
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

    /// A hardware keyboard drives the bar as desktop's keys do (ADR 0098,
    /// ROADMAP iOS 0.2 item 7): → and ← step, Return replaces, ⌘. ends.
    @MainActor
    func testHardwareKeysDriveTheBar() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        // Repeats to step through: Find and Hide name on the surname.
        app.buttons["document-menu"].tap()
        app.buttons["find-in-document"].tap()
        typeInFindField(app, "Bennett")
        let hide = app.buttons["find-hide-name"]
        XCTAssertTrue(hide.waitForExistence(timeout: 5))
        hide.tap()
        XCTAssertTrue(app.descendants(matching: .any)["review-bar"].firstMatch.waitForExistence(timeout: 10))

        let progress = app.buttons["review-progress"]
        let first = progress.label
        app.typeKey(.rightArrow, modifierFlags: [])
        expectChange(of: progress, from: first)
        sleep(1)
        app.typeKey(.leftArrow, modifierFlags: [])
        if UIDevice.current.userInterfaceIdiom == .pad {
            // lawkitt/mdoc#610: on iPad ← straight after → is dropped.
            XCTExpectFailure("iPad drops ← after → (#610)", strict: false) {
                expectLabel(of: progress, first)
            }
        } else {
            expectLabel(of: progress, first)
        }

        app.typeKey(.return, modifierFlags: [])
        expectEnabled(app.buttons["review-undo"])

        app.typeKey(".", modifierFlags: .command)
        let gone = NSPredicate(format: "exists == false")
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: gone, evaluatedWith: app.descendants(matching: .any)["review-bar"].firstMatch)], timeout: 5), .completed)
    }

    /// Types into the Find sheet's search field once it has focus, which
    /// the sheet gives it when it is up (#634).
    @MainActor
    private func typeInFindField(_ app: XCUIApplication, _ text: String) {
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        let focused = NSPredicate(format: "hasKeyboardFocus == true")
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: focused, evaluatedWith: field)], timeout: 5), .completed,
                       "the Find field never took focus")
        field.typeText(text)
    }

    private func expectLabel(of element: XCUIElement, _ label: String) {
        let same = NSPredicate(format: "label == %@", label)
        XCTAssertEqual(XCTWaiter.wait(for: [expectation(for: same, evaluatedWith: element)], timeout: 5), .completed)
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
