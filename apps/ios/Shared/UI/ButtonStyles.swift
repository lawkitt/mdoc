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
    /// The label's least height, so the button is `height` high.
    static var labelHeight: CGFloat { height - 2 * systemPadding }
}

/// `primaryAction()` and `secondaryAction()`: the system's glass button
/// styles around a label at least `BigButton.labelHeight` high.
private struct BigButtonStyle: PrimitiveButtonStyle {
    let prominent: Bool
    let circle: Bool

    @ViewBuilder
    func makeBody(configuration: Configuration) -> some View {
        let button = Button(role: configuration.role, action: configuration.trigger) {
            configuration.label
                .frame(
                    minWidth: circle ? BigButton.labelHeight : nil,
                    minHeight: BigButton.labelHeight)
                .modifier(BigButtonWords(prominent: prominent))
        }
        if prominent {
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

/// The words: on-accent (primary) or ink (secondary) while the button can
/// be tapped, the secondary colour when it can't. The system's prominent
/// button keeps white words whatever its tint, and they vanish on the
/// near-white ink of dark mode.
private struct BigButtonWords: ViewModifier {
    let prominent: Bool
    @Environment(\.isEnabled) private var isEnabled

    func body(content: Content) -> some View {
        content.foregroundStyle(isEnabled ? (prominent ? Color.onAccent : Color.ink) : Color.secondary)
    }
}
