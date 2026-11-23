import MdocFFI
import PDFKit
import SwiftUI

/// A DOCX file's original page: docxide renders it in memory. Its comments
/// sit on the page as margin markers that open the comment sheet
/// at their thread (`onOpenComment`; the document
/// screen holds the sheet), or on an iPad's wide pane as the rail. The
/// title menu's Show comments shows or hides them, an app-wide setting,
/// and opens those not on the page (`onComments` tells it what there is).
struct DocxOriginalView: View {
    let source: OriginalBytes
    @Binding var unlocked: UnlockedDocx?
    var importPending = false
    /// A long job runs on the document: the render waits for it, because
    /// one long job runs at a time and docxide beside the detector is
    /// memory the budgets never measured.
    var waits = false
    /// The page shows dark (`darkPage(_:)`); its markers go with it.
    var darkPage = false
    /// The rendered comments, for the title menu's Show comments and "Not
    /// on the page (N)".
    var onComments: (DocxComments) -> Void = { _ in }
    /// The thread the comment sheet shows: its passage is strengthened and
    /// the page keeps it above the sheet.
    var activeComment: String?
    /// A marker or passage opens the comment sheet at this thread.
    var onOpenComment: (String) -> Void = { _ in }
    @Environment(\.preferences) private var preferences
    @State private var rendering = Rendering.working
    @State private var request = UUID()
    /// The expanded rail card.
    @State private var expanded: String?
    @State private var width: CGFloat = 0
    @State private var viewport = PageViewport()

    enum Rendering: Sendable {
        case working
        case ready(RenderedDocx)
        case password(DocumentViewModel.PasswordPrompt)
        case failed(String)
    }

    private struct TaskKey: Equatable {
        let source: OriginalBytes
        let waits: Bool
        let unlocked: Bool
    }

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(uiColor: .secondarySystemBackground))
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .task(id: TaskKey(source: source, waits: waits, unlocked: unlocked != nil)) {
                if waits {
                    request = UUID()
                    return
                }
                if case .ready(let shown) = rendering, shown.source == source { return }
                await show()
            }
            .onDisappear {
                request = UUID()
                rendering = .working
            }
    }

    private func show(password: String? = nil) async {
        guard !waits else { return }
        let current = UUID()
        request = current
        rendering = .working
        let (result, newUnlock) = await RenderedDocx.render(source, unlocked: unlocked, password: password)
        guard !Task.isCancelled, request == current else { return }
        rendering = result
        if case .ready(let rendered) = result {
            if let newUnlock { unlocked = newUnlock }
            onComments(rendered.comments)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch rendering {
        case .working where waits:
            ContentUnavailableView(
                "Original waits", systemImage: "hourglass",
                description: Text("The original page shows once mdoc finishes the current job."))
        case .working:
            ProgressView("Rendering the original…")
                .accessibilityIdentifier("docx-rendering")
        case .password(let prompt):
            OriginalPasswordCard(prompt: prompt, waits: waits, importPending: importPending) { password in
                await show(password: password)
            }
        case .failed(let message):
            ContentUnavailableView(
                "No original page", systemImage: "doc.plaintext",
                description: Text(message))
        case .ready(let rendered):
            CommentedPDFView(
                rendered: rendered,
                showsComments: preferences.showsComments,
                rail: showsRail,
                active: activeComment ?? expanded,
                onMarker: { marker in
                    if let first = marker.threads.first { onOpenComment(first.id) }
                },
                onCard: { id in expanded = expanded == id ? nil : id },
                onPassage: onOpenComment,
                viewport: viewport)
            .darkPage(darkPage)
            .originalPage()
            .accessibilityLabel(Text("Original document"))
            .accessibilityIdentifier("docx-original")
            .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
        }
    }

    /// The rail needs an iPad's wide pane: landscape, beside the text.
    private var showsRail: Bool {
        UIDevice.current.userInterfaceIdiom == .pad && width >= 560
    }
}

/// The PDF bytes and the comments shown by the pane.
final class RenderedDocx: Sendable {
    let id = UUID()
    let source: OriginalBytes
    let pdf: Data
    let comments: DocxComments

    private init(original: DocxOriginal, source: OriginalBytes, pdf: Data) {
        self.source = source
        self.pdf = pdf
        comments = DocxComments(comments: original.comments(), anchors: original.anchors())
    }

    static func render(
        _ source: OriginalBytes, unlocked: UnlockedDocx? = nil, password: String? = nil
    ) async -> (rendering: DocxOriginalView.Rendering, unlocked: UnlockedDocx?) {
        await Task.detached(priority: .userInitiated) {
            do {
                let input = try unlocked.map { DocxInput.unlocked(source: $0) } ?? .file(
                    bytes: readSource(source), password: password)
                let marks = CommentMarks()
                let original = try DocxOriginal.render(input: input, marks: marks)
                // The callback only runs for a document with marks to place.
                let pdf = marks.renderedPDF ?? original.pdfBytes()
                return (.ready(RenderedDocx(original: original, source: source, pdf: pdf)), original.unlockedSource())
            } catch {
                switch CoreError(error) {
                case .needsPassword: return (.password(.needed), nil)
                case .wrongPassword: return (.password(.wrong), nil)
                case let failure: return (.failed(failure.localizedDescription), nil)
                }
            }
        }.value
    }

    static func readSource(_ source: OriginalBytes) throws -> Data {
        // One byte past the core's limit so an oversized source fails there
        // without sending the entire file across the bridge.
        let limit = Int(docxPreviewMaxInputBytes()) + 1
        switch try source.resolved() {
        case .memory(let data):
            return data.bytes.prefix(limit)
        case .file(let url):
            let file = try FileHandle(forReadingFrom: url)
            defer { try? file.close() }
            return try file.read(upToCount: limit) ?? Data()
        case .stored:
            throw CoreError.notFound
        }
    }
}
