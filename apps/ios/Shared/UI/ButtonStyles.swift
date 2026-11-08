import SwiftUI

// The app's buttons (ADR 0099, amended 2026-11-08; docs/mobile-ui.md): big
// buttons are Liquid Glass, 44 pt high. The primary, at most one per view,
// is prominent glass tinted ink; every other big button is clear glass with
// ink words. Screens use these two and `textAction()`, never a literal
// material or colour. The system's glass adapts to dark mode, Increase
// Contrast and Reduce Transparency on its own.

extension View {
    /// The one primary button of a view (Replace, Share, Find names, Scan):
    /// ink-tinted prominent glass with the words that read on ink, white in
    /// light mode and black in dark mode. Disabled, its words are the
    /// secondary colour.
    func primaryAction() -> some View {
        buttonStyle(BigButtonStyle(prominent: true, circle: false))
    }

    /// Every other big button (Keep, Undo, Copy, Cancel, Identities'
    /// Replace N): clear glass with ink words, the secondary colour when
    /// disabled. `circle` makes an icon-only button a 44 pt circle (Undo).
    func secondaryAction(circle: Bool = false) -> some View {
        buttonStyle(BigButtonStyle(prominent: false, circle: circle))
    }

    /// `primaryAction()` and `secondaryAction()` for a `Menu`, whose label
    /// takes only the system's button styles, not a custom one: the system's
    /// glass here, with `bigButtonLabel(prominent:)` on the label.
    func primaryMenu() -> some View {
        buttonStyle(.glassProminent)
            .tint(Color.ink)
    }

    func secondaryMenu() -> some View {
        buttonStyle(.glass)
    }

    /// A big button's label: tall enough for 44 pt, in the words' colour.
    /// The button styles add it themselves; a `Menu`'s label needs it.
    func bigButtonLabel(prominent: Bool) -> some View {
        frame(minHeight: BigButton.labelHeight)
            .modifier(BigButtonWords(prominent: prominent))
    }

    /// A text-only button ("Select", "Not now", "Try a sample"): the primary
    /// text colour at medium weight, so it stands out without a colour
    /// (ADR 0099).
    func textAction() -> some View {
        tint(.primary)
            .fontWeight(.medium)
    }
}

/// A big button's measures.
enum BigButton {
    /// Every big button's height at the default text size, the review
    /// bar's included; it grows with the text, never below (`Size.barButton`).
    static let height: CGFloat = 44
    /// What the system's glass button adds above and below its label at the
    /// regular control size; `ComponentSnapshotTests` checks the sum is 44.
    static let systemPadding: CGFloat = 7
    /// What it adds each side of an icon in a circle.
    static let systemCirclePadding: CGFloat = 12
    /// The label's least height, so the button is `height` high.
    static var labelHeight: CGFloat { height - 2 * systemPadding }
    /// An icon's width in a circle `height` across.
    static var circleLabelWidth: CGFloat { height - 2 * systemCirclePadding }
}

/// `primaryAction()` and `secondaryAction()`: the system's glass button
/// styles around a label at least `BigButton.labelHeight` high. A disabled
/// primary is clear glass: the system's greyed ink glass is near-white in
/// dark mode, where the secondary words vanish on it.
private struct BigButtonStyle: PrimitiveButtonStyle {
    let prominent: Bool
    let circle: Bool
    @Environment(\.isEnabled) private var isEnabled
    /// A circle's icon width, growing with the text as its height does, so
    /// the circle stays round at every size.
    @ScaledMetric private var circleWidth = BigButton.circleLabelWidth

    @ViewBuilder
    func makeBody(configuration: Configuration) -> some View {
        let button = Button(role: configuration.role, action: configuration.trigger) {
            configuration.label
                .frame(
                    width: circle ? circleWidth : nil,
                    height: circle ? circleWidth + 2 * (BigButton.systemCirclePadding - BigButton.systemPadding) : nil)
                .bigButtonLabel(prominent: prominent)
        }
        if prominent && isEnabled {
            button
                .buttonStyle(.glassProminent)
                .tint(Color.ink)
        } else if circle {
            button
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
        } else {
            button.buttonStyle(.glass)
        }
    }
}

/// The words: on-accent and semibold (primary) or ink (secondary) while
/// the button can be tapped, the secondary colour when it can't. The
/// system's prominent button keeps white words whatever its tint, and they
/// vanish on the near-white ink of dark mode. A role's colour (destructive
/// red) is not kept: no big button has one.
private struct BigButtonWords: ViewModifier {
    let prominent: Bool
    @Environment(\.isEnabled) private var isEnabled

    func body(content: Content) -> some View {
        content
            .fontWeight(prominent ? .semibold : nil)
            .foregroundStyle(isEnabled ? (prominent ? Color.onAccent : Color.ink) : Color.secondary)
    }
}
