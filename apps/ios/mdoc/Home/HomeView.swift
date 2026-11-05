import PhotosUI
import SwiftUI
import VisionKit

/// Every document, newest first, with the setup card, search, the case
/// filter and adding at the bottom (ADR 0099).
struct HomeView: View {
    @Bindable var app: AppModel

    @State private var showsScanner = false
    @State private var showsPhotos = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showsFiles = false
    @State private var showsPaste = false
    @State private var shareItems: ShareItems?

    private var home: HomeViewModel { app.home }

    var body: some View {
        List(selection: $app.selection) {
            if showsSetupCard {
                Section {
                    SetupCard(model: app.setup)
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                }
                // Under the card, not over the list: the card grows while
                // the models download.
                if home.isEmpty {
                    Section {
                        emptyState
                            .listRowBackground(Color.clear)
                    }
                }
            }
            // Its own section, so the setup card above reads apart from it.
            Section {
                ForEach(home.shownDocuments) { document in
                    DocumentRow(document: document, caseSummary: home.caseSummary(of: document), findsNames: home.findsNames)
                        .tag(document.id)
                        .swipeActions {
                            Button("Delete", role: .destructive) {
                                app.deleteDocument(document.id)
                            }
                        }
                        .contextMenu {
                            Button {
                                app.selection = document.id
                            } label: {
                                Label("Open", systemImage: "doc.text")
                            }
                            // Only a reviewed document leaves mdoc, through
                            // Copy for AI's export path (ADR 0085).
                            if document.phase == .ready {
                                Button {
                                    shareItems = app.shareItems(for: document.id)
                                } label: {
                                    Label("Share", systemImage: "square.and.arrow.up")
                                }
                            }
                            Button(role: .destructive) {
                                app.deleteDocument(document.id)
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        } preview: {
                            DocumentPreview(document: document, caseSummary: home.caseSummary(of: document), findsNames: home.findsNames)
                        }
                }
            }
        }
        .overlay {
            if home.isEmpty && !showsSetupCard {
                emptyState
            }
        }
        .navigationTitle("mdoc")
        .modifier(HomeSearch(home: home))
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    app.showsSettings = true
                } label: {
                    Label("Settings", systemImage: "gearshape")
                }
                .accessibilityIdentifier("settings-button")
            }
            if home.offersCaseFilter {
                ToolbarItem(placement: .primaryAction) {
                    caseFilterMenu
                }
            }
            // Adding sits within thumb reach: Scan prominent, the rest in
            // the + menu (ADR 0099).
            ToolbarItemGroup(placement: .bottomBar) {
                if VNDocumentCameraViewController.isSupported {
                    Button {
                        showsScanner = true
                    } label: {
                        Label("Scan", systemImage: "doc.viewfinder")
                            .labelStyle(.titleAndIcon)
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("scan-button")
                }
                Spacer()
                addMenu
            }
        }
        .refreshable { home.refresh() }
        .fullScreenCover(isPresented: $showsScanner) {
            DocumentScanner { images in
                showsScanner = false
                app.receiveImages(images, scanned: true)
            } onCancel: {
                showsScanner = false
            }
            .ignoresSafeArea()
        }
        .photosPicker(isPresented: $showsPhotos, selection: $photoItems, maxSelectionCount: 20, matching: .images)
        .onChange(of: photoItems) { _, items in
            guard !items.isEmpty else { return }
            photoItems = []
            Task {
                var images: [Data] = []
                for item in items {
                    if let data = try? await item.loadTransferable(type: Data.self) {
                        images.append(data)
                    }
                }
                app.receiveImages(images, scanned: false)
            }
        }
        .fileImporter(isPresented: $showsFiles, allowedContentTypes: AppModel.importTypes) { result in
            switch result {
            case .success(let url): app.receiveFile(url)
            case .failure(let error): home.errorMessage = error.localizedDescription
            }
        }
        .sheet(item: $shareItems) { items in
            ActivityView(items: items.items)
        }
        .sheet(isPresented: $showsPaste) {
            PasteTextSheet(cases: home.cases) { text, title, caseChoice in
                showsPaste = false
                app.addText(text, title: title, into: caseChoice)
            }
        }
        .sheet(item: Bindable(home).pendingImport) { pending in
            ImportSheet(pending: pending, cases: home.cases) { title, caseChoice in
                app.finishImport(pending, title: title, into: caseChoice)
            }
        }
        .alert(
            "Something went wrong",
            isPresented: Binding(get: { home.errorMessage != nil }, set: { if !$0 { home.errorMessage = nil } }),
            presenting: home.errorMessage
        ) { _ in
            Button("OK") { home.errorMessage = nil }
        } message: { message in
            Text(message)
        }
    }

    private var showsSetupCard: Bool {
        app.setup.showsCard && !app.openDocumentShowsSetup
    }

    /// No documents yet: three large tiles and the sample (ADR 0099).
    private var emptyState: some View {
        VStack(spacing: Space.l) {
            VStack(spacing: Space.xs) {
                Image(systemName: "doc.text.magnifyingglass")
                    .font(.system(size: 44))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text("No documents yet")
                    .font(.title3.weight(.semibold))
                // Below the floor mdoc finds no names: no promise it does
                // not keep (ADR 0088).
                Text(home.findsNames
                    ? "Add a letter, contract or note. mdoc finds the names in it so you can share it with an AI without them."
                    : "Add a letter, contract or note. Select the names in it to replace them, then share it with an AI.")
                    .font(Typography.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: Space.s) { addTiles }
                VStack(spacing: Space.s) { addTiles }
            }
            Button("Try a sample") {
                app.trySample()
            }
            .font(Typography.chip)
            .frame(minHeight: Size.target)
            .accessibilityIdentifier("try-sample")
        }
        .padding(Space.m)
        .frame(maxWidth: 520)
    }

    @ViewBuilder
    private var addTiles: some View {
        if VNDocumentCameraViewController.isSupported {
            AddTile(title: "Scan", spoken: "Scan a document", id: "empty-scan", systemImage: "doc.viewfinder", color: .teal) { showsScanner = true }
        }
        AddTile(title: "Files", spoken: "Add from Files", id: "empty-files", systemImage: "folder", color: .blue) { showsFiles = true }
        AddTile(title: "Paste", spoken: "Paste text", id: "empty-paste", systemImage: "doc.on.clipboard", color: .orange) { showsPaste = true }
    }

    /// From two cases: Home shows one case's documents, or all.
    private var caseFilterMenu: some View {
        Menu {
            Picker("Case", selection: Bindable(home).caseFilter) {
                Text("All cases").tag(CaseID?.none)
                ForEach(home.cases) { summary in
                    Label {
                        Text(summary.name)
                    } icon: {
                        Image(systemName: "circle.fill")
                    }
                    .tint(summary.colour.color)
                    .tag(CaseID?.some(summary.id))
                }
            }
        } label: {
            Label("Case", systemImage: home.caseFilter == nil
                ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
        }
        .accessibilityIdentifier("case-filter")
    }

    /// Paste, Files, Photos, Scan: the first item opens nearest the button,
    /// so the camera is last, away from it, and a double tap cannot open it
    /// (ADR 0088).
    private var addMenu: some View {
        Menu {
            Button {
                showsPaste = true
            } label: {
                Label("Paste", systemImage: "doc.on.clipboard")
            }
            Button {
                showsFiles = true
            } label: {
                Label("Files", systemImage: "folder")
            }
            Button {
                showsPhotos = true
            } label: {
                Label("Photos", systemImage: "photo.on.rectangle")
            }
            if VNDocumentCameraViewController.isSupported {
                Button {
                    showsScanner = true
                } label: {
                    Label("Scan", systemImage: "doc.viewfinder")
                }
            }
        } label: {
            Label("Add", systemImage: "plus")
        }
        .menuOrder(.priority)
        .accessibilityIdentifier("add-menu")
    }
}

