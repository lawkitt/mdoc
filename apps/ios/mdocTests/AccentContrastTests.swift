import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// The ink accent (ADR 0099, amended 2026-11-08) and the words on it meet
/// WCAG AA, 4.5:1, in light and dark mode, with and without Increase
/// Contrast.
final class AccentContrastTests: XCTestCase {
    private static let styles: [(String, UITraitCollection)] = [
        ("light", UITraitCollection(userInterfaceStyle: .light)),
        ("dark", UITraitCollection(userInterfaceStyle: .dark)),
        ("light, more contrast", UITraitCollection(traitsFrom: [
            UITraitCollection(userInterfaceStyle: .light), UITraitCollection(accessibilityContrast: .high),
        ])),
        ("dark, more contrast", UITraitCollection(traitsFrom: [
            UITraitCollection(userInterfaceStyle: .dark), UITraitCollection(accessibilityContrast: .high),
        ])),
    ]

    func testTheAccentIsInk() throws {
        let accent = try XCTUnwrap(UIColor(named: "AccentColor"), "the app's AccentColor")
        XCTAssertEqual(Self.hex(accent.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light))), "1c1c1e")
        XCTAssertEqual(Self.hex(accent.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark))), "f2f2f7")
        XCTAssertEqual(Self.hex(UIColor.ink.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark))), "f2f2f7")
    }

    func testWordsOnTheAccentMeetAA() throws {
        let accent = try XCTUnwrap(UIColor(named: "AccentColor"), "the app's AccentColor")
        for (name, traits) in Self.styles {
            let ratio = Self.contrast(accent.resolvedColor(with: traits), UIColor.onAccent.resolvedColor(with: traits))
            XCTAssertGreaterThanOrEqual(ratio, 4.5, "\(name): \(ratio)")
        }
    }

    /// iOS draws a swipe action's words white in both modes.
    func testWhiteOnTheSwipeAccentMeetsAA() {
        for (name, traits) in Self.styles {
            let fill = UIColor(Palette.swipeAccent).resolvedColor(with: traits)
            let ratio = Self.contrast(fill, .white)
            XCTAssertGreaterThanOrEqual(ratio, 4.5, "\(name): \(ratio)")
        }
    }

    // MARK: WCAG 2

    private static func components(_ colour: UIColor) -> (CGFloat, CGFloat, CGFloat) {
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        colour.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (red, green, blue)
    }

    private static func luminance(_ colour: UIColor) -> CGFloat {
        func linear(_ c: CGFloat) -> CGFloat { c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4) }
        let (red, green, blue) = components(colour)
        return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }

    static func contrast(_ a: UIColor, _ b: UIColor) -> CGFloat {
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    private static func hex(_ colour: UIColor) -> String {
        let (red, green, blue) = components(colour)
        return [red, green, blue].map { String(format: "%02x", Int(($0 * 255).rounded())) }.joined()
    }
}
