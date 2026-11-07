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
    /// for. Gives up after `timeout` and returns the latest picture: a
    /// blinking caret never holds still.
    @MainActor
    @discardableResult
    static func settledScreen(
        unlike previous: XCUIScreenshot? = nil, steady: TimeInterval = 0.5, timeout: TimeInterval = 5
    ) -> XCUIScreenshot {
        let deadline = Date().addingTimeInterval(timeout)
        var latest = XCUIScreen.main.screenshot()
        var latestPNG = latest.pngRepresentation
        if let before = previous?.pngRepresentation {
            while latestPNG == before, Date() < deadline {
                latest = XCUIScreen.main.screenshot()
                latestPNG = latest.pngRepresentation
            }
        }
        var steadySince = Date()
        while Date() < deadline {
            let next = XCUIScreen.main.screenshot()
            let png = next.pngRepresentation
            if png != latestPNG {
                latest = next
                latestPNG = png
                steadySince = Date()
            } else if Date().timeIntervalSince(steadySince) >= steady {
                return next
            }
        }
        return latest
    }
}
