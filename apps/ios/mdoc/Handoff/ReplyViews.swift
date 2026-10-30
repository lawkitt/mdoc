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
                Text(caseName.map { String(localized: "Aliases are restored with the names in “\($0)”.") }
                    ?? String(localized: "Aliases are restored with this document's names."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                PasteButton(payloadType: String.self) { strings in
                    if let text = strings.first {
                        reply = text
                    }
                }
                .labelStyle(.titleAndIcon)
                TextEditor(text: $reply)
                    .font(.body)
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

/// The document's restored reply with Copy and Share.
struct ReplyPane: View {
    let reply: ReplySnapshot
    let onClear: () -> Void
    @State private var share: ShareItems?
    @State private var copied = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                RestoreSummary(result: reply.restored)
                Spacer()
                Menu {
                    Button {
                        UIPasteboard.general.string = reply.restored.text
                        copied = true
                    } label: {
                        Label("Copy reply", systemImage: "doc.on.doc")
                    }
                    Button {
                        share = ShareItems(items: [reply.restored.text])
                    } label: {
                        Label("Share reply", systemImage: "square.and.arrow.up")
                    }
                    Button(role: .destructive, action: onClear) {
                        Label("Remove reply", systemImage: "trash")
                    }
                } label: {
                    Label(copied ? "Copied" : "Reply", systemImage: copied ? "checkmark" : "ellipsis.circle")
                }
                .accessibilityIdentifier("reply-actions")
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            Divider()
            RestoredTextView(result: reply.restored)
        }
        .sheet(item: $share) { items in
            ActivityView(items: items.items)
        }
    }
}