/// One of the empty state's large add tiles.
private struct AddTile: View {
    let title: LocalizedStringKey
    /// Apart from the + menu's item of the same name.
    let spoken: LocalizedStringKey
    let id: String
    let systemImage: String
    let color: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: Space.xs) {
                IconTile(systemImage: systemImage, color: color, size: 44)
                Text(title)
                    .font(Typography.chip)
                    .foregroundStyle(.primary)
            }
            .frame(maxWidth: .infinity, minHeight: 96)
            .card(padding: Space.s)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(spoken))
        .accessibilityIdentifier(id)
    }
}

/// Search over titles and case names, from `HomeViewModel.searchThreshold`
/// documents.
private struct HomeSearch: ViewModifier {
    @Bindable var home: HomeViewModel

    func body(content: Content) -> some View {
        if home.offersSearch {
            content.searchable(text: $home.query, prompt: Text("Documents and cases"))
        } else {
            content
        }
    }
}

/// A document on Home (ADR 0099): a source tile, the title, then the state
/// in its color, when it last changed, and the case as a tag.
struct DocumentRow: View {
    let document: DocumentSummary
    let caseSummary: CaseSummary?
    let findsNames: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var status: String { HomeViewModel.status(of: document, findsNames: findsNames) }

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
                        caseTag
                    }
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        statusAndTime(stacked: false)
                        caseTag
                    }
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        statusAndTime(stacked: true)
                        caseTag
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
            status, systemImage: HomeViewModel.statusSymbol(of: document, findsNames: findsNames),
            kind: HomeViewModel.statusKind(of: document))
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
    private var caseTag: some View {
        if let caseSummary {
            CaseTag(summary: caseSummary, showsName: caseSummary.name != document.title)
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
        if let caseSummary {
            parts.append(String(localized: "case \(caseSummary.name)"))
        }
        return parts.joined(separator: ", ")
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
    let document: DocumentSummary
    let caseSummary: CaseSummary?
    let findsNames: Bool

    var body: some View {
        DocumentRow(document: document, caseSummary: caseSummary, findsNames: findsNames)
            .padding(Space.m)
            .frame(width: 320, alignment: .leading)
    }
}

/// The case as a small tag: its colour dot, and its name when it says
/// something the title does not.
struct CaseTag: View {
    let summary: CaseSummary
    let showsName: Bool

    var body: some View {
        HStack(spacing: Space.xxs) {
            CaseDot(colour: summary.colour)
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

/// The title and the case for something just brought in. A new case is the
/// default, named after the document.
struct ImportSheet: View {
    let pending: PendingImport
    let cases: [CaseSummary]
    let onImport: (String, CaseChoice) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var title: String
    @State private var existingCase: CaseID?

    init(pending: PendingImport, cases: [CaseSummary], onImport: @escaping (String, CaseChoice) -> Void) {
        self.pending = pending
        self.cases = cases
        self.onImport = onImport
        _title = State(initialValue: pending.suggestedTitle)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Title", text: $title)
                        .accessibilityIdentifier("import-title")
                }
                Section {
                    Picker("Case", selection: $existingCase) {
                        Text("New case").tag(CaseID?.none)
                        ForEach(cases) { item in
                            Text(item.name).tag(Optional(item.id))
                        }
                    }
                } footer: {
                    Text("A case keeps the names and their aliases together, so an AI reply about any of its documents can be restored.")
                }
            }
            .navigationTitle("Add document")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
                        onImport(title, existingCase.map(CaseChoice.existing)
                            ?? .newCase(name: name.isEmpty ? pending.suggestedTitle : name))
                    }
                    .accessibilityIdentifier("import-add")
                }
            }
        }
        .reviewSheetDetents()
    }
}

