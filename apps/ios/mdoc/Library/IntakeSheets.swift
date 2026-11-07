import SwiftUI
import VisionKit

/// The case something new goes into: a new case (the default) or one of the
/// library's, with what a case is for. `leading` rows (Paste's title) sit
/// above the picker in the same section.
struct CaseChoiceSection<Leading: View>: View {
    let cases: [CaseSummary]
    @Binding var existingCase: CaseID?
    @ViewBuilder var leading: () -> Leading

    var body: some View {
        Section {
            leading()
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
}

extension CaseChoiceSection where Leading == EmptyView {
    init(cases: [CaseSummary], existingCase: Binding<CaseID?>) {
        self.init(cases: cases, existingCase: existingCase) { EmptyView() }
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
                CaseChoiceSection(cases: cases, existingCase: $existingCase)
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
                CaseChoiceSection(cases: cases, existingCase: $existingCase) {
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
