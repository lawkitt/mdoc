import SwiftUI

/// A document's menu, the same on its sidebar row and on its title
/// (ADR 0102): Rename…, Move to case, Share, Delete.
struct DocumentMenuItems: View {
    let app: AppModel
    let document: DocumentSummary

    var body: some View {
        Button {
            app.renamingDocument = document.id
        } label: {
            Label("Rename…", systemImage: "pencil")
        }
        Menu {
            let recent = app.recentCases(excluding: document.caseID)
            ForEach(recent) { summary in
                Button {
                    app.move(document.id, to: .existing(summary.id))
                } label: {
                    Text(summary.name)
                }
            }
            if !recent.isEmpty { Divider() }
            Button {
                app.movingToNewCase = document.id
            } label: {
                Label("New case…", systemImage: "plus")
            }
            if app.home.cases.count > recent.count + (document.caseID == nil ? 0 : 1) {
                Button {
                    app.choosingCase = document.id
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
        Button(role: .destructive) {
            app.deleteDocument(document.id)
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }
}

/// The dialogs the document and case menus open, on the root so every
/// screen shares them: Rename…, New case…, All cases…, Share, and the
/// app's toast.
struct DocumentCommandsPresenter: ViewModifier {
    @Bindable var app: AppModel
    @State private var draft = ""

    private var home: HomeViewModel { app.home }

    private func title(of id: DocumentID?) -> String {
        home.documents.first { $0.id == id }?.title ?? ""
    }

    func body(content: Content) -> some View {
        content
            .alert(
                "Rename document",
                isPresented: Binding(get: { app.renamingDocument != nil }, set: { if !$0 { app.renamingDocument = nil } })
            ) {
                TextField("Title", text: $draft)
                    .accessibilityIdentifier("rename-field")
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = app.renamingDocument { app.rename(id, to: draft) }
                }
            } message: {
                Text("Its case keeps its names.")
            }
            .onChange(of: app.renamingDocument) { _, id in draft = title(of: id) }
            .alert(
                "Rename case",
                isPresented: Binding(get: { app.renamingCase != nil }, set: { if !$0 { app.renamingCase = nil } })
            ) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = app.renamingCase { app.renameCase(id, to: draft) }
                }
            }
            .onChange(of: app.renamingCase) { _, id in draft = home.cases.first { $0.id == id }?.name ?? "" }
            .alert(
                "New case",
                isPresented: Binding(get: { app.movingToNewCase != nil }, set: { if !$0 { app.movingToNewCase = nil } })
            ) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Move") {
                    if let id = app.movingToNewCase { app.move(id, to: .newCase(name: draft)) }
                }
            } message: {
                Text("The document leaves its case. Replies already sent still restore with the old case.")
            }
            .onChange(of: app.movingToNewCase) { draft = "" }
            .sheet(item: Binding(get: { app.choosingCase.map(Chosen.init) }, set: { if $0 == nil { app.choosingCase = nil } })) { chosen in
                CasePicker(cases: home.cases.filter { $0.id != home.documents.first { $0.id == chosen.id }?.caseID }) { caseID in
                    app.choosingCase = nil
                    app.move(chosen.id, to: .existing(caseID))
                }
            }
            .sheet(item: $app.sharing) { items in
                ActivityView(items: items.items)
            }
            .toast($app.toast)
    }

    private struct Chosen: Identifiable {
        let id: DocumentID
    }
}

/// Move to case › All cases…: every other case, searchable.
private struct CasePicker: View {
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