/// Paste: text from the clipboard, its title and its case in one sheet
/// (ADR 0087); a new case named after the title is the default.
struct PasteTextSheet: View {
    let cases: [CaseSummary]
    let onAdd: (String, String, CaseChoice) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var title = ""
    @State private var existingCase: CaseID?

    private var suggestedTitle: String { AppModel.suggestedTitle(for: text) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    PasteButton(payloadType: String.self) { strings in
                        if let first = strings.first {
                            text = first
                        }
                    }
                    .labelStyle(.titleAndIcon)
                    // A long paste scrolls inside the editor, so the title
                    // and the case stay on the sheet.
                    TextEditor(text: $text)
                        .frame(minHeight: 180, maxHeight: 260)
                        .overlay(alignment: .topLeading) {
                            if text.isEmpty {
                                Text("Paste or type the text to check")
                                    .foregroundStyle(.tertiary)
                                    .padding(.top, 8)
                                    .padding(.leading, 5)
                                    .allowsHitTesting(false)
                                    .accessibilityHidden(true)
                            }
                        }
                        .accessibilityLabel(Text("Text"))
                }
                Section {
                    TextField("Title", text: $title, prompt: Text(suggestedTitle))
                        .accessibilityIdentifier("import-title")
                    Picker("Case", selection: $existingCase) {
                        Text("New case").tag(CaseID?.none)
                        ForEach(cases) { item in
                            Text(item.name).tag(Optional(item.id))
                        }
                    }
                } footer: {
                    Text("A case keeps the names and their aliases together, so an AI reply about any of its documents can be restored.")
                }
            }
            .navigationTitle("Paste text")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
                        let chosen = name.isEmpty ? suggestedTitle : name
                        onAdd(text, chosen, existingCase.map(CaseChoice.existing) ?? .newCase(name: chosen))
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("import-add")
                }
            }
        }
    }
}

/// VisionKit's document camera: capture only; recognition stays PP-OCR.
struct DocumentScanner: UIViewControllerRepresentable {
    let onScan: ([Data]) -> Void
    let onCancel: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIViewController(context: Context) -> VNDocumentCameraViewController {
        let controller = VNDocumentCameraViewController()
        controller.delegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: VNDocumentCameraViewController, context: Context) {}

    /// VisionKit calls its delegate on the main thread.
    @MainActor
    final class Coordinator: NSObject, @preconcurrency VNDocumentCameraViewControllerDelegate {
        let parent: DocumentScanner

        init(parent: DocumentScanner) {
            self.parent = parent
        }

        func documentCameraViewController(_ controller: VNDocumentCameraViewController, didFinishWith scan: VNDocumentCameraScan) {
            let pages = (0..<scan.pageCount).compactMap { scan.imageOfPage(at: $0).jpegData(compressionQuality: 0.9) }
            parent.onScan(pages)
        }

        func documentCameraViewControllerDidCancel(_ controller: VNDocumentCameraViewController) {
            parent.onCancel()
        }

        func documentCameraViewController(_ controller: VNDocumentCameraViewController, didFailWithError error: Error) {
            parent.onCancel()
        }
    }
}

#if DEBUG
#Preview("Home") {
    NavigationStack { HomeView(app: PreviewFixtures.app()) }
}

#Preview("Home, dark, AX5") {
    NavigationStack { HomeView(app: PreviewFixtures.app()) }
        .preferredColorScheme(.dark)
        .environment(\.dynamicTypeSize, .accessibility5)
}
#endif
