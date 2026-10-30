import SwiftUI
import UIKit

// System colours adapt to dark mode and Increase Contrast on their own.

extension Category {
    var uiColor: UIColor {
        switch self {
        case .person: .systemBlue
        case .organization: .systemPurple
        case .email: .systemTeal
        case .phone: .systemGreen
        case .address: .systemOrange
        case .date: .systemBrown
        case .identity: .systemIndigo
        case .tax: .systemPink
        case .bank: .systemRed
        case .other: .systemGray
        }
    }

    var color: Color { Color(uiColor: uiColor) }

    var symbol: String {
        switch self {
        case .person: "person"
        case .organization: "building.2"
        case .email: "envelope"
        case .phone: "phone"
        case .address: "mappin.and.ellipse"
        case .date: "calendar"
        case .identity: "person.text.rectangle"
        case .tax: "number"
        case .bank: "banknote"
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
