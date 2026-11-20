import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// The password card at every text size. On an iPhone 17 Pro with the
/// keyboard up, the card has about 364 pt between the panes row (its bottom
/// near 150 pt) and the keyboard (its top near 530 pt), less the bar's
/// padding. There the whole card fits, its title whole, while the privacy
/// line scrolls at accessibility sizes.
@MainActor
final class PasswordCardLayoutTests: XCTestCase {
    private let width: CGFloat = 402
    private let room: CGFloat = 364

    private func height<V: View>(_ view: V, size: DynamicTypeSize) -> CGFloat {
        let host = UIHostingController(rootView: view.padding(.horizontal).environment(\.dynamicTypeSize, size))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: width, height: 874))
        window.rootViewController = host
        window.isHidden = false
        host.view.layoutIfNeeded()
        return host.sizeThatFits(in: CGSize(width: width, height: .greatestFiniteMagnitude)).height
    }

    func testTheCardFitsAboveTheKeyboardWithItsTitleWholeAtEveryTextSize() {
        for size in [DynamicTypeSize.large, .xxxLarge, .accessibility1, .accessibility5] {
            // The title as the card draws it: its text stops growing at
            // accessibility2.
            let title = height(
                Label("This document is password-protected", systemImage: "lock")
                    .font(.headline)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .dynamicTypeSize(...DynamicTypeSize.accessibility2),
                size: size)
            var heights: [CGFloat] = []
            for prompt in [DocumentViewModel.PasswordPrompt.needed, .wrong] {
                let card = height(PasswordCard(prompt: prompt) { _ in }, size: size)
                heights.append(card)
                XCTAssertLessThanOrEqual(card, room, "fits above the keyboard at \(size), \(prompt)")
                // The title sits whole over the words' room: the card holds
                // all of the title, the room, the field's row and the two
                // gaps between them. A title inside the scrolling room would
                // leave the card short by its own height.
                let words = size.isAccessibilitySize ? PasswordCard.wordsHeight : 0
                XCTAssertGreaterThanOrEqual(
                    card, title + words + Size.target + 24 - 0.5, "the title is whole at \(size), \(prompt)")
            }
            if size.isAccessibilitySize {
                // The wrong-password line shows in the words' room: the card
                // keeps one height, so nothing under the toolbar moves.
                XCTAssertEqual(heights[0], heights[1], accuracy: 0.5, "one height at \(size)")
            }
        }
    }
}
