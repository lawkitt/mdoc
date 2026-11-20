import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// The password card at every text size: at the largest accessibility size
/// its words scroll in a bounded area, so the whole card fits in the half
/// of an iPhone screen the keyboard leaves, with the field and Unlock on it.
@MainActor
final class PasswordCardLayoutTests: XCTestCase {
    private let screen = CGSize(width: 402, height: 874)

    private func height(_ prompt: DocumentViewModel.PasswordPrompt, size: DynamicTypeSize) -> CGFloat {
        let card = PasswordCard(prompt: prompt, maxWordsHeight: screen.height * 0.18) { _ in }
            .padding(.horizontal)
            .environment(\.dynamicTypeSize, size)
        let host = UIHostingController(rootView: card)
        let window = UIWindow(frame: CGRect(origin: .zero, size: screen))
        window.rootViewController = host
        window.isHidden = false
        host.view.layoutIfNeeded()
        return host.sizeThatFits(in: CGSize(width: screen.width, height: .greatestFiniteMagnitude)).height
    }

    func testTheCardFitsAboveTheKeyboardAtEveryTextSize() {
        for size in [DynamicTypeSize.large, .xxxLarge, .accessibility1, .accessibility5] {
            for prompt in [DocumentViewModel.PasswordPrompt.needed, .wrong] {
                let height = height(prompt, size: size)
                XCTAssertGreaterThan(height, 100, "laid out at \(size), \(prompt)")
                XCTAssertLessThanOrEqual(height, screen.height / 2, "fits above the keyboard at \(size), \(prompt)")
            }
        }
    }
}
