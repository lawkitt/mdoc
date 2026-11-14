import XCTest

/// Identities' header (ADR 0115): the title with "N of M decided" under it
/// and Select; a search field, always shown, and one filter capsule whose
/// menu lists All and each category with its count; Replace N floating at
/// the panel's bottom. And the document menu, which lists Copy for AI and
/// Share always, disabled with the reason while names are left.
final class IdentitiesHeaderUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTheHeaderSearchesFiltersAndReplacesAll() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 30))
        // The agreement's email, and its surname marked from Find: two
        // names, in two categories.
        MdocUITest.hideFromFind("Bennett", app)
        XCTAssertTrue(MdocUITest.showIdentities(app))

        let summary = app.staticTexts["identities-summary"].firstMatch
        XCTAssertTrue(summary.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(summary.label, "0 of 2 decided")
        XCTAssertFalse(app.buttons["identities-search-button"].exists, "no search button: the field shows")
        XCTAssertFalse(app.descendants(matching: .any)["identities-categories"].exists, "no chip row")
        let rows = app.descendants(matching: .any).matching(identifier: "identity-row")
        XCTAssertEqual(rows.count, 2)

        // The search field, without a tap on anything first.
        let search = app.textFields["identities-search"]
        XCTAssertTrue(search.waitUntilHittable(timeout: MdocUITest.timeout))
        search.tap()
        search.typeText("zzzz")
        XCTAssertTrue(rows.firstMatch.waitForNonExistence(timeout: 5), "nothing matches zzzz")
        app.buttons["identities-search-clear"].firstMatch.tap()
        XCTAssertTrue(rows.firstMatch.waitForExistence(timeout: 5))
        XCTAssertEqual(rows.count, 2)

        // The filter: All and each category with its count; its value is
        // the filter now.
        let filter = app.buttons["identities-filter"].firstMatch
        XCTAssertTrue(filter.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(filter.value as? String, "All")
        filter.tap()
        XCTAssertTrue(app.buttons["All 2"].firstMatch.waitForExistence(timeout: MdocUITest.timeout), "All with its count")
        let email = app.buttons["Email 1"].firstMatch
        XCTAssertTrue(email.exists, "each category with its count")
        email.tap()
        XCTAssertTrue(filter.wait(until: NSPredicate(format: "value == 'Email'"), timeout: 5))
        XCTAssertTrue(rows.element(boundBy: 1).waitForNonExistence(timeout: 5))
        XCTAssertEqual(rows.count, 1, "only the email")
        filter.tap()
        let all = app.buttons["All 2"].firstMatch
        XCTAssertTrue(all.waitForExistence(timeout: MdocUITest.timeout))
        all.tap()
        XCTAssertTrue(filter.wait(until: NSPredicate(format: "value == 'All'"), timeout: 5))
        XCTAssertTrue(rows.element(boundBy: 1).waitForExistence(timeout: 5))

        // Replace N floats at the bottom; Select's bar takes its place.
        let replaceAll = app.buttons["identities-replace-all"].firstMatch
        XCTAssertTrue(replaceAll.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(replaceAll.label, "Replace all 2 names left")
        XCTAssertGreaterThan(replaceAll.frame.minY, rows.element(boundBy: 1).frame.maxY, "below the rows")
        app.buttons["identities-select"].firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["identities-selection-bar"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(replaceAll.waitForNonExistence(timeout: 5), "Select's bar in its place")
        XCTAssertTrue(search.exists, "search stays in Select")
        app.buttons["identities-select-cancel"].firstMatch.tap()
        XCTAssertTrue(replaceAll.waitUntilHittable(timeout: 5))
        replaceAll.tap()
        XCTAssertTrue(summary.wait(until: NSPredicate(format: "label == %@", "2 names · all decided ✓"), timeout: 10))
        XCTAssertTrue(replaceAll.waitForNonExistence(timeout: 5), "nothing left to replace")
    }

    /// Copy for AI and Share are in the document menu while names are left,
    /// disabled with the reason under them; once all are decided, enabled.
    /// Home's Continue card reads its count as words.
    @MainActor
    func testCopyForAIAndShareWaitWithTheirReason() throws {
        let app = MdocUITest.launch()
        // Home's Continue card: "N left" reads as words, never "Error".
        let card = app.buttons["continue-card"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 60))
        XCTAssertTrue(card.wait(until: NSPredicate(format: "label ENDSWITH 'names left to review' OR label ENDSWITH '1 name left to review'"), timeout: 10),
                      "the card reads \(card.label)")
        XCTAssertFalse(card.label.contains("Error"))
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 30))
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        app.buttons["title-menu"].tap()
        // The menu's items; once ready the action bar has a Copy for AI too.
        let copy = app.collectionViews.buttons["copy-for-ai"]
        let share = app.collectionViews.buttons["share-document"]
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout), "Copy for AI is listed while names are left")
        XCTAssertTrue(share.exists, "so is Share")
        XCTAssertFalse(copy.isEnabled)
        XCTAssertFalse(share.isEnabled)
        // The reason under the title ("1 name left to decide") is the
        // menu item's subtitle, which XCUITest doesn't expose: the PR's
        // screenshots show it.
        // Close the menu.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)).tap()
        XCTAssertTrue(copy.waitForNonExistence(timeout: 5))

        XCTAssertTrue(MdocUITest.showIdentities(app))
        let replaceAll = app.buttons["identities-replace-all"].firstMatch
        XCTAssertTrue(replaceAll.waitUntilHittable(timeout: MdocUITest.timeout))
        replaceAll.tap()
        XCTAssertTrue(replaceAll.waitForNonExistence(timeout: 10))
        MdocUITest.closeIdentities(app)

        app.buttons["title-menu"].tap()
        XCTAssertTrue(copy.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(copy.isEnabled, "ready: Copy for AI")
        XCTAssertTrue(share.isEnabled, "ready: Share")
    }
}
