import SwiftUI

/// The panes' own row under the navigation bar (ADR 0111): Text, Original
/// and Reply on iPhone, the right pane's choices on iPad. It is pinned: the
/// text scrolls under it. On iPhone it takes the content's width, its
/// segments equal; in a regular window it keeps its natural width, centred.
struct PanesRow: View {
    let panes: [DocumentViewModel.Shown]
    @Binding var selection: DocumentViewModel.Shown
    /// A regular window: the natural width, centred.
    let natural: Bool

    var body: some View {
        Picker("View", selection: $selection) {
            ForEach(panes, id: \.self) { shown in
                switch shown {
                case .pane(.text): Text("Text").tag(shown)
                case .pane(.original): Text("Original").tag(shown)
                case .pane(.reply): Text("Reply").tag(shown)
                case .identities: Text("Identities").tag(shown)
                }
            }
        }
        .pickerStyle(.segmented)
        .accessibilityIdentifier("pane-picker")
        // The system's segments keep their text size and shrink a label
        // that does not fit, so three fit a 320 pt window at every size.
        .fixedSize(horizontal: natural, vertical: false)
        .frame(maxWidth: .infinity)
        .padding(.horizontal, Space.m)
        .padding(.top, Space.xxs)
        .padding(.bottom, Space.xs)
    }
}

#Preview("Panes row") {
    @Previewable @State var phone = DocumentViewModel.Shown.pane(.text)
    @Previewable @State var pad = DocumentViewModel.Shown.pane(.original)
    PreviewMatrix {
        VStack(spacing: Space.xs) {
            PanesRow(panes: [.pane(.text), .pane(.original), .pane(.reply)], selection: $phone, natural: false)
            // iPad: the right column's choices at their natural width.
            PanesRow(panes: [.pane(.original), .identities, .pane(.reply)], selection: $pad, natural: true)
        }
    }
}
