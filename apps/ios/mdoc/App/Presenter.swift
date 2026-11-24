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
                } onFail: { error in
                    // The error replaces the camera, which closes.
                    app.present(error)
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
                    // A photo Photos can't hand over stays in the batch as
                    // nil, so the importer says so instead of dropping it.
                    var images: [Data?] = []
                    for item in items {
                        images.append((try? await item.loadTransferable(type: Data.self)) ?? nil)
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

/// Rename…, New Project… and errors.
private struct Dialogs: ViewModifier {
    let app: AppModel
    @Binding var draft: String

    private var navigator: Navigator { app.navigator }

    private var renamingDocument: DocumentID? {
        if case .renameDocument(let id) = navigator.presentation { id } else { nil }
    }

    private var renamingProject: ProjectID? {
        if case .renameProject(let id) = navigator.presentation { id } else { nil }
    }

    private var movingToNewProject: DocumentID? {
        if case .moveToNewProject(let id) = navigator.presentation { id } else { nil }
    }

    private var error: String? {
        if case .error(let message) = navigator.presentation { message } else { nil }
    }

    private var restoringSample: DocumentID? {
        if case .restoreSample(let id) = navigator.presentation { id } else { nil }
    }

    private var repeatedImport: RepeatedImport? {
        if case .repeatedImport(let repeated) = navigator.presentation { repeated } else { nil }
    }

    func body(content: Content) -> some View {
        content
            .modifier(ProjectDialogs(app: app, draft: $draft))
            // An import of a file already in mdoc, in #1107's words.
            .alert(
                Text("“\(repeatedImport?.name ?? "")” is already in mdoc"),
                isPresented: navigator.isPresenting { if case .repeatedImport = $0 { true } else { false } },
                presenting: repeatedImport
            ) { repeated in
                Button("Open Existing") { app.openExisting(repeated) }
                Button("Import Again") { app.importAgain(repeated) }
                Button("Cancel", role: .cancel) { app.cancelImport(repeated) }
            } message: { _ in
                Text("Open the document you imported before, or import the file again as a new document.")
            }
            .alert("Rename document", isPresented: navigator.isPresenting { if case .renameDocument = $0 { true } else { false } }) {
                TextField("Title", text: $draft)
                    .accessibilityIdentifier("rename-field")
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = renamingDocument { app.rename(id, to: draft) }
                }
            } message: {
                Text("Its names stay with it.")
            }
            .onChange(of: renamingDocument) { _, id in draft = id.flatMap { app.library.document($0)?.title } ?? "" }
            .alert("Rename Project", isPresented: navigator.isPresenting { if case .renameProject = $0 { true } else { false } }) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Rename") {
                    if let id = renamingProject { app.renameProject(id, to: draft) }
                }
            }
            .onChange(of: renamingProject) { _, id in draft = app.library.projects.first { $0.id == id }?.name ?? "" }
            .alert("New Project", isPresented: navigator.isPresenting { if case .moveToNewProject = $0 { true } else { false } }) {
                TextField("Name", text: $draft)
                Button("Cancel", role: .cancel) {}
                Button("Move") {
                    if let id = movingToNewProject { app.move(id, to: .newProject(name: draft)) }
                }
                // A Project needs a name (#677).
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            } message: {
                Text("The document leaves where it is for a new Project. Replies already sent still restore with the names they were sent with.")
            }
            .onChange(of: movingToNewProject) { draft = "" }
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

/// New Project and Delete Project (#1091 Decisions 9 and 34).
private struct ProjectDialogs: ViewModifier {
    let app: AppModel
    @Binding var draft: String

    private var navigator: Navigator { app.navigator }

    /// The Project Delete Project… asks about, while it is there.
    private var deleting: ProjectSummary? {
        guard case .deleteProject(let id) = navigator.presentation else { return nil }
        return app.library.project(id)
    }

    private var isDeleting: Binding<Bool> {
        navigator.isPresenting { if case .deleteProject = $0 { true } else { false } }
    }

    func body(content: Content) -> some View {
        let deleting = deleting
        let count = deleting.map { app.library.documents(in: $0.id).count } ?? 0
        content
            .alert("New Project", isPresented: navigator.isPresenting { if case .newProject = $0 { true } else { false } }) {
                TextField("Name", text: $draft)
                    .accessibilityIdentifier("new-project-name")
                Button("Cancel", role: .cancel) {}
                Button("Create") { app.newProject(named: draft) }
                    // A Project needs a name (#677).
                    .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            } message: {
                Text("Documents you add from its page land in it.")
            }
            .onChange(of: navigator.presentation?.id == Presentation.newProject.id) { _, shown in
                if shown { draft = "" }
            }
            // A Project with documents: move them to Unfiled or delete
            // them; its own AI replies go either way (the maintainer's
            // answer on #1091).
            .alert(
                Text("Delete “\(deleting?.name ?? "")”?"),
                isPresented: count > 0 ? isDeleting : .constant(false),
                presenting: deleting
            ) { project in
                Button("Move its documents to Unfiled") { app.deleteProject(project.id, documents: .moveToUnfiled) }
                Button("Delete them", role: .destructive) { app.deleteProject(project.id, documents: .deleteDocuments) }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text(count == 1
                    ? "Its document can move to Unfiled, or be deleted with it. Its own AI replies are deleted either way."
                    : "Its \(count) documents can move to Unfiled, or be deleted with it. Its own AI replies are deleted either way.")
            }
            // An empty Project asks plainly, as desktop does.
            .alert(
                "Delete Project?",
                isPresented: deleting != nil && count == 0 ? isDeleting : .constant(false),
                presenting: deleting
            ) { project in
                Button("Delete", role: .destructive) { app.deleteProject(project.id, documents: .deleteDocuments) }
                Button("Cancel", role: .cancel) {}
            } message: { project in
                Text("“\(project.name)” has no documents. Its names and its AI replies are deleted with it.")
            }
    }
}

/// Settings and the sheets that carry a value: All projects…, Share
/// and the import sheet.
private struct Sheets: ViewModifier {
    let app: AppModel

    private var navigator: Navigator { app.navigator }
    private var library: LibraryModel { app.library }

    /// A sheet that carries a value.
    private enum Sheet: Identifiable {
        case chooseProject(DocumentID), share(ShareItems), importing(PendingImport)

        init?(_ presentation: Presentation) {
            switch presentation {
            case .chooseProject(let id): self = .chooseProject(id)
            case .share(let items): self = .share(items)
            case .importing(let pending): self = .importing(pending)
            default: return nil
            }
        }

        var id: String {
            switch self {
            case .chooseProject(let id): Presentation.chooseProject(id).id
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
            // A reply saved through Restore with mdoc from a share sheet
            // shows once it closes.
            .sheet(item: navigator.presented(Sheet.init), onDismiss: { app.takeInChanges() }) { sheet in
                switch sheet {
                case .chooseProject(let id):
                    ProjectPicker(projects: library.projects.filter { $0.id != library.document(id)?.projectID }) { projectID in
                        navigator.dismiss()
                        app.move(id, to: .existing(projectID))
                    }
                case .share(let items):
                    ActivityView(items: items.items)
                case .importing(let pending):
                    ImportSheet(pending: pending, projects: library.projects, startingProject: pending.project) { title, project in
                        app.finishImport(pending, title: title, into: project)
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
