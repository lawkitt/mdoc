import SwiftUI

/// A document's menu on its sidebar row and its Documents page row (ADR
/// 0102): Rename…, Move to case, Share, Delete. The title's menu has the
/// same items in its own groups (ADR 0114). What a command asks for is
/// presented by `Presenter`.
struct DocumentMenuItems: View {
    let app: AppModel
    let document: LibraryDocument

    var body: some View {
        DocumentFileItems(app: app, document: document)
        DocumentShareButton(app: app, document: document)
        DocumentDeleteButton(app: app, document: document)
    }
}

/// Rename… and Move to case ›.
struct DocumentFileItems: View {
    let app: AppModel
    let document: LibraryDocument

    var body: some View {
        Button {
            app.navigator.present(.renameDocument(document.id))
        } label: {
            Label("Rename…", systemImage: "pencil")
        }
        Menu {
            let recent = app.library.recentCases(excluding: document.caseID)
            ForEach(recent) { summary in
                Button {
                    app.move(document.id, to: .existing(summary.id))
                } label: {
                    Text(summary.name)
                }
            }
            if !recent.isEmpty { Divider() }
            Button {
                app.navigator.present(.moveToNewCase(document.id))
            } label: {
                Label("New case…", systemImage: "plus")
            }
            if app.library.cases.count > recent.count + (document.caseID == nil ? 0 : 1) {
                Button {
                    app.navigator.present(.chooseCase(document.id))
                } label: {
                    Label("All cases…", systemImage: "list.bullet")
                }
            }
        } label: {
            Label("Move to case", systemImage: "folder")
        }
    }
}

/// Share: only a reviewed document leaves mdoc, through Copy for AI's
/// export path (ADR 0085).
struct DocumentShareButton: View {
    let app: AppModel
    let document: LibraryDocument
    /// Why sharing waits, from the open document (ADR 0115), which goes
    /// before the library's phase where it is listed always.
    var blockedReason: String?
    /// The title's menu lists Share always, disabled until the document is
    /// ready; a row's menu only once it is.
    var alwaysListed = false

    private var reason: String? {
        if alwaysListed { return blockedReason }
        return document.phase == .ready ? nil : String(localized: "The document isn't ready yet")
    }

    var body: some View {
        if reason == nil || alwaysListed {
            Button {
                app.share(document.id)
            } label: {
                Label("Share", systemImage: "square.and.arrow.up")
                if let reason {
                    Text(reason)
                }
            }
            .disabled(reason != nil)
            .accessibilityIdentifier("share-document")
        }
    }
}

/// Delete; the toast offers Undo (Q17).
struct DocumentDeleteButton: View {
    let app: AppModel
    let document: LibraryDocument

    var body: some View {
        Button(role: .destructive) {
            app.delete(document.id)
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }
}

/// Move to case › All cases…: every other case, searchable.
struct CasePicker: View {
    let cases: [CaseSummary]
    let onPick: (CaseID) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        NavigationStack {
            List(cases.filter { query.isEmpty || $0.name.localizedStandardContains(query) }) { summary in
                Button {
                    onPick(summary.id)
                } label: {
                    HStack(spacing: Space.s) {
                        CaseDot(colour: summary.colour)
                        Text(summary.name)
                            .foregroundStyle(.primary)
                    }
                    .frame(minHeight: Size.target)
                }
            }
            .searchable(text: $query)
            .navigationTitle("Move to case")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

extension EnvironmentValues {
    /// The app's model, for menus a screen shares with the sidebar (the
    /// document's title menu).
    @Entry var appModel: AppModel? = nil
}
