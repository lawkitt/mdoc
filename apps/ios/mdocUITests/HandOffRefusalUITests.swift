import UIKit
import XCTest

/// Copy for AI and Share refusals in plain words (#1209, Vault & Projects
/// item 37): not yet reviewed offers Replace N and copy; a known name written
/// out offers Review; changed since its review and excluded from AI each say
/// so; below the floor the title menu agrees with the bar. No refusal shows
/// the core's words or "Something went wrong". The UI tests run below the
/// floor. Changed since review and excluded from AI come only from a
/// computer today, so `MDOC_UI_READINESS` stands in for the Vault's answer.
final class HandOffRefusalUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    // MARK: Steps

    /// The seeded agreement, open, its one email proposed.
    @MainActor
    private func openAgreement(readiness: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(MdocUITest.defaultSeed).path
        if let readiness { app.launchEnvironment["MDOC_UI_READINESS"] = readiness }
        app.launch()
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(app.buttons["primary-action"].waitForExistence(timeout: MdocUITest.timeout))
        return app
    }

    /// The email's review bar, from its underline.
    @MainActor
    private func reviewEmail(_ app: XCUIApplication) {
        let text = app.textViews["document-text"]
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        MdocUITest.tapAboveBottomBar(email, in: text, app)
    }

    /// The title menu, open on Copy for AI.
    @MainActor
    private func openMenu(_ app: XCUIApplication) {
        app.buttons["title-menu"].tap()
        XCTAssertTrue(MdocUITest.menuItem("Copy for AI", app).waitForExistence(timeout: MdocUITest.timeout))
    }

    /// Closes the open title menu with a tap beside it.
    @MainActor
    private func closeMenu(_ app: XCUIApplication) {
        MdocUITest.closeMenu(app)
        XCTAssertTrue(MdocUITest.menuItem("Copy for AI", app).waitForNonExistence(timeout: 5))
    }

    /// Copy for AI and Share in the open menu are off. Their reason is a
    /// menu item's subtitle, which XCUITest does not read:
    /// HandOffRefusalTests pin its words.
    @MainActor
    private func expectRefused(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertFalse(MdocUITest.menuItem("Copy for AI", app).isEnabled, "Copy for AI on", file: file, line: line)
        XCTAssertFalse(MdocUITest.menuItem("Share", app).isEnabled, "Share on", file: file, line: line)
    }

    /// Nothing on screen in the core's words, nor its catch-all alert.
    @MainActor
    private func expectNoCoreText(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertFalse(app.alerts["Something went wrong"].exists, "the catch-all alert", file: file, line: line)
        for words in ["not ready for AI", "NamesNotReplaced", "ChangedSinceReview", "NotReviewed", "excluded from AI."] {
            let found = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", words))
            XCTAssertEqual(found.count, 0, "“\(words)” on screen", file: file, line: line)
        }
    }

    /// The copy reached the clipboard, which `clearClipboard` emptied:
    /// Paste on the bar lights up. The "Copied for AI" notice is gone
    /// again by the time a menu item's tap returns.
    @MainActor
    private func expectCopied(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let paste = app.descendants(matching: .any)["paste-reply"].firstMatch
        XCTAssertTrue(paste.waitForExistence(timeout: MdocUITest.timeout), "no Paste", file: file, line: line)
        XCTAssertTrue(
            paste.wait(until: NSPredicate(format: "isEnabled == true"), timeout: MdocUITest.timeout), "nothing copied",
            file: file, line: line)
    }

    private func clearClipboard() {
        UIPasteboard.general.items = []
    }

    /// A new document typed by hand with no names in it, out of edit mode;
    /// `detectorTier`: on a phone that finds names.
    @MainActor
    private func typedDocument(detectorTier: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if detectorTier { app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1" }
        app.launch()
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let item = app.collectionViews.buttons["New document"].firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: MdocUITest.timeout))
        item.tap()
        let done = app.buttons["done-editing"]
        XCTAssertTrue(done.waitForExistence(timeout: MdocUITest.timeout))
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: MdocUITest.timeout))
        text.typeText("Lunch notes\nThe park is lovely today.")
        done.tap()
        return app
    }

    /// The bar's refusal note.
    @MainActor
    private func refusalNote(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["hand-off-refusal"].firstMatch
    }

    // MARK: Tests

    /// Not yet reviewed: Copy for AI and Share wait with the names left, and
    /// Replace 1 and copy replaces the email and copies in one tap.
    @MainActor
    func testNotYetReviewedOffersReplaceAndCopy() throws {
        let app = openAgreement()
        openMenu(app)
        expectRefused(app)
        let step = MdocUITest.menuItem("Replace 1 and copy", app)
        XCTAssertTrue(step.exists, "no Replace 1 and copy")
        clearClipboard()
        step.tap()

        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: MdocUITest.timeout), "not replaced")
        expectCopied(app)
        expectNoCoreText(app)
    }

    /// A known name written out: the email kept as written is a name the
    /// document knows, so the review is done but the text may not leave.
    /// The bar and the menu say so before any tap, and Review opens the bar
    /// on the email; replaced, Share comes back.
    @MainActor
    func testAKnownNameWrittenOutOffersReview() throws {
        let app = openAgreement()
        reviewEmail(app)
        let keep = app.buttons["review-keep"]
        XCTAssertTrue(keep.waitForExistence(timeout: MdocUITest.timeout))
        keep.tap()
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        let note = refusalNote(app)
        XCTAssertTrue(note.waitForExistence(timeout: MdocUITest.timeout), "the bar offers Share")
        XCTAssertTrue(note.label.hasPrefix("1 known name is written out."), note.label)
        let primary = app.buttons["primary-action"]
        XCTAssertEqual(primary.label, "Review")
        XCTAssertFalse(app.buttons["copy-for-ai"].exists)
        openMenu(app)
        expectRefused(app)
        XCTAssertTrue(MdocUITest.menuItem("Review", app).exists)
        closeMenu(app)
        expectNoCoreText(app)

        primary.tap()
        let replace = app.buttons["review-replace"]
        XCTAssertTrue(replace.waitForExistence(timeout: MdocUITest.timeout), "Review opens no bar")
        replace.tap()
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }
        XCTAssertTrue(note.waitForNonExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout))
    }

    /// Changed since its review: said in its own words; below the floor no
    /// step looks for names again, so none is offered.
    @MainActor
    func testChangedSinceReviewSaysSo() throws {
        let app = openAgreement(readiness: "changed")
        app.buttons["primary-action"].tap()  // Replace 1: the review is done.
        let note = refusalNote(app)
        XCTAssertTrue(note.waitForExistence(timeout: MdocUITest.timeout), "the bar offers Share")
        XCTAssertTrue(note.label.hasPrefix("Changed since its review."), note.label)
        XCTAssertFalse(app.buttons["primary-action"].exists, "a step below the floor")
        openMenu(app)
        expectRefused(app)
        closeMenu(app)
        expectNoCoreText(app)
    }

    /// Excluded from AI: said in its own words, with no step on the phone.
    @MainActor
    func testExcludedFromAISaysSo() throws {
        let app = openAgreement(readiness: "excluded")
        app.buttons["primary-action"].tap()
        let note = refusalNote(app)
        XCTAssertTrue(note.waitForExistence(timeout: MdocUITest.timeout), "the bar offers Share")
        XCTAssertTrue(note.label.hasPrefix("Excluded from AI."), note.label)
        XCTAssertFalse(app.buttons["primary-action"].exists)
        openMenu(app)
        expectRefused(app)
        closeMenu(app)
        expectNoCoreText(app)
    }

    /// Below the floor a typed document with no names goes: the bar offers
    /// Share, and the title menu's Copy for AI is on too and copies, where
    /// it said "Find names first" before (#1209).
    @MainActor
    func testBelowTheFloorTheMenuAgreesWithTheBar() throws {
        let app = typedDocument()

        XCTAssertTrue(
            app.descendants(matching: .any)["below-floor-note"].firstMatch.waitForExistence(timeout: MdocUITest.timeout),
            "no below-floor note")
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(
            primary.wait(until: NSPredicate(format: "label BEGINSWITH 'Share'"), timeout: MdocUITest.timeout),
            "the bar offers no Share")
        openMenu(app)
        let copy = MdocUITest.menuItem("Copy for AI", app)
        XCTAssertTrue(copy.isEnabled, "the menu disagrees with the bar's Share")
        XCTAssertTrue(MdocUITest.menuItem("Share", app).isEnabled)
        clearClipboard()
        copy.tap()
        expectCopied(app)
        expectNoCoreText(app)
    }

    /// On a phone that finds names, a document never scanned waits for
    /// Find names: the bar offers it, and the menu lists it after Copy for
    /// AI and Share, which are off.
    @MainActor
    func testNotScannedOffersFindNames() throws {
        let app = typedDocument(detectorTier: true)
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(
            primary.wait(until: NSPredicate(format: "label == 'Find names'"), timeout: MdocUITest.timeout),
            "the bar offers no Find names")
        openMenu(app)
        expectRefused(app)
        let step = MdocUITest.menuItem("Find names", app)
        XCTAssertTrue(step.exists, "no Find names in the menu")
        XCTAssertTrue(step.isEnabled)
        closeMenu(app)
        expectNoCoreText(app)
    }

    /// Share from a document's row refuses with why and the step: Review
    /// opens the document on the name kept as written.
    @MainActor
    func testARowsShareRefusesWithItsStep() throws {
        let app = openAgreement()
        reviewEmail(app)
        let keep = app.buttons["review-keep"]
        XCTAssertTrue(keep.waitForExistence(timeout: MdocUITest.timeout))
        keep.tap()
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }
        XCTAssertTrue(refusalNote(app).waitForExistence(timeout: MdocUITest.timeout))

        let row = MdocUITest.listedRow(app, title: MdocUITest.title)
        XCTAssertTrue(row.waitForExistence(timeout: MdocUITest.timeout))
        row.press(forDuration: 1.0)
        let share = app.buttons["Share"].firstMatch
        XCTAssertTrue(share.waitForExistence(timeout: MdocUITest.timeout), "no Share in the row's menu")
        share.tap()

        let alert = app.alerts["1 known name is written out"]
        XCTAssertTrue(alert.waitForExistence(timeout: MdocUITest.timeout), "no refusal")
        expectNoCoreText(app)
        alert.buttons["Review"].tap()
        XCTAssertTrue(app.buttons["review-replace"].waitForExistence(timeout: MdocUITest.timeout), "Review opens no bar")
    }
}
