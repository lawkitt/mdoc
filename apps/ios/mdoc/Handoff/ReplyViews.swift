import SwiftUI
import UIKit

/// Paste reply: the AI's answer, restored with this document's case.
struct ReplySheet: View {
    let projectName: String?
    let onRestore: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var reply = ""

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 12) {
                Text(projectName.map { String(localized: "mdoc puts back the names from the Project “\($0)”.") }
                    ?? String(localized: "mdoc puts back the names from this document."))
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
/// it (ADR 0087), so here is only its summary and Remove. With more than
/// one saved reply (its Project's, or its own while Unfiled), a menu picks
/// the one shown, newest first. Vault & Projects item 11 moves replies to
/// the Project page.
struct ReplyPane: View {
    let reply: ReplySnapshot
    var replies: [SavedReply] = []
    var onShow: (ReplyID) -> Void = { _ in }
    let onClear: () -> Void
    @State private var confirmsRemove = false

    var body: some View {
        VStack(spacing: 0) {
            if replies.count > 1 {
                picker
                    .padding(.horizontal)
                    .padding(.top, 8)
            }
            HStack {
                RestoreSummary(result: reply.restored)
                Spacer()
                // The reply is kept in the Vault (ADR 0078) and has no
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

    /// The saved replies, newest first: the shown one is checked.
    private var picker: some View {
        Menu {
            Picker("Reply", selection: Binding(get: { reply.id }, set: { if let id = $0 { onShow(id) } })) {
                ForEach(replies) { saved in
                    Text(SavedReplyRow.label(saved)).tag(Optional(saved.id))
                }
            }
        } label: {
            HStack {
                Text(replies.first { $0.id == reply.id }.map(SavedReplyRow.label) ?? "")
                    .fontWeight(.semibold)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption)
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityLabel(Text("Reply: \(replies.first { $0.id == reply.id }.map(SavedReplyRow.label) ?? "")"))
        .accessibilityHint(Text("Shows another saved reply."))
        .accessibilityIdentifier("reply-picker")
    }
}

/// A saved reply in a list: its title and when it was saved.
struct SavedReplyRow: View {
    let reply: SavedReply

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(reply.title)
                .lineLimit(2)
            Text(reply.created, format: .dateTime.day().month().hour().minute())
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }

    /// Title and date on one line, for a menu.
    static func label(_ reply: SavedReply) -> String {
        "\(reply.title) · \(reply.created.formatted(.dateTime.day().month().hour().minute()))"
    }
}

/// A saved reply restored, from a Project's list of replies: Copy and
/// Share, as the Reply pane's bar has.
struct SavedReplySheet: View {
    let reply: SavedReply
    let restored: Result<RestoreResult, CoreError>
    @Environment(\.dismiss) private var dismiss
    @State private var share: ShareItems?
    @State private var copied = false

    var body: some View {
        NavigationStack {
            Group {
                switch restored {
                case .success(let result):
                    VStack(spacing: 0) {
                        RestoreSummary(result: result)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                        Divider()
                        RestoredTextView(result: result)
                    }
                    .toolbar {
                        ToolbarItemGroup(placement: .bottomBar) {
                            Button {
                                share = ShareItems(items: [TitledText(text: result.text, title: reply.title)])
                            } label: {
                                Label("Share", systemImage: "square.and.arrow.up")
                            }
                            .accessibilityIdentifier("share-reply")
                            Spacer()
                            Button {
                                RestoredPasteboard.copy(result.text)
                                copied = true
                            } label: {
                                Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                                    .labelStyle(.titleAndIcon)
                            }
                            .primaryGlass()
                            .accessibilityIdentifier("copy-reply")
                        }
                    }
                case .failure(let error):
                    ContentUnavailableView(
                        "Not restored", systemImage: "exclamationmark.triangle",
                        description: Text(error.localizedDescription))
                }
            }
            .navigationTitle(reply.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .sheet(item: $share) { items in
            ActivityView(items: items.items)
        }
    }
}
