import SwiftUI
import VisionKit

/// Where something new goes: Unfiled, or a Project, with what a Project is
/// for. It starts on the Project the intake was started from, else Unfiled;
/// no import makes a Project (#1091 Decision 35). `leading` rows (Paste's
/// title) sit above the picker in the same section.
struct ProjectChoiceSection<Leading: View>: View {
    let projects: [ProjectSummary]
    @Binding var existingProject: ProjectID?
    @ViewBuilder var leading: () -> Leading

    var body: some View {
        Section {
            leading()
            Picker("Project", selection: $existingProject) {
                Text("Unfiled").tag(ProjectID?.none)
                ForEach(projects) { item in
                    Text(item.name).tag(Optional(item.id))
                }
            }
        } footer: {
            Text("A Project keeps the names and their aliases together, so an AI reply about any of its documents can be restored. An Unfiled document keeps its own.")
        }
    }
}

extension ProjectChoiceSection where Leading == EmptyView {
    init(projects: [ProjectSummary], existingProject: Binding<ProjectID?>) {
        self.init(projects: projects, existingProject: existingProject) { EmptyView() }
    }
}

/// The title and the Project for something just brought in: Unfiled, or
/// the Project it was started from.
struct ImportSheet: View {
    let pending: PendingImport
    let projects: [ProjectSummary]
    let onImport: (String, ProjectID?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var title: String
    @State private var existingProject: ProjectID?

    init(
        pending: PendingImport, projects: [ProjectSummary], startingProject: ProjectID? = nil,
        onImport: @escaping (String, ProjectID?) -> Void
    ) {
        self.pending = pending
        self.projects = projects
        self.onImport = onImport
        _title = State(initialValue: pending.suggestedTitle)
        _existingProject = State(initialValue: startingProject)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Title", text: $title)
                        .accessibilityIdentifier("import-title")
                }
                ProjectChoiceSection(projects: projects, existingProject: $existingProject)
            }
            .navigationTitle("Add document")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        onImport(title, existingProject)
                    }
                    .accessibilityIdentifier("import-add")
                }
            }
        }
        .reviewSheetDetents()
    }
}

/// Paste: text from the clipboard, its title and its Project in one sheet
/// (ADR 0087); Unfiled, or the Project it was started from.
struct PasteTextSheet: View {
    let projects: [ProjectSummary]
    let onAdd: (String, String, ProjectID?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var title = ""
    @State private var existingProject: ProjectID?

    init(projects: [ProjectSummary], startingProject: ProjectID? = nil, onAdd: @escaping (String, String, ProjectID?) -> Void) {
        self.projects = projects
        self.onAdd = onAdd
        _existingProject = State(initialValue: startingProject)
    }

    private var suggestedTitle: String { Importer.suggestedTitle(for: text) }

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
                    // and the Project stay on the sheet.
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
                ProjectChoiceSection(projects: projects, existingProject: $existingProject) {
                    TextField("Title", text: $title, prompt: Text(suggestedTitle))
                        .accessibilityIdentifier("import-title")
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
                        onAdd(text, chosen, existingProject)
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
    /// Each page as a JPEG, nil for one that couldn't be encoded: the
    /// importer refuses the scan rather than drop it.
    let onScan: ([Data?]) -> Void
    let onCancel: () -> Void
    /// The camera failed, as when another app holds it: not a cancel.
    let onFail: (Error) -> Void

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
            let pages = (0..<scan.pageCount).map { scan.imageOfPage(at: $0).jpegData(compressionQuality: 0.9) }
            parent.onScan(pages)
        }

        func documentCameraViewControllerDidCancel(_ controller: VNDocumentCameraViewController) {
            parent.onCancel()
        }

        func documentCameraViewController(_ controller: VNDocumentCameraViewController, didFailWithError error: Error) {
            cameraFailed(error)
        }

        /// The camera failed, as when another app holds it: not a cancel.
        /// Apart from the delegate call so tests need no camera controller,
        /// which the Simulator has no camera for.
        func cameraFailed(_ error: Error) {
            parent.onFail(error)
        }
    }
}
