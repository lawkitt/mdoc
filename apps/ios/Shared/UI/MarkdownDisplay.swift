import Foundation
import SwiftUI

/// How Markdown text is shown: rendered as desktop draws it, or as source
/// with dimmed markers while Show Markdown is on (ADR 0096), in the system
/// font or New York (ADR 0099). One setting for the app and its share
/// extension, so it lives in the App Group.
enum MarkdownDisplay {
    static let showsSourceKey = "showsMarkdownSource"
    /// Settings › Appearance › Document font: Serif.
    static let serifKey = "documentFontSerif"
    static var store: UserDefaults { UserDefaults(suiteName: MdocStorage.appGroup) ?? .standard }

    static var serif: Bool { store.bool(forKey: serifKey) }
}

/// Settings › Appearance › Theme (ADR 0099): Automatic follows the system.
enum AppTheme: String, CaseIterable, Identifiable, Sendable {
    case automatic, light, dark

    static let key = "theme"

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

    var interfaceStyle: UIUserInterfaceStyle {
        switch self {
        case .automatic: .unspecified
        case .light: .light
        case .dark: .dark
        }
    }

    static var current: AppTheme {
        AppTheme(rawValue: MarkdownDisplay.store.string(forKey: key) ?? "") ?? .automatic
    }
}
