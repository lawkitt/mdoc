import SwiftUI

/// All Projects (the sidebar's All Projects row): each Project, pinned first,
/// then most recently updated, with its colour and how many documents it
/// holds; a tap opens its page.
struct ProjectsView: View {
    @Bindable var app: AppModel
    @Environment(\.showSidebar) private var showSidebar

    private var library: LibraryModel { app.library }

    var body: some View {
        List {
            if library.projects.isEmpty {
                ContentUnavailableView(
                    "No Projects yet",
                    systemImage: "briefcase",
                    description: Text("Each document you add lands in Unfiled, or in the Project whose page you add it from."))
                    .listRowBackground(Color.clear)
            }
            ForEach(library.projects) { summary in
                Button {
                    app.openProject(summary.id)
                } label: {
                    HStack(spacing: Space.s) {
                        ProjectDot(colour: summary.colour)
                        VStack(alignment: .leading, spacing: Space.xxs) {
                            Text(summary.name)
                                .font(Typography.body.weight(.medium))
                                .foregroundStyle(.primary)
                            Text(count(summary))
                                .font(Typography.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                    }
                    .frame(minHeight: Size.target)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .contextMenu {
                    Button {
                        app.navigator.present(.renameProject(summary.id))
                    } label: {
                        Label("Rename…", systemImage: "pencil")
                    }
                    Button(role: .destructive) {
                        app.askToDeleteProject(summary.id)
                    } label: {
                        Label("Delete Project…", systemImage: "trash")
                    }
                }
                .swipeActions {
                    Button("Rename") { app.navigator.present(.renameProject(summary.id)) }
                        .tint(Palette.swipeAccent)
                }
                .accessibilityAction(named: Text("Rename")) { app.navigator.present(.renameProject(summary.id)) }
                .accessibilityIdentifier("project-row")
            }
        }
        .navigationTitle("All Projects")
        .toolbar {
            if let showSidebar {
                SidebarButton(action: showSidebar)
            }
            ToolbarItem(placement: .primaryAction) {
                Button {
                    app.navigator.present(.newProject)
                } label: {
                    Label("New Project", systemImage: "plus")
                }
                .accessibilityIdentifier("new-project")
            }
        }
    }

    func count(_ summary: ProjectSummary) -> String {
        let documents = library.documents.filter { $0.projectID == summary.id }
        let toReview = documents.filter { $0.phase == .reviewing }.count
        let base = documents.count == 1 ? String(localized: "1 document") : String(localized: "\(documents.count) documents")
        return toReview > 0 ? base + " · " + String(localized: "\(toReview) to review") : base
    }
}
