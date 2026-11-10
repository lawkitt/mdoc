import XCTest

/// The UI tests wait for an element, its state or a settled screen, never
/// for a fixed time (ADR 0104, Timing).
extension XCUIElement {
    /// Waits up to `timeout` for the element to take taps.
    @discardableResult
    func waitUntilHittable(timeout: TimeInterval) -> Bool {
        wait(until: NSPredicate(format: "isHittable == true"), timeout: timeout)
    }

    /// Waits up to `timeout` for the element to match `predicate`.
    @discardableResult
    func wait(until predicate: NSPredicate, timeout: TimeInterval) -> Bool {
        let matched = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter().wait(for: [matched], timeout: timeout) == .completed
    }
}

extension MdocUITest {
    /// Waits up to `timeout` for `done`, checking it every `interval` (ten
    /// times a second by default, as XCTest's own waits do) while the run
    /// loop goes on.
    @MainActor
    @discardableResult
    static func wait(timeout: TimeInterval, every interval: TimeInterval = 0.1, until done: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while !done() {
            guard Date() < deadline else { return false }
            RunLoop.current.run(until: min(deadline, Date().addingTimeInterval(interval)))
        }
        return true
    }

    /// The first of `elements` to appear within `timeout`, or nil.
    @MainActor
    @discardableResult
    static func firstToAppear(_ elements: [XCUIElement], timeout: TimeInterval) -> XCUIElement? {
        var found: XCUIElement?
        wait(timeout: timeout) {
            found = elements.first(where: \.exists)
            return found != nil
        }
        return found
    }

    /// The screen once it has stopped changing: the same picture for
    /// `steady` seconds. For a screenshot after a scroll, an animation or an
    /// appearance change, where no element says the view has finished
    /// drawing. With `unlike`, first waits for the screen to differ from
    /// that picture: an appearance change draws a moment after it is asked
    /// for. Looks four times a second, letting the run loop go on between
    /// looks: back-to-back screenshots hold up an appearance change on the
    /// Simulator. Gives up after `timeout` and returns the latest picture: a
    /// blinking caret never holds still.
    @MainActor
    @discardableResult
    static func settledScreen(
        unlike previous: XCUIScreenshot? = nil, steady: TimeInterval = 0.5, timeout: TimeInterval = 5
    ) -> XCUIScreenshot {
        let deadline = Date().addingTimeInterval(timeout)
        func look() -> (XCUIScreenshot, Data) {
            let screen = XCUIScreen.main.screenshot()
            return (screen, screen.pngRepresentation)
        }
        func pause() { RunLoop.current.run(until: min(deadline, Date().addingTimeInterval(0.25))) }
        var (latest, latestPNG) = look()
        if let before = previous?.pngRepresentation {
            while latestPNG == before, Date() < deadline {
                pause()
                (latest, latestPNG) = look()
            }
        }
        var steadySince = Date()
        while Date() < deadline {
            pause()
            let (next, png) = look()
            if png != latestPNG {
                (latest, latestPNG) = (next, png)
                steadySince = Date()
            } else if Date().timeIntervalSince(steadySince) >= steady {
                return next
            }
        }
        return latest
    }

    /// A tap in the text, near its top left: in edit mode with no
    /// selection the keyboard waits for one (ADR 0110). Waits for the
    /// keyboard.
    @MainActor
    static func tapIntoText(_ app: XCUIApplication) {
        let text = app.textViews["document-text"]
        text.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: 60, dy: 28)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10), "a tap in the text brings the keyboard")
    }

    /// ⌘↓ from the hardware keyboard: the caret to the end of the text. The
    /// first hardware key after typing on the software keyboard hides that
    /// keyboard, and on a slow runner the key itself can be lost in the
    /// switch (the caret stayed at the start in queue run 42740103288), so
    /// once the software keyboard has gone ⌘↓ is pressed again; at the end
    /// it changes nothing.
    @MainActor
    static func caretToTheEnd(_ app: XCUIApplication) {
        app.typeKey(.downArrow, modifierFlags: .command)
        app.keyboards.firstMatch.wait(until: NSPredicate(format: "exists == false"), timeout: 2)
        app.typeKey(.downArrow, modifierFlags: .command)
    }
}
