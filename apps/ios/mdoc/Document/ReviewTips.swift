import SwiftUI

/// The review's hints (docs/mobile-ui.md, Tips): one quiet line inside the
/// bar, at most one per document, gone once the gesture is used. No
/// popovers and no tip cards. Settings › Review › Reset tips shows them
/// again; UI tests turn them off.
enum ReviewHint: String, CaseIterable, Identifiable {
    /// After the second card: the card swipes.
    case swipe
    /// After a few decisions: a name's long-press menu.
    case longPress

    var id: String { rawValue }

    var text: LocalizedStringKey {
        switch self {
        case .swipe: "Swipe the card for the next name"
        case .longPress: "Long-press any name for more"
        }
    }

    var symbol: String {
        switch self {
        case .swipe: "hand.draw"
        case .longPress: "hand.tap"
        }
    }

    private var learnedKey: String { "hint.\(rawValue).learned" }

    /// Used once, or dismissed: never shown again.
    var isLearned: Bool { UserDefaults.standard.bool(forKey: learnedKey) }

    func learn() { UserDefaults.standard.set(true, forKey: learnedKey) }

    /// The grabber's one bounce that says the bar pulls up.
    static let grabberBouncedKey = "hint.grabber.bounced"
    /// The card's one nudge that says it swipes.
    static let nudgedKey = "reviewNudged"

    /// Off in UI tests, whose layout a hint would shift.
    nonisolated(unsafe) static var enabled = true

    static func configure(testing: Bool) {
        enabled = !testing
    }

    /// Settings › Review › Reset tips: every hint shows again, at once.
    static func resetAll() {
        for hint in allCases { UserDefaults.standard.removeObject(forKey: hint.learnedKey) }
        UserDefaults.standard.removeObject(forKey: grabberBouncedKey)
        UserDefaults.standard.removeObject(forKey: nudgedKey)
    }
}

/// The hint line under the progress line: a symbol, the words, and × to
/// put it away for good.
struct ReviewHintLine: View {
    let hint: ReviewHint
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: Space.xs) {
            Image(systemName: hint.symbol)
                .foregroundStyle(Palette.accent)
                .accessibilityHidden(true)
            Text(hint.text)
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            Spacer(minLength: 0)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.caption2.weight(.semibold))
                    // A concrete grey: the button's tint would take over.
                    .foregroundStyle(Color(uiColor: .tertiaryLabel))
                    .frame(width: Size.target, height: 24)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel(Text("Hide tip"))
        }
        .padding(.leading, Space.xxs)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("review-hint-line")
    }
}
