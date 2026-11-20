import SwiftUI

/// A message about the document, inset at the top of its text: the partial
/// import, a job that stopped when mdoc left the screen. A quiet fill with
/// the orange only in the icon, never an orange fill; an informational card
/// closes with ×, and one whose button is the only way on has none.
struct NoticeCard<Action: View>: View {
    let message: Text
    var symbol = "exclamationmark.triangle"
    /// Closes the card; nil when it can't be closed.
    var onClose: (() -> Void)?
    @ViewBuilder var action: () -> Action

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
            Image(systemName: symbol)
                .foregroundStyle(Palette.warning)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Space.xs) {
                message
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
                action()
            }
            if let onClose {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.secondary)
                        // A 44 pt target that doesn't make the card taller.
                        .frame(width: Size.target, height: Size.target)
                        .contentShape(Rectangle())
                        .padding(.vertical, -Space.s)
                        .padding(.trailing, -Space.s)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Close"))
                .accessibilityIdentifier("notice-close")
            }
        }
        .font(.footnote)
        // Grows with the text, no further than a bar does.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .padding(Space.s)
        .background(Palette.noticeFill, in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
        .accessibilityElement(children: .contain)
    }
}

extension NoticeCard where Action == EmptyView {
    init(message: Text, symbol: String = "exclamationmark.triangle", onClose: (() -> Void)? = nil) {
        self.init(message: message, symbol: symbol, onClose: onClose) { EmptyView() }
    }
}

#Preview("Notice cards") {
    PreviewMatrix {
        VStack(spacing: Space.s) {
            NoticeCard(
                message: Text(verbatim: "Partial import: pages 2, 5 of 5 require OCR and were skipped."),
                onClose: {})
            NoticeCard(message: Text("Recognition stopped when mdoc left the screen. Finished pages are kept.")) {
                Button("Resume") {}
                    .secondaryAction()
            }
        }
    }
}
