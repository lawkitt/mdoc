import MdocFFI
import PDFKit
import SwiftUI

/// A DOCX file's original page: docxide renders it in process into a
/// temporary PDF each time the pane shows, and the PDF goes when it leaves
/// (`DocxOriginal`). Its comments sit on the page as margin markers that
/// open a sheet with their thread, or on an iPad's wide pane as the rail.
/// "Comments N" shows or hides them, an app-wide setting.
struct DocxOriginalView: View {
    let source: URL
    /// A long job runs on the document: the render waits for it, because
    /// one long job runs at a time and docxide beside the detector is
    /// memory the budgets never measured.
    var waits = false
    @Environment(\.preferences) private var preferences
    @State private var rendering = Rendering.working
    /// The sheet: a marker's threads, or those with no place on the page.
    @State private var sheet: Sheet?
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

    struct Sheet: Identifiable {
        let id: String
        let title: String
        let threads: [DocxComments.Thread]
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
            }
            // The PDF goes with the pane.
            .onDisappear { rendering = .working; sheet = nil }
            .sheet(item: $sheet) { sheet in
                CommentThreadSheet(title: sheet.title, threads: sheet.threads)
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
                active: sheet?.threads.first?.id ?? expanded,
                onMarker: { marker in
                    sheet = Sheet(
                        id: marker.id,
                        title: marker.threads.count == 1
                            ? String(localized: "Comment") : String(localized: "\(marker.threads.count) comments"),
                        threads: marker.threads)
                },
                onCard: { id in expanded = expanded == id ? nil : id },
                viewport: viewport)
            .accessibilityLabel(Text("Original document"))
            .accessibilityIdentifier("docx-original")
            .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
            .safeAreaInset(edge: .top, spacing: 0) {
                if !rendered.comments.all.isEmpty {
                    commentsBar(rendered.comments)
                }
            }
        }
    }

    /// The rail needs an iPad's wide pane: landscape, beside the text.
    private var showsRail: Bool {
        UIDevice.current.userInterfaceIdiom == .pad && width >= 560
    }

    /// "Comments N" shows or hides them; "Not on the page (N)" opens those
    /// with no place on the page.
    private func commentsBar(_ comments: DocxComments) -> some View {
        HStack(spacing: Space.xs) {
            Toggle(isOn: Bindable(preferences).showsComments) {
                Label(String(localized: "Comments \(comments.all.count)"), systemImage: "text.bubble")
            }
            .toggleStyle(.button)
            .accessibilityLabel(Text("Show comments, \(comments.all.count)"))
            .accessibilityIdentifier("show-comments")
            if preferences.showsComments, !comments.unplaced.isEmpty {
                Button {
                    sheet = Sheet(
                        id: "unplaced", title: String(localized: "Not on the page"), threads: comments.unplaced)
                } label: {
                    Text("Not on the page (\(comments.unplaced.count))")
                }
                .accessibilityIdentifier("comments-not-on-page")
            }
            Spacer(minLength: 0)
        }
        .font(.subheadline)
        .buttonStyle(.bordered)
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xxs)
        .background(.bar)
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
