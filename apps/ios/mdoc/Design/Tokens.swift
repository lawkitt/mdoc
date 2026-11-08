import SwiftUI

// The app's design tokens (ADR 0099, docs/mobile-ui.md). Screens use these
// and the components beside them, never literal spacing, radii or colors.

/// Spacing on a 4 pt grid.
enum Space {
    static let xxs: CGFloat = 4
    static let xs: CGFloat = 8
    static let s: CGFloat = 12
    static let m: CGFloat = 16
    static let l: CGFloat = 24
}

/// Corner radii, continuous.
enum Radius {
    /// Chips and pills: half their height.
    static let chip: CGFloat = 999
    /// Buttons and small cards.
    static let control: CGFloat = 12
    /// Cards, the review bar and grouped rows.
    static let card: CGFloat = 20
    /// Icon tiles.
    static let tile: CGFloat = 8
    /// The screen's corners while a drawer slides it aside.
    static let screen: CGFloat = 44
}

/// Sizes that recur.
enum Size {
    /// Minimum hit target (Apple's 44 pt).
    static let target: CGFloat = 44
    /// The review bar's Undo, Keep and Replace.
    static let barButton: CGFloat = 40
    /// A chip's height at default text size.
    static let chip: CGFloat = 30
    /// Settings and Start icon tiles.
    static let tile: CGFloat = 30
    /// A toast's countdown ring at default text size, and its line.
    static let toastRing: CGFloat = 26
    static let toastRingLine: CGFloat = 2.5
}

/// The review bar's measures (ADR 0098, 0101, docs/mobile-ui.md).
enum ReviewBarMetrics {
    /// The bar's widest: centred beyond it on iPad.
    static let maxWidth: CGFloat = 560
    /// A card dragged sideways further than this, or a third of its width
    /// if more, steps to its neighbor; at either end it bumps.
    static let swipeThreshold: CGFloat = 60
    /// The room each side of the card, inside the bar's padding, where a
    /// neighbor peeks in.
    static let cardInset: CGFloat = 14
    /// Between two cards in the strip.
    static let cardGap: CGFloat = Space.s
    /// The progress line's track.
    static let trackWidth: CGFloat = 56

    /// The most the mentions may take when pulled up: about 60% of the
    /// screen with the compact bar (about 190 pt) below, never under 160 pt,
    /// so the active name stays in view.
    static func detailsHeight(screen height: CGFloat) -> CGFloat {
        max(160, height * 0.6 - 190)
    }
}

/// Named colors. System colors adapt to dark mode and Increase Contrast.
enum Palette {
    /// The primary action and the active mention's outline: ink, near-black
    /// in light mode and near-white in dark mode (ADR 0099).
    static let accent = Color.ink
    /// Words and symbols on an accent fill: white in light mode, black in
    /// dark mode.
    static let onAccent = Color.onAccent
    /// A swipe action's accent fill. iOS draws a swipe action's words white
    /// in both modes, so in dark mode the ink stays a dark grey.
    static let swipeAccent = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? .systemGray3.resolvedColor(with: traits) : .ink.resolvedColor(with: traits)
    })
    static let surface = Color(uiColor: .secondarySystemGroupedBackground)
    static let background = Color(uiColor: .systemGroupedBackground)
    static let chipFill = Color(uiColor: .tertiarySystemFill)
    static let separator = Color(uiColor: .separator)
    static let secondaryText = Color.secondary

    /// A document's state on the Documents page and in the review bar.
    static let toReview = Color.orange
    static let ready = Color.green
    static let restored = Color.blue
}

/// Type styles, all Dynamic Type text styles.
enum Typography {
    static let title = Font.headline
    static let body = Font.body
    static let caption = Font.footnote
    static let chip = Font.subheadline.weight(.medium)
    static let alias = Font.subheadline.monospaced().weight(.medium)
    static let progress = Font.caption.monospacedDigit()
}

/// Motion: spring for movement, cross-fade under Reduce Motion.
enum Motion {
    static let snappy = Animation.snappy(duration: 0.25)
    static let fade = Animation.easeInOut(duration: 0.2)

    /// The animation to use, given the Reduce Motion setting.
    static func move(reduced: Bool) -> Animation { reduced ? fade : snappy }
}
