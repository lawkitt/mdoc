import UIKit
import XCTest

/// Who owns a sideways swipe in an open drawer (ADR 0106, 0115): Identities
/// rows swipe left only (Replace, Same as… and Keep, no full swipe), and
/// an open drawer closes by a swipe toward its side anywhere over it, or
/// either way from its sliver or inner edge. iPhone only: on iPad Identities
/// and the sidebar stand beside the text. Drag-to-merge runs on both.
final class IdentitiesGesturesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func openDocument(slowToasts: Bool = false, phoneOnly: Bool = true) throws -> XCUIApplication {
        guard !phoneOnly || UIDevice.current.userInterfaceIdiom == .phone else {
            throw XCTSkip("the drawers are iPhone's")
        }
        let app = MdocUITest.launch(slowToasts: slowToasts)
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        XCTAssertTrue(text.links[MdocUITest.email].firstMatch.waitForExistence(timeout: 30))
        return app
    }

    /// A sideways drag of `dx` points across `row`, starting at `from` of
    /// the panel's width (a revealed row's own frame has slid aside): slow
    /// enough to be a pan, not a flick.
    @MainActor
    private func drag(
        _ row: XCUIElement, in panel: XCUIElement, from: CGFloat = 0.5, by dx: CGFloat, velocity: XCUIGestureVelocity = 500,
        _ app: XCUIApplication
    ) {
        let x = panel.frame.minX + panel.frame.width * from
        let start = app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: row.frame.midY))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: 0)),
                    withVelocity: velocity, thenHoldForDuration: 0.2)
    }

    /// A drag on the screen from `x` points, at `y` or across the middle.
    @MainActor
    private func drag(_ app: XCUIApplication, fromX x: CGFloat, y: CGFloat? = nil, by dx: CGFloat) {
        let start = app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y ?? app.frame.height * 0.5))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: 0)),
                    withVelocity: 800, thenHoldForDuration: 0.1)
    }

    @MainActor
    func testRowsSwipeLeftAndASwipeRightClosesIdentities() throws {
        let app = try openDocument()
        XCTAssertTrue(MdocUITest.showIdentities(app))
        // Select shows while the drawer is open (no close button, ADR 0114).
        let close = app.buttons["identities-select"].firstMatch
        XCTAssertTrue(close.waitUntilHittable(timeout: MdocUITest.timeout))
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label ENDSWITH ', proposed'")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: MdocUITest.timeout))
        // The same row, while it is still to review.
        let proposed = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label == %@", row.label)).firstMatch
        let panel = app.descendants(matching: .any)["identities-panel"].firstMatch
        let replace = app.buttons["Replace"].firstMatch
        let keep = app.buttons["Keep"].firstMatch
        let sameAs = app.buttons["Same as…"].firstMatch
        let width = panel.frame.width

        // A swipe left shows Replace, Same as… and Keep; nothing is decided.
        drag(proposed, in: panel, from: 0.8, by: -120, velocity: 150, app)
        XCTAssertTrue(keep.waitForExistence(timeout: 3), "a swipe left shows Keep")
        XCTAssertTrue(sameAs.exists, "and Same as…")
        XCTAssertTrue(replace.exists, "and Replace")
        XCTAssertFalse(proposed.waitForNonExistence(timeout: 2), "nothing decided")

        // Right over the revealed actions puts them away; the drawer stays
        // and nothing is decided (a full swipe ran Replace here).
        drag(proposed, in: panel, from: 0.15, by: 120, app)
        XCTAssertTrue(keep.waitForNonExistence(timeout: 3), "a swipe right puts the actions away")
        XCTAssertFalse(close.waitForNonExistence(timeout: 2), "the drawer stays open")
        XCTAssertTrue(proposed.exists, "nothing decided")

        // A long swipe left only shows the actions; Replace is a tap.
        let name = String(row.label.prefix { $0 != "," })
        drag(proposed, in: panel, from: 0.95, by: -width * 0.85, app)
        XCTAssertTrue(replace.waitForExistence(timeout: 3), "a long swipe left shows Replace")
        XCTAssertTrue(proposed.exists, "and decides nothing")
        replace.tap()
        let replaced = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier == 'identity-row' AND label BEGINSWITH %@ AND label ENDSWITH ', replaced'", name + ",")).firstMatch
        XCTAssertTrue(proposed.waitForNonExistence(timeout: 5), "Replace decides")
        XCTAssertTrue(replaced.waitForExistence(timeout: MdocUITest.timeout), "and replaces")
        XCTAssertTrue(close.isHittable, "the drawer stays open")

        // A swipe right over a row closes Identities and decides nothing.
        let label = replaced.label
        drag(replaced, in: panel, from: 0.3, by: 200, app)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a swipe right over a row closes Identities")
        XCTAssertTrue(MdocUITest.showIdentities(app))
        XCTAssertTrue(app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label == %@", label)).firstMatch
            .waitForExistence(timeout: MdocUITest.timeout), "the row is as it was")
    }

    /// Both drawers close from the sliver, the panel's inner edge and, toward
    /// their side, anywhere over the panel (ADR 0115); a swipe the other way
    /// over the panel, or in its search field, leaves them open. With
    /// Identities open, a swipe right from the screen's left edge closes
    /// Identities and shows the document; it never opens the sidebar, whose
    /// edge lay under Identities' sliver (its own test until ADR 0112's
    /// sweep, which saved the launch).
    /// Whether `element` stops moving: its frame the same 0.2 s apart.
    @MainActor
    private func settles(_ element: XCUIElement) -> Bool {
        var last = CGRect.null
        return MdocUITest.wait(timeout: 5, every: 0.2) {
            let frame = element.frame
            defer { last = frame }
            return frame == last
        }
    }

    @MainActor
    func testTheDrawersCloseAsTheyOpen() throws {
        let app = try openDocument()
        let width = app.frame.width

        // Identities: the panel on the right; its inner edge on the left.
        // Select shows while the drawer is open (no close button, ADR 0114).
        let close = app.buttons["identities-select"].firstMatch
        let panel = app.descendants(matching: .any)["identities-panel"].firstMatch
        // Open and at rest: mid-slide the panel is up to 24 pt short of its
        // place, and a drag meant for its inner edge lands inside it.
        func openIdentities() {
            XCTAssertTrue(MdocUITest.showIdentities(app))
            XCTAssertTrue(close.waitUntilHittable(timeout: MdocUITest.timeout))
            XCTAssertTrue(settles(panel), "Identities at rest")
        }
        openIdentities()
        let edge = panel.frame.minX
        // Over the header, where no row's swipe answers a drag left.
        let header = close.frame.midY
        drag(app, fromX: edge + 200, y: header, by: -150)
        XCTAssertFalse(close.waitForNonExistence(timeout: 2), "a drag left over the panel leaves it open")
        drag(app, fromX: edge + 120, y: header, by: 200)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a drag right over the panel closes it")
        openIdentities()
        drag(app, fromX: edge / 2, by: 200)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a drag from the sliver closes it")
        openIdentities()
        // Over a row: the edge closes the drawer and swipes no row too.
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label ENDSWITH ', proposed'")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: MdocUITest.timeout))
        let label = row.label
        drag(app, fromX: edge + 8, y: row.frame.midY, by: 280)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a drag from the inner edge closes it")
        openIdentities()
        let same = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label == %@", label)).firstMatch
        XCTAssertTrue(same.waitForExistence(timeout: MdocUITest.timeout), "the row under the edge stays to review")
        // From the screen's left edge: Identities closes, the sidebar stays shut.
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        drag(app, fromX: 6, by: 240)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "the swipe from the left edge closes Identities")
        XCTAssertTrue(app.textViews["document-text"].waitUntilHittable(timeout: MdocUITest.timeout), "the document shows")
        XCTAssertFalse(sidebar.exists, "the sidebar stays shut")

        // The sidebar: the panel on the left; its inner edge on the right.
        func openSidebar() {
            MdocUITest.revealSidebar(app)
            XCTAssertTrue(sidebar.waitUntilHittable(timeout: MdocUITest.timeout))
            XCTAssertTrue(settles(sidebar), "the sidebar at rest")
        }
        openSidebar()
        let inner = sidebar.frame.maxX
        // A drag in the search field is the field's: the sidebar stays.
        let search = app.descendants(matching: .any)["sidebar-search"].firstMatch
        XCTAssertTrue(search.waitUntilHittable(timeout: MdocUITest.timeout))
        drag(app, fromX: min(search.frame.maxX - 20, inner - 30), y: search.frame.midY, by: -150)
        XCTAssertFalse(sidebar.waitForNonExistence(timeout: 2), "a drag left in the search field leaves it open")
        drag(app, fromX: inner - 200, by: 150)
        XCTAssertFalse(sidebar.waitForNonExistence(timeout: 2), "a drag right over the sidebar leaves it open")
        drag(app, fromX: inner - 120, by: -200)
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "a drag left over the sidebar closes it")
        openSidebar()
        drag(app, fromX: (inner + width) / 2, by: -200)
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "a drag from the sliver closes it")
        openSidebar()
        drag(app, fromX: inner - 8, by: -200)
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "a drag from the inner edge closes it")
    }

    /// A sideways swipe anywhere on the document opens a drawer (ADR 0114):
    /// right the sidebar, left Identities; up and down scroll the text. The
    /// review card keeps its sideways swipe, and while editing only the
    /// screen's edge opens the sidebar.
    @MainActor
    func testASwipeAnywhereOnTheDocumentOpensADrawer() throws {
        let app = try openDocument()
        let width = app.frame.width
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        let identities = app.descendants(matching: .any)["identities-panel"].firstMatch
        let text = app.textViews["document-text"]

        // Up and down: the text's.
        text.swipeUp()
        text.swipeDown()
        XCTAssertFalse(sidebar.exists, "a vertical swipe opens no drawer")
        XCTAssertFalse(identities.exists, "a vertical swipe opens no drawer")

        drag(app, fromX: width * 0.4, by: 200)
        XCTAssertTrue(sidebar.waitUntilHittable(timeout: 5), "a swipe right on the text opens the sidebar")
        app.buttons["Close sidebar"].firstMatch.tap()
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "the sliver closes it")

        drag(app, fromX: width * 0.6, by: -200)
        XCTAssertTrue(identities.waitUntilHittable(timeout: 5), "a swipe left on the text opens Identities")
        drag(app, fromX: 30, by: 200)
        XCTAssertTrue(identities.waitForNonExistence(timeout: 5), "a swipe back from the sliver closes it")

        // The review card's sideways swipe stays the card's.
        let email = text.links[MdocUITest.email].firstMatch
        MdocUITest.tapAboveBottomBar(email, in: text, app)
        let card = app.descendants(matching: .any)["review-card"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: MdocUITest.timeout))
        drag(app, fromX: width * 0.5, y: card.frame.midY, by: 160)
        drag(app, fromX: width * 0.5, y: card.frame.midY, by: -160)
        XCTAssertFalse(sidebar.waitForExistence(timeout: 1), "a swipe on the card opens no sidebar")
        XCTAssertFalse(identities.exists, "a swipe on the card opens no Identities")
        app.buttons["review-end"].firstMatch.tap()

        // Editing: the middle of the text is the text's, the edge opens.
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: MdocUITest.timeout))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        drag(app, fromX: width * 0.4, by: 200)
        XCTAssertFalse(sidebar.waitForExistence(timeout: 2), "editing, a swipe in the text opens no sidebar")
        drag(app, fromX: 6, by: 200)
        XCTAssertTrue(sidebar.waitUntilHittable(timeout: 5), "editing, the left edge opens the sidebar")
    }

    /// A table wider than the screen keeps every sideways swipe that starts
    /// on it, even at its edge; the text around it opens the drawers
    /// (ADR 0114).
    @MainActor
    func testATableWiderThanTheScreenKeepsItsSwipes() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the drawers are iPhone's") }
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo
            .appendingPathComponent("tests/fixtures/mobile-markdown/rendering/itinerary.md").path
        app.launch()
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 20))
        let cell = app.textViews.matching(NSPredicate(format: "value BEGINSWITH 'GF 44'")).firstMatch
        XCTAssertTrue(cell.waitForExistence(timeout: MdocUITest.timeout), "the table shows")
        let width = app.frame.width
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        let identities = app.descendants(matching: .any)["identities-panel"].firstMatch

        // At its left edge, then scrolled along and back: the table's. The
        // row's height, taken once: scrolled along, the cell leaves the screen.
        let tableRow = cell.frame.midY
        drag(app, fromX: width * 0.4, y: tableRow, by: 200)
        XCTAssertFalse(sidebar.waitForExistence(timeout: 2), "a swipe right on the table opens no sidebar")
        drag(app, fromX: width * 0.6, y: tableRow, by: -200)
        XCTAssertFalse(identities.waitForExistence(timeout: 2), "a swipe left on the table opens no Identities")
        drag(app, fromX: width * 0.4, y: tableRow, by: 240)
        XCTAssertFalse(sidebar.waitForExistence(timeout: 2), "back at its edge, still the table's")

        // The text above the table opens the sidebar.
        let heading = app.textViews["document-text"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.05))
        let above = heading.screenPoint.y
        XCTAssertLessThan(above, tableRow - 30, "the text above the table")
        drag(app, fromX: width * 0.4, y: above, by: 200)
        XCTAssertTrue(sidebar.waitUntilHittable(timeout: 5), "a swipe right on the text opens the sidebar")
    }

    /// Drag-to-merge (ADR 0106): in Select, a row's ≡ handle drags it onto
    /// another name and the two become one, under the target's alias, with
    /// the selection kept; letting go over the row itself does nothing; one
    /// Undo brings both back. On iPad, in Identities beside the text.
    @MainActor
    func testDraggingAHandleOntoAnotherNameMakesThemOne() throws {
        // The agreement's email, and its surname marked from Find: two names.
        let app = try openDocument(slowToasts: true, phoneOnly: false)
        app.buttons["title-menu"].tap()
        app.buttons["find-in-document"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(field.wait(until: NSPredicate(format: "hasKeyboardFocus == true"), timeout: 10))
        // The Simulator drops a keystroke now and then while matches load:
        // type again until the field holds the whole word.
        let typed = NSPredicate(format: "value == 'Bennett'")
        for _ in 0..<3 where !field.wait(until: typed, timeout: 0.5) {
            let current = (field.value as? String) ?? ""
            let placeholder = field.placeholderValue ?? ""
            let length = current == placeholder ? 0 : current.count
            field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: length) + "Bennett")
        }
        XCTAssertEqual(field.value as? String, "Bennett")
        let hide = app.buttons["find-hide-name"]
        XCTAssertTrue(hide.waitForExistence(timeout: MdocUITest.timeout))
        hide.tap()
        let end = app.buttons["review-end"]
        XCTAssertTrue(end.waitForExistence(timeout: 10))
        end.tap()
        XCTAssertTrue(MdocUITest.showIdentities(app))
        app.buttons["identities-select"].firstMatch.tap()
        let rows = app.descendants(matching: .any).matching(identifier: "identity-row")
        let handles = app.descendants(matching: .any).matching(identifier: "identity-drag-handle")
        XCTAssertTrue(handles.firstMatch.waitForExistence(timeout: MdocUITest.timeout), "Select shows the handles")
        let count = rows.count
        XCTAssertGreaterThanOrEqual(count, 2)
        XCTAssertEqual(handles.count, count, "a handle on every row")
        XCTAssertEqual(handles.firstMatch.label, "Drag to make one with another name")
        attach("identities-select-handles")

        // "anna.smirnova@example.com, Email, EMAIL_1, 1 mentions, proposed".
        func parts(_ row: XCUIElement) -> (name: String, alias: String) {
            let parts = row.label.components(separatedBy: ", ")
            return (parts[0], parts[parts.count - 3])
        }
        let dragged = parts(rows.element(boundBy: 0))
        let target = parts(rows.element(boundBy: 1))
        func row(_ name: String) -> XCUIElement {
            app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier == 'identity-row' AND label BEGINSWITH %@", name + ",")).firstMatch
        }
        // Select both: the merged name stays selected.
        row(target.name).tap()
        row(dragged.name).tap()
        XCTAssertTrue(row(target.name).isSelected)

        // Let go over itself: nothing.
        let handle = handles.element(boundBy: 0)
        let start = handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 0, dy: 6)),
                    withVelocity: 200, thenHoldForDuration: 0.3)
        MdocUITest.settledScreen()
        XCTAssertEqual(rows.count, count, "a row dropped on itself stays")
        // Let go off the list, over the header: nothing.
        let header = app.staticTexts["Identities"].firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        handles.element(boundBy: 0).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.1, thenDragTo: header, withVelocity: 300, thenHoldForDuration: 0.3)
        MdocUITest.settledScreen()
        XCTAssertEqual(rows.count, count, "let go off the list: nothing")
        XCTAssertFalse(app.buttons["toast-action"].firstMatch.exists)

        // Onto the second row: one name, the target's alias.
        let onto = row(target.name).coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.5))
        handles.element(boundBy: 0).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.1, thenDragTo: onto, withVelocity: 300, thenHoldForDuration: 0.4)
        let undo = app.buttons["toast-action"].firstMatch
        XCTAssertTrue(undo.waitForExistence(timeout: MdocUITest.timeout), "the Undo toast")
        XCTAssertTrue(app.staticTexts["\(dragged.alias) is now \(target.alias)"].exists)
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { rows.count == count - 1 }, "one name fewer")
        let merged = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier == 'identity-row' AND label CONTAINS %@ AND label CONTAINS %@",
            ", \(target.alias),", dragged.name)).firstMatch
        XCTAssertTrue(merged.exists, "the target's alias names both")
        XCTAssertTrue(merged.isSelected, "the selection stays")
        XCTAssertFalse(app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier == 'identity-row' AND label CONTAINS %@", ", \(dragged.alias),")).firstMatch.exists)

        // Undo: both again. The toast goes after 6 s, so no screenshot
        // before the tap: on a slow runner the toast was gone by then.
        undo.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: MdocUITest.timeout) { rows.count == count }, "Undo brings both back")
        XCTAssertTrue(row(dragged.name).label.contains(", \(dragged.alias),"))
        XCTAssertTrue(row(target.name).label.contains(", \(target.alias),"))
        attach("identities-drag-undone")
    }

    /// The settled screen, kept with the result for a look.
    @MainActor
    private func attach(_ name: String) {
        let shot = XCTAttachment(screenshot: MdocUITest.settledScreen())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
