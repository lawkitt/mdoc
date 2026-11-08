import MdocFFI
import XCTest
@testable import mdoc

/// Drag-to-merge's geometry (ADR 0106): the row under the finger, from the
/// rows' frames and the finger's point, never the dragged row itself; and
/// how fast the list scrolls near its edges.
@MainActor
final class IdentityDragTests: XCTestCase {
    private let a = IdentityID(rawValue: "1")
    private let b = IdentityID(rawValue: "2")
    private let c = IdentityID(rawValue: "3")
    /// The list on an iPhone 17 Pro's drawer: 330 points wide, from y = 200.
    private let list = CGRect(x: 72, y: 200, width: 330, height: 500)

    /// Three rows of 52 points with 8 between them, the drawer's rows.
    private var rows: [IdentityID: CGRect] {
        [
            a: CGRect(x: 88, y: 210, width: 306, height: 52),
            b: CGRect(x: 88, y: 270, width: 306, height: 52),
            c: CGRect(x: 88, y: 330, width: 306, height: 52),
        ]
    }

    private func target(_ x: CGFloat, _ y: CGFloat, dragging: IdentityID? = nil, visible: CGRect? = nil) -> IdentityID? {
        IdentityDrag.target(at: CGPoint(x: x, y: y), rows: rows, visible: visible ?? list, dragged: dragging ?? a)
    }

    func testTheTargetIsTheRowUnderTheFinger() {
        XCTAssertEqual(target(200, 296), b)
        XCTAssertEqual(target(100, 331), c, "its top edge")
        XCTAssertEqual(target(390, 380), c, "its far corner")
    }

    func testTheDraggedRowIsNeverItsOwnTarget() {
        XCTAssertNil(target(200, 236), "over itself")
        XCTAssertNil(target(200, 236, dragging: a))
        XCTAssertEqual(target(200, 236, dragging: b), a, "another row's drag takes it")
    }

    func testNothingOutsideTheRowsOrTheVisibleList() {
        XCTAssertNil(target(200, 450), "below the last row")
        XCTAssertNil(target(40, 296), "on the sliver, beside the list")
        XCTAssertNil(target(200, 150), "over the header")
        // b scrolled under the selection bar: still a frame, but hidden.
        let aboveTheBar = CGRect(x: list.minX, y: list.minY, width: list.width, height: 80)
        XCTAssertNil(target(200, 296, visible: aboveTheBar))
        XCTAssertNil(IdentityDrag.target(at: CGPoint(x: 200, y: 296), rows: [:], visible: list, dragged: a))
    }

    func testTheGapBetweenTwoRowsGoesToTheNearer() {
        // b ends at 322 and c starts at 330.
        XCTAssertEqual(target(200, 324), b, "just under b")
        XCTAssertEqual(target(200, 328), c, "just over c")
        XCTAssertEqual(target(200, 265, dragging: c), a, "between a and b, nearer a")
        XCTAssertEqual(target(200, 267, dragging: a), b, "a dragged: the gap above b is b's")
    }

    func testTheListScrollsNearItsEdgesOnly() {
        let edge = IdentityDrag.edge
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: list.midY, visible: list), 0, "the middle")
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: list.minY + edge, visible: list), 0, "the top edge's end")
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: list.maxY - edge, visible: list), 0)
        let up = IdentityDrag.scrollSpeed(at: list.minY + edge / 2, visible: list)
        XCTAssertLessThan(up, 0, "near the top: up")
        XCTAssertEqual(up, -IdentityDrag.maxSpeed / 2, accuracy: 0.001, "faster the nearer")
        XCTAssertGreaterThan(IdentityDrag.scrollSpeed(at: list.maxY - 10, visible: list), 0, "near the bottom: down")
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: list.minY - 40, visible: list), -IdentityDrag.maxSpeed, "past the top: the fastest")
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: list.maxY + 40, visible: list), IdentityDrag.maxSpeed)
        let short = CGRect(x: 0, y: 0, width: 330, height: edge * 2)
        XCTAssertEqual(IdentityDrag.scrollSpeed(at: 4, visible: short), 0, "a list too short to scroll by its edges")
    }

    func testTheScrollStaysInTheContent() {
        XCTAssertEqual(IdentityDrag.offset(100, by: -30, lowest: -10, highest: 400), 70)
        XCTAssertEqual(IdentityDrag.offset(5, by: -30, lowest: -10, highest: 400), -10, "the top")
        XCTAssertEqual(IdentityDrag.offset(390, by: 30, lowest: -10, highest: 400), 400, "the bottom")
        XCTAssertEqual(IdentityDrag.offset(0, by: 30, lowest: -10, highest: -200), -10, "content shorter than the list")
    }

    /// The selection bar covers the list's bottom: no handle or name under
    /// it, and the list scrolls at its top. Outside Select no touch is a
    /// handle's, even where one was.
    func testTheSelectionBarHidesTheListsBottom() {
        let tracking = IdentityDragTracking()
        tracking.list = list
        tracking.handles = [b: CGRect(x: 350, y: 274, width: 44, height: 44)]
        XCTAssertNil(tracking.handle(at: CGPoint(x: 370, y: 296)), "outside Select")
        tracking.active = true
        XCTAssertEqual(tracking.visible, list)
        XCTAssertEqual(tracking.handle(at: CGPoint(x: 370, y: 296)), b)
        XCTAssertNil(tracking.handle(at: CGPoint(x: 200, y: 296)), "the row, not its handle")
        tracking.barTop = 260
        XCTAssertEqual(tracking.visible, CGRect(x: list.minX, y: list.minY, width: list.width, height: 60))
        XCTAssertNil(tracking.handle(at: CGPoint(x: 370, y: 296)), "under the bar")
        tracking.barTop = 900
        XCTAssertEqual(tracking.visible, list, "a bar below the list hides none of it")
    }

    /// The lifted copy follows the finger up and down, never sideways.
    func testTheCopyFollowsTheFingerUpAndDown() {
        var state = IdentityDragState(
            dragged: a, frame: rows[a]!, start: CGPoint(x: 370, y: 236), location: CGPoint(x: 370, y: 236))
        state.location = CGPoint(x: 300, y: 336)
        XCTAssertEqual(state.copyFrame, rows[a]!.offsetBy(dx: 0, dy: 100))
    }
}
