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
                PasteButton(payloadType: String.self) { strings in
                    if let text = strings.first {
                        reply = text
                    }
                }
                .labelStyle(.titleAndIcon)
                TextEditor(text: $reply)
                    .font(.body)
                    .frame(minHeight: 120)
                    .scrollContentBackground(.hidden)
                    .padding(8)
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: 8))
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

/// The document's restored reply; the hand-off bar below shares and copies
/// it (ADR 0087), so here is only its summary and Remove.
struct ReplyPane: View {
    let reply: ReplySnapshot
    let onClear: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                RestoreSummary(result: reply.restored)
                Spacer()
                Button(role: .destructive, action: onClear) {
                    Label("Remove reply", systemImage: "trash")
                        .labelStyle(.iconOnly)
                }
                .accessibilityIdentifier("reply-actions")
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            Divider()
            RestoredTextView(result: reply.restored)
        }
    }
}
