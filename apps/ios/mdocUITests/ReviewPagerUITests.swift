import XCTest

/// The review card swipes as a carousel (docs/mobile-ui.md, Review bar):
/// let go past a third of the card, or with a fling, and the strip settles
/// on the neighbor; a short slow drag springs back. The same both ways.
final class ReviewPagerUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A drag of `dx` across (and `dy` down) at `velocity` points a second, held
    /// still for `hold` before letting go, across the card's first line
    /// between the name and the mentions chip, where nothing takes a tap.
    @MainActor
    private func drag(_ app: XCUIApplication, by dx: CGFloat, dy: CGFloat = 0, velocity: CGFloat, hold: TimeInterval) {
        // The card under review's: the neighbors carry no identifiers.
        let chips = app.buttons.matching(identifier: "review-mentions")
        XCTAssertEqual(chips.count, 1, "the neighbors' controls are not the card's")
        let chip = chips.firstMatch
        let point = CGVector(dx: chip.frame.minX - 40, dy: chip.frame.midY)
        let start = app.coordinate(withNormalizedOffset: .zero).withOffset(point)
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: dy)),
                    withVelocity: XCUIGestureVelocity(velocity), thenHoldForDuration: hold)
    }

    @MainActor
    func testTheCardSwipesAsACarouselAndStillPullsTheBar() throws {
        let app = MdocUITest.launch()
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        // Names to step through: Find and Hide name on the surname.
        app.buttons["document-menu"].tap()
        let find = app.buttons["find-in-document"]
        XCTAssertTrue(find.waitForExistence(timeout: 5))
        find.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertTrue(field.wait(until: NSPredicate(format: "hasKeyboardFocus == true"), timeout: 10))
        field.typeText("Bennett")
        let hide = app.buttons["find-hide-name"]
        XCTAssertTrue(hide.waitForExistence(timeout: 5))
        hide.tap()
        let card = app.descendants(matching: .any)["review-card"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        let progress = app.buttons["review-progress"]
        // "3 of 15, 0 decided": two steps one way, then two back, toward the
        // side with room for them (a swipe left is the next card).
        let numbers = Array(progress.label.split { !$0.isNumber }.compactMap { Int($0) }.prefix(2))
        XCTAssertEqual(numbers.count, 2, progress.label)
        let roomAhead = (numbers.last ?? 0) - (numbers.first ?? 0)
        XCTAssertGreaterThanOrEqual(max(roomAhead, (numbers.first ?? 1) - 1), 2, "room for two steps: \(progress.label)")
        XCTAssertTrue(app.buttons["review-mentions"].firstMatch.exists, "the surname repeats")
        // The card's own width: the strip less the peeks at its sides.
        let width = app.frame.width - 60

        for direction: CGFloat in roomAhead >= 2 ? [-1, 1] : [1, -1] {
            let start = progress.label
            // A slow drag of a fifth of the card: back where it was.
            drag(app, by: direction * width * 0.22, velocity: 60, hold: 0)
            XCTAssertFalse(
                progress.wait(until: NSPredicate(format: "label != %@", start), timeout: 2),
                "a short slow drag springs back (\(direction)): \(start) → \(progress.label)")
            MdocUITest.settledScreen()

            // A fling of a few points: on to the neighbor.
            drag(app, by: direction * 50, velocity: 3000, hold: 0)
            XCTAssertTrue(
                progress.wait(until: NSPredicate(format: "label != %@", start), timeout: 3),
                "a fling steps (\(direction))")
            MdocUITest.settledScreen()

            // Slowly past a third: on again.
            let next = progress.label
            drag(app, by: direction * width * 0.45, velocity: 60, hold: 0)
            XCTAssertTrue(
                progress.wait(until: NSPredicate(format: "label != %@", next), timeout: 3),
                "past a third steps (\(direction))")
            MdocUITest.settledScreen()
        }

        // Up and down on the card still pull the bar open and shut.
        let details = app.descendants(matching: .any)["review-details"].firstMatch
        let shown = { details.exists && details.frame.height > 40 }
        drag(app, by: 0, dy: -220, velocity: 600, hold: 0)
        XCTAssertTrue(MdocUITest.wait(timeout: 5, until: shown), "a drag up on the card shows the mentions")
        MdocUITest.settledScreen()
        drag(app, by: 0, dy: 260, velocity: 600, hold: 0)
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { !shown() }, "a drag down on the card hides them")
    }
}
