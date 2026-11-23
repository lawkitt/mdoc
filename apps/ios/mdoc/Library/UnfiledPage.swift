import SwiftUI
import VisionKit

/// Unfiled (the sidebar's Unfiled row): the documents in no Project,
/// newest first. Select picks several to delete at once (#1091 Decision
/// 5); their toast offers Undo, as one Delete's does. What + adds lands
/// here.
struct UnfiledPage: View {
    @Bindable var app: AppModel

    @Environment(\.showSidebar) private var showSidebar
    @State private var selecting = false
    @State private var selected: Set<DocumentID> = []

    private var library: LibraryModel { app.library }

    /// Outside Select, the list's selection is the open document.
    private var open: Binding<DocumentID?> {
        Binding(get: { app.navigator.openDocument }, set: { if let id = $0 { app.open(id) } })
    }

    var body: some View {
        let documents = library.unfiled
        Group {
            if selecting {
                List(selection: $selected) { rows(documents) }
                    .environment(\.editMode, .constant(.active))
            } else {
                List(selection: open) { rows(documents) }
            }
        }
        .navigationTitle("Unfiled")
        .toolbar {
            if let showSidebar, !selecting {
                SidebarButton(action: showSidebar)
            }
            if !documents.isEmpty || selecting {
                ToolbarItem(placement: .primaryAction) {
                    Button(selecting ? "Done" : "Select") {
                        selected = []
                        selecting.toggle()
                    }
                    .accessibilityIdentifier("unfiled-select")
                }
            }
            ToolbarItemGroup(placement: .bottomBar) {
                if selecting {
                    Button(role: .destructive) {
                        app.delete(documents.map(\.id).filter(selected.contains))
                        selected = []
                        selecting = false
                    } label: {
                        Label(deleteTitle, systemImage: "trash")
                            .labelStyle(.titleAndIcon)
                    }
                    .disabled(selected.isEmpty)
                    .accessibilityIdentifier("unfiled-delete")
                    Spacer()
                } else {
                    if VNDocumentCameraViewController.isSupported {
                        Button {
                            app.navigator.present(.intake(.scan))
                        } label: {
                            Label("Scan", systemImage: "doc.viewfinder")
                                .labelStyle(.titleAndIcon)
                        }
                        .primaryGlass()
                        .accessibilityIdentifier("scan-button")
                    }
                    Spacer()
                    AddMenu(app: app)
                }
            }
        }
        // A document deleted or moved elsewhere leaves the selection.
        .onChange(of: documents.map(\.id)) { _, ids in
            selected.formIntersection(ids)
        }
        .refreshable {
            app.takeInChanges()
            library.refresh()
        }
    }

    @ViewBuilder
    private func rows(_ documents: [LibraryDocument]) -> some View {
        if documents.isEmpty {
            ContentUnavailableView(
                "Nothing in Unfiled",
                systemImage: "tray",
                description: Text("Documents you add outside a Project land here."))
                .listRowBackground(Color.clear)
        }
        Section {
            ForEach(documents) { document in
                Group {
                    if selecting {
                        DocumentRow(document: document, projectSummary: nil, findsNames: library.findsNames)
                    } else {
                        DocumentListRow(app: app, document: document, showsProject: false)
                    }
                }
                .tag(document.id)
            }
        }
    }

    private var deleteTitle: String {
        selected.isEmpty ? String(localized: "Delete") : String(localized: "Delete \(selected.count)")
    }
}
