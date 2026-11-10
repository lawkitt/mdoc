import PhotosUI
import SwiftUI

/// The one place in the view tree that presents (ADR 0104): the navigator's
/// `presentation`, whatever screen asked for it, and the app's toast. Every
/// dialog and sheet binds to that one value, so only one shows at a time.
struct Presenter: ViewModifier {
    @Bindable var app: AppModel
    @State private var draft = ""
    @State private var photoItems: [PhotosPickerItem] = []

    private var navigator: Navigator { app.navigator }
    private var library: LibraryModel { app.library }

    func body(content: Content) -> some View {
        content
            .modifier(dialogs)
            .modifier(sheets)
            .fullScreenCover(isPresented: navigator.isPresenting { if case .intake(.scan) = $0 { true } else { false } }) {
                DocumentScanner { images in
                    navigator.dismiss()
                    app.receiveImages(images, scanned: true)
                } onCancel: {
                    navigator.dismiss()
                }
                .ignoresSafeArea()
            }
            .photosPicker(
                isPresented: navigator.isPresenting { if case .intake(.photos) = $0 { true } else { false } },
                selection: $photoItems, maxSelectionCount: 20, matching: .images)
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
            .fileImporter(
                isPresented: navigator.isPresenting { if case .intake(.files) = $0 { true } else { false } },
                allowedContentTypes: Importer.importTypes
            ) { result in
                switch result {
                case .success(let url): app.receiveFile(url)
                case .failure(let error): app.present(error)
                }
            }
            .toast(Binding(get: { app.toast }, set: { if $0 == nil, let id = app.toast?.id { app.toastEnded(id) } }))
    }

    private var dialogs: Dialogs { Dialogs(app: app, draft: $draft) }
    private var sheets: Sheets { Sheets(app: app) }
}

/// Rename…, New case… and errors.
private struct Dialogs: ViewModifier {
    let app: AppModel
    @Binding var draft: String

    private var navigator: Navigator { app.navigator }

    private var renamingDocument: DocumentID? {
        if case .renameDocument(let id) = navigator.presentation { id } else { nil }
    }

    private var renamingCase: CaseID? {
        if case .renameCase(let id) = navigator.presentation { id } else { nil }
    }

    private var movingToNewCase: DocumentID? {
        if case .moveToNewCase(let id) = navigator.presentation { id } else { nil }
    }

    private var error: String? {
        if case .error(let message) = navigator.presentation { message } else { nil }
    }

    private var restoringSample: DocumentID? {
        if case .restoreSample(let id) = navigator.presentation { id } else { nil }
    }

    func body(content: Content) -> some View {
        content
            .alert("Rename document", isPresented: navigator.isPresenting { if case .renameDocument = $0 { true } else { false } }) {
                TextField("Title", text: $draft)
                    .accessibilityIdentifier("rename-field")
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = renamingDocument { app.rename(id, to: draft) }
                }
            } message: {
                Text("Its case keeps its names.")
            }
            .onChange(of: renamingDocument) { _, id in draft = id.flatMap { app.library.document($0)?.title } ?? "" }
            .alert("Rename case", isPresented: navigator.isPresenting { if case .renameCase = $0 { true } else { false } }) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = renamingCase { app.renameCase(id, to: draft) }
                }
            }
            .onChange(of: renamingCase) { _, id in draft = app.library.cases.first { $0.id == id }?.name ?? "" }
            .alert("New case", isPresented: navigator.isPresenting { if case .moveToNewCase = $0 { true } else { false } }) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Move") {
                    if let id = movingToNewCase { app.move(id, to: .newCase(name: draft)) }
                }
                // A case needs a name (#677).
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            } message: {
                Text("The document leaves its case. Replies already sent still restore with the old case.")
            }
            .onChange(of: movingToNewCase) { draft = "" }
            // Try a sample on an edited sample (ADR 0109).
            .alert("Restore the sample?", isPresented: navigator.isPresenting { if case .restoreSample = $0 { true } else { false } }) {
                Button("Cancel", role: .cancel) {}
                Button("Restore", role: .destructive) {
                    if let id = restoringSample { app.restoreSample(id) }
                }
            } message: {
                Text("Your edits to it will be lost.")
            }
            .alert(
                "Something went wrong",
                isPresented: navigator.isPresenting { if case .error = $0 { true } else { false } },
                presenting: error
            ) { _ in
                Button("OK") { navigator.dismiss() }
            } message: { message in
                Text(message)
            }
    }
}

/// Settings, Paste, and the sheets that carry a value: All cases…, Share
/// and the import sheet.
private struct Sheets: ViewModifier {
    let app: AppModel

    private var navigator: Navigator { app.navigator }
    private var library: LibraryModel { app.library }

    /// A sheet that carries a value.
    private enum Sheet: Identifiable {
        case chooseCase(DocumentID), share(ShareItems), importing(PendingImport)

        init?(_ presentation: Presentation) {
            switch presentation {
            case .chooseCase(let id): self = .chooseCase(id)
            case .share(let items): self = .share(items)
            case .importing(let pending): self = .importing(pending)
            default: return nil
            }
        }

        var id: String {
            switch self {
            case .chooseCase(let id): Presentation.chooseCase(id).id
            case .share(let items): Presentation.share(items).id
            case .importing(let pending): Presentation.importing(pending).id
            }
        }
    }

    func body(content: Content) -> some View {
        content
            .sheet(isPresented: navigator.isPresenting { if case .settings = $0 { true } else { false } }) {
                SettingsView(setup: app.setup, preferences: app.preferences) {
                    app.trySample()
                }
            }
            .sheet(isPresented: navigator.isPresenting { if case .intake(.paste) = $0 { true } else { false } }) {
                PasteTextSheet(cases: library.cases) { text, title, caseChoice in
                    navigator.dismiss()
                    app.addText(text, title: title, into: caseChoice)
                }
            }
            .sheet(item: navigator.presented(Sheet.init)) { sheet in
                switch sheet {
                case .chooseCase(let id):
                    CasePicker(cases: library.cases.filter { $0.id != library.document(id)?.caseID }) { caseID in
                        navigator.dismiss()
                        app.move(id, to: .existing(caseID))
                    }
                case .share(let items):
                    ActivityView(items: items.items)
                case .importing(let pending):
                    ImportSheet(pending: pending, cases: library.cases) { title, caseChoice in
                        app.finishImport(pending, title: title, into: caseChoice)
                    }
                }
            }
    }
}

extension Navigator {
    /// True while what is presented matches; false dismisses it.
    func isPresenting(_ match: @escaping (Presentation) -> Bool) -> Binding<Bool> {
        Binding(
            get: { self.presentation.map(match) ?? false },
            set: { shown in
                if !shown, let current = self.presentation, match(current) { self.dismiss(current.id) }
            })
    }

    /// The value `extract` finds in what is presented; nil dismisses it.
    func presented<Value>(_ extract: @escaping (Presentation) -> Value?) -> Binding<Value?> {
        Binding(
            get: { self.presentation.flatMap(extract) },
            set: { value in
                if value == nil, let current = self.presentation, extract(current) != nil { self.dismiss(current.id) }
            })
    }
}
