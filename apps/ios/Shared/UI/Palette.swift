import SwiftUI
import UIKit

// System colours adapt to dark mode and Increase Contrast on their own.

extension Category {
    var uiColor: UIColor {
        switch self {
        case .person: .systemBlue
        case .organization: .systemPurple
        case .email: .systemTeal
        case .url: .systemMint
        case .phone: .systemGreen
        case .address: .systemOrange
        case .date: .systemBrown
        case .identity: .systemIndigo
        case .tax: .systemPink
        case .bank: .systemRed
        case .caseNumber: .systemYellow
        case .other: .systemGray
        }
    }

    var color: Color { Color(uiColor: uiColor) }

    var symbol: String {
        switch self {
        case .person: "person"
        case .organization: "building.2"
        case .email: "envelope"
        case .url: "link"
        case .phone: "phone"
        case .address: "mappin.and.ellipse"
        case .date: "calendar"
        case .identity: "person.text.rectangle"
        case .tax: "number"
        case .bank: "banknote"
        case .caseNumber: "number.square"
        case .other: "eye.slash"
        }
    }
}

extension CaseColour {
    var color: Color {
        switch self {
        case .red: .red
        case .orange: .orange
        case .amber: .yellow
        case .green: .green
        case .teal: .teal
        case .blue: .blue
        case .violet: .purple
        case .pink: .pink
        }
    }
}

/// The case's colour dot; it marks the case, never a state (ADR 0077).
struct CaseDot: View {
    let colour: CaseColour

    var body: some View {
        Circle()
            .fill(colour.color)
            .frame(width: 10, height: 10)
            .accessibilityHidden(true)
    }
}

// The accent is ink (ADR 0099, amended 2026-11-08): near-black `#1c1c1e` in
// light mode and near-white `#f2f2f7` in dark mode, the app's `AccentColor`.
// What sits on it flips with it.

extension UIColor {
    /// The accent: the app's `AccentColor`, or the label colour where there
    /// is no asset catalog (the Restore extension).
    static let ink = UIColor(named: "AccentColor") ?? .label

    /// Words and symbols on an accent fill: white on the near-black of light
    /// mode, black on the near-white of dark mode.
    static let onAccent = UIColor { $0.userInterfaceStyle == .dark ? .black : .white }
}

extension Color {
    /// The accent (`UIColor.ink`), named rather than the environment's
    /// tint, so offscreen renders (snapshots) draw it too.
    static let ink = Color(uiColor: .ink)
    /// Words and symbols on an accent fill (`UIColor.onAccent`).
    static let onAccent = Color(uiColor: .onAccent)
}

extension View {
    /// The filled accent button (Replace N, Share, the review bar's
    /// Replace): the accent's fill with the words that read on it, white in
    /// light mode and black in dark mode. The system's filled button keeps
    /// white words whatever its fill, and they vanish on near-white. A
    /// disabled one has grey words on the greyed fill.
    func accentProminent() -> some View {
        modifier(AccentProminent())
    }

    /// A text-only button ("Select", "Not now", "Try a sample"): the primary
    /// text colour at medium weight, so it stands out without a colour
    /// (ADR 0099).
    func textAction() -> some View {
        tint(.primary)
            .fontWeight(.medium)
    }
}

/// `accentProminent()`: the on-accent words only while the button can be
/// tapped; a disabled one has the secondary colour on the greyed fill.
private struct AccentProminent: ViewModifier {
    @Environment(\.isEnabled) private var isEnabled

    func body(content: Content) -> some View {
        content
            .buttonStyle(.borderedProminent)
            .tint(Color.ink)
            .foregroundStyle(isEnabled ? Color.onAccent : Color.secondary)
    }
}
