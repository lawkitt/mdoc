import SwiftUI

/// The sidebar (docs/specs/mobile.md, Navigation), after the Claude app's:
/// search at the top, then Home, *To review*, the Projects, Unfiled and All
/// Documents, with Settings and New floating at the bottom. On iPhone it
/// lies under the screen, which slides aside to show it; on a wide iPad it
/// is the standing left column.
struct SidebarView: View {
    @Bindable var app: AppModel
    @State private var query = ""
    @FocusState private var searching: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    /// The icon column grows with the text.
    @ScaledMetric(relativeTo: .body) private var iconWidth: CGFloat = 24

    private var library: LibraryModel { app.library }
    private var navigator: Navigator { app.navigator }

    private var toReview: [LibraryDocument] { library.toReview }

    private var matches: (documents: [LibraryDocument], projects: [ProjectSummary]) {
        library.search(query)
    }

    private var isSearching: Bool { searching || !query.isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            searchField
                .padding(.horizontal, Space.m)
                .padding(.top, Space.xs)
                .padding(.bottom, Space.xs)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    if isSearching {
                        searchResults
                    } else {
                        sections
                    }
                }
                .padding(.horizontal, Space.xs)
                // Room under the last row for the floating buttons.
                .padding(.bottom, 96)
            }
            .scrollDismissesKeyboard(.immediately)
        }
        .overlay(alignment: .bottom) {
            if !isSearching {
                bottomBar
            }
        }
        .background(Palette.background)
        // ⌘K: the request waits until a sidebar takes it, on appearing too,
        // since the drawer makes this view only once it opens (#675).
        .onChange(of: navigator.searchPending, initial: true) { _, pending in
            if pending && navigator.takeSearchRequest() { searching = true }
        }
        // A container: its rows and buttons keep identifiers of their own.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("sidebar")
    }

    // MARK: Search

    private var searchField: some View {
        HStack(spacing: Space.xs) {
            HStack(spacing: Space.xs) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                TextField("Search documents and Projects", text: $query)
                    .focused($searching)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.search)
                    .accessibilityIdentifier("sidebar-search")
            }
            .padding(.horizontal, Space.s)
            .frame(minHeight: Size.target)
            .background(Palette.chipFill, in: Capsule())
            if isSearching {
                Button {
                    query = ""
                    searching = false
                } label: {
                    Image(systemName: "xmark")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: Size.target, height: Size.target)
                        .background(Palette.chipFill, in: Circle())
                }
                .accessibilityLabel(Text("Cancel search"))
                .transition(.opacity.combined(with: .scale))
            }
        }
        .animation(Motion.snappy, value: isSearching)
    }

    @ViewBuilder
    private var searchResults: some View {
        let found = matches
        if query.isEmpty {
            header("Recent")
            ForEach(library.documents.prefix(8)) { row($0) }
        } else if found.documents.isEmpty && found.projects.isEmpty {
            Text("No documents or Projects match “\(query)”.")
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .padding(Space.s)
        } else {
            if !found.projects.isEmpty {
                header("Projects")
                ForEach(found.projects) { summary in
                    projectRow(summary)
                }
            }
            if !found.documents.isEmpty {
                header("Documents")
                ForEach(found.documents) { row($0) }
            }
        }
    }

    // MARK: Sections

    /// Home, To review while names wait, the Projects (the first five,
    /// All Projects and New Project), then Unfiled and All Documents
    /// (#1091 Decision 33).
    @ViewBuilder
    private var sections: some View {
        homeRow
        if !toReview.isEmpty {
            header("To review")
            ForEach(toReview) { row($0) }
        }
        header("Projects")
        ForEach(library.topProjects) { summary in
            projectRow(summary)
        }
        navRow(.projects, title: "All Projects", systemImage: "briefcase")
        actionRow(title: "New Project", systemImage: "plus", id: "sidebar-new-project") {
            navigator.present(.newProject)
        }
        navRow(.unfiled, title: "Unfiled", systemImage: "tray")
            .padding(.top, Space.m)
        navRow(.documents, title: "All Documents", systemImage: "doc.on.doc")
    }

    private var homeRow: some View {
        Button {
            app.showStart()
        } label: {
            SidebarRowLabel(isSelected: navigator.destination == .start) {
                Image(systemName: "house")
                    .frame(width: iconWidth)
                    .accessibilityHidden(true)
                Text("Home")
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(navigator.destination == .start ? .isSelected : [])
        .accessibilityIdentifier("sidebar-home")
    }

    private func actionRow(
        title: LocalizedStringKey, systemImage: String, id: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            SidebarRowLabel(isSelected: false) {
                Image(systemName: systemImage)
                    .foregroundStyle(.secondary)
                    .frame(width: iconWidth)
                    .accessibilityHidden(true)
                Text(title)
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(id)
    }

    private func header(_ title: LocalizedStringKey) -> some View {
        Text(title)
            .font(Typography.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, Space.s)
            .padding(.top, Space.m)
            .padding(.bottom, Space.xxs)
            .accessibilityAddTraits(.isHeader)
    }

    private func navRow(_ section: SidebarSection, title: LocalizedStringKey, systemImage: String) -> some View {
        Button {
            app.open(section)
        } label: {
            SidebarRowLabel(isSelected: navigator.section == section) {
                Image(systemName: systemImage)
                    .frame(width: iconWidth)
                    .accessibilityHidden(true)
                Text(title)
                Spacer(minLength: 0)
                if let count = count(of: section), count > 0 {
                    Text("\(count)")
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(navigator.section == section ? .isSelected : [])
        .accessibilityIdentifier("sidebar-\(section.rawValue)")
    }

    /// All Projects counts the Projects, Unfiled its documents.
    private func count(of section: SidebarSection) -> Int? {
        switch section {
        case .projects: library.projects.count
        case .unfiled: library.unfiled.count
        case .documents: nil
        }
    }

    private func projectRow(_ summary: ProjectSummary) -> some View {
        Button {
            app.openProject(summary.id)
            query = ""
            searching = false
        } label: {
            SidebarRowLabel(isSelected: navigator.project == summary.id) {
                ProjectDot(colour: summary.colour)
                    .frame(width: iconWidth)
                Text(summary.name)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(navigator.project == summary.id ? .isSelected : [])
        .accessibilityIdentifier("sidebar-project")
    }

    // MARK: Document rows

    private func row(_ document: LibraryDocument) -> some View {
        Button {
            app.open(document.id)
            query = ""
            searching = false
        } label: {
            SidebarRowLabel(isSelected: navigator.openDocument == document.id) {
                // At accessibility sizes the title takes the icon's room too.
                if !dynamicTypeSize.isAccessibilitySize {
                    Image(systemName: DocumentRow.symbol(of: document.kind))
                        .foregroundStyle(.secondary)
                        .frame(width: iconWidth)
                        .accessibilityHidden(true)
                }
                // At accessibility sizes a Text squeezed beside the count
                // hyphenates mid-word; this one wraps at words.
                if dynamicTypeSize.isAccessibilitySize {
                    UnhyphenatedText(document.title)
                } else {
                    Text(document.title)
                        .lineLimit(1)
                }
                Spacer(minLength: Space.xs)
                trailing(document)
            }
        }
        .buttonStyle(.plain)
        .contextMenu { DocumentMenuItems(app: app, document: document) }
        .accessibilityLabel(Text(spoken(document)))
        .accessibilityAddTraits(navigator.openDocument == document.id ? .isSelected : [])
        .accessibilityIdentifier("document-row")
    }

    /// Working: a spinner. To review: how many names are left, in orange.
    /// Ready: a green dot. Otherwise nothing.
    @ViewBuilder
    private func trailing(_ document: LibraryDocument) -> some View {
        if app.workingDocument == document.id {
            ProgressView()
                .controlSize(.small)
                // A fresh view per job: a reused one can stay blank in a
                // lazy stack.
                .id(app.jobs.current?.id)
                .accessibilityHidden(true)
        } else if document.phase == .reviewing {
            Text("\(document.pendingCount)")
                .font(Typography.caption.weight(.semibold).monospacedDigit())
                .foregroundStyle(Palette.toReview)
                .padding(.horizontal, Space.xs)
                .padding(.vertical, 2)
                .background(Palette.toReview.opacity(0.14), in: Capsule())
        } else if document.phase == .ready && document.candidateCount > 0 {
            Circle()
                .fill(Palette.ready)
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
        }
    }

    private func spoken(_ document: LibraryDocument) -> String {
        var parts = [document.title]
        if app.workingDocument == document.id {
            parts.append(String(localized: "working"))
        }
        parts.append(LibraryModel.status(of: document, findsNames: library.findsNames))
        if let summary = library.projectSummary(of: document) {
            parts.append(String(localized: "Project \(summary.name)"))
        }
        return parts.joined(separator: ", ")
    }

    // MARK: Bottom

    /// Settings and the theme as clear glass circles, New as the sidebar's
    /// primary button, floating over a fade.
    private var bottomBar: some View {
        HStack {
            Button {
                navigator.present(.settings)
            } label: {
                Image(systemName: "gearshape")
            }
            .secondaryAction(circle: true)
            .accessibilityLabel(Text("Settings"))
            .accessibilityIdentifier("settings-button")
            ThemeButton()
            Spacer()
            Menu {
                NewMenuItems(app: app)
            } label: {
                Label("New", systemImage: "plus")
                    .fontWeight(.semibold)
                    .padding(.horizontal, Space.xs)
                    .bigButtonLabel(prominent: true)
            }
            .primaryGlass()
            .menuOrder(.priority)
            .accessibilityIdentifier("new-menu")
        }
        .padding(.horizontal, Space.m)
        .padding(.bottom, Space.xs)
        .padding(.top, Space.l)
        .background(
            LinearGradient(colors: [Palette.background.opacity(0), Palette.background], startPoint: .top, endPoint: .center)
                .allowsHitTesting(false))
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }
}

/// A sidebar row: icon, words and trailing detail on one line, with the
/// filled highlight of the open row.
struct SidebarRowLabel<Content: View>: View {
    let isSelected: Bool
    @ViewBuilder var content: () -> Content

    var body: some View {
        HStack(spacing: Space.s) {
            content()
        }
        .font(Typography.body)
        .foregroundStyle(.primary)
        .padding(.horizontal, Space.s)
        .frame(minHeight: Size.target)
        .background(
            isSelected ? Palette.chipFill : .clear,
            in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
    }
}
