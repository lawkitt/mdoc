import SwiftUI

extension View {
    /// The Original's Dark page (#1036): the page drawn light, then
    /// inverted and turned half the colour wheel, so white paper goes
    /// near-black and black text light while a yellow highlight stays
    /// yellowish. PDFKit has no dark mode of its own. Apply it to the page
    /// alone, never to controls over it. Off, the filters are identities,
    /// so switching keeps the page view, its zoom and its place.
    func darkPage(_ isOn: Bool) -> some View {
        modifier(DarkPageEffect(isOn: isOn))
    }
}

private struct DarkPageEffect: ViewModifier {
    let isOn: Bool
    @Environment(\.colorScheme) private var colorScheme

    func body(content: Content) -> some View {
        content
            // Light under the inversion: its margins and cards come out dark.
            .environment(\.colorScheme, isOn ? .light : colorScheme)
            .contrast(isOn ? -1 : 1)
            .hueRotation(.degrees(isOn ? 180 : 0))
    }
}

/// Whether the Original's page shows dark: the preference, only while the
/// app is dark.
struct DarkPageState {
    let preference: Bool
    let colorScheme: ColorScheme

    /// The title menu offers the toggle: the app is dark.
    var offered: Bool { colorScheme == .dark }
    var isOn: Bool { offered && preference }
}

/// The one-time hint over the Original while the app is dark: a capsule
/// that turns Dark page on, or × for good (ADR 0114: a quiet line, no
/// popover). Settings › Review › Reset tips brings it back.
struct DarkPageHintCapsule: View {
    let onTurnOn: () -> Void
    let onClose: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onTurnOn) {
                Label {
                    Text(ReviewHint.darkPage.text)
                } icon: {
                    Image(systemName: ReviewHint.darkPage.symbol)
                }
                .font(.subheadline.weight(.semibold))
                .padding(.leading, Space.m)
                .padding(.trailing, Space.xs)
                .frame(minHeight: Size.target)
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .accessibilityHint(Text("Turns the page dark. It's also in the title menu."))
            .accessibilityIdentifier("dark-page-hint")
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: Size.target, height: Size.target)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Close"))
            .accessibilityIdentifier("dark-page-hint-close")
        }
        .foregroundStyle(.primary)
        .glassEffect(.regular.interactive(), in: .capsule)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .transition(reduceMotion ? .opacity : .opacity.combined(with: .move(edge: .bottom)))
    }
}
