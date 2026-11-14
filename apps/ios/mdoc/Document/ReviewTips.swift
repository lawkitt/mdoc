import SwiftUI
import TipKit

/// The hints (docs/mobile-ui.md, Tips; ADR 0114): TipKit decides whether a
/// hint may show (learned, reset, the UI tests' switch) and mdoc draws it.
/// No `TipView` and no popovers: the card swipe and the long press are one
/// quiet line inside the review bar, at most one per document; drag-merge
/// is Identities' Select demo, and Adjust Words has its own demo in the
/// text (ADR 0115). Doing the action, or ×, learns a hint for
/// good; Settings › Review › Reset tips shows them all again.
enum ReviewHint: String, CaseIterable, Identifiable, Sendable {
    /// After the second card: the card swipes.
    case swipe
    /// After a few decisions: a name's long-press menu.
    case longPress
    /// The first time Select opens with two names or more: the Select demo,
    /// a ghost row dragged onto the next (ADR 0114). Learned by the first
    /// drag-merge or Same as, or by dismissing the demo.
    case dragMerge
    /// The first time a name's words are adjusted: a ghost handle glides
    /// along the words under a caption (ADR 0115). Learned by moving a
    /// handle, Done, Cancel or a tap on the caption.
    case adjustWords

    var id: String { rawValue }

    var text: LocalizedStringResource {
        switch self {
        case .swipe: "Swipe the card for the next name"
        case .longPress: "Long-press any name for more"
        case .dragMerge: "Tick names to act on them, or drag ≡ onto another name to make them one"
        case .adjustWords: "Drag the handles onto the name's words"
        }
    }

    var symbol: String {
        switch self {
        case .swipe: "hand.draw"
        case .longPress: "hand.tap"
        case .dragMerge: "hand.point.up.left"
        case .adjustWords: "hand.point.up.left.and.text"
        }
    }

    /// The TipKit tip that decides whether this hint may show.
    var tip: any Tip {
        switch self {
        case .swipe: SwipeTip()
        case .longPress: LongPressTip()
        case .dragMerge: DragMergeTip()
        case .adjustWords: AdjustWordsTip()
        }
    }

    /// rc.9 and earlier kept a learned hint in `Preferences`; newer hints
    /// never were.
    fileprivate var oldKey: Preferences.Key? {
        switch self {
        case .swipe: .swipeLearned
        case .longPress: .longPressLearned
        case .dragMerge: .dragMergeLearned
        case .adjustWords: nil
        }
    }

    /// Used once, or dismissed: never shown again, until Reset tips.
    @MainActor var isLearned: Bool { !Hints.shared.available.contains(self) }

    @MainActor func learn() { Hints.shared.learn(self) }

    /// The hints still to teach: those TipKit lets show. While the Select
    /// demo plays, only it, so two hints never show at once on iPad.
    @MainActor static var teachable: Set<ReviewHint> { Hints.shared.teachable }

    /// Off in UI tests (`Tips.hideAllTipsForTesting()`), whose layout a hint
    /// or a one-time motion would shift.
    @MainActor static var enabled: Bool { Hints.shared.mode == .live }

    /// Settings › Review › Reset tips: every hint shows again.
    @MainActor static func resetAll() { Hints.shared.resetAll() }
}

// The tips themselves: TipKit's record of each, with no rules of their own.
// When within a document is mdoc's (`ReviewState`, the Select demo).

struct SwipeTip: Tip {
    var title: Text { Text(ReviewHint.swipe.text) }
    var image: Image? { Image(systemName: ReviewHint.swipe.symbol) }
}

struct LongPressTip: Tip {
    var title: Text { Text(ReviewHint.longPress.text) }
    var image: Image? { Image(systemName: ReviewHint.longPress.symbol) }
}

struct DragMergeTip: Tip {
    var title: Text { Text(ReviewHint.dragMerge.text) }
    var image: Image? { Image(systemName: ReviewHint.dragMerge.symbol) }
}

