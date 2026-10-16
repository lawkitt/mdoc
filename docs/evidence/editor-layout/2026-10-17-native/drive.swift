// Usage: swiftc -O drive.swift -o drive; ./drive click|scroll|type|search <x> <y>
// Posts native HID events at a screen point (in points): a click, 4 × 360
// pixel-wheel events at ~120 Hz, typing at 10 keys/s, or a search (Cmd+F, a
// query, Return × 30 at 5/s). Q="17,4,14" sets the query's key codes ("the").
// Needs Accessibility permission for the terminal; keep the desktop idle.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
let mode = args[1]
let point = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
let source = CGEventSource(stateID: .hidSystemState)
func tick() { usleep(8_333) }

CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
usleep(200_000)
if mode == "click" {
    for type in [CGEventType.leftMouseDown, .leftMouseUp] {
        CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
    }
} else if mode == "scroll" {
    // Down through the document and back, pixel deltas like a trackpad.
    for pass in 0..<4 {
        let delta: Int32 = pass % 2 == 0 ? -40 : 40
        for _ in 0..<360 {
            let e = CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0)!
            e.location = point
            e.post(tap: .cghidEventTap)
            tick()
        }
    }
} else if mode == "search" {
    // Cmd+F, type "company", then Return 30 times at 5/s, then Escape.
    func key(_ code: CGKeyCode, _ flags: CGEventFlags = []) {
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)!
            e.flags = flags
            e.post(tap: .cghidEventTap)
        }
    }
    key(3, .maskCommand); usleep(300_000)
    let q = (ProcessInfo.processInfo.environment["Q"] ?? "8,31,46,35,0,45,16").split(separator: ",").map { CGKeyCode($0)! }
    for code in q { key(code); usleep(150_000) }
    usleep(500_000)
    for _ in 0..<30 { key(36); usleep(200_000) }
    key(53)
} else if mode == "type" {
    // 'x' (keycode 7) 60 times at ~10 keys/s, then backspace (51) the same.
    for code: CGKeyCode in [7, 51] {
        for _ in 0..<60 {
            for down in [true, false] {
                CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)!.post(tap: .cghidEventTap)
            }
            usleep(100_000)
        }
    }
}
