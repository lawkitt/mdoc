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

    /// Where the caret draws inside `element`, in screen points: the
    /// pixels that change while it blinks, nothing else on screen moving.
    /// Looks at the element's left `share` only, away from the scroll
    /// indicator. Nil when no blink was seen within `timeout`.
    @MainActor
    static func caretRect(in element: XCUIElement, share: CGFloat = 0.6, timeout: TimeInterval = 4) -> CGRect? {
        let frame = element.frame
        let region = CGRect(x: frame.minX, y: frame.minY, width: frame.width * share, height: frame.height)
        func pixels() -> (data: [UInt8], width: Int, height: Int, scale: CGFloat)? {
            let image = XCUIScreen.main.screenshot().image
            guard let cg = image.cgImage else { return nil }
            let scale = CGFloat(cg.width) / image.size.width
            let crop = CGRect(x: region.minX * scale, y: region.minY * scale, width: region.width * scale, height: region.height * scale)
                .integral
            guard let part = cg.cropping(to: crop) else { return nil }
            var data = [UInt8](repeating: 0, count: part.width * part.height * 4)
            guard let context = CGContext(
                data: &data, width: part.width, height: part.height, bitsPerComponent: 8, bytesPerRow: part.width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            context.draw(part, in: CGRect(x: 0, y: 0, width: part.width, height: part.height))
            return (data, part.width, part.height, scale)
        }
        guard let first = pixels() else { return nil }
        var box: (minX: Int, minY: Int, maxX: Int, maxY: Int)?
        let deadline = Date().addingTimeInterval(timeout)
        var looks = 0
        while Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.12))
            guard let next = pixels(), next.width == first.width, next.height == first.height else { continue }
            looks += 1
            for y in 0..<first.height {
                for x in 0..<first.width {
                    let index = (y * first.width + x) * 4
                    let change = abs(Int(first.data[index]) - Int(next.data[index]))
                        + abs(Int(first.data[index + 1]) - Int(next.data[index + 1]))
                        + abs(Int(first.data[index + 2]) - Int(next.data[index + 2]))
                    guard change > 90 else { continue }
                    box = box.map { (min($0.minX, x), min($0.minY, y), max($0.maxX, x), max($0.maxY, y)) } ?? (x, y, x, y)
                }
            }
            // A blink seen and a few more looks: it has drawn whole.
            if box != nil, looks >= 12 { break }
        }
        guard let box else { return nil }
        return CGRect(
            x: region.minX + CGFloat(box.minX) / first.scale, y: region.minY + CGFloat(box.minY) / first.scale,
            width: CGFloat(box.maxX - box.minX + 1) / first.scale, height: CGFloat(box.maxY - box.minY + 1) / first.scale)
    }
}