struct AdjustWordsTip: Tip {
    var title: Text { Text(ReviewHint.adjustWords.text) }
    var image: Image? { Image(systemName: ReviewHint.adjustWords.symbol) }
}

/// What TipKit says about the hints, as mdoc's views read it: the hints
/// whose tip may show (`statusUpdates`), kept here so a view hears of a
/// change and a learned hint goes at once.
@MainActor @Observable
final class Hints {
    static let shared = Hints()

    enum Mode: Equatable {
        /// The app: TipKit with its own store.
        case live
        /// UI tests: TipKit with every tip hidden, or every tip shown for a
        /// test of a hint (`MDOC_SHOW_TIPS`).
        case uiTesting(showsTips: Bool)
        /// Unit tests: no TipKit; tests say what is available.
        case unitTesting
    }

    private(set) var mode: Mode = .unitTesting
    /// The hints whose tip TipKit lets show now.
    private(set) var available: Set<ReviewHint> = []
    /// The hint on screen that keeps the others back: the Select demo.
    private(set) var showing: ReviewHint?
    @ObservationIgnored private var watchers: [Task<Void, Never>] = []

    var teachable: Set<ReviewHint> {
        if let showing { return available.intersection([showing]) }
        return available
    }

    /// At launch: TipKit, with each tip watched. Reset tips and the UI
    /// tests' switch go through TipKit too.
    func start(_ mode: Mode) {
        self.mode = mode
        guard mode != .unitTesting, watchers.isEmpty else { return }
        if case .uiTesting(let showsTips) = mode {
            if showsTips { Tips.showAllTipsForTesting() } else { Tips.hideAllTipsForTesting() }
        }
        do {
            // One at a time is mdoc's: one line per document, the demo only
            // in Select. TipKit's frequency would hold a tip back for a day
            // once the one before it was eligible, not shown.
            try Tips.configure([.displayFrequency(.immediate)])
        } catch {
            return
        }
        moveLearnedHints()
        for hint in ReviewHint.allCases {
            watchers.append(Task { [weak self] in
                for await status in hint.tip.statusUpdates {
                    self?.update(hint, available: status == .available)
                }
            })
        }
    }

    private func update(_ hint: ReviewHint, available shows: Bool) {
        if shows { available.insert(hint) } else { available.remove(hint) }
    }

    /// The action was done (`closed`: the hint was dismissed): never again.
    func learn(_ hint: ReviewHint, closed: Bool = false) {
        available.remove(hint)
        if showing == hint { showing = nil }
        guard mode != .unitTesting else { return }
        hint.tip.invalidate(reason: closed ? .tipClosed : .actionPerformed)
    }

    /// The Select demo shows (`hint`) or went (nil).
    func show(_ hint: ReviewHint?) {
        showing = hint
    }

    /// Settings › Review › Reset tips. `Tips.resetDatastore()` works only
    /// before `Tips.configure`, so each tip's eligibility is reset instead
    /// (iOS 26), and the hints come back through `statusUpdates`.
    func resetAll() {
        Preferences.shared.resetTips()
        showing = nil
        switch mode {
        case .live:
            Task {
                for hint in ReviewHint.allCases { await hint.tip.resetEligibility() }
            }
        case .uiTesting:
            break
        case .unitTesting:
            available = Set(ReviewHint.allCases)
        }
    }

    /// Unit tests: these hints may show, and none is on screen.
    func setAvailableForTesting(_ hints: Set<ReviewHint>) {
        precondition(mode == .unitTesting)
        available = hints
        showing = nil
    }

    /// A hint learned before TipKit kept them stays learned.
    private func moveLearnedHints() {
        for hint in ReviewHint.allCases {
            guard let key = hint.oldKey, Preferences.shared.isLearned(key) else { continue }
            hint.tip.invalidate(reason: .actionPerformed)
            Preferences.shared.forget(key)
        }
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
