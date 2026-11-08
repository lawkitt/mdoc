import SwiftUI
import UIKit

/// Paste reply: the AI's answer, restored with this document's case.
struct ReplySheet: View {
    let caseName: String?
    let onRestore: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var reply = ""

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 12) {
                Text(caseName.map { String(localized: "mdoc puts back the names from the case “\($0)”.") }
                    ?? String(localized: "mdoc puts back the names from this document's case."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    // Its full height up to six lines (a case is named after a
                    // title, which can be long), so the editor keeps its room.
                    .lineLimit(6)
                    .fixedSize(horizontal: false, vertical: true)
                PasteButton(supportedContentTypes: [.html, .plainText]) { providers in
                    Task { @MainActor in
                        if let text = await PastedReply.text(from: providers) {
                            reply = text
                        }
                    }
                }
                .labelStyle(.titleAndIcon)
                ReplyEditor(text: $reply)
                    .frame(minHeight: 120)
                    .padding(8)
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: 8))
                    .overlay(alignment: .topLeading) {
                        if reply.isEmpty {
                            Text("Paste the AI's reply here")
                                .foregroundStyle(.tertiary)
                                .padding(.top, 16)
                                .padding(.leading, 13)
                                .allowsHitTesting(false)
                                .accessibilityHidden(true)
                        }
                    }
                    .accessibilityLabel(Text("AI reply"))
                    .accessibilityIdentifier("reply-editor")
            }
            .padding()
            .navigationTitle("Paste AI reply")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Restore") {
                        onRestore(reply)
                    }
                    .disabled(reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("reply-restore")
                }
            }
        }
    }
}

/// The reply's text box. Its Paste, from the menu or ⌘V, reads a copy as the
/// Paste button does, so a table copied from the ChatGPT app stays one (ADR
/// 0105).
private struct ReplyEditor: UIViewRepresentable {
    @Binding var text: String

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    func makeUIView(context: Context) -> ReplyTextView {
        let view = ReplyTextView()
        view.font = .preferredFont(forTextStyle: .body)
        view.adjustsFontForContentSizeCategory = true
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 5
        view.delegate = context.coordinator
        return view
    }

    func updateUIView(_ view: ReplyTextView, context: Context) {
        context.coordinator.text = $text
        if view.text != text { view.text = text }
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var text: Binding<String>

        init(text: Binding<String>) { self.text = text }

        func textViewDidChange(_ view: UITextView) {
            text.wrappedValue = view.text
        }
    }
}

final class ReplyTextView: UITextView {
    override func paste(_ sender: Any?) {
        guard let pasted = PastedReply.text(from: .general), let range = selectedTextRange else {
            super.paste(sender)
            return
        }
        replace(range, withText: pasted)
        delegate?.textViewDidChange?(self)
    }
}

/// The document's restored reply; the hand-off bar below shares and copies
/// it (ADR 0087), so here is only its summary and Remove.
struct ReplyPane: View {
    let reply: ReplySnapshot
    let onClear: () -> Void
    @State private var confirmsRemove = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                RestoreSummary(result: reply.restored)
                Spacer()
                // The reply is kept with the document (ADR 0078) and has no
                // undo, so removing it asks first.
                Button(role: .destructive) {
                    confirmsRemove = true
                } label: {
                    Label("Remove reply", systemImage: "trash")
                        .labelStyle(.iconOnly)
                        .frame(minWidth: 44, minHeight: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityIdentifier("reply-actions")
                .confirmationDialog("Remove the reply?", isPresented: $confirmsRemove, titleVisibility: .visible) {
                    Button("Remove reply", role: .destructive, action: onClear)
                        .accessibilityIdentifier("reply-remove-confirm")
                } message: {
                    Text("The document and its names stay as they are.")
                }
            }
            .padding(.leading)
            .padding(.trailing, 4)
            Divider()
            RestoredTextView(result: reply.restored)
        }
    }
}
