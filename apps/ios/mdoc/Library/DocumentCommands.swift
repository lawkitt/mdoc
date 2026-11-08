import SwiftUI

/// A document's menu, the same on its sidebar row, its Documents page row
/// and its title (ADR 0102): Rename…, Move to case, Share, Delete. What a
/// command asks for is presented by `Presenter`.
struct DocumentMenuItems: View {
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
        // Only a reviewed document leaves mdoc, through Copy for AI's export
        // path (ADR 0085).
        if document.phase == .ready {
            Button {
                app.share(document.id)
            } label: {
                Label("Share", systemImage: "square.and.arrow.up")
            }
        }
        // The toast offers Undo (Q17).
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
