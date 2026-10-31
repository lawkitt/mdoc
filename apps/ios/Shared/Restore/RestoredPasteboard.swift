import Foundation
import UIKit
import UniformTypeIdentifiers

/// Copies restored text, which holds originals, so it stays on this device
/// and leaves the clipboard after ten minutes: no Universal Clipboard hop to
/// the user's other devices, and nothing lingering (#321). Copy for AI holds
/// aliases only and is meant to leave the device, so it keeps a plain copy.
enum RestoredPasteboard {
    static let lifetime: TimeInterval = 10 * 60

    static func copy(_ text: String, to pasteboard: UIPasteboard = .general, now: Date = .now) {
        pasteboard.setItems([[UTType.utf8PlainText.identifier: text]], options: options(now: now))
    }

    static func options(now: Date) -> [UIPasteboard.OptionsKey: Any] {
        [.localOnly: true, .expirationDate: now.addingTimeInterval(lifetime)]
    }
}
