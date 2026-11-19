import MdocFFI
import PDFKit
import SwiftUI

/// A DOCX file's original page: docxide renders it in process into a
/// temporary PDF each time the pane shows, and the PDF goes when it leaves
/// (`DocxOriginal`). Its comments sit on the page as margin markers that
/// open the comment sheet at their thread (`onOpenComment`; the document
/// screen holds the sheet), or on an iPad's wide pane as the rail. The
/// title menu's Show comments shows or hides them, an app-wide setting,
/// and opens those not on the page (`onComments` tells it what there is).
struct DocxOriginalView: View {
    let source: URL
    /// A long job runs on the document: the render waits for it, because
    /// one long job runs at a time and docxide beside the detector is
    /// memory the budgets never measured.
    var waits = false
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
    /// The expanded rail card.
    @State private var expanded: String?
    @State private var width: CGFloat = 0
    @State private var viewport = PageViewport()

    enum Rendering: Sendable {
        case working
        case ready(RenderedDocx)
        case failed(String)
    }

    private struct TaskKey: Equatable {
        let source: URL
        let waits: Bool
    }

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(uiColor: .secondarySystemBackground))
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .task(id: TaskKey(source: source, waits: waits)) {
                guard !waits else { return }
                if case .ready(let shown) = rendering, shown.source == source { return }
                rendering = .working
                rendering = await RenderedDocx.render(source)
                if case .ready(let rendered) = rendering { onComments(rendered.comments) }
            }
            // The PDF goes with the pane.
            .onDisappear { rendering = .working }
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
        case .failed(let message):
            // The import counted the comments here, so they are counted here
            // when the page can't show them.
            ContentUnavailableView(
                "No original page", systemImage: "doc.plaintext",
                description: Text(([message] + [docxCommentsNotShown(source: source.path)].compactMap { $0 })
                    .joined(separator: "\n")))
        case .ready(let rendered):
            CommentedPDFView(
                url: rendered.pdf,
                comments: rendered.comments,
                showsComments: preferences.showsComments,
                rail: showsRail,
                active: activeComment ?? expanded,
                onMarker: { marker in
                    if let first = marker.threads.first { onOpenComment(first.id) }
                },
                onCard: { id in expanded = expanded == id ? nil : id },
                viewport: viewport)
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

/// A rendered DOCX original: the core's object keeps the PDF on disk while
/// this lives.
final class RenderedDocx: Sendable {
    let original: DocxOriginal
    let source: URL
    let pdf: URL
    let comments: DocxComments

    private init(original: DocxOriginal, source: URL) {
        self.original = original
        self.source = source
        pdf = URL(fileURLWithPath: original.pdfPath())
        comments = DocxComments(comments: original.comments(), anchors: original.anchors())
    }

    /// Renders off the main thread into a temporary folder under Data
    /// Protection Complete.
    static func render(_ source: URL) async -> DocxOriginalView.Rendering {
        await Task.detached(priority: .userInitiated) {
            do {
                let folder = try folder()
                let original = try DocxOriginal.render(source: source.path, folder: folder.path, marks: CommentMarks())
                return .ready(RenderedDocx(original: original, source: source))
            } catch {
                return .failed(CoreError(error).localizedDescription)
            }
        }.value
    }

    /// The renders' folder in the app's temporary folder: its files take
    /// its Data Protection Complete and are never backed up. A render left
    /// by a crash goes at the next launch (`removeLeftovers`).
    static func folder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("originals", isDirectory: true)
        try FileManager.default.createDirectory(
            at: folder, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
        return folder
    }

    static func removeLeftovers() {
        try? FileManager.default.removeItem(
            at: FileManager.default.temporaryDirectory.appendingPathComponent("originals", isDirectory: true))
    }
}
