import XCTest
@testable import mdoc

/// Which drags move an open drawer (ADR 0106): those that start on the
/// dimmed sliver or at the panel's inner edge. A drag over the panel's
/// content is the content's: an Identities row's swipe right is Replace,
/// not a close.
final class DrawerDragTests: XCTestCase {
    /// An iPhone 17 Pro: a 402-point screen, a 330-point panel and the
    /// 72-point sliver.
    private let width: CGFloat = 402
    private let panel: CGFloat = 330

    func testTheTrailingDrawerMovesOnlyFromTheSliverAndItsInnerEdge() {
        // Identities: the panel on the right, its inner edge at x = 72.
        func moves(_ x: CGFloat) -> Bool {
            DrawerDrag.movesOpenDrawer(startingAt: x, width: width, panelWidth: panel, panelOnLeft: false)
        }
        XCTAssertTrue(moves(10), "the sliver")
        XCTAssertTrue(moves(71), "the sliver's last point")
        XCTAssertTrue(moves(72 + DrawerDrag.innerEdge), "the inner edge")
        XCTAssertFalse(moves(72 + DrawerDrag.innerEdge + 1), "just past the inner edge")
        XCTAssertFalse(moves(237), "a row's middle, where a swipe starts")
        XCTAssertFalse(moves(390), "near the screen's edge")
    }

    func testTheLeadingDrawerMovesOnlyFromTheSliverAndItsInnerEdge() {
        // The sidebar: the panel on the left, its inner edge at x = 330.
        func moves(_ x: CGFloat) -> Bool {
            DrawerDrag.movesOpenDrawer(startingAt: x, width: width, panelWidth: panel, panelOnLeft: true)
        }
        XCTAssertTrue(moves(390), "the sliver")
        XCTAssertTrue(moves(331), "the sliver's first point")
        XCTAssertTrue(moves(330 - DrawerDrag.innerEdge), "the inner edge")
        XCTAssertFalse(moves(330 - DrawerDrag.innerEdge - 1), "just past the inner edge")
        XCTAssertFalse(moves(165), "a row's middle")
        XCTAssertFalse(moves(4), "near the screen's edge")
    }

    /// Which moves start a drawer drag (ADR 0114): past 10 pt and mostly
    /// sideways; up or down is the text's scroll, a diagonal too.
    func testADragStartsOnlyWhenClearlySideways() {
        // The sidebar: the screen moves right to open.
        func decide(_ dx: CGFloat, _ dy: CGFloat, open: Bool = false) -> Bool? {
            DrawerDrag.decide(CGVector(dx: dx, dy: dy), sign: 1, isOpen: open)
        }
        XCTAssertNil(decide(3, 1), "not yet clear")
        XCTAssertEqual(decide(12, 2), true, "sideways toward the open side")
        XCTAssertEqual(decide(-5, 0), false, "closed, away from the open side")
        XCTAssertEqual(decide(-12, 1, open: true), true, "open, either way")
        XCTAssertEqual(decide(1, 4), false, "down: the text's scroll")
        XCTAssertEqual(decide(10, 11), false, "a diagonal that goes farther down")
        XCTAssertEqual(decide(12, 10), false, "a diagonal not mostly sideways")
        XCTAssertNil(decide(8, 7), "a diagonal under 10 pt waits")
    }
}
