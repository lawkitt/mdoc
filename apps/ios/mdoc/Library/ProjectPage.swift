import SwiftUI
import VisionKit

/// A Project's page (#1091 Decision 34): its colour and name in the bar,
/// never "Documents"; `Documents` (first), `Replies` and `Names`; a `+` that
/// adds into it; and a ⋯ menu with Rename and Delete.
struct ProjectPage: View {
    @Bindable var app: AppModel
    let projectID: ProjectID

    enum Segment: String, CaseIterable, Identifiable {
        case documents, replies, names
        var id: String { rawValue }

        var title: LocalizedStringKey {
            switch self {
            case .documents: "Documents"
            case .replies: "Replies"
            case .names: "Names"
            }
        }
    }

    @Environment(\.showSidebar) private var showSidebar
    @State private var segment: Segment = .documents
    /// Names' search.
    @State private var query = ""
    /// A saved reply shown restored.
    @State private var shownReply: SavedReply?

    private var library: LibraryModel { app.library }
    private var project: ProjectSummary? { library.project(projectID) }

    /// The list's selection is the open document.
    private var selection: Binding<DocumentID?> {
        Binding(get: { app.navigator.openDocument }, set: { if let id = $0 { app.open(id) } })
    }

    var body: some View {
        List(selection: selection) {
            Section {
                Picker("Show", selection: $segment) {
                    ForEach(Segment.allCases) { segment in
                        Text(segment.title).tag(segment)
                    }
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
                .accessibilityIdentifier("project-segments")
            }
            switch segment {
            case .documents: documents
            case .replies: replies
            case .names: names
            }
        }
        .modifier(NamesSearch(shown: segment == .names, query: $query))
        .navigationTitle(project?.name ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let showSidebar {
                SidebarButton(action: showSidebar)
            }
            ToolbarItem(placement: .principal) {
                title
            }
            ToolbarItem(placement: .primaryAction) {
                projectMenu
            }
            ToolbarItemGroup(placement: .bottomBar) {
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
        .sheet(item: $shownReply) { reply in
            SavedReplySheet(reply: reply, restored: library.restore(reply.id))
        }
        .refreshable {
            // A reply Restore with mdoc saved meanwhile.
            app.takeInChanges()
            library.refresh()
        }
        .accessibilityIdentifier("project-page")
    }

    /// The Project's colour and name, never "Documents".
    private var title: some View {
        HStack(spacing: Space.xs) {
            if let project {
                ProjectDot(colour: project.colour)
                Text(project.name)
                    .font(.headline)
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
        .accessibilityIdentifier("project-title")
    }

    /// ⋯: Rename… and Delete Project… (Colour, Categories and the lists'
    /// own commands are later work).
    private var projectMenu: some View {
        Menu {
            Button {
                app.navigator.present(.renameProject(projectID))
            } label: {
                Label("Rename…", systemImage: "pencil")
            }
            Button(role: .destructive) {
                app.askToDeleteProject(projectID)
            } label: {
                Label("Delete Project…", systemImage: "trash")
            }
            .accessibilityIdentifier("delete-project")
        } label: {
            Label("Project", systemImage: "ellipsis")
        }
        .accessibilityIdentifier("project-menu")
    }

    // MARK: Documents

    @ViewBuilder
    private var documents: some View {
        let documents = library.documents(in: projectID)
        if documents.isEmpty {
            ContentUnavailableView(
                "No documents yet",
                systemImage: "doc.on.doc",
                description: Text("What you add with + lands in this Project."))
                .listRowBackground(Color.clear)
        }
        Section {
            ForEach(documents) { document in
                DocumentListRow(app: app, document: document, showsProject: false)
                    .tag(document.id)
            }
        }
    }

    // MARK: Replies (#1091 Decision 31)

    @ViewBuilder
    private var replies: some View {
        let replies = library.replies(in: projectID)
        if replies.isEmpty {
            ContentUnavailableView(
                "No replies yet",
                systemImage: "text.bubble",
                description: Text("AI replies you save with Restore with mdoc or Paste reply show here, restored."))
                .listRowBackground(Color.clear)
        }
        Section {
            ForEach(replies) { reply in
                Button {
                    shownReply = reply
                } label: {
                    SavedReplyRow(reply: reply)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("reply-row")
            }
        }
    }

    // MARK: Names (#1091 Decision 29)

    @ViewBuilder
    private var names: some View {
        let details = library.details(of: projectID)
        let groups = LibraryModel.names(details.names, matching: query)
        let always = details.always.filter { !LibraryModel.wordings([$0.wording], matching: query).isEmpty }
        let never = LibraryModel.wordings(details.never, matching: query)
        if details.names.isEmpty && query.isEmpty {
            ContentUnavailableView(
                "No names yet",
                systemImage: "person.text.rectangle",
                description: Text("The names its documents replace show here, with their aliases."))
                .listRowBackground(Color.clear)
        } else if groups.isEmpty && always.isEmpty && never.isEmpty {
            ContentUnavailableView.search(text: query)
                .listRowBackground(Color.clear)
        }
        ForEach(groups, id: \.0) { category, rows in
            Section(category.label) {
                ForEach(rows) { name in
                    ProjectNameView(name: name)
                }
            }
        }
        if query.isEmpty || !always.isEmpty {
            Section("Always replace") {
                if always.isEmpty {
                    emptyList
                }
                ForEach(always, id: \.wording) { entry in
                    listedRow(entry.wording, detail: entry.category.label, always: true)
                }
            }
        }
        if query.isEmpty || !never.isEmpty {
            Section("Never replace") {
                if never.isEmpty {
                    emptyList
                }
                ForEach(never, id: \.self) { wording in
                    listedRow(wording, detail: nil, always: false)
                }
            }
        }
    }

    private var emptyList: some View {
        Text("None")
            .font(Typography.caption)
            .foregroundStyle(.secondary)
    }

    /// An Always or Never wording, with Remove at its end and on a swipe.
    /// Nothing is added here, as on desktop.
    private func listedRow(_ wording: String, detail: String?, always: Bool) -> some View {
        let remove = { app.removeListed(wording, always: always, from: projectID) }
        return HStack(spacing: Space.s) {
            HStack(spacing: Space.s) {
                UnhyphenatedText(wording)
                Spacer(minLength: Space.xs)
                if let detail {
                    Text(detail)
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .accessibilityElement(children: .combine)
            Button(action: remove) {
                Image(systemName: "minus.circle")
                    .foregroundStyle(.secondary)
                    .frame(width: Size.target, height: Size.target)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(Text("Remove “\(wording)”"))
            .accessibilityIdentifier("remove-listed")
        }
        .swipeActions {
            Button("Remove", role: .destructive, action: remove)
        }
        .accessibilityIdentifier(always ? "always-row" : "never-row")
    }
}

/// One of a Project's names: its alias as the text shows it, then the
/// wordings it stands for.
private struct ProjectNameView: View {
    let name: ProjectNameRow

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            AliasTag(alias: name.alias, category: name.category)
            if !name.originals.isEmpty {
                UnhyphenatedText(name.originals.joined(separator: " · "), style: .caption1, color: .secondaryLabel)
            }
        }
        .padding(.vertical, Space.xxs)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("project-name-row")
    }
}

/// Names' search, only while Names shows.
private struct NamesSearch: ViewModifier {
    let shown: Bool
    @Binding var query: String

    func body(content: Content) -> some View {
        if shown {
            content.searchable(text: $query, prompt: Text("Names and wordings"))
        } else {
            content
        }
    }
}
