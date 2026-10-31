import PhotosUI
import SwiftUI
import VisionKit

/// Documents grouped by case, the setup card and the add menu.
struct HomeView: View {
    @Bindable var app: AppModel

    @State private var showsScanner = false
    @State private var showsPhotos = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showsFiles = false
    @State private var showsPaste = false

    private var home: HomeViewModel { app.home }

    var body: some View {
        List(selection: $app.selection) {
            if app.setup.showsCard {
                Section {
                    SetupCard(model: app.setup)
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                }
            }
            // Its own section, so the setup card above reads apart from it.
            Section {
                ForEach(home.documents) { document in
                    DocumentRow(document: document, caseSummary: home.caseSummary(of: document), findsNames: home.findsNames)
                        .tag(document.id)
                        .swipeActions {
                            Button("Delete", role: .destructive) {
                                app.deleteDocument(document.id)
                            }
                        }
                }
            }
        }
        .overlay {
            if home.isEmpty {
                ContentUnavailableView {
                    Label("No documents yet", systemImage: "doc.text.magnifyingglass")
                } description: {
                    // Below the floor mdoc finds no names: no promise it does
                    // not keep (ADR 0088).
                    if home.findsNames {
                        Text("Scan a letter, or add a PDF, Word file, photo or text. mdoc finds the names in it so you can share it with an AI without them.")
                    } else {
                        Text("Scan a letter, or add a PDF, Word file, photo or text. Select the names in it to replace them, then share it with an AI.")
                    }
                } actions: {
                    addMenu
                }
            }
        }
        .navigationTitle("mdoc")
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    app.showsSettings = true
                } label: {
                    Label("Settings", systemImage: "gearshape")
                }
                .accessibilityIdentifier("settings-button")
            }
            ToolbarItem(placement: .primaryAction) {
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

private struct DocumentRow: View {
    let document: DocumentSummary
    let caseSummary: CaseSummary?
    let findsNames: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var status: String { HomeViewModel.status(of: document, findsNames: findsNames) }

    var body: some View {
        HStack(spacing: 12) {
            // At accessibility sizes the title takes the icon's room too.
            if !dynamicTypeSize.isAccessibilitySize {
                Image(systemName: symbol)
                    .foregroundStyle(.secondary)
                    .frame(width: 24)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(document.title)
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? 4 : 2)
                // The case only when its name says something the title does not.
                if let caseSummary, caseSummary.name != document.title {
                    HStack(spacing: 6) {
                        CaseDot(colour: caseSummary.colour)
                        Text(caseSummary.name)
                            .lineLimit(1)
                    }
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                }
                HStack(spacing: 8) {
                    // The case's dot, when its name line is left out.
                    if let caseSummary, caseSummary.name == document.title {
                        CaseDot(colour: caseSummary.colour)
                    }
                    Label {
                        Text(status)
                    } icon: {
                        Image(systemName: HomeViewModel.statusSymbol(of: document, findsNames: findsNames))
                    }
                    .labelStyle(StatusLabelStyle())
                    .fontWeight(document.phase == .reviewing ? .semibold : .regular)
                    .foregroundStyle(document.phase == .reviewing ? .primary : .secondary)
                }
                .font(.footnote)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(spoken))
        .accessibilityIdentifier("document-row")
    }

    /// "Settlement agreement, 1 to review, case Settlement agreement".
    private var spoken: String {
        var parts = [document.title, status]
        if let caseSummary {
            parts.append(String(localized: "case \(caseSummary.name)"))
        }
        return parts.joined(separator: ", ")
    }

    private var symbol: String {
        switch document.kind {
        case .pdf: "doc.richtext"
        case .docx: "doc.text"
        case .image: "photo"
        case .scan: "doc.viewfinder"
        case .text: "text.alignleft"
        }
    }
}

/// A status line: a small icon (or the case's dot) before the words.
private struct StatusLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 6) {
            configuration.icon
                .imageScale(.small)
            configuration.title
        }
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
        .presentationDetents([.medium, .large])
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
                    TextEditor(text: $text)
                        .frame(minHeight: 180)
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
