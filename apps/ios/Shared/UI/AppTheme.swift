import Foundation
import SwiftUI

/// Settings › Appearance › Theme (ADR 0099): Automatic follows the system.
/// Stored in `Preferences`, which the app and its share extension read.
enum AppTheme: String, CaseIterable, Identifiable, Sendable {
    case automatic, light, dark

    var id: String { rawValue }

    var title: LocalizedStringKey {
        switch self {
        case .automatic: "Automatic"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .automatic: nil
        case .light: .light
        case .dark: .dark
        }
    }

    /// The sidebar's theme button and its menu (#1070).
    var symbol: String {
        switch self {
        case .automatic: "circle.lefthalf.filled"
        case .light: "sun.max"
        case .dark: "moon"
        }
    }

    var interfaceStyle: UIUserInterfaceStyle {
        switch self {
        case .automatic: .unspecified
        case .light: .light
        case .dark: .dark
        }
    }
}
