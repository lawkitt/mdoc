import SwiftUI

/// The review's hints (docs/mobile-ui.md, Tips): one quiet line inside the
/// bar, at most one per document, gone once the gesture is used, and one in
/// Identities (`dragMerge`). No popovers and no tip cards. Settings ›
/// Review › Reset tips shows them again; UI tests turn them off.
enum ReviewHint: String, CaseIterable, Identifiable {
    /// After the second card: the card swipes.
    case swipe
    /// After a few decisions: a name's long-press menu.
    case longPress
    /// In Identities, while two names could be one: drag-to-merge (ADR
    /// 0106). Learned by the first drag-merge or Same as.
    case dragMerge

    var id: String { rawValue }

    var text: LocalizedStringKey {
        switch self {
        case .swipe: "Swipe the card for the next name"
        case .longPress: "Long-press any name for more"
        case .dragMerge: "Drag a name onto another to make them one"
        }
    }

    var symbol: String {
        switch self {
        case .swipe: "hand.draw"
        case .longPress: "hand.tap"
        case .dragMerge: "hand.point.up.left"
        }
    }

    private var learnedKey: Preferences.Key {
        switch self {
        case .swipe: .swipeLearned
        case .longPress: .longPressLearned
        case .dragMerge: .dragMergeLearned
        }
    }

    /// Used once, or dismissed: never shown again (`Preferences`).
    var isLearned: Bool { Preferences.shared.isLearned(learnedKey) }

    func learn() { Preferences.shared.learn(learnedKey) }

    /// Off in UI tests, whose layout a hint would shift.
    nonisolated(unsafe) static var enabled = true

    /// The hints still to teach: none while hints are off, else those not
    /// yet learned. `ReviewState` shows one of them at its moment.
    static var teachable: Set<ReviewHint> {
        enabled ? Set(allCases.filter { !$0.isLearned }) : []
    }

    static func configure(testing: Bool) {
        enabled = !testing
    }

    /// Settings › Review › Reset tips: every hint shows again, at once.
    static func resetAll() {
        Preferences.shared.resetTips()
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
                    .frame(width: Size.target, height: Size.target)
                    .contentShape(Rectangle())
            }
            // A full 44-point target that doesn't make the line taller.
            .padding(.vertical, -10)
            .accessibilityLabel(Text("Hide tip"))
        }
        .padding(.leading, Space.xxs)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("review-hint-line")
    }
}
