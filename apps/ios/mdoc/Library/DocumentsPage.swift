import SwiftUI
import VisionKit

/// The Documents page (the sidebar's Documents row): every document,
/// newest first, with search, the case filter and adding at the bottom
/// (ADR 0099). Its empty state is the setup card and Start's ways in.
struct DocumentsPage: View {
    @Bindable var app: AppModel

    @Environment(\.showSidebar) private var showSidebar
    /// A saved reply shown restored, from the Project's Replies.
    @State private var shownReply: SavedReply?

    private var library: LibraryModel { app.library }

    /// The list's selection is the open document.
    private var selection: Binding<DocumentID?> {
        Binding(get: { app.navigator.openDocument }, set: { if let id = $0 { app.open(id) } })
    }

    var body: some View {
        List(selection: selection) {
            if showsSetupCard {
                Section {
                    // Scan, in the bottom bar, is the page's one primary
                    // button (ADR 0099).
                    SetupCard(model: app.setup, prominent: false)
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                }
                // Under the card, not over the list: the card grows while
                // the models download.
                if showsEmptyState {
                    Section {
                        emptyState
                            .listRowBackground(Color.clear)
                    }
                }
            }
            // Its own section, so the setup card above reads apart from it.
            Section {
                ForEach(library.shownDocuments) { document in
                    DocumentRow(document: document, projectSummary: library.projectSummary(of: document), findsNames: library.findsNames)
                        .tag(document.id)
                        .swipeActions {
                            Button("Delete", role: .destructive) {
                                app.delete(document.id)
                            }
                        }
                        .contextMenu {
                            Button {
                                app.open(document.id)
                            } label: {
                                Label("Open", systemImage: "doc.text")
                            }
                            DocumentMenuItems(app: app, document: document)
                        } preview: {
                            DocumentPreview(document: document, projectSummary: library.projectSummary(of: document), findsNames: library.findsNames)
                        }
                }
            }
            // Filtered to a Project: its saved AI replies, which a tap shows
            // restored (Vault & Projects item 11 moves them to the Project
            // page's Replies).
            if let project = library.projectFilter, !library.replies(in: project).isEmpty {
                Section("Replies") {
                    ForEach(library.replies(in: project)) { reply in
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
        }
        .sheet(item: $shownReply) { reply in
            SavedReplySheet(reply: reply, restored: library.restore(reply.id))
        }
        .overlay {
            if showsEmptyState && !showsSetupCard {
                emptyState
            }
        }
        .navigationTitle("Documents")
        .modifier(DocumentsSearch(library: library))
        .toolbar {
            if let showSidebar {
                SidebarButton(action: showSidebar)
            }
            if library.offersProjectFilter {
                ToolbarItem(placement: .primaryAction) {
                    projectFilterMenu
                }
            }
            // Adding sits within thumb reach: Scan prominent, the rest in
            // the + menu (ADR 0099).
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
                addMenu
            }
        }
        .refreshable {
            // A reply Restore with mdoc saved meanwhile.
            app.takeInChanges()
            library.refresh()
        }
    }

    /// No documents, and no replies of the Project filtered to: a
    /// Project's own replies stay when its documents go.
    private var showsEmptyState: Bool {
        library.isEmpty && !(library.projectFilter.map { !library.replies(in: $0).isEmpty } ?? false)
    }

    private var showsSetupCard: Bool {
        app.setup.showsCard && !app.openDocumentShowsSetup
    }

    /// No documents yet: Start's ways in and the sample (ADR 0099).
    private var emptyState: some View {
        AddSection(app: app)
            .padding(Space.m)
            .frame(maxWidth: 520)
    }

    /// From two projects: the page shows one case's documents, or all.
    private var projectFilterMenu: some View {
        Menu {
            Picker("Project", selection: Bindable(library).projectFilter) {
                Text("All Projects").tag(ProjectID?.none)
                ForEach(library.projects) { summary in
                    Label {
                        Text(summary.name)
                    } icon: {
                        Image(systemName: "circle.fill")
                    }
                    .tint(summary.colour.color)
                    .tag(ProjectID?.some(summary.id))
                }
            }
        } label: {
            Label("Project", systemImage: library.projectFilter == nil
                ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
        }
        .accessibilityIdentifier("case-filter")
    }

    private var addMenu: some View {
        Menu {
            NewMenuItems(app: app)
        } label: {
            Label("Add", systemImage: "plus")
        }
        .menuOrder(.priority)
        .accessibilityIdentifier("add-menu")
    }
}

/// Search over titles and case names, from `LibraryModel.searchThreshold`
/// documents.
private struct DocumentsSearch: ViewModifier {
    @Bindable var library: LibraryModel

    func body(content: Content) -> some View {
        if library.offersSearch {
            content.searchable(text: $library.query, prompt: Text("Documents and Projects"))
        } else {
            content
        }
    }
}

/// A document on the Documents page (ADR 0099): a source tile, the title, then the state
/// in its color, when it last changed, and the case as a tag.
struct DocumentRow: View {
    let document: LibraryDocument
    let projectSummary: ProjectSummary?
    let findsNames: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var status: String { LibraryModel.status(of: document, findsNames: findsNames) }

    var body: some View {
        HStack(spacing: Space.s) {
            // At accessibility sizes the title takes the tile's room too.
            if !dynamicTypeSize.isAccessibilitySize {
                IconTile(systemImage: source.symbol, color: source.color, tinted: true, size: 40)
            }
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(document.title)
                    .font(Typography.body.weight(.medium))
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? 3 : 1)
                    .fixedSize(horizontal: false, vertical: true)
                // One line when it fits; else the case on its own line;
                // else every part on its own line, so nothing truncates.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: Space.xs) {
                        statusAndTime(stacked: false)
                        projectTag
                    }
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        statusAndTime(stacked: false)
                        projectTag
                    }
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        statusAndTime(stacked: true)
                        projectTag
                    }
                }
            }
        }
        .padding(.vertical, Space.xxs)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(spoken))
        .accessibilityIdentifier("document-row")
    }

    /// Stacked (narrow or large text): the status, the time and the case
    /// each on a line of their own, so nothing truncates.
    @ViewBuilder
    private func statusAndTime(stacked: Bool) -> some View {
        let pill = StatusPill(
            status, systemImage: LibraryModel.statusSymbol(of: document, findsNames: findsNames),
            kind: LibraryModel.statusKind(of: document))
        let time = Text(document.updated, format: Self.timeFormat(document.updated))
            .font(Typography.caption)
            .foregroundStyle(.secondary)
        if stacked {
            pill.fixedSize(horizontal: false, vertical: true)
            time
        } else {
            HStack(spacing: Space.xxs) {
                pill
                Text(verbatim: "·")
                    .font(Typography.caption)
                    .foregroundStyle(.tertiary)
                time
            }
            .lineLimit(1)
        }
    }

    @ViewBuilder
    private var projectTag: some View {
        if let projectSummary {
            ProjectTag(summary: projectSummary, showsName: projectSummary.name != document.title)
        }
    }

    /// Today the time, earlier the day.
    static func timeFormat(_ date: Date) -> Date.FormatStyle {
        Calendar.current.isDateInToday(date)
            ? .dateTime.hour().minute()
            : .dateTime.day().month(.abbreviated)
    }

    /// "Settlement agreement, 1 to review, case Settlement agreement".
    private var spoken: String {
        var parts = [document.title, status]
        if let projectSummary {
            parts.append(String(localized: "Project \(projectSummary.name)"))
        }
        return parts.joined(separator: ", ")
    }

    /// The source's symbol, as the sidebar shows it.
    static func symbol(of kind: SourceKind) -> String {
        switch kind {
        case .pdf: "doc.richtext"
        case .docx: "doc.text"
        case .image: "photo"
        case .scan: "doc.viewfinder"
        case .text: "text.alignleft"
        }
    }

    private var source: (symbol: String, color: Color) {
        switch document.kind {
        case .pdf: ("doc.richtext", .red)
        case .docx: ("doc.text", .blue)
        case .image: ("photo", .green)
        case .scan: ("doc.viewfinder", .teal)
        case .text: ("text.alignleft", .gray)
        }
    }
}

