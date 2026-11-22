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

/// Whether the Original's page shows dark: the preference (on by default,
/// #1070), only while the app is dark.
struct DarkPageState {
    let preference: Bool
    let colorScheme: ColorScheme

    /// The moon over the Original offers it: the app is dark.
    var offered: Bool { colorScheme == .dark }
    var isOn: Bool { offered && preference }
}

/// The one-time hint over the Original while its page shows dark (#1070):
/// a quiet capsule saying the moon at the top switches it (ADR 0114: no
/// popover). A tap anywhere on it closes it for good; Settings › Review ›
/// Reset tips brings it back.
struct DarkPageHintCapsule: View {
    let onClose: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Button(action: onClose) {
            HStack(spacing: Space.xs) {
                Text("Dark page · \(Image(systemName: "moon")) above switches it")
                    .font(.subheadline.weight(.semibold))
                Image(systemName: "xmark")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .padding(.leading, Space.m)
            .padding(.trailing, Space.s)
            .frame(minHeight: Size.target)
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
        .glassEffect(.regular.interactive(), in: .capsule)
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .accessibilityLabel(Text("Dark page is on. The moon at the top switches it."))
        .accessibilityHint(Text("Closes this tip."))
        .accessibilityIdentifier("dark-page-hint")
        .transition(reduceMotion ? .opacity : .opacity.combined(with: .move(edge: .bottom)))
    }
}
