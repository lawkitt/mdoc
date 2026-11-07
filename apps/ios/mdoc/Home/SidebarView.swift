import SwiftUI

/// The sidebar (docs/mobile-ui.md, Sidebar), after the Claude app's: search
/// at the top, the Cases and Documents rows, then *To review* and *Recent*,
/// with Settings and New floating at the bottom. On iPhone it lies under the
/// screen, which slides aside to show it; on a wide iPad it is the standing
/// left column.
struct SidebarView: View {
    @Bindable var app: AppModel
    @State private var query = ""
    @FocusState private var searching: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    /// The icon column grows with the text.
    @ScaledMetric(relativeTo: .body) private var iconWidth: CGFloat = 24

    private var home: HomeViewModel { app.home }

    /// Recent stops at this many; the Documents page has the rest.
    static let recentLimit = 20

    private var toReview: [DocumentSummary] {
        home.documents.filter { $0.phase == .reviewing }
    }

    private var recent: [DocumentSummary] {
        Array(home.documents.filter { $0.phase != .reviewing }.prefix(Self.recentLimit))
    }

    private var matches: (documents: [DocumentSummary], cases: [CaseSummary]) {
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return ([], []) }
        let documents = home.documents.filter { document in
            document.title.localizedStandardContains(needle)
                || (home.caseSummary(of: document)?.name.localizedStandardContains(needle) ?? false)
        }
        return (documents, home.cases.filter { $0.name.localizedStandardContains(needle) })
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
        .onChange(of: app.searchRequest) { searching = true }
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
                TextField("Search documents and cases", text: $query)
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
            ForEach(home.documents.prefix(8)) { row($0) }
        } else if found.documents.isEmpty && found.cases.isEmpty {
            Text("No documents or cases match “\(query)”.")
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .padding(Space.s)
        } else {
            if !found.cases.isEmpty {
                header("Cases")
                ForEach(found.cases) { summary in
                    caseRow(summary)
                }
            }
            if !found.documents.isEmpty {
                header("Documents")
                ForEach(found.documents) { row($0) }
            }
        }
    }

    // MARK: Sections

    @ViewBuilder
    private var sections: some View {
        navRow(.cases, title: "Cases", systemImage: "briefcase")
        navRow(.documents, title: "Documents", systemImage: "doc.on.doc")
        if !toReview.isEmpty {
            header("To review")
            ForEach(toReview) { row($0) }
        }
        if !recent.isEmpty {
            header("Recent")
            ForEach(recent) { row($0) }
        }
        if home.isEmpty {
            Text("Documents you add show here.")
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, Space.s)
                .padding(.top, Space.m)
        }
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
            SidebarRowLabel(isSelected: app.section == section && app.selection == nil) {
                Image(systemName: systemImage)
                    .frame(width: iconWidth)
                    .accessibilityHidden(true)
                Text(title)
                Spacer(minLength: 0)
                if section == .cases, !home.cases.isEmpty {
                    Text("\(home.cases.count)")
                        .font(Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(app.section == section ? .isSelected : [])
        .accessibilityIdentifier("sidebar-\(section.rawValue)")
    }

    private func caseRow(_ summary: CaseSummary) -> some View {
        Button {
            home.caseFilter = summary.id
            app.open(.documents)
        } label: {
            SidebarRowLabel(isSelected: false) {
                CaseDot(colour: summary.colour)
                    .frame(width: iconWidth)
                Text(summary.name)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
    }

    // MARK: Document rows

    private func row(_ document: DocumentSummary) -> some View {
        Button {
            app.selection = document.id
            query = ""
            searching = false
        } label: {
            SidebarRowLabel(isSelected: app.selection == document.id) {
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
        .accessibilityAddTraits(app.selection == document.id ? .isSelected : [])
        .accessibilityIdentifier("document-row")
    }

    /// Working: a spinner. To review: how many names are left, in orange.
    /// Ready: a green dot. Otherwise nothing.
    @ViewBuilder
    private func trailing(_ document: DocumentSummary) -> some View {
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

    private func spoken(_ document: DocumentSummary) -> String {
        var parts = [document.title]
        if app.workingDocument == document.id {
            parts.append(String(localized: "working"))
        }
        parts.append(HomeViewModel.status(of: document, findsNames: home.findsNames))
        if let summary = home.caseSummary(of: document) {
            parts.append(String(localized: "case \(summary.name)"))
        }
        return parts.joined(separator: ", ")
    }

    // MARK: Bottom

    /// Settings as a round button, New as a capsule, floating over a fade.
    private var bottomBar: some View {
        HStack {
            Button {
                app.showsSettings = true
            } label: {
                Image(systemName: "gearshape")
                    .font(.title3)
                    .foregroundStyle(.primary)
                    .frame(width: 52, height: 52)
                    .background(.regularMaterial, in: Circle())
                    .overlay(Circle().strokeBorder(Palette.separator.opacity(0.5)))
            }
            .accessibilityLabel(Text("Settings"))
            .accessibilityIdentifier("settings-button")
            Spacer()
            Menu {
                NewMenuItems(app: app)
            } label: {
                Label("New", systemImage: "plus")
                    .font(Typography.chip.weight(.semibold))
                    .foregroundStyle(Color(uiColor: .systemBackground))
                    .padding(.horizontal, Space.l)
                    .frame(height: 52)
                    .background(Color.primary, in: Capsule())
            }
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