/// The long-press preview of a document: its row, larger.
private struct DocumentPreview: View {
    let document: LibraryDocument
    let projectSummary: ProjectSummary?
    let findsNames: Bool

    var body: some View {
        DocumentRow(document: document, projectSummary: projectSummary, findsNames: findsNames)
            .padding(Space.m)
            .frame(width: 320, alignment: .leading)
    }
}

/// The case as a small tag: its colour dot, and its name when it says
/// something the title does not.
struct ProjectTag: View {
    let summary: ProjectSummary
    let showsName: Bool

    var body: some View {
        HStack(spacing: Space.xxs) {
            ProjectDot(colour: summary.colour)
            if showsName {
                Text(summary.name)
                    .lineLimit(1)
            }
        }
        .font(Typography.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, showsName ? Space.xs : 0)
        .padding(.vertical, 2)
        .background(showsName ? summary.colour.color.opacity(0.12) : .clear, in: Capsule())
    }
}

#if DEBUG
#Preview("Documents") {
    NavigationStack { DocumentsPage(app: PreviewFixtures.app()) }
}

#Preview("Documents, dark, AX5") {
    NavigationStack { DocumentsPage(app: PreviewFixtures.app()) }
        .preferredColorScheme(.dark)
        .environment(\.dynamicTypeSize, .accessibility5)
}
#endif
