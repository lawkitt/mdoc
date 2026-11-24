import SwiftUI
import UIKit

/// Type a reply: an AI reply written or edited by hand, restored with this
/// document's case. A copied reply needs none of it: the bar's Paste
/// restores it in one tap, HTML tables included.
struct TypeReplySheet: View {
    let projectName: String?
    let onRestore: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var reply = ""
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: Space.s) {
                Text(projectName.map { String(localized: "mdoc puts back the names from the Project “\($0)”.") }
                    ?? String(localized: "mdoc puts back the names from this document."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    // Its full height up to six lines (a Project is named
                    // after a title, which can be long), so the box keeps
                    // its room.
                    .lineLimit(6)
                    .fixedSize(horizontal: false, vertical: true)
                TextEditor(text: $reply)
                    .focused($focused)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 120)
                    .padding(Space.xs)
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: Radius.control))
                    .overlay(alignment: .topLeading) {
                        if reply.isEmpty {
                            Text("Type or paste the AI's reply")
                                .foregroundStyle(.tertiary)
                                .padding(.top, Space.xs + 8)
                                .padding(.leading, Space.xs + 5)
                                .allowsHitTesting(false)
                                .accessibilityHidden(true)
                        }
                    }
                    .accessibilityLabel(Text("AI reply"))
                    .accessibilityIdentifier("reply-editor")
            }
            .padding()
            .navigationTitle("Type a reply")
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
            .onAppear { focused = true }
        }
    }
}

/// The document's restored reply; the hand-off bar below shares and copies
/// it (ADR 0087), so here is only its summary and Remove. With more than
/// one saved reply (its Project's, or its own while Unfiled), a menu picks
/// the one shown, newest first. A Project's page lists its replies too; an
/// Unfiled document's are only here (#1091 Decisions 4 and 31). Promote to
/// Document makes a new document of it (#1091 Decision 3).
struct ReplyPane: View {
    let reply: ReplySnapshot
    var replies: [SavedReply] = []
    var onShow: (ReplyID) -> Void = { _ in }
    /// Nil: the reply can't be promoted here.
    var onPromote: (() -> Void)? = nil
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
                if let onPromote {
                    PromoteButton(action: onPromote)
                }
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

/// Promote to Document (#1091 Decision 3): the restored reply becomes a
/// new document in the reply's place, and the reply stays.
struct PromoteButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Promote to Document", systemImage: "doc.badge.plus")
                .labelStyle(.iconOnly)
                .frame(minWidth: 44, minHeight: 44)
                .contentShape(Rectangle())
        }
        .accessibilityHint(Text("Makes a new document of the restored reply. The reply stays."))
        .accessibilityIdentifier("promote-reply")
    }
}

/// A saved reply restored, from a Project's list of replies or a promoted
/// document's way back: Copy and Share, as the Reply pane's bar has, and
/// Promote to Document.
struct SavedReplySheet: View {
    let reply: SavedReply
    let restored: Result<RestoreResult, CoreError>
    /// Nil: the reply can't be promoted here. Called as the sheet closes:
    /// the presenter promotes once it has gone, so an error can show.
    var onPromote: (() -> Void)? = nil
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
                            if let onPromote {
                                PromoteButton {
                                    onPromote()
                                    dismiss()
                                }
                            }
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
