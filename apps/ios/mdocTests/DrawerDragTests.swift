import SwiftUI
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

    /// The panel that is not showing is hidden under the one that is: a
    /// fade in Identities showed the sidebar through it (17 Pro, 2026-11-17).
    /// Shut, both are drawn again, ready for the next drag.
    @MainActor
    func testTheOtherPanelHidesWhileOneShows() {
        let controller = DrawerController()
        controller.loadViewIfNeeded()
        for side in [DrawerSide.leading, .trailing] {
            controller.configure(side, present: true, closeLabel: "Close", opens: .anywhere, view: AnyView(EmptyView())) { _ in }
        }
        controller.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        controller.view.layoutIfNeeded()
        XCTAssertFalse(controller.panelIsHidden(.leading), "shut: the sidebar is drawn")
        XCTAssertFalse(controller.panelIsHidden(.trailing), "shut: Identities is drawn")
        controller.show(leading: false, trailing: true, animation: nil)
        XCTAssertTrue(controller.panelIsHidden(.leading), "Identities open: the sidebar is hidden")
        XCTAssertFalse(controller.panelIsHidden(.trailing))
        controller.show(leading: false, trailing: false, animation: nil)
        XCTAssertFalse(controller.panelIsHidden(.leading), "shut again: the sidebar is drawn")
        XCTAssertFalse(controller.panelIsHidden(.trailing))
    }

    // MARK: What keeps its own sideways swipes (`DrawerSwipes`)

    // Hit-testing moved here from `IdentitiesGesturesUITests` (#1076),
    // which keeps real drags on a wide table and on the review card, and
    // the drawers' own drags.

    /// A screen-sized view in a window, as the drawer's container is.
    @MainActor
    private func container() -> (UIWindow, UIView) {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: width, height: 874))
        let root = UIViewController()
        window.rootViewController = root
        window.isHidden = false
        root.view.frame = window.bounds
        return (window, root.view)
    }

    /// Over a text field (the panels' search) or an editable text view, a
    /// swipe is the text's: it moves the caret, and closing the panel
    /// would clear it. Read-only text and other views are not.
    @MainActor
    func testATextFieldOrEditableTextTakesTheSwipe() {
        let (window, view) = container()
        let field = UITextField(frame: CGRect(x: 16, y: 20, width: 300, height: 44))
        let editable = UITextView(frame: CGRect(x: 16, y: 100, width: 300, height: 120))
        editable.isEditable = true
        let readOnly = UITextView(frame: CGRect(x: 16, y: 260, width: 300, height: 120))
        readOnly.isEditable = false
        let plain = UIView(frame: CGRect(x: 16, y: 420, width: 300, height: 120))
        [field, editable, readOnly, plain].forEach(view.addSubview)
        window.layoutIfNeeded()
        XCTAssertTrue(DrawerSwipes.isTextInput(at: CGPoint(x: 100, y: 42), in: view), "the search field")
        XCTAssertTrue(DrawerSwipes.isTextInput(at: CGPoint(x: 100, y: 160), in: view), "editable text")
        XCTAssertFalse(DrawerSwipes.isTextInput(at: CGPoint(x: 100, y: 320), in: view), "read-only text")
        XCTAssertFalse(DrawerSwipes.isTextInput(at: CGPoint(x: 100, y: 480), in: view), "a plain view")
        XCTAssertFalse(DrawerSwipes.isTextInput(at: CGPoint(x: 100, y: 700), in: view), "nothing but the container")
    }

    /// While a row shows its swipe actions, a swipe over the panel is the
    /// row's: laid out and visible, nested anywhere in the panel. Hidden,
    /// see-through or squeezed shut (put away), they show nothing.
    @MainActor
    func testARowShowingItsSwipeActionsKeepsTheSwipe() {
        let list = UIView(frame: CGRect(x: 0, y: 0, width: panel, height: 874))
        let row = UIView(frame: CGRect(x: 0, y: 100, width: list.bounds.width, height: 60))
        list.addSubview(row)
        XCTAssertFalse(DrawerSwipes.showsSwipeActions(in: list), "no actions")
        let pull = UISwipeActionPullView(frame: CGRect(x: 200, y: 0, width: 130, height: 60))
        row.addSubview(pull)
        XCTAssertTrue(DrawerSwipes.showsSwipeActions(in: list), "a row's actions showing")
        pull.isHidden = true
        XCTAssertFalse(DrawerSwipes.showsSwipeActions(in: list), "hidden")
        pull.isHidden = false
        pull.alpha = 0
        XCTAssertFalse(DrawerSwipes.showsSwipeActions(in: list), "see-through")
        pull.alpha = 1
        pull.frame.size.width = 0
        XCTAssertFalse(DrawerSwipes.showsSwipeActions(in: list), "put away")
        pull.removeFromSuperview()
        row.addSubview(UISwipeActionStandardButton(frame: CGRect(x: 260, y: 0, width: 70, height: 60)))
        XCTAssertTrue(DrawerSwipes.showsSwipeActions(in: list), "an action's button")
    }

    /// Content wider than its scroll view, insets counted, scrolls
    /// sideways (a table wider than the screen, a zoomed page) wherever it
    /// is scrolled to; content that fits, or a scroll view that doesn't
    /// scroll, does not.
    @MainActor
    func testWhatScrollsSidewaysIsWiderThanItsView() {
        let scroll = UIScrollView(frame: CGRect(x: 0, y: 0, width: width, height: 300))
        scroll.contentSize = CGSize(width: width, height: 900)
        XCTAssertFalse(DrawerSwipes.scrollsSideways(scroll), "as wide as its view")
        scroll.contentSize.width = 800
        XCTAssertTrue(DrawerSwipes.scrollsSideways(scroll), "wider")
        scroll.contentOffset.x = 800 - width
        XCTAssertTrue(DrawerSwipes.scrollsSideways(scroll), "scrolled to its end")
        scroll.isScrollEnabled = false
        XCTAssertFalse(DrawerSwipes.scrollsSideways(scroll), "not scrolling")
        scroll.isScrollEnabled = true
        scroll.contentOffset = .zero
        scroll.contentSize.width = width - 6
        XCTAssertFalse(DrawerSwipes.scrollsSideways(scroll), "narrower")
        scroll.contentInset = UIEdgeInsets(top: 0, left: 4, bottom: 0, right: 4)
        XCTAssertTrue(DrawerSwipes.scrollsSideways(scroll), "wider with its insets")
    }

    /// A drawer swipe never starts on a view marked `keepsSideSwipes()`
    /// inside the screen (the review bar's cards), on what scrolls
    /// sideways, or on a selection's handle; elsewhere, and on a marked
    /// view outside the screen, it does.
    @MainActor
    func testWhatKeepsSideSwipesInsideTheScreen() {
        let (window, view) = container()
        let content = UIView(frame: view.bounds)
        view.addSubview(content)
        // The review card, marked; a mark outside the screen (a panel's).
        let keeper = SideSwipeKeeperView(frame: CGRect(x: 0, y: 700, width: width, height: 100))
        content.addSubview(keeper)
        let outside = SideSwipeKeeperView(frame: CGRect(x: 0, y: 20, width: width, height: 60))
        view.addSubview(outside)
        // A table wider than the screen, a cell's view inside it.
        let table = UIScrollView(frame: CGRect(x: 0, y: 100, width: width, height: 200))
        table.contentSize = CGSize(width: 800, height: 200)
        let cell = UIView(frame: CGRect(x: 0, y: 0, width: 200, height: 44))
        table.addSubview(cell)
        content.addSubview(table)
        window.layoutIfNeeded()

        func keeps(_ x: CGFloat, _ y: CGFloat) -> Bool {
            DrawerSwipes.keepsSideSwipes(at: CGPoint(x: x, y: y), in: view, within: content)
        }
        XCTAssertTrue(keeps(200, 750), "the marked card")
        XCTAssertFalse(keeps(200, 50), "a mark outside the screen")
        XCTAssertTrue(keeps(100, 120), "a cell in the wide table")
        XCTAssertTrue(keeps(300, 250), "the wide table")
        table.contentSize.width = width
        XCTAssertFalse(keeps(300, 250), "a table that fits")
        XCTAssertFalse(keeps(200, 500), "the text")
        keeper.removeFromSuperview()
        XCTAssertFalse(keeps(200, 750), "the card gone")
    }

    /// A touch within reach of a selection's either end is on its handle;
    /// a caret has none.
    @MainActor
    func testATouchByASelectionsEndIsOnItsHandle() throws {
        let (window, view) = container()
        let text = UITextView(frame: CGRect(x: 0, y: 300, width: width, height: 300))
        text.isEditable = false
        text.font = .systemFont(ofSize: 17)
        text.text = "Maya Bennett signs for Daniel Okafor on the first page."
        view.addSubview(text)
        window.layoutIfNeeded()
        text.selectedRange = NSRange(location: 5, length: 7)
        let range = try XCTUnwrap(text.selectedTextRange)
        let start = text.caretRect(for: range.start)
        let end = text.caretRect(for: range.end)
        XCTAssertTrue(DrawerSwipes.onSelectionHandle(text, at: CGPoint(x: start.midX, y: start.midY)), "the start's handle")
        XCTAssertTrue(DrawerSwipes.onSelectionHandle(text, at: CGPoint(x: end.maxX + 20, y: end.maxY + 20)), "within reach of the end")
        XCTAssertFalse(DrawerSwipes.onSelectionHandle(text, at: CGPoint(x: end.maxX + 60, y: end.midY)), "past its reach")
        XCTAssertFalse(DrawerSwipes.onSelectionHandle(text, at: CGPoint(x: start.midX, y: start.maxY + 120)), "lines below")
        // Through the screen: the handle keeps the swipe.
        let onHandle = text.convert(CGPoint(x: start.midX, y: start.midY), to: view)
        XCTAssertTrue(DrawerSwipes.keepsSideSwipes(at: onHandle, in: view, within: view), "a selection's handle keeps the swipe")
        text.selectedRange = NSRange(location: 5, length: 0)
        XCTAssertFalse(DrawerSwipes.onSelectionHandle(text, at: CGPoint(x: start.midX, y: start.midY)), "a caret has no handle")
    }
}

/// Stand-ins named as UIKit's private swipe action views are, which
/// `DrawerSwipes.showsSwipeActions` finds by name.
final class UISwipeActionPullView: UIView {}
final class UISwipeActionStandardButton: UIButton {}
