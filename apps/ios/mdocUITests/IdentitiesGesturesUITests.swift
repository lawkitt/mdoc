import UIKit
import XCTest

/// Who owns a sideways swipe in an open drawer (ADR 0106): over the panel,
/// its content; the drawer moves only from its sliver or its inner edge.
/// In Identities, swipe right is Replace (a full swipe too); swipe left
/// shows Same as… and Keep, and a long swipe left keeps nothing. iPhone
/// only: on iPad Identities and the sidebar stand beside the text.
final class IdentitiesGesturesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func openDocument() throws -> XCUIApplication {
        guard UIDevice.current.userInterfaceIdiom == .phone else {
            throw XCTSkip("the drawers are iPhone's")
        }
        let app = MdocUITest.launch()
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
    private func drag(_ row: XCUIElement, in panel: XCUIElement, from: CGFloat = 0.5, by dx: CGFloat, _ app: XCUIApplication) {
        let x = panel.frame.minX + panel.frame.width * from
        let start = app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: row.frame.midY))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: 0)),
                    withVelocity: 500, thenHoldForDuration: 0.2)
    }

    /// A drag on the screen from `x` points, at `y` or across the middle.
    @MainActor
    private func drag(_ app: XCUIApplication, fromX x: CGFloat, y: CGFloat? = nil, by dx: CGFloat) {
        let start = app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y ?? app.frame.height * 0.5))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: 0)),
                    withVelocity: 800, thenHoldForDuration: 0.1)
    }

    @MainActor
    func testRowsOwnTheirSwipesInTheDrawer() throws {
        let app = try openDocument()
        XCTAssertTrue(MdocUITest.showIdentities(app))
        let close = app.buttons["identities-done"].firstMatch
        XCTAssertTrue(close.waitUntilHittable(timeout: 5))
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label ENDSWITH ', proposed'")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        // The same row, while it is still to review.
        let proposed = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label == %@", row.label)).firstMatch
        let panel = app.descendants(matching: .any)["identities-panel"].firstMatch
        let replace = app.buttons["Replace"].firstMatch
        let keep = app.buttons["Keep"].firstMatch
        let sameAs = app.buttons["Same as…"].firstMatch
        let width = panel.frame.width

        // Right: Replace shows, and the drawer stays open.
        drag(proposed, in: panel, from: 0.3, by: 120, app)
        XCTAssertTrue(replace.waitForExistence(timeout: 3), "a swipe right shows Replace")
        XCTAssertFalse(close.waitForNonExistence(timeout: 2), "the drawer stays open")
        XCTAssertTrue(close.isHittable)
        XCTAssertTrue(proposed.exists, "nothing decided")
        // Left puts Replace away.
        drag(proposed, in: panel, from: 0.7, by: -120, app)
        XCTAssertTrue(replace.waitForNonExistence(timeout: 3))

        // A long swipe left shows Same as… and Keep, and keeps nothing.
        drag(proposed, in: panel, from: 0.95, by: -width * 0.85, app)
        XCTAssertTrue(keep.waitForExistence(timeout: 3), "a swipe left shows Keep")
        XCTAssertTrue(sameAs.exists, "and Same as…")
        XCTAssertTrue(proposed.exists, "a long swipe left keeps nothing")

        // Right over the revealed actions puts them away; the drawer stays.
        drag(proposed, in: panel, from: 0.2, by: 120, app)
        XCTAssertTrue(keep.waitForNonExistence(timeout: 3), "a swipe right puts Keep and Same as… away")
        XCTAssertFalse(close.waitForNonExistence(timeout: 2), "the drawer stays open")
        XCTAssertTrue(proposed.exists, "nothing decided")

        // A long swipe right replaces: Replace's full swipe.
        let name = String(row.label.prefix { $0 != "," })
        drag(proposed, in: panel, from: 0.12, by: width * 0.85, app)
        let replaced = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier == 'identity-row' AND label BEGINSWITH %@ AND label ENDSWITH ', replaced'", name + ",")).firstMatch
        XCTAssertTrue(proposed.waitForNonExistence(timeout: 5), "a full swipe right decides")
        XCTAssertTrue(replaced.waitForExistence(timeout: 5), "and replaces")
        XCTAssertTrue(close.isHittable, "the drawer stays open")
    }

    /// Both drawers still close from the sliver and from the panel's inner
    /// edge, and not from the middle of the panel.
    @MainActor
    func testTheDrawersCloseFromTheSliverAndTheInnerEdge() throws {
        let app = try openDocument()
        let width = app.frame.width

        // Identities: the panel on the right; its inner edge on the left.
        let close = app.buttons["identities-done"].firstMatch
        let panel = app.descendants(matching: .any)["identities-panel"].firstMatch
        func openIdentities() {
            XCTAssertTrue(MdocUITest.showIdentities(app))
            XCTAssertTrue(close.waitUntilHittable(timeout: 5))
        }
        openIdentities()
        let edge = panel.frame.minX
        drag(app, fromX: edge + 120, by: 200)
        XCTAssertFalse(close.waitForNonExistence(timeout: 2), "a drag over the panel leaves it open")
        drag(app, fromX: edge / 2, by: 200)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a drag from the sliver closes it")
        openIdentities()
        // Over a row: the edge closes the drawer and swipes no row too.
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label ENDSWITH ', proposed'")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        let label = row.label
        drag(app, fromX: edge + 8, y: row.frame.midY, by: 280)
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "a drag from the inner edge closes it")
        openIdentities()
        let same = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'identity-row' AND label == %@", label)).firstMatch
        XCTAssertTrue(same.waitForExistence(timeout: 5), "the row under the edge stays to review")
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 5))

        // The sidebar: the panel on the left; its inner edge on the right.
        let sidebar = app.descendants(matching: .any)["sidebar"].firstMatch
        func openSidebar() {
            MdocUITest.revealSidebar(app)
            XCTAssertTrue(sidebar.waitUntilHittable(timeout: 5))
        }
        openSidebar()
        let inner = sidebar.frame.maxX
        drag(app, fromX: inner - 120, by: -200)
        XCTAssertFalse(sidebar.waitForNonExistence(timeout: 2), "a drag over the sidebar leaves it open")
        drag(app, fromX: (inner + width) / 2, by: -200)
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "a drag from the sliver closes it")
        openSidebar()
        drag(app, fromX: inner - 8, by: -200)
        XCTAssertTrue(sidebar.waitForNonExistence(timeout: 5), "a drag from the inner edge closes it")
    }
}
