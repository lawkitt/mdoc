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
}

/// Sizes that recur.
enum Size {
    /// Minimum hit target (Apple's 44 pt).
    static let target: CGFloat = 44
    /// The review bar's Undo, Keep and Replace.
    static let barButton: CGFloat = 40
    /// A chip's height at default text size.
    static let chip: CGFloat = 30
    /// Settings and Home icon tiles.
    static let tile: CGFloat = 30
}

/// Named colors. System colors adapt to dark mode and Increase Contrast.
enum Palette {
    /// The primary action and the active mention's outline.
    static let accent = Color.accentColor
    static let surface = Color(uiColor: .secondarySystemGroupedBackground)
    static let background = Color(uiColor: .systemGroupedBackground)
    static let chipFill = Color(uiColor: .tertiarySystemFill)
    static let separator = Color(uiColor: .separator)
    static let secondaryText = Color.secondary

    /// A document's state on Home and in the review bar.
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
