import XCTest
@testable import mdoc

/// Which drags move a drawer. Open (ADR 0115): either way from the dimmed
/// sliver or the panel's inner edge, and over the panel's content only
/// toward closing: the other way is the content's (an Identities row's
/// actions are on a left swipe).
final class DrawerDragTests: XCTestCase {
    /// An iPhone 17 Pro: a 402-point screen, a 330-point panel and the
    /// 72-point sliver.
    private let width: CGFloat = 402
    private let panel: CGFloat = 330

    func testTheTrailingDrawerMovesEitherWayOnlyFromTheSliverAndItsInnerEdge() {
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

    func testTheLeadingDrawerMovesEitherWayOnlyFromTheSliverAndItsInnerEdge() {
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

    /// Over an open panel's content only a swipe toward closing is the
    /// drawer's (ADR 0115): right over Identities closes it, left is a
    /// row's; up and down scroll the list.
    func testOverTheOpenPanelOnlyAClosingSwipeIsTheDrawers() {
        // Identities: the screen moves left to open, so right closes.
        func decide(_ dx: CGFloat, _ dy: CGFloat) -> Bool? {
            DrawerDrag.decide(CGVector(dx: dx, dy: dy), sign: -1, isOpen: true, overPanel: true)
        }
        XCTAssertEqual(decide(12, 1), true, "right closes Identities")
        XCTAssertEqual(decide(-5, 0), false, "left is a row's swipe")
        XCTAssertEqual(decide(-12, 1), false, "left, clearly")
        XCTAssertEqual(decide(1, 5), false, "down: the list's scroll")
        XCTAssertNil(decide(3, 0), "not yet clear")
        // The sidebar: the screen moves right to open, so left closes.
        XCTAssertEqual(DrawerDrag.decide(CGVector(dx: -12, dy: 0), sign: 1, isOpen: true, overPanel: true), true)
        XCTAssertEqual(DrawerDrag.decide(CGVector(dx: 12, dy: 0), sign: 1, isOpen: true, overPanel: true), false)
    }

    /// Which panel shows when the bindings change: the one just asked for
    /// wins (⌘K's search over Identities opens the sidebar), then the open one.
    func testThePanelJustAskedForShows() {
        func wanted(_ leading: Bool, _ trailing: Bool, was: (Bool, Bool), open: DrawerSide?) -> DrawerSide? {
            DrawerDrag.wanted(leading: leading, trailing: trailing, wasLeading: was.0, wasTrailing: was.1, open: open)
        }
        XCTAssertEqual(wanted(true, true, was: (false, true), open: .trailing), .leading, "search over Identities")
        XCTAssertEqual(wanted(true, true, was: (true, false), open: .leading), .trailing, "Identities over the sidebar")
        XCTAssertEqual(wanted(false, true, was: (true, true), open: .leading), .trailing, "the sidebar closes, Identities stays asked for")
        XCTAssertNil(wanted(false, false, was: (false, true), open: .trailing), "Identities closes")
        XCTAssertEqual(wanted(true, false, was: (false, false), open: nil), .leading, "☰")
    }

    /// Reduce Motion puts the panel over the screen to fade it in; turned off
    /// again, the panel goes back under the screen (review of #864: a closed
    /// sidebar, always shown, covered the screen).
    @MainActor
    func testPanelsGoBackUnderTheScreenWhenReduceMotionEnds() {
        let controller = DrawerController()
        controller.loadViewIfNeeded()
        XCTAssertTrue(controller.panelsLieUnderScreen, "at first")
        controller.reduceMotion = true
        XCTAssertFalse(controller.panelsLieUnderScreen, "Reduce Motion: the panel over the screen")
        controller.reduceMotion = false
        XCTAssertTrue(controller.panelsLieUnderScreen, "Reduce Motion off: under the screen again")
    }
}
